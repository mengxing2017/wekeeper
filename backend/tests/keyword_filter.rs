//! Global configuration and worker behavior against an explicitly isolated database.
use serde_json::Value;
use sqlx::{PgPool, Row, postgres::PgPoolOptions};
use uuid::Uuid;
use wekeeper_api::{
    business::Actor,
    message_orchestration::{
        domain::{AcceptMessage, EventKind, OriginalMessage},
        north::local::MessageIntake,
        south::adapter::postgres::Repository,
    },
    source_access::{
        domain::keyword_filter::{Command, Entry, Publication},
        north::local::keyword_filter::Service,
        south::adapter::postgres::keyword_filter::KeywordRepository,
    },
};

async fn current(service: &Service<KeywordRepository>, actor: &Actor) -> Value {
    service
        .execute(
            actor,
            Command::Versions {
                scope: "global".into(),
                page: 1,
            },
        )
        .await
        .unwrap()["items"][0]
        .clone()
}
fn publication(revision: i64, enabled: bool) -> Command {
    Command::Publish {
        scope: "global".into(),
        input: Publication {
            expected_revision: revision,
            filter_enabled: enabled,
            reason: "test global switch".into(),
            entries: vec![
                Entry {
                    word: "诈骗".into(),
                    enabled: true,
                },
                Entry {
                    word: "欺诈".into(),
                    enabled: true,
                },
            ],
        },
    }
}
async fn message(pool: &PgPool, platform: Uuid, unit: Uuid) -> Uuid {
    MessageIntake(Repository(pool.clone()))
        .accept(AcceptMessage {
            platform_id: platform,
            unit_id: unit,
            source_event_id: Uuid::new_v4(),
            evidence_ref: Uuid::new_v4(),
            original: OriginalMessage {
                event_kind: EventKind::Upsert,
                occurred_at: chrono::Utc::now(),
                content: "诈骗\n欺诈".into(),
            },
        })
        .await
        .unwrap()
        .id
}
async fn dispatch(pool: &PgPool, id: Uuid) -> Value {
    sqlx::query_scalar("SELECT to_jsonb(d) FROM orchestration_dispatches d WHERE message_id=$1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn global_switch_preserves_words_versions_authorization_and_frozen_decisions() {
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(
            &std::env::var("WEKEEPER_TEST_DATABASE_URL")
                .expect("explicit isolated test database required"),
        )
        .await
        .unwrap();
    wekeeper_api::database::migrate(&pool).await.unwrap();
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO accounts(id,username,display_name,platform_admin) VALUES($1,$2,'admin',true)",
    )
    .bind(id)
    .bind(id.to_string())
    .execute(&pool)
    .await
    .unwrap();
    let actor = Actor {
        id,
        username: "test".into(),
        display_name: "test".into(),
        platform_admin: true,
        csrf: String::new(),
        session_hash: String::new(),
        request_id: Uuid::new_v4().to_string(),
        browser: false,
        wecom_authenticated: false,
        wecom_binding_revision: 0,
    };
    let service = Service(KeywordRepository(pool.clone()));
    let initial = current(&service, &actor).await;
    // Migration creates a new disabled version without rewriting old immutable versions.
    let migrated: bool = sqlx::query_scalar("SELECT filter_enabled FROM keyword_rule_versions WHERE reason='引入敏感词拦截总开关，默认关闭' AND scope_id='global'")
        .fetch_one(&pool).await.unwrap();
    assert!(!migrated);
    let old_enabled: bool = sqlx::query_scalar("SELECT bool_and(filter_enabled) FROM keyword_rule_versions WHERE scope_id='global' AND revision < (SELECT revision FROM keyword_rule_versions WHERE scope_id='global' AND reason='引入敏感词拦截总开关，默认关闭')")
        .fetch_one(&pool).await.unwrap();
    assert!(old_enabled);
    let revision = initial["revision"].as_i64().unwrap();
    service
        .execute(&actor, publication(revision, false))
        .await
        .unwrap();
    let disabled = current(&service, &actor).await;
    let mut identities = vec![];
    for _ in 0..2 {
        let platform = Uuid::new_v4();
        let unit = Uuid::new_v4();
        sqlx::query("INSERT INTO source_callers(source_id) VALUES($1)")
            .bind(platform)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO business_units(id,platform_id,name,created_by,updated_by) VALUES($1,$2,'test',$3,$3)")
            .bind(unit).bind(platform).bind(id).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO source_routes(route_id,source_id,business_unit_id,route_name,status) VALUES($1,$2,$3,'test','active')")
            .bind(Uuid::new_v4().to_string()).bind(platform).bind(unit).execute(&pool).await.unwrap();
        identities.push((platform, unit));
    }
    let worker = Repository(pool.clone());
    let mut skipped = vec![];
    for &(platform, unit) in &identities {
        skipped.push(message(&pool, platform, unit).await);
    }
    worker.run_once().await.unwrap();
    for &message in &skipped {
        let d = dispatch(&pool, message).await;
        assert_eq!(d["keyword_decision"]["outcome"], "skipped");
        assert_eq!(d["keyword_decision"]["filter_enabled"], false);
        // Other business checks still run: these units deliberately have no targets.
        assert_eq!(d["reason_code"], "route_no_targets");
    }
    let revision = disabled["revision"].as_i64().unwrap();
    let (a, b) = tokio::join!(
        service.execute(&actor, publication(revision, true)),
        service.execute(&actor, publication(revision, true))
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    let conflict = a.err().or_else(|| b.err()).unwrap();
    assert_eq!(conflict.0, "revision_conflict");
    let enabled = current(&Service(KeywordRepository(pool.clone())), &actor).await;
    assert_eq!(enabled["filter_enabled"], true);
    assert_eq!(enabled["entries"], disabled["entries"]);
    // A legacy ready dispatch already froze its content before the switch changed.
    // A missing historical decision must not trigger a new check of its matching body.
    let frozen_id = message(&pool, identities[0].0, identities[0].1).await;
    sqlx::query("INSERT INTO orchestration_dispatches(message_id,content,decision,plans,state) VALUES($1,'frozen body','{}','[]','ready')")
        .bind(frozen_id).execute(&pool).await.unwrap();
    let mut blocked = vec![];
    for &(platform, unit) in &identities {
        blocked.push(message(&pool, platform, unit).await);
    }
    worker.run_once().await.unwrap();
    let frozen = dispatch(&pool, frozen_id).await;
    assert_eq!(frozen["content"], "frozen body");
    assert_eq!(frozen["state"], "failed");
    assert_eq!(frozen["reason_code"], "channel_resources_missing");
    assert!(frozen["keyword_decision"].is_null());
    for &message in &blocked {
        let d = dispatch(&pool, message).await;
        assert_eq!(d["reason_code"], "keyword_blocked");
        assert_eq!(d["keyword_decision"]["outcome"], "blocked");
    }
    service
        .execute(
            &actor,
            publication(enabled["revision"].as_i64().unwrap(), false),
        )
        .await
        .unwrap();
    worker.run_once().await.unwrap();
    for &message in &blocked {
        let d = dispatch(&pool, message).await;
        assert_eq!(d["reason_code"], "keyword_blocked");
        assert_eq!(d["keyword_decision"]["filter_enabled"], true);
    }
    for &message in &skipped {
        assert_eq!(
            dispatch(&pool, message).await["keyword_decision"]["outcome"],
            "skipped"
        );
    }
    let audit = sqlx::query("SELECT detail,request_id FROM audit_events WHERE actor_id=$1 AND detail->>'action'='publish' ORDER BY created_at DESC LIMIT 1")
        .bind(id).fetch_one(&pool).await.unwrap();
    let detail: Value = audit.get("detail");
    assert_eq!(detail["previous_filter_enabled"], true);
    assert_eq!(detail["filter_enabled"], false);
    assert_eq!(audit.get::<String, _>("request_id"), actor.request_id);
    for sql in [
        "UPDATE accounts SET platform_admin=false WHERE id=$1",
        "UPDATE accounts SET platform_admin=true,active=false WHERE id=$1",
    ] {
        sqlx::query(sql).bind(id).execute(&pool).await.unwrap();
        assert!(
            service
                .execute(
                    &actor,
                    Command::Versions {
                        scope: "global".into(),
                        page: 1
                    }
                )
                .await
                .is_err()
        );
        assert!(
            service
                .execute(
                    &actor,
                    publication(enabled["revision"].as_i64().unwrap() + 1, true)
                )
                .await
                .is_err()
        );
    }
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn switch_migration_preserves_existing_word_states_and_historical_dispatches() {
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(
            &std::env::var("WEKEEPER_TEST_DATABASE_URL")
                .expect("explicit isolated test database required"),
        )
        .await
        .unwrap();
    let mut tx = pool.begin().await.unwrap();
    // Transaction-local pre-switch tables isolate the upgrade fixture from other tests.
    sqlx::raw_sql("CREATE TEMP TABLE keyword_rule_versions(id uuid PRIMARY KEY,scope_id text NOT NULL,revision bigint NOT NULL,entries jsonb NOT NULL,reason text NOT NULL) ON COMMIT DROP;
        CREATE TEMP TABLE keyword_scope_heads(id text PRIMARY KEY,revision bigint NOT NULL,version_id uuid NOT NULL) ON COMMIT DROP;
        CREATE TEMP TABLE orchestration_dispatches(message_id uuid PRIMARY KEY,state text NOT NULL) ON COMMIT DROP;
        INSERT INTO keyword_rule_versions VALUES('00000000-0000-0000-0000-000000000001','global',7,'[{\"word\":\"诈骗\",\"enabled\":true},{\"word\":\"欺诈\",\"enabled\":false}]','old');
        INSERT INTO keyword_scope_heads VALUES('global',7,'00000000-0000-0000-0000-000000000001');
        INSERT INTO orchestration_dispatches VALUES('00000000-0000-0000-0000-000000000002','ready');")
        .execute(&mut *tx).await.unwrap();
    sqlx::raw_sql(include_str!(
        "../migrations/202611160189_keyword_filter_global_switch.sql"
    ))
    .execute(&mut *tx)
    .await
    .unwrap();
    let row = sqlx::query("SELECT h.revision,v.filter_enabled,v.entries FROM keyword_scope_heads h JOIN keyword_rule_versions v ON v.id=h.version_id")
        .fetch_one(&mut *tx).await.unwrap();
    assert_eq!(row.get::<i64, _>("revision"), 8);
    assert!(!row.get::<bool, _>("filter_enabled"));
    let old =
        sqlx::query("SELECT entries,filter_enabled FROM keyword_rule_versions WHERE revision=7")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert!(old.get::<bool, _>("filter_enabled"));
    assert_eq!(
        row.get::<Value, _>("entries"),
        old.get::<Value, _>("entries")
    );
    let historical: Value =
        sqlx::query_scalar("SELECT to_jsonb(d) FROM orchestration_dispatches d")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert_eq!(historical["state"], "ready");
    assert!(historical["keyword_decision"].is_null());
    sqlx::query("INSERT INTO keyword_rule_versions(id,scope_id,revision,entries,reason) VALUES($1,'global',9,'[]','future')")
        .bind(Uuid::new_v4()).execute(&mut *tx).await.unwrap();
    let default_enabled: bool =
        sqlx::query_scalar("SELECT filter_enabled FROM keyword_rule_versions WHERE revision=9")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert!(!default_enabled);
    tx.rollback().await.unwrap();
}
