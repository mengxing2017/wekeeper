use serde_json::{Value, json};
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use uuid::Uuid;
use wekeeper_api::{
    business::{Actor, Page, Result},
    database,
    distribution::{
        domain::broadcast::*,
        north::broadcast::Service,
        south::{
            adapter::postgres::broadcast::Repository,
            port::broadcast::{Client, Command, Store},
        },
    },
};
#[derive(Clone)]
struct Fake {
    calls: Arc<AtomicUsize>,
    unknown: bool,
    corp: String,
}
impl Client for Fake {
    async fn generation(&self) -> Result<(String, i64)> {
        Ok((self.corp.clone(), 1))
    }
    async fn verify(&self, _: &Work) -> Result<()> {
        Ok(())
    }
    async fn invoke(&self, w: &Work) -> Outcome {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.unknown {
            Outcome::Unknown
        } else if w.operation == "create" {
            Outcome::Created {
                msgid: format!("msg-{}", w.id),
                failed: vec![],
            }
        } else {
            Outcome::Done
        }
    }
    async fn results(&self, w: &Work) -> Result<Results> {
        Ok(Results {
            member_status: Some(2),
            groups: vec![Observation {
                chat_id: w.chats[0].clone(),
                status: 1,
                send_time: Some(1700000000),
            }],
        })
    }
    async fn candidates(&self, _: i64, _: i64) -> Result<Vec<Value>> {
        Ok(vec![])
    }
}
async fn setup() -> (PgPool, Actor, Uuid, DraftInput, Fake) {
    let url = std::env::var("WEKEEPER_TEST_DATABASE_URL")
        .expect("explicit isolated test database required");
    let base = PgPool::connect(&url).await.unwrap();
    let schema = format!("broadcast_{}", Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE SCHEMA {schema}"))
        .execute(&base)
        .await
        .unwrap();
    let pool = PgPoolOptions::new()
        .max_connections(8)
        .after_connect(move |c, _| {
            let sql = format!("SET search_path TO {schema},public");
            Box::pin(async move {
                sqlx::query(&sql).execute(c).await?;
                Ok(())
            })
        })
        .connect(&url)
        .await
        .unwrap();
    database::migrate(&pool).await.unwrap();
    let actor = Uuid::new_v4();
    let s = Uuid::new_v4();
    let member = Uuid::new_v4();
    let version = Uuid::new_v4();
    let template = Uuid::new_v4();
    let post = Uuid::new_v4();
    let corp = format!("corp-{s}");
    sqlx::query("INSERT INTO schools(id,name) VALUES($1,'School')")
        .bind(s)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO wecom_directory_state(singleton,corp_id,credential_scope) VALUES(true,$1,'scope')").bind(&corp).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO wecom_members(id,userid,name,status,verification,checked_at) VALUES($1,'owner','Owner',1,'verified',now())").bind(member).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO accounts(id,username,display_name,wecom_member_id,platform_admin) VALUES($1,'admin','Admin',$2,true)").bind(actor).bind(member).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO school_memberships VALUES($1,$2,'admin')")
        .bind(s)
        .bind(actor)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO templates(id,school_id,name) VALUES($1,$2,'Notice')")
        .bind(template)
        .bind(s)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO template_versions(id,school_id,template_id,body,published) VALUES($1,$2,$3,'{{post.title}}',true)").bind(version).bind(s).bind(template).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO posts(id,school_id,author_id,title,content,audit_status) VALUES($1,$2,$3,'Notice','Body','approved')").bind(post).bind(s).bind(actor).execute(&pool).await.unwrap();
    let mut groups = Vec::new();
    for i in 0..2 {
        let group = Uuid::new_v4();
        let directory = Uuid::new_v4();
        let chat = format!("chat-{i}");
        sqlx::query("INSERT INTO wecom_asset_objects(id,kind,corp_id,key,data,checked_at,credential_generation,visibility,verification) VALUES($1,'groups',$2,$3,$4,now(),1,'visible','verified')").bind(directory).bind(&corp).bind(&chat).bind(json!({"owner":"owner"})).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO wecom_groups(id,school_id,corp_id,chat_id,name,directory_group_id) VALUES($1,$2,$3,$4,$4,$5)").bind(group).bind(s).bind(&corp).bind(chat).bind(directory).execute(&pool).await.unwrap();
        groups.push(group);
    }
    let a:Actor=serde_json::from_value(json!({"id":actor,"username":"admin","display_name":"Admin","platform_admin":true,"bound_school_id":null,"grants":[{"school_id":s,"role":"admin"}],"wecom_authenticated":false})).unwrap();
    let input = DraftInput {
        template_version_id: Some(version),
        content_source: ContentSource::Posts,
        text_content: None,
        post_ids: vec![post],
        group_ids: groups,
        revision: None,
        source_task_id: None,
        source_draft_id: None,
        reason: String::new(),
    };
    (
        pool,
        a,
        s,
        input,
        Fake {
            calls: Arc::new(AtomicUsize::new(0)),
            unknown: false,
            corp,
        },
    )
}
fn action(revision: Option<i64>) -> Action {
    Action {
        revision,
        batch_id: None,
        decision: None,
        msgid: None,
        reason: String::new(),
    }
}
#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn broadcast_once_partial_results_and_school_boundaries() {
    let (pool, a, s, input, fake) = setup().await;
    let service = Service {
        store: Repository(pool.clone()),
        client: fake.clone(),
        enabled: true,
        management_available: true,
        origin: "https://school.test".into(),
    };
    let t = service.save(&a, s, None, input.clone()).await.unwrap();
    assert_eq!(t.total, 2);
    assert_eq!(t.batch_count, 1);
    assert_eq!(fake.calls.load(Ordering::SeqCst), 0);
    assert!(service.task(&a, Uuid::new_v4(), t.id).await.is_err());
    let confirm = action(Some(t.revision));
    service
        .action(&a, s, t.id, "confirm", "key", confirm.clone())
        .await
        .unwrap();
    service
        .action(&a, s, t.id, "confirm", "key", confirm)
        .await
        .unwrap();
    assert!(
        service
            .action(&a, s, t.id, "confirm", "key", action(Some(999)))
            .await
            .is_err()
    );
    let (x, y) = tokio::join!(service.run_once(), service.run_once());
    x.unwrap();
    y.unwrap();
    assert_eq!(fake.calls.load(Ordering::SeqCst), 1);
    service.run_once().await.unwrap();
    let t = service.task(&a, s, t.id).await.unwrap();
    assert_eq!(t.sent, 1);
    assert_eq!(t.pending, 1, "missing group is not fabricated as sent");
    let rows = service
        .rows(&a, s, t.id, "targets", Page::default())
        .await
        .unwrap();
    assert!(rows.iter().any(|r| r["official_status"].is_null()));
    let duplicate = service.save(&a, s, None, input.clone()).await.unwrap();
    assert!(
        service
            .action(&a, s, duplicate.id, "confirm", "duplicate", action(Some(1)))
            .await
            .is_err()
    );
    service
        .action(&a, s, t.id, "stop", "stop", action(None))
        .await
        .unwrap();
    service.run_once().await.unwrap();
    assert_eq!(fake.calls.load(Ordering::SeqCst), 2);
    // Successes are excluded by default in the UI, but an administrator can explicitly
    // select an original successful target with a reason after the old task is stopped.
    let mut explicit_repeat = input.clone();
    explicit_repeat.source_task_id = Some(t.id);
    explicit_repeat.reason = "Explicit repeat of a previously sent notice".into();
    explicit_repeat.group_ids = rows
        .iter()
        .filter(|r| r["official_status"] == 1)
        .map(|r| r["group_id"].as_str().unwrap().parse().unwrap())
        .collect();
    service.save(&a, s, None, explicit_repeat).await.unwrap();
    let mut retry = input;
    retry.source_task_id = Some(t.id);
    retry.reason = "failed targets only".into();
    retry.group_ids = rows
        .iter()
        .filter(|r| r["official_status"] != 1)
        .map(|r| r["group_id"].as_str().unwrap().parse().unwrap())
        .collect();
    let retry = service.save(&a, s, None, retry).await.unwrap();
    service
        .action(&a, s, retry.id, "confirm", "retry", action(Some(1)))
        .await
        .unwrap();
    // Platform administration alone is never a school sending grant.
    sqlx::query("DELETE FROM school_memberships WHERE account_id=$1")
        .bind(a.id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(service.list(&a, s, Page::default()).await.is_err());
    service.run_once().await.unwrap();
    assert_eq!(fake.calls.load(Ordering::SeqCst), 2);
}
#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn lost_response_and_crash_never_requeue_a_create() {
    let (pool, a, s, input, mut fake) = setup().await;
    fake.unknown = true;
    let service = Service {
        store: Repository(pool.clone()),
        client: fake.clone(),
        enabled: true,
        management_available: true,
        origin: "https://school.test".into(),
    };
    let t = service.save(&a, s, None, input.clone()).await.unwrap();
    service
        .action(&a, s, t.id, "confirm", "confirm", action(Some(1)))
        .await
        .unwrap();
    service.run_once().await.unwrap();
    service.run_once().await.unwrap();
    assert_eq!(fake.calls.load(Ordering::SeqCst), 1);
    assert_eq!(service.task(&a, s, t.id).await.unwrap().uncertain, 1);
    let duplicate = service.save(&a, s, None, input).await.unwrap();
    assert!(
        service
            .action(&a, s, duplicate.id, "confirm", "dup", action(Some(1)))
            .await
            .is_err()
    );
    let w = service.store.work(Some(t.id)).await.unwrap().remove(0);
    assert!(!service.store.claim(&w).await.unwrap());
    // Evidence is independent of platform state, and unlocks only explicitly confirmed non-creation.
    let mut reconcile = action(None);
    reconcile.batch_id = Some(w.id);
    reconcile.decision = Some("not_created".into());
    reconcile.reason = "checked enterprise admin logs".into();
    service
        .action(&a, s, t.id, "reconcile", "evidence", reconcile)
        .await
        .unwrap();
    assert_eq!(service.task(&a, s, t.id).await.unwrap().uncertain, 1);
    assert_eq!(fake.calls.load(Ordering::SeqCst), 1);
    // Simulate a process dying after the persisted invocation claim.
    sqlx::query("UPDATE broadcast_batches SET state='submitting',reconciliation=NULL,claimed_at=now()-interval '3 minutes' WHERE id=$1").bind(w.id).execute(&pool).await.unwrap();
    service.run_once().await.unwrap();
    assert_eq!(fake.calls.load(Ordering::SeqCst), 1);
    assert_eq!(service.task(&a, s, t.id).await.unwrap().uncertain, 1);
}
#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn drafts_are_revisioned_and_preflight_rechecks_content() {
    let (pool, a, s, input, fake) = setup().await;
    let service = Service {
        store: Repository(pool.clone()),
        client: fake.clone(),
        enabled: true,
        management_available: true,
        origin: "https://school.test".into(),
    };
    let t = service.save(&a, s, None, input.clone()).await.unwrap();
    let mut edit = input.clone();
    edit.revision = Some(1);
    let updated = service.save(&a, s, Some(t.id), edit.clone()).await.unwrap();
    assert_eq!(updated.revision, 2);
    assert!(service.save(&a, s, Some(t.id), edit).await.is_err());
    sqlx::query("UPDATE posts SET revision=revision+1 WHERE id=$1")
        .bind(input.post_ids[0])
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        service
            .action(&a, s, t.id, "confirm", "c", action(Some(2)))
            .await
            .is_err()
    );
    assert_eq!(fake.calls.load(Ordering::SeqCst), 0);
    // Disabled deployments cannot enqueue an external creation even with valid local content.
    let disabled = Service {
        enabled: false,
        ..service.clone()
    };
    let fresh = disabled.save(&a, s, None, input).await.unwrap();
    assert!(
        disabled
            .action(&a, s, fresh.id, "confirm", "c", action(Some(1)))
            .await
            .is_err()
    );
    assert_eq!(
        service.store.list(s, Page::default()).await.unwrap().len(),
        2
    );
    // Exercise command port without any HTTP-layer SQL.
    service
        .store
        .execute(
            &a,
            s,
            Command::Action {
                id: fresh.id,
                operation: "abandon".into(),
                key: "abandon".into(),
                input: action(None),
            },
        )
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn stopping_during_creation_covers_late_created_task() {
    let (pool, a, s, input, fake) = setup().await;
    let service = Service {
        store: Repository(pool),
        client: fake.clone(),
        enabled: true,
        management_available: true,
        origin: "https://school.test".into(),
    };
    let t = service.save(&a, s, None, input).await.unwrap();
    service
        .action(&a, s, t.id, "confirm", "confirm", action(Some(1)))
        .await
        .unwrap();
    let w = service.store.work(Some(t.id)).await.unwrap().remove(0);
    assert!(service.store.claim(&w).await.unwrap());
    service
        .action(&a, s, t.id, "stop", "stop", action(None))
        .await
        .unwrap();
    service
        .store
        .finish(
            &w,
            Outcome::Created {
                msgid: "late-msgid".into(),
                failed: vec![],
            },
        )
        .await
        .unwrap();
    service.run_once().await.unwrap();
    assert_eq!(
        fake.calls.load(Ordering::SeqCst),
        1,
        "only the late stop is invoked"
    );
    let rows = service
        .rows(&a, s, t.id, "targets", Page::default())
        .await
        .unwrap();
    assert!(rows.iter().all(|r| r["stop_state"] == "stopped"));
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn forward_upgrade_preserves_legacy_snapshots() {
    use sqlx::migrate::Migrator;
    use std::borrow::Cow;
    let url = std::env::var("WEKEEPER_TEST_DATABASE_URL").unwrap();
    let base = PgPool::connect(&url).await.unwrap();
    let schema = format!("upgrade_{}", Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE SCHEMA {schema}"))
        .execute(&base)
        .await
        .unwrap();
    let pool = PgPoolOptions::new()
        .after_connect(move |c, _| {
            let sql = format!("SET search_path TO {schema}");
            Box::pin(async move {
                sqlx::query(&sql).execute(c).await?;
                Ok(())
            })
        })
        .connect(&url)
        .await
        .unwrap();
    let old = Migrator {
        migrations: Cow::Owned(
            database::MIGRATOR
                .iter()
                .filter(|m| m.version < 202610220002)
                .cloned()
                .collect(),
        ),
        ..Migrator::DEFAULT
    };
    old.run(&pool).await.unwrap();
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO schools(id,name) VALUES($1,'Legacy')")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO accounts(id,username,display_name) VALUES($1,'legacy','Legacy')")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO wecom_groups(id,school_id,corp_id,chat_id,name) VALUES($1,$1,'corp','chat','Legacy')").bind(id).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO templates(id,school_id,name) VALUES($1,$1,'Template')")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO template_versions(id,school_id,template_id,body) VALUES($1,$1,$1,'Text')",
    )
    .bind(id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO delivery_drafts(id,school_id,group_id,template_version_id,creator_id,content,content_hash) VALUES($1,$1,$1,$1,$1,'Frozen text','hash')").bind(id).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO delivery_attempts(id,school_id,draft_id,actor_id,idempotency_key,state) VALUES($1,$1,$1,$1,'legacy','unknown')").bind(id).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO broadcast_tasks(id,school_id,creator_id,template_version_id,content,content_hash,corp_id,generation,state,revision) VALUES($1,$1,$1,$1,'Old task snapshot','old-hash','corp',1,'confirmed',3)").bind(id).execute(&pool).await.unwrap();
    database::migrate(&pool).await.unwrap();
    database::migrate(&pool).await.unwrap();
    let old_task: Value =
        sqlx::query_scalar("SELECT to_jsonb(t) FROM broadcast_tasks t WHERE id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(old_task["content_source"], "posts");
    assert_eq!(old_task["content"], "Old task snapshot");
    assert_eq!(old_task["content_hash"], "old-hash");
    assert_eq!(old_task["revision"], 3);
    assert_eq!(old_task["template_version_id"], id.to_string());
    assert!(
        sqlx::query("UPDATE broadcast_tasks SET content_source='text' WHERE id=$1")
            .bind(id)
            .execute(&pool)
            .await
            .is_err()
    );

    let content: String = sqlx::query_scalar("SELECT content FROM delivery_drafts WHERE id=$1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(content, "Frozen text");
    let state: String = sqlx::query_scalar("SELECT state FROM delivery_attempts WHERE id=$1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(state, "unknown");
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn http_scope_csrf_retirement_and_real_task_contract() {
    use axum::{
        body::{Body, to_bytes},
        http::Request,
    };
    use tower::ServiceExt;
    let (pool, a, s, input, fake) = setup().await;
    let service = Service {
        store: Repository(pool.clone()),
        client: fake,
        enabled: true,
        management_available: true,
        origin: "https://school.test".into(),
    };
    let t = service.save(&a, s, None, input).await.unwrap();
    let mut application = wekeeper_api::bootstrap::build_application(pool.clone());
    application.options.public_origin = "https://school.test".into();
    application.options.insecure_local_auth = true;
    let (token, actor) = application
        .identity
        .0
        .session(a.id, "browser", false)
        .await
        .unwrap();
    let app = wekeeper_api::http::app_router(application);
    let path = format!("/api/v1/schools/{s}/broadcast-tasks/{}", t.id);
    let request = |method: &str, path: &str, csrf: bool, body: Value| {
        let mut builder = Request::builder()
            .method(method)
            .uri(path)
            .header("cookie", format!("wekeeper_session={token}"))
            .header("origin", "https://school.test")
            .header("content-type", "application/vnd.api+json")
            .header("idempotency-key", "http-confirm");
        if csrf {
            builder = builder.header("x-csrf-token", &actor.csrf);
        }
        builder
            .body(if body.is_null() {
                Body::empty()
            } else {
                Body::from(body.to_string())
            })
            .unwrap()
    };
    let response = app
        .clone()
        .oneshot(request("GET", &path, false, Value::Null))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let value: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 65536).await.unwrap()).unwrap();
    assert_eq!(value["data"]["type"], "broadcast-tasks");
    assert_eq!(value["data"]["attributes"]["total"], 2);
    let body = json!({"data":{"type":"broadcast-tasks","attributes":{"revision":1}}});
    assert_eq!(
        app.clone()
            .oneshot(request(
                "POST",
                &format!("{path}/confirm"),
                false,
                body.clone()
            ))
            .await
            .unwrap()
            .status(),
        403
    );
    assert_eq!(
        app.clone()
            .oneshot(request("POST", &format!("{path}/confirm"), true, body))
            .await
            .unwrap()
            .status(),
        422,
        "missing server credentials or disabled broadcast must not invoke Tencent"
    );
    assert_eq!(
        app.clone()
            .oneshot(request(
                "POST",
                &format!("/api/v1/schools/{s}/delivery-drafts"),
                true,
                json!({"data":{"type":"delivery-drafts","attributes":{}}})
            ))
            .await
            .unwrap()
            .status(),
        422
    );
    sqlx::query("DELETE FROM school_memberships WHERE account_id=$1")
        .bind(a.id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        app.clone()
            .oneshot(request("GET", &path, false, Value::Null))
            .await
            .unwrap()
            .status(),
        403,
        "platform privilege does not bypass school grant"
    );
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn reminders_are_limited_and_stop_unknown_requires_evidence() {
    let (pool, a, s, input, fake) = setup().await;
    let service = Service {
        store: Repository(pool),
        client: fake,
        enabled: true,
        management_available: true,
        origin: "https://school.test".into(),
    };
    let t = service.save(&a, s, None, input).await.unwrap();
    service
        .action(&a, s, t.id, "confirm", "confirm", action(Some(1)))
        .await
        .unwrap();
    let w = service.store.work(Some(t.id)).await.unwrap().remove(0);
    service.store.claim(&w).await.unwrap();
    service
        .store
        .finish(
            &w,
            Outcome::Created {
                msgid: "reminder-test".into(),
                failed: vec![],
            },
        )
        .await
        .unwrap();
    for n in 0..3 {
        service
            .action(&a, s, t.id, "remind", &format!("remind-{n}"), action(None))
            .await
            .unwrap();
        let op = service
            .store
            .work(None)
            .await
            .unwrap()
            .into_iter()
            .find(|w| w.operation == "remind")
            .unwrap();
        assert!(service.store.claim(&op).await.unwrap());
        service.store.finish(&op, Outcome::Done).await.unwrap();
    }
    assert!(
        service
            .action(&a, s, t.id, "remind", "fourth", action(None))
            .await
            .is_err()
    );
    service
        .action(&a, s, t.id, "stop", "stop", action(None))
        .await
        .unwrap();
    let op = service
        .store
        .work(None)
        .await
        .unwrap()
        .into_iter()
        .find(|w| w.operation == "stop")
        .unwrap();
    assert!(service.store.claim(&op).await.unwrap());
    service.store.finish(&op, Outcome::Unknown).await.unwrap();
    assert!(
        !service
            .store
            .work(None)
            .await
            .unwrap()
            .iter()
            .any(|w| w.operation == "stop")
    );
    assert!(
        service
            .action(&a, s, t.id, "remind", "stopped-reminder", action(None))
            .await
            .is_err()
    );
    let mut evidence = action(None);
    evidence.batch_id = Some(w.id);
    evidence.decision = Some("stop_confirmed".into());
    evidence.reason = "checked official task stopped".into();
    service
        .action(&a, s, t.id, "reconcile", "evidence", evidence)
        .await
        .unwrap();
    let rows = service
        .rows(&a, s, t.id, "targets", Page::default())
        .await
        .unwrap();
    assert!(rows.iter().all(|r| r["stop_state"] == "stopped"));
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn direct_text_without_posts_admin_confirmation_and_revocation() {
    let (pool, a, s, mut input, fake) = setup().await;
    sqlx::query("DELETE FROM posts")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM template_versions")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM templates")
        .execute(&pool)
        .await
        .unwrap();
    let service = Service {
        store: Repository(pool.clone()),
        client: fake.clone(),
        enabled: true,
        management_available: true,
        origin: "https://school.test".into(),
    };
    input.content_source = ContentSource::Text;
    input.template_version_id = None;
    input.post_ids.clear();
    input.text_content = Some("直接通知\r\n第二行😀".into());
    sqlx::query("UPDATE school_memberships SET role='operator' WHERE account_id=$1")
        .bind(a.id)
        .execute(&pool)
        .await
        .unwrap();
    let t = service.save(&a, s, None, input.clone()).await.unwrap();
    assert_eq!(t.content, "直接通知\n第二行😀");
    assert_eq!(t.content_source, ContentSource::Text);
    assert_eq!(t.template_version_id, None);
    assert!(t.post_ids.is_empty());
    assert_eq!(fake.calls.load(Ordering::SeqCst), 0);
    assert!(
        service
            .action(&a, s, t.id, "confirm", "operator", action(Some(t.revision)))
            .await
            .is_err()
    );
    // The repository enforces the same rule, not just the HTTP/service boundary.
    assert!(
        service
            .store
            .execute(
                &a,
                s,
                Command::Action {
                    id: t.id,
                    operation: "confirm".into(),
                    key: "bypass".into(),
                    input: action(Some(t.revision))
                }
            )
            .await
            .is_err()
    );
    sqlx::query("UPDATE school_memberships SET role='admin' WHERE account_id=$1")
        .bind(a.id)
        .execute(&pool)
        .await
        .unwrap();
    service
        .action(&a, s, t.id, "confirm", "admin", action(Some(t.revision)))
        .await
        .unwrap();
    let work = service.store.work(Some(t.id)).await.unwrap();
    assert_eq!(work[0].content, t.content);
    sqlx::query("UPDATE school_memberships SET role='operator' WHERE account_id=$1")
        .bind(a.id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(service.store.claim(&work[0]).await.is_err());
    sqlx::query("UPDATE school_memberships SET role='admin' WHERE account_id=$1")
        .bind(a.id)
        .execute(&pool)
        .await
        .unwrap();
    service.run_once().await.unwrap();
    assert_eq!(fake.calls.load(Ordering::SeqCst), 1);
    assert!(service.save(&a, s, Some(t.id), input).await.is_err());
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn content_source_switch_and_cross_source_duplicate_guard() {
    let (pool, a, s, posts, fake) = setup().await;
    let service = Service {
        store: Repository(pool.clone()),
        client: fake,
        enabled: true,
        management_available: true,
        origin: "https://school.test".into(),
    };
    let first = service.save(&a, s, None, posts.clone()).await.unwrap();
    let mut text = posts.clone();
    text.content_source = ContentSource::Text;
    text.template_version_id = None;
    text.post_ids.clear();
    text.text_content = Some(first.content.clone());
    text.revision = Some(first.revision);
    let changed = service
        .save(&a, s, Some(first.id), text.clone())
        .await
        .unwrap();
    assert_eq!(changed.revision, first.revision + 1);
    assert!(changed.post_ids.is_empty());
    assert!(
        service
            .action(
                &a,
                s,
                changed.id,
                "confirm",
                "stale",
                action(Some(first.revision))
            )
            .await
            .is_err()
    );
    let mut back = posts.clone();
    back.revision = Some(changed.revision);
    let restored = service.save(&a, s, Some(first.id), back).await.unwrap();
    assert_eq!(restored.content_source, ContentSource::Posts);
    assert_eq!(restored.post_ids, posts.post_ids);
    service
        .action(
            &a,
            s,
            restored.id,
            "confirm",
            "posts",
            action(Some(restored.revision)),
        )
        .await
        .unwrap();
    text.revision = None;
    let duplicate = service.save(&a, s, None, text).await.unwrap();
    assert!(
        service
            .action(
                &a,
                s,
                duplicate.id,
                "confirm",
                "text",
                action(Some(duplicate.revision))
            )
            .await
            .is_err()
    );
}
