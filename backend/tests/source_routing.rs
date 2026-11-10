//! Isolated PostgreSQL tests for routing; no phone, network sender or production URL.
use serde_json::{Value, json};
use sqlx::{PgPool, postgres::PgPoolOptions};
use uuid::Uuid;
use wekeeper_api::{
    business::Actor,
    source_access::{
        domain::routing::*, north::local::routing::Service,
        south::adapter::postgres::routing::RoutingRepository,
    },
};
struct Fixture {
    test_guard: Option<sqlx::Transaction<'static, sqlx::Postgres>>,
    pool: PgPool,
    base: PgPool,
    schema: String,
    actor: Actor,
    source: Uuid,
    key: String,
    route: String,
    unit: Uuid,
    group: Uuid,
    workspace: Uuid,
    targets: Vec<Uuid>,
}
impl Fixture {
    async fn new() -> Self {
        Self::new_with_guard(true).await
    }
    async fn new_with_guard(ordinary: bool) -> Self {
        let url = std::env::var("WEKEEPER_TEST_DATABASE_URL")
            .expect("explicit isolated database required");
        let base = PgPool::connect(&url).await.unwrap();
        // The cross-pool test holds a full transaction deliberately. Exclude unrelated
        // tests that mutate the legacy global device barrier while that test runs.
        let test_guard = if ordinary {
            let mut guard = base.begin().await.unwrap();
            sqlx::query("SELECT pg_advisory_xact_lock_shared(74182318)")
                .execute(&mut *guard)
                .await
                .unwrap();
            Some(guard)
        } else {
            None
        };
        let schema = format!("source_routing_{}", Uuid::new_v4().simple());
        sqlx::query(&format!("CREATE SCHEMA {schema}"))
            .execute(&base)
            .await
            .unwrap();
        let search = format!("SET search_path TO {schema},public");
        let pool = PgPoolOptions::new()
            .max_connections(8)
            .after_connect(move |c, _| {
                let sql = search.clone();
                Box::pin(async move {
                    sqlx::query(&sql).execute(c).await?;
                    Ok(())
                })
            })
            .connect(&url)
            .await
            .unwrap();
        wekeeper_api::database::migrate(&pool).await.unwrap();
        let actor = Uuid::new_v4();
        let source = Uuid::new_v4();
        let key = Uuid::new_v4().to_string();
        let route = Uuid::new_v4().to_string();
        let workspace = Uuid::new_v4();
        sqlx::query("INSERT INTO accounts(id,username,display_name,platform_admin) VALUES($1,'routing-admin','Admin',true)").bind(actor).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO source_callers(source_id) VALUES($1)")
            .bind(source)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO source_credentials(key_id,source_id,unit_name,encryption_version,status,valid_from,created_by) VALUES($1,$2,'Caller','v1','active',now()-interval '1 day',$3)").bind(&key).bind(source).bind(actor).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO source_routes(route_id,source_id,route_name,status) VALUES($1,$2,'Route','active')").bind(&route).bind(source).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO source_key_routes(key_id,route_id,source_id,status) VALUES($1,$2,$3,'active')").bind(&key).bind(&route).bind(source).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO mobile_workspaces(id,name) VALUES($1,'Mobile')")
            .bind(workspace)
            .execute(&pool)
            .await
            .unwrap();
        let device = Uuid::new_v4();
        sqlx::query("INSERT INTO execution_devices(id,name,enabled,credential_generation) VALUES($1,'Offline',true,1)").bind(device).execute(&pool).await.unwrap();
        let mut targets = Vec::new();
        for n in 0..4 {
            let id = Uuid::new_v4();
            targets.push(id);
            sqlx::query("INSERT INTO mobile_targets(id,school_id,search_text,created_by,updated_by) VALUES($1,$2,$3,$4,$4)").bind(id).bind(workspace).bind(format!("群{n}")).bind(actor).execute(&pool).await.unwrap();
            sqlx::query("INSERT INTO mobile_device_target_bindings(id,school_id,device_id,target_id,created_by,updated_by) VALUES($1,$2,$3,$4,$5,$5)").bind(Uuid::new_v4()).bind(workspace).bind(device).bind(id).bind(actor).execute(&pool).await.unwrap();
        }
        let actor:Actor=serde_json::from_value(json!({"id":actor,"username":"routing-admin","display_name":"Admin","platform_admin":true,"grants":[],"wecom_authenticated":false})).unwrap();
        let service = Service(RoutingRepository(pool.clone()));
        let u = service
            .execute(
                &actor,
                Command::Entity {
                    kind: "routing-units".into(),
                    id: None,
                    input: EntityInput {
                        name: "同名单位".into(),
                        description: "".into(),
                        active: true,
                        revision: None,
                        unit_id: None,
                    },
                },
            )
            .await
            .unwrap();
        let unit = serde_json::from_value(u["id"].clone()).unwrap();
        let g = service
            .execute(
                &actor,
                Command::Entity {
                    kind: "routing-groups".into(),
                    id: None,
                    input: EntityInput {
                        name: "受众".into(),
                        description: "".into(),
                        active: true,
                        revision: None,
                        unit_id: Some(unit),
                    },
                },
            )
            .await
            .unwrap();
        let group = serde_json::from_value(g["id"].clone()).unwrap();
        Self {
            test_guard,
            pool,
            base,
            schema,
            actor,
            source,
            key,
            route,
            unit,
            group,
            workspace,
            targets,
        }
    }
    fn service(&self) -> Service<RoutingRepository> {
        Service(RoutingRepository(self.pool.clone()))
    }
    async fn revision(&self) -> i64 {
        sqlx::query_scalar("SELECT revision FROM source_routes WHERE route_id=$1")
            .bind(&self.route)
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }
    async fn configure(&self, targets: Vec<Uuid>) -> Value {
        self.service()
            .execute(
                &self.actor,
                Command::Configure {
                    route: self.route.clone(),
                    input: ConfigurationInput {
                        revision: self.revision().await,
                        unit_id: self.unit,
                        group_id: self.group,
                        execution: Some(ExecutionConfig {
                            channel: Channel::Mobile,
                            workspace_id: self.workspace,
                            target_ids: targets,
                        }),
                    },
                },
            )
            .await
            .unwrap()
    }
    async fn lifecycle(&self, action: LifecycleAction) {
        self.service()
            .execute(
                &self.actor,
                Command::Lifecycle {
                    route: self.route.clone(),
                    input: LifecycleInput {
                        revision: self.revision().await,
                        action,
                        reason: "业务操作".into(),
                    },
                },
            )
            .await
            .unwrap();
    }
    async fn receipt(&self) -> Uuid {
        let id = Uuid::new_v4();
        sqlx::query("INSERT INTO source_receipts(receipt_id,source_id,event_id,route_id,body_sha256,schema_version) VALUES($1,$2,$3,$4,'hash',2)").bind(id).bind(self.source).bind(Uuid::new_v4()).bind(&self.route).execute(&self.pool).await.unwrap();
        id
    }
    async fn close(self) {
        self.pool.close().await;
        sqlx::query(&format!("DROP SCHEMA {} CASCADE", self.schema))
            .execute(&self.base)
            .await
            .unwrap();
        if let Some(guard) = self.test_guard {
            guard.rollback().await.unwrap();
        }
        self.base.close().await;
    }
}
#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn precise_expansion_concurrent_resolution_and_recovery() {
    let f = Fixture::new().await;
    let assigned = f
        .service()
        .execute(&f.actor, Command::Assigned)
        .await
        .unwrap();
    assert_eq!(assigned["items"].as_array().unwrap().len(), 1);
    assert_eq!(assigned["items"][0]["id"], f.route);
    f.configure(vec![f.targets[0], f.targets[1], f.targets[2], f.targets[0]])
        .await;
    let check = f
        .service()
        .execute(
            &f.actor,
            Command::Check {
                route: f.route.clone(),
            },
        )
        .await
        .unwrap();
    assert_eq!(
        check["ready"], true,
        "offline phones must not block enable: {check}"
    );
    f.lifecycle(LifecycleAction::Enable).await;
    let receipt = f.receipt().await;
    let one = f.service();
    let two = f.service();
    let (a, b) = tokio::join!(one.resolve(receipt), two.resolve(receipt));
    let a = match a.unwrap() {
        Resolution::Ready { snapshot } => snapshot,
        _ => panic!("expected decision"),
    };
    assert_eq!(a.targets.len(), 3);
    assert!(a.targets.iter().all(|t| t.id != f.targets[3]));
    assert!(matches!(b.unwrap(), Resolution::Ready { .. }));
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM source_current_routing_decisions WHERE receipt_id=$1",
    )
    .bind(receipt)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(count, 1);
    f.lifecycle(LifecycleAction::Pause).await;
    let paused = f.receipt().await;
    assert!(
        matches!(f.service().resolve(paused).await.unwrap(),Resolution::Waiting{reason_code} if reason_code=="route_paused")
    );
    f.lifecycle(LifecycleAction::Resume).await;
    assert!(matches!(
        f.service().resolve(paused).await.unwrap(),
        Resolution::Ready { .. }
    ));
    sqlx::query("UPDATE mobile_targets SET enabled=false WHERE school_id=$1")
        .bind(f.workspace)
        .execute(&f.pool)
        .await
        .unwrap();
    let empty = f.receipt().await;
    assert!(
        matches!(f.service().resolve(empty).await.unwrap(),Resolution::Waiting{reason_code} if reason_code=="route_no_targets")
    );
    sqlx::query("UPDATE mobile_targets SET enabled=true WHERE school_id=$1")
        .bind(f.workspace)
        .execute(&f.pool)
        .await
        .unwrap();
    assert!(matches!(
        f.service().resolve(empty).await.unwrap(),
        Resolution::Ready { .. }
    ));
    // Until an executable task commits, repair must replace the provisional check.
    f.configure(vec![f.targets[3]]).await;
    match f.service().resolve(receipt).await.unwrap() {
        Resolution::Ready { snapshot } => {
            assert_eq!(snapshot.targets.len(), 1);
            assert_eq!(snapshot.targets[0].id, f.targets[3]);
        }
        _ => panic!("lost frozen decision"),
    }
    let immutable =
        sqlx::query("UPDATE source_route_revisions SET snapshot='{}' WHERE route_id=$1")
            .bind(&f.route)
            .execute(&f.pool)
            .await;
    assert!(immutable.is_err());
    f.close().await;
}
#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn scope_cas_retirement_and_credential_deletion() {
    let f = Fixture::new().await;
    let svc = f.service();
    f.configure(vec![f.targets[0]]).await;
    let input = ConfigurationInput {
        revision: 1,
        unit_id: f.unit,
        group_id: f.group,
        execution: None,
    };
    assert_eq!(
        svc.execute(
            &f.actor,
            Command::Configure {
                route: f.route.clone(),
                input
            }
        )
        .await
        .unwrap_err()
        .0,
        "revision_conflict"
    );
    let duplicate = svc
        .execute(
            &f.actor,
            Command::Entity {
                kind: "routing-units".into(),
                id: None,
                input: EntityInput {
                    name: "同名单位".into(),
                    description: "".into(),
                    active: true,
                    revision: None,
                    unit_id: None,
                },
            },
        )
        .await
        .unwrap();
    assert_ne!(duplicate["id"], json!(f.unit));
    let moved = svc
        .execute(
            &f.actor,
            Command::Entity {
                kind: "routing-groups".into(),
                id: Some(f.group),
                input: EntityInput {
                    name: "受众".into(),
                    description: "".into(),
                    active: true,
                    revision: Some(1),
                    unit_id: Some(serde_json::from_value(duplicate["id"].clone()).unwrap()),
                },
            },
        )
        .await
        .unwrap_err();
    assert_eq!(moved.0, "group_in_use");
    let receipt = f.receipt().await;
    f.lifecycle(LifecycleAction::Retire).await;
    assert!(
        matches!(svc.resolve(receipt).await.unwrap(),Resolution::Waiting{reason_code} if reason_code=="route_retired")
    );
    let resume = svc
        .execute(
            &f.actor,
            Command::Lifecycle {
                route: f.route.clone(),
                input: LifecycleInput {
                    revision: f.revision().await,
                    action: LifecycleAction::Resume,
                    reason: "恢复".into(),
                },
            },
        )
        .await
        .unwrap_err();
    assert_eq!(resume.0, "route_retired");
    wekeeper_api::source_access::south::adapter::postgres::Repository(f.pool.clone())
        .manage(
            &f.actor,
            "source-credentials",
            Some(&f.key),
            "DELETE",
            Some(serde_json::from_value(json!({})).unwrap()),
            1,
            "",
            "",
            None,
            (None, None),
        )
        .await
        .unwrap();
    let tombstone: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM source_route_tombstones WHERE route_id=$1)",
    )
    .bind(&f.route)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert!(tombstone);
    let reused=sqlx::query("INSERT INTO source_routes(route_id,source_id,route_name,status) VALUES($1,$2,'Reused','active')").bind(&f.route).bind(f.source).execute(&f.pool).await;
    assert!(reused.is_err());
    let history: i64 =
        sqlx::query_scalar("SELECT count(*) FROM source_route_revisions WHERE route_id=$1")
            .bind(&f.route)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(history, 1);
    f.close().await;
}
#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn direct_requests_require_route_and_current_workspace_permission() {
    let f = Fixture::new().await;
    f.configure(vec![f.targets[0]]).await;
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO accounts(id,username,display_name) VALUES($1,'operator','Operator')")
        .bind(id)
        .execute(&f.pool)
        .await
        .unwrap();
    let mut actor = f.actor.clone();
    actor.id = id;
    actor.platform_admin = false;
    let cmd = || Command::Configuration {
        route: f.route.clone(),
    };
    assert_eq!(
        f.service().execute(&actor, cmd()).await.unwrap_err().0,
        "forbidden"
    );
    f.service()
        .execute(
            &f.actor,
            Command::Manager {
                route: f.route.clone(),
                input: ManagerInput {
                    revision: f.revision().await,
                    username: "operator".into(),
                    granted: true,
                    can_recheck: false,
                },
            },
        )
        .await
        .unwrap();
    assert_eq!(
        f.service().execute(&actor, cmd()).await.unwrap_err().0,
        "forbidden"
    );
    sqlx::query(
        "INSERT INTO mobile_workspace_members(workspace_id,account_id,role) VALUES($1,$2,'admin')",
    )
    .bind(f.workspace)
    .bind(id)
    .execute(&f.pool)
    .await
    .unwrap();
    f.service().execute(&actor, cmd()).await.unwrap();
    sqlx::query("DELETE FROM mobile_workspace_members WHERE account_id=$1")
        .bind(id)
        .execute(&f.pool)
        .await
        .unwrap();
    assert_eq!(
        f.service().execute(&actor, cmd()).await.unwrap_err().0,
        "forbidden"
    );
    // No cross-workspace target injection even with valid route access.
    let other = Uuid::new_v4();
    sqlx::query("INSERT INTO mobile_workspaces(id,name) VALUES($1,'Other')")
        .bind(other)
        .execute(&f.pool)
        .await
        .unwrap();
    let injected = f
        .service()
        .execute(
            &f.actor,
            Command::Configure {
                route: f.route.clone(),
                input: ConfigurationInput {
                    revision: f.revision().await,
                    unit_id: f.unit,
                    group_id: f.group,
                    execution: Some(ExecutionConfig {
                        channel: Channel::Mobile,
                        workspace_id: other,
                        target_ids: vec![f.targets[0]],
                    }),
                },
            },
        )
        .await
        .unwrap_err();
    assert_eq!(injected.0, "target_unavailable");
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn accepted_inbox_creates_internal_mobile_tasks_and_restores_without_resubmission() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("wekeeper_api=debug")
        .try_init();
    let f = Fixture::new().await;
    f.configure(vec![f.targets[0], f.targets[1], f.targets[2]])
        .await;
    f.lifecycle(LifecycleAction::Enable).await;
    let receipt = f.receipt().await;
    let body = json!({"data":{"type":"source-events","id":Uuid::new_v4(),"attributes":{"event_id":Uuid::new_v4(),"schema_version":2,"route_id":f.route,"event_type":"content.upsert","external_content_id":"notice-1","version":"1","sequence":1,"occurred_at":"2026-11-09T00:00:00Z","title":"通知","content":"正文","detail_url":"https://example.com/notice/1"}}});
    sqlx::query("INSERT INTO source_inbox(receipt_id,request_body) VALUES($1,$2)")
        .bind(receipt)
        .bind(body.to_string().as_bytes())
        .execute(&f.pool)
        .await
        .unwrap();
    let a = RoutingRepository(f.pool.clone());
    let b = RoutingRepository(f.pool.clone());
    let (one, two) = tokio::join!(a.run_once(), b.run_once());
    one.unwrap();
    two.unwrap();
    let count:i64=sqlx::query_scalar("SELECT count(*) FROM source_execution_tasks l JOIN mobile_v9_task_items i ON i.task_id=l.task_id WHERE l.receipt_id=$1").bind(receipt).fetch_one(&f.pool).await.unwrap();
    let reason: Value =
        sqlx::query_scalar("SELECT to_jsonb(r) FROM source_receipts r WHERE receipt_id=$1")
            .bind(receipt)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(
        count, 3,
        "must create channel tasks after acceptance: {reason}"
    );
    let control:i64=sqlx::query_scalar("SELECT count(*) FROM source_execution_tasks l JOIN mobile_v9_task_items i ON i.task_id=l.task_id WHERE l.receipt_id=$1 AND i.target_id=$2").bind(receipt).bind(f.targets[3]).fetch_one(&f.pool).await.unwrap();
    assert_eq!(control, 0);
    let origin:String=sqlx::query_scalar("SELECT t.authorization_kind FROM mobile_v9_tasks t JOIN source_execution_tasks l ON l.task_id=t.id WHERE l.receipt_id=$1").bind(receipt).fetch_one(&f.pool).await.unwrap();
    assert_eq!(origin, "source_route");
    let sending: Value =
        sqlx::query_scalar("SELECT sending FROM source_receipts WHERE receipt_id=$1")
            .bind(receipt)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(sending["pending"], 3);
    assert_eq!(sending["sent"], 0);
    // Injecting a real persisted success updates the source receipt; receipt itself is never sent.
    sqlx::query("UPDATE mobile_v9_attempts SET state='observed_sent' WHERE item_id=(SELECT i.id FROM mobile_v9_task_items i JOIN source_execution_tasks l ON l.task_id=i.task_id WHERE l.receipt_id=$1 ORDER BY i.position LIMIT 1)").bind(receipt).execute(&f.pool).await.unwrap();
    a.run_once().await.unwrap();
    let sending: Value =
        sqlx::query_scalar("SELECT sending FROM source_receipts WHERE receipt_id=$1")
            .bind(receipt)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(sending["sent"], 1);
    let linked: i64 =
        sqlx::query_scalar("SELECT count(*) FROM source_execution_tasks WHERE receipt_id=$1")
            .bind(receipt)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(linked, 1);
    f.close().await;
}

async fn inbox(f: &Fixture, content: &str, sequence: i64, event: &str) -> Uuid {
    let receipt = f.receipt().await;
    let body = json!({"data":{"type":"source-events","id":Uuid::new_v4(),"attributes":{"event_id":Uuid::new_v4(),"schema_version":2,"route_id":f.route,"event_type":event,"external_content_id":content,"version":sequence.to_string(),"sequence":sequence,"occurred_at":"2026-11-09T00:00:00Z","title":"通知","content":"正文","detail_url":"https://example.com/notice/1"}}});
    sqlx::query("INSERT INTO source_inbox(receipt_id,request_body) VALUES($1,$2)")
        .bind(receipt)
        .bind(body.to_string().as_bytes())
        .execute(&f.pool)
        .await
        .unwrap();
    receipt
}
#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn paused_empty_route_recovers_and_deleted_credentials_preserve_existing_task() {
    let f = Fixture::new().await;
    f.configure(vec![f.targets[0]]).await;
    f.lifecycle(LifecycleAction::Enable).await;
    f.lifecycle(LifecycleAction::Pause).await;
    f.configure(vec![]).await;
    let receipt = inbox(&f, "recover", 1, "content.upsert").await;
    let worker = RoutingRepository(f.pool.clone());
    worker.run_once().await.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM source_execution_tasks")
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    f.configure(vec![f.targets[0]]).await;
    f.lifecycle(LifecycleAction::Resume).await;
    // A new repository instance represents restart; no source re-submission is needed.
    RoutingRepository(f.pool.clone()).run_once().await.unwrap();
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM source_execution_tasks WHERE receipt_id=$1")
            .bind(receipt)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(count, 1);
    f.lifecycle(LifecycleAction::Pause).await;
    let mut tx = f.pool.begin().await.unwrap();
    let denied = wekeeper_api::source_access::south::adapter::postgres::routing::guard_submission(
        &mut tx,
        receipt,
        f.targets[0],
    )
    .await
    .unwrap_err();
    assert_eq!(denied.0, "route_paused");
    tx.rollback().await.unwrap();
    f.lifecycle(LifecycleAction::Resume).await;
    // Temporary target disable remains recoverable, unlike explicit route membership removal.
    sqlx::query("UPDATE mobile_targets SET enabled=false WHERE id=$1")
        .bind(f.targets[0])
        .execute(&f.pool)
        .await
        .unwrap();
    let mut tx = f.pool.begin().await.unwrap();
    let denied = wekeeper_api::source_access::south::adapter::postgres::routing::guard_submission(
        &mut tx,
        receipt,
        f.targets[0],
    )
    .await
    .unwrap_err();
    assert_eq!(denied.0, "target_unavailable");
    tx.rollback().await.unwrap();
    sqlx::query("UPDATE mobile_targets SET enabled=true WHERE id=$1")
        .bind(f.targets[0])
        .execute(&f.pool)
        .await
        .unwrap();
    // The normal credential removal deletes the route; historical authorization survives.
    sqlx::query("DELETE FROM source_key_routes WHERE route_id=$1")
        .bind(&f.route)
        .execute(&f.pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM source_routes WHERE route_id=$1")
        .bind(&f.route)
        .execute(&f.pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM source_credentials WHERE key_id=$1")
        .bind(&f.key)
        .execute(&f.pool)
        .await
        .unwrap();
    let mut tx = f.pool.begin().await.unwrap();
    wekeeper_api::source_access::south::adapter::postgres::routing::guard_submission(
        &mut tx,
        receipt,
        f.targets[0],
    )
    .await
    .unwrap();
    tx.rollback().await.unwrap();
    f.close().await;
}
#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn accepted_inbox_creates_only_configured_official_targets_and_withdraws_unsent_work() {
    let f = Fixture::new().await;
    let (workspace, _) = configure_official(&f).await;
    f.lifecycle(LifecycleAction::Enable).await;
    select_ad(&f, "wecom", workspace).await;
    let receipt = inbox(&f, "official", 1, "content.upsert").await;
    let worker = RoutingRepository(f.pool.clone());
    worker.run_once().await.unwrap();
    let detail:Value=sqlx::query_scalar("SELECT jsonb_build_object('kind',t.authorization_kind,'human_confirmation',t.confirmed_by,'count',count(x.id)) FROM source_execution_tasks l JOIN broadcast_tasks t ON t.id=l.task_id JOIN broadcast_targets x ON x.task_id=t.id WHERE l.receipt_id=$1 GROUP BY t.id").bind(receipt).fetch_one(&f.pool).await.unwrap();
    assert_eq!(
        composition(&f, receipt).await["composer"],
        "source-ad-text/1"
    );
    let shared:bool=sqlx::query_scalar("SELECT s.snapshot_id=c.message_snapshot_id FROM source_execution_tasks l JOIN source_message_compositions c ON c.id=l.composition_id JOIN broadcast_task_snapshots s ON s.task_id=l.task_id WHERE l.receipt_id=$1").bind(receipt).fetch_one(&f.pool).await.unwrap();
    assert!(shared);
    assert_eq!(detail["count"], 3);
    assert_eq!(detail["kind"], "source_route");
    assert!(detail["human_confirmation"].is_null());
    let mobile: i64 = sqlx::query_scalar("SELECT count(*) FROM mobile_v9_tasks")
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(mobile, 0);
    inbox(&f, "official", 2, "content.withdrawn").await;
    worker.run_once().await.unwrap();
    let cancelled: i64 =
        sqlx::query_scalar("SELECT count(*) FROM broadcast_batches WHERE state='cancelled'")
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(cancelled, 3);
    f.close().await;
}

async fn receipt_value(f: &Fixture, receipt: Uuid) -> Value {
    sqlx::query_scalar("SELECT to_jsonb(r) FROM source_receipts r WHERE receipt_id=$1")
        .bind(receipt)
        .fetch_one(&f.pool)
        .await
        .unwrap()
}
async fn change_title(f: &Fixture, receipt: Uuid, title: &str) {
    sqlx::query("UPDATE source_inbox SET request_body=convert_to(jsonb_set(convert_from(request_body,'UTF8')::jsonb,'{data,attributes,title}',to_jsonb($2::text))::text,'UTF8') WHERE receipt_id=$1")
        .bind(receipt).bind(title).execute(&f.pool).await.unwrap();
}
#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn projection_equivalence_conflict_stale_tombstone_and_default_no_resend() {
    let f = Fixture::new().await;
    f.configure(vec![f.targets[0], f.targets[1]]).await;
    f.lifecycle(LifecycleAction::Enable).await;
    let worker = RoutingRepository(f.pool.clone());
    let original = inbox(&f, "versions", 2, "content.upsert").await;
    worker.run_once().await.unwrap();
    let duplicate = inbox(&f, "versions", 2, "content.upsert").await;
    let conflict = inbox(&f, "versions", 2, "content.upsert").await;
    change_title(&f, conflict, "冲突正文").await;
    let stale = inbox(&f, "versions", 1, "content.upsert").await;
    let concurrent = RoutingRepository(f.pool.clone());
    let (a, b) = tokio::join!(worker.run_once(), concurrent.run_once());
    a.unwrap();
    b.unwrap();
    let d = receipt_value(&f, duplicate).await;
    assert_eq!(d["canonical_receipt_id"], json!(original));
    assert_eq!(d["processing_stage"], "projection_duplicate");
    assert_eq!(
        receipt_value(&f, conflict).await["reason_code"],
        "content_version_conflict"
    );
    assert_eq!(receipt_value(&f, stale).await["observed_sequence"], 2);
    assert_eq!(receipt_value(&f, stale).await["status"], "ignored_stale");
    let tasks: i64 = sqlx::query_scalar("SELECT count(*) FROM source_execution_tasks")
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(tasks, 1);
    // Unknown facts survive replacement; an update cannot create a second task.
    sqlx::query("UPDATE mobile_v9_attempts SET state='unknown' WHERE item_id IN (SELECT i.id FROM mobile_v9_task_items i JOIN source_execution_tasks l ON l.task_id=i.task_id WHERE l.receipt_id=$1)").bind(original).execute(&f.pool).await.unwrap();
    worker.run_once().await.unwrap();
    assert_eq!(receipt_value(&f, original).await["sending"]["unknown"], 2);
    let updated = inbox(&f, "versions", 7, "content.upsert").await;
    worker.run_once().await.unwrap();
    assert_eq!(
        receipt_value(&f, updated).await["processing_stage"],
        "no_resend"
    );
    assert!(receipt_value(&f, updated).await["sending"].is_null());
    let withdrawn = inbox(&f, "versions", 8, "content.withdrawn").await;
    worker.run_once().await.unwrap();
    assert_eq!(
        receipt_value(&f, withdrawn).await["processing_stage"],
        "withdrawn"
    );
    // Late success is counted once per target, even after withdrawal.
    sqlx::query("UPDATE mobile_v9_attempts SET state='observed_sent' WHERE item_id IN (SELECT i.id FROM mobile_v9_task_items i JOIN source_execution_tasks l ON l.task_id=i.task_id WHERE l.receipt_id=$1)").bind(original).execute(&f.pool).await.unwrap();
    worker.run_once().await.unwrap();
    let sent = receipt_value(&f, original).await;
    assert_eq!(
        sent["sending"],
        json!({"sent":2,"unknown":0,"failed":0,"pending":0})
    );
    assert_eq!(sent["summary_total"], 2);
    use wekeeper_api::source_access::{
        domain::projection::Command as P, north::local::projection::Service as PS,
        south::adapter::postgres::projection::ReceiptsRepository,
    };
    let receipts = PS(ReceiptsRepository(f.pool.clone()));
    let detail = receipts
        .execute(
            &f.actor,
            P::Detail {
                route: f.route.clone(),
                receipt: withdrawn,
            },
        )
        .await
        .unwrap();
    assert_eq!(detail["sending"], sent["sending"]);
    assert_eq!(detail["summary_revision"], sent["summary_revision"]);
    let duplicate_withdrawal = inbox(&f, "versions", 8, "content.withdrawn").await;
    worker.run_once().await.unwrap();
    let detail = receipts
        .execute(
            &f.actor,
            P::Detail {
                route: f.route.clone(),
                receipt: duplicate_withdrawal,
            },
        )
        .await
        .unwrap();
    assert_eq!(detail["sending"], sent["sending"]);
    assert_eq!(detail["summary_revision"], sent["summary_revision"]);

    let revision = sent["summary_revision"].clone();
    worker.run_once().await.unwrap();
    assert_eq!(
        receipt_value(&f, original).await["summary_revision"],
        revision
    );
    let tombstone = inbox(&f, "first-withdrawal", 6, "content.withdrawn").await;
    worker.run_once().await.unwrap();
    let late = inbox(&f, "first-withdrawal", 5, "content.upsert").await;
    worker.run_once().await.unwrap();
    assert_eq!(receipt_value(&f, late).await["status"], "ignored_stale");
    assert_eq!(receipt_value(&f, tombstone).await["status"], "applied");
    let tasks: i64 = sqlx::query_scalar("SELECT count(*) FROM source_execution_tasks")
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(tasks, 1);
    f.close().await;
}
#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn rejected_message_does_not_recover_when_rules_are_relaxed() {
    use wekeeper_api::source_access::{
        domain::projection::{Command as P, RecheckInput},
        north::local::projection::Service as PS,
        south::adapter::postgres::projection::ReceiptsRepository,
    };
    let f = Fixture::new().await;
    f.configure(vec![f.targets[0]]).await;
    f.lifecycle(LifecycleAction::Enable).await;
    publish_keywords(&f, "global", &["正文"]).await;
    let worker = RoutingRepository(f.pool.clone());
    let receipt = inbox(&f, "recheck", 1, "content.upsert").await;
    worker.run_once().await.unwrap();
    let service = PS(ReceiptsRepository(f.pool.clone()));
    let detail = service
        .execute(
            &f.actor,
            P::Detail {
                route: f.route.clone(),
                receipt,
            },
        )
        .await
        .unwrap();
    assert_eq!(detail["reason_code"], "keyword_blocked");
    assert_eq!(detail["can_recheck"], false);
    publish_keywords(&f, "global", &[]).await;
    worker.run_once().await.unwrap();
    assert_eq!(
        receipt_value(&f, receipt).await["reason_code"],
        "keyword_blocked"
    );
    assert_eq!(
        service
            .execute(
                &f.actor,
                P::Recheck {
                    route: f.route.clone(),
                    receipt,
                    input: RecheckInput {
                        revision: detail["revision"].as_i64().unwrap(),
                        reason: "重检".into()
                    }
                }
            )
            .await
            .unwrap_err()
            .0,
        "recheck_not_allowed"
    );
    let tasks: i64 =
        sqlx::query_scalar("SELECT count(*) FROM source_execution_tasks WHERE receipt_id=$1")
            .bind(receipt)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(tasks, 0);
    let newer = inbox(&f, "recheck", 2, "content.upsert").await;
    worker.run_once().await.unwrap();
    assert_eq!(
        receipt_value(&f, newer).await["processing_stage"],
        "task_created"
    );
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn sensitive_words_http_list_save_and_conflict() {
    use axum::{
        body::{Body, to_bytes},
        http::Request,
    };
    use tower::ServiceExt;
    let f = Fixture::new().await;
    let app = wekeeper_api::bootstrap::build_application(f.pool.clone());
    let (token, actor) = app
        .identity
        .0
        .session(f.actor.id, "browser", false)
        .await
        .unwrap();
    let router = wekeeper_api::http::app_router(app);
    let get = || {
        Request::builder()
            .uri("/api/v2/admin/sensitive-words")
            .header("cookie", format!("wekeeper_session={token}"))
            .body(Body::empty())
            .unwrap()
    };
    let response = router.clone().oneshot(get()).await.unwrap();
    assert_eq!(response.status(), 200);
    let doc: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 100000).await.unwrap()).unwrap();
    let revision = doc["data"]["attributes"]["revision"].as_i64().unwrap();
    let input = json!({"data":{"type":"sensitive-words","attributes":{"expected_revision":revision,"entries":[{"word":"诈骗","enabled":true}]}}});
    let put = || {
        Request::builder()
            .method("PUT")
            .uri("/api/v2/admin/sensitive-words")
            .header("cookie", format!("wekeeper_session={token}"))
            .header("origin", "http://localhost:5173")
            .header("x-csrf-token", &actor.csrf)
            .header("content-type", "application/vnd.api+json")
            .body(Body::from(input.to_string()))
            .unwrap()
    };
    assert_eq!(router.clone().oneshot(put()).await.unwrap().status(), 200);
    assert_eq!(router.clone().oneshot(put()).await.unwrap().status(), 409);
    let doc: Value = serde_json::from_slice(
        &to_bytes(router.oneshot(get()).await.unwrap().into_body(), 100000)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(doc["data"]["attributes"]["entries"][0]["word"], "诈骗");
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn whitespace_obfuscated_sensitive_word_is_rejected() {
    let f = Fixture::new().await;
    f.configure(vec![f.targets[0]]).await;
    f.lifecycle(LifecycleAction::Enable).await;
    publish_keywords(&f, "global", &["诈骗"]).await;
    let receipt = inbox(&f, "whitespace-keyword", 1, "content.upsert").await;
    change_title(&f, receipt, "诈 \n骗").await;
    RoutingRepository(f.pool.clone()).run_once().await.unwrap();
    assert_eq!(
        receipt_value(&f, receipt).await["reason_code"],
        "keyword_blocked"
    );
    let sample: serde_json::Value = sqlx::query_scalar(
        "SELECT samples FROM keyword_check_results WHERE receipt_id=$1 AND outcome='blocked'",
    )
    .bind(receipt)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(sample[0]["start"], 0);
    assert_eq!(sample[0]["end"], 4);
    let tasks: i64 =
        sqlx::query_scalar("SELECT count(*) FROM source_execution_tasks WHERE receipt_id=$1")
            .bind(receipt)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(tasks, 0);
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn policy_publication_does_not_recheck_previously_accepted_messages() {
    let f = Fixture::new().await;
    let worker = RoutingRepository(f.pool.clone());
    for n in 0..80 {
        inbox(&f, &format!("page-{n}"), 1, "content.upsert").await;
    }
    worker.run_once().await.unwrap();
    worker.run_once().await.unwrap();
    let checked: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM source_content_versions WHERE check_state='passed'",
    )
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(checked, 80);
    publish_keywords(&f, "global", &["正文"]).await;
    worker.run_once().await.unwrap();
    let first: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM source_content_versions WHERE check_state='blocked'",
    )
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(first, 0);
    RoutingRepository(f.pool.clone()).run_once().await.unwrap();
    let all: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM source_content_versions WHERE check_state='blocked'",
    )
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(all, 0);
    let complete: bool =
        sqlx::query_scalar("SELECT COALESCE(bool_and(complete),true) FROM keyword_recheck_batches")
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert!(complete);
    f.close().await;
}

async fn seed_legacy_workspace_words(f: &Fixture, scope: &str, word: &str) {
    let id = Uuid::new_v4();
    let mut tx = f.pool.begin().await.unwrap();
    sqlx::query("INSERT INTO keyword_rule_versions(id,scope_id,revision,entries,reason) VALUES($1,$2,1,$3,'legacy')").bind(id).bind(scope).bind(json!([{"word":word,"enabled":true}])).execute(&mut *tx).await.unwrap();
    sqlx::query("UPDATE keyword_scope_heads SET version_id=$2,revision=1 WHERE id=$1")
        .bind(scope)
        .bind(id)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
}

async fn publish_keywords(f: &Fixture, scope: &str, words: &[&str]) -> Value {
    use wekeeper_api::source_access::{
        domain::keyword_filter::{Command, Entry, Publication},
        north::local::keyword_filter::Service,
        south::adapter::postgres::keyword_filter::KeywordRepository,
    };
    let revision: i64 = sqlx::query_scalar("SELECT revision FROM keyword_scope_heads WHERE id=$1")
        .bind(scope)
        .fetch_one(&f.pool)
        .await
        .unwrap();
    Service(KeywordRepository(f.pool.clone()))
        .execute(
            &f.actor,
            Command::Publish {
                scope: scope.into(),
                input: Publication {
                    expected_revision: revision,
                    reason: "integration policy change".into(),
                    entries: words
                        .iter()
                        .map(|s| Entry {
                            word: (*s).into(),
                            enabled: true,
                        })
                        .collect(),
                },
            },
        )
        .await
        .unwrap()
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn only_original_content_is_checked_and_evidence_is_immutable() {
    let f = Fixture::new().await;
    f.configure(vec![f.targets[0]]).await;
    f.lifecycle(LifecycleAction::Enable).await;
    let worker = RoutingRepository(f.pool.clone());
    publish_keywords(&f, "global", &["EXAMPLE.COM"]).await;
    let receipt = inbox(&f, "final-url", 1, "content.upsert").await;
    worker.run_once().await.unwrap();
    assert_eq!(
        receipt_value(&f, receipt).await["processing_stage"],
        "task_created"
    );
    let checks: i64 =
        sqlx::query_scalar("SELECT count(*) FROM keyword_check_results WHERE receipt_id=$1")
            .bind(receipt)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(checks, 1);
    let id: Uuid = sqlx::query_scalar("SELECT id FROM keyword_check_results WHERE receipt_id=$1")
        .bind(receipt)
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert!(
        sqlx::query("UPDATE keyword_check_results SET outcome='blocked' WHERE id=$1")
            .bind(id)
            .execute(&f.pool)
            .await
            .is_err()
    );
    publish_keywords(&f, "global", &["ＳＴＲＡＳＳＥ"]).await;
    let raw = inbox(&f, "raw-fullfold", 1, "content.upsert").await;
    change_title(&f, raw, "Straße").await;
    worker.run_once().await.unwrap();
    assert_eq!(
        receipt_value(&f, raw).await["reason_code"],
        "keyword_blocked"
    );
    let tasks: i64 =
        sqlx::query_scalar("SELECT count(*) FROM source_execution_tasks WHERE receipt_id=$1")
            .bind(raw)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(tasks, 0);
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn keyword_workspace_authorization_cas_and_receipt_check_ownership() {
    use wekeeper_api::source_access::{
        domain::keyword_filter::{Command, Publication},
        north::local::keyword_filter::Service,
        south::adapter::postgres::keyword_filter::KeywordRepository,
    };
    let f = Fixture::new().await;
    f.configure(vec![f.targets[0]]).await;
    f.lifecycle(LifecycleAction::Enable).await;
    let r = inbox(&f, "scope-auth", 1, "content.upsert").await;
    RoutingRepository(f.pool.clone()).run_once().await.unwrap();
    let service = Service(KeywordRepository(f.pool.clone()));
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO accounts(id,username,display_name) VALUES($1,'keyword-admin','Rule Admin')",
    )
    .bind(id)
    .execute(&f.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO mobile_workspace_members(workspace_id,account_id,role) VALUES($1,$2,'admin')",
    )
    .bind(f.workspace)
    .bind(id)
    .execute(&f.pool)
    .await
    .unwrap();
    let mut actor = f.actor.clone();
    actor.id = id;
    actor.platform_admin = false;
    let scope = format!("mobile:{}", f.workspace);
    let scopes = service.execute(&actor, Command::Scopes).await.unwrap();
    assert_eq!(scopes["items"].as_array().unwrap().len(), 0);
    assert_eq!(
        service
            .execute(
                &actor,
                Command::Versions {
                    scope: "global".into(),
                    page: 1
                }
            )
            .await
            .unwrap_err()
            .0,
        "forbidden"
    );
    let publication = || Command::Publish {
        scope: scope.clone(),
        input: Publication {
            expected_revision: 0,
            reason: "local change".into(),
            entries: vec![],
        },
    };
    assert_eq!(
        service.execute(&actor, publication()).await.unwrap_err().0,
        "forbidden"
    );
    assert_eq!(
        service
            .execute(&f.actor, publication())
            .await
            .unwrap_err()
            .0,
        "forbidden"
    );
    assert_eq!(
        service
            .execute(
                &actor,
                Command::Checks {
                    route: f.route.clone(),
                    receipt: r,
                    page: 1
                }
            )
            .await
            .unwrap_err()
            .0,
        "forbidden"
    );
    sqlx::query(
        "INSERT INTO source_route_managers(route_id,account_id,granted_by) VALUES($1,$2,$3)",
    )
    .bind(&f.route)
    .bind(id)
    .bind(f.actor.id)
    .execute(&f.pool)
    .await
    .unwrap();
    let checks = service
        .execute(
            &actor,
            Command::Checks {
                route: f.route.clone(),
                receipt: r,
                page: 1,
            },
        )
        .await
        .unwrap();
    let check: Uuid = checks["items"][0]["id"].as_str().unwrap().parse().unwrap();
    service
        .execute(
            &actor,
            Command::Hits {
                route: f.route.clone(),
                receipt: r,
                check,
                page: 1,
            },
        )
        .await
        .unwrap();
    let other = inbox(&f, "other-content", 1, "content.upsert").await;
    assert_eq!(
        service
            .execute(
                &actor,
                Command::Hits {
                    route: f.route.clone(),
                    receipt: other,
                    check,
                    page: 1
                }
            )
            .await
            .unwrap_err()
            .0,
        "receipt_not_found"
    );
    sqlx::query("DELETE FROM mobile_workspace_members WHERE account_id=$1")
        .bind(id)
        .execute(&f.pool)
        .await
        .unwrap();
    assert_eq!(
        service.execute(&actor, publication()).await.unwrap_err().0,
        "forbidden"
    );
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn keyword_store_failure_never_prevents_withdrawal_tombstone() {
    let f = Fixture::new().await;
    f.configure(vec![f.targets[0]]).await;
    f.lifecycle(LifecycleAction::Enable).await;
    let worker = RoutingRepository(f.pool.clone());
    let old = inbox(&f, "withdraw-independent", 1, "content.upsert").await;
    worker.run_once().await.unwrap();
    sqlx::query("ALTER TABLE keyword_scope_heads RENAME COLUMN version_id TO unavailable_version")
        .execute(&f.pool)
        .await
        .unwrap();
    let pending = inbox(&f, "store-failure", 1, "content.upsert").await;
    let withdrawn = inbox(&f, "withdraw-independent", 2, "content.withdrawn").await;
    worker.run_once().await.unwrap();
    assert_eq!(
        receipt_value(&f, withdrawn).await["processing_stage"],
        "withdrawn"
    );
    assert_eq!(receipt_value(&f, pending).await["status"], "received");
    assert!(sqlx::query_scalar::<_,bool>("SELECT t.stopped FROM source_execution_tasks l JOIN mobile_v9_tasks t ON t.id=l.task_id WHERE l.receipt_id=$1").bind(old).fetch_one(&f.pool).await.unwrap());
    assert!(
        sqlx::query_scalar::<_, bool>(
            "SELECT withdrawn FROM source_content_versions WHERE receipt_id=$1"
        )
        .bind(withdrawn)
        .fetch_one(&f.pool)
        .await
        .unwrap()
    );
    sqlx::query("ALTER TABLE keyword_scope_heads RENAME COLUMN unavailable_version TO version_id")
        .execute(&f.pool)
        .await
        .unwrap();
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn only_global_rules_filter_source_messages() {
    let f = Fixture::new().await;
    f.configure(vec![f.targets[0], f.targets[1]]).await;
    f.lifecycle(LifecycleAction::Enable).await;
    let worker = RoutingRepository(f.pool.clone());
    // A same-ID workspace on the other channel is an independent namespace.
    sqlx::query("INSERT INTO wecom_workspaces(id,name) VALUES($1,'Other channel')")
        .bind(f.workspace)
        .execute(&f.pool)
        .await
        .unwrap();
    seed_legacy_workspace_words(&f, &format!("wecom:{}", f.workspace), "正文").await;
    let free = inbox(&f, "other-channel", 1, "content.upsert").await;
    worker.run_once().await.unwrap();
    assert_eq!(
        receipt_value(&f, free).await["processing_stage"],
        "task_created"
    );
    // Historic caller, unit and route words remain readable but no longer filter.
    for scope in [
        format!("source:{}", f.source),
        format!("unit:{}", f.unit),
        format!("route:{}", f.route),
    ] {
        seed_legacy_workspace_words(&f, &scope, "正文").await;
    }
    let legacy = inbox(&f, "legacy-scopes", 1, "content.upsert").await;
    worker.run_once().await.unwrap();
    assert_eq!(
        receipt_value(&f, legacy).await["processing_stage"],
        "task_created"
    );
    publish_keywords(&f, "global", &["正文"]).await;
    let blocked = inbox(&f, "global-blocked", 1, "content.upsert").await;
    worker.run_once().await.unwrap();
    assert_eq!(
        receipt_value(&f, blocked).await["reason_code"],
        "keyword_blocked"
    );
    publish_keywords(&f, "global", &[]).await;
    seed_legacy_workspace_words(&f, &format!("mobile:{}", f.workspace), "正文").await;
    let local = inbox(&f, "workspace-original", 1, "content.upsert").await;
    worker.run_once().await.unwrap();
    assert_eq!(receipt_value(&f, local).await["status"], "applied");
    assert_eq!(
        receipt_value(&f, local).await["processing_stage"],
        "task_created"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM keyword_check_results WHERE receipt_id=$1 AND stage='final'"
        )
        .bind(local)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        0
    );
    // Rule changes never rewrite successful or unknown target facts.
    sqlx::query("UPDATE mobile_v9_attempts a SET state=CASE WHEN i.target_id=$2 THEN 'observed_sent' ELSE 'unknown' END FROM mobile_v9_task_items i JOIN source_execution_tasks l ON l.task_id=i.task_id WHERE a.item_id=i.id AND l.receipt_id=$1").bind(free).bind(f.targets[0]).execute(&f.pool).await.unwrap();
    for _ in 0..15 {
        worker.run_once().await.unwrap();
    }
    assert_eq!(
        receipt_value(&f, free).await["sending"],
        json!({"sent":1,"unknown":1,"failed":0,"pending":0})
    );
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn wecom_workspace_legacy_keyword_rules_do_not_filter_source_messages() {
    let f = Fixture::new().await;
    let (workspace, _) = configure_official(&f).await;
    seed_legacy_workspace_words(&f, &format!("wecom:{workspace}"), "正文").await;
    f.lifecycle(LifecycleAction::Enable).await;
    let receipt = inbox(&f, "wecom-independent-keywords", 1, "content.upsert").await;
    RoutingRepository(f.pool.clone()).run_once().await.unwrap();
    assert_eq!(
        receipt_value(&f, receipt).await["processing_stage"],
        "task_created"
    );
    let checks: i64 = sqlx::query_scalar("SELECT count(*) FROM keyword_check_results WHERE receipt_id=$1 AND stage <> 'original_common'").bind(receipt).fetch_one(&f.pool).await.unwrap();
    assert_eq!(checks, 0);
    f.close().await;
}

fn ads(
    f: &Fixture,
) -> wekeeper_api::advertising::north::local::Service<
    wekeeper_api::advertising::south::adapter::postgres::Repository,
> {
    wekeeper_api::advertising::north::local::Service(
        wekeeper_api::advertising::south::adapter::postgres::Repository(f.pool.clone()),
    )
}
async fn select_ad(f: &Fixture, channel: &str, workspace: Uuid) -> Uuid {
    use wekeeper_api::advertising::domain as a;
    let ad = ads(f)
        .execute(
            &f.actor,
            a::Command::Create(a::Create {
                owner_unit_id: f.unit,
                content: a::Content {
                    name: "本单位广告".into(),
                    body: "广告正文".into(),
                    link: Some("https://example.com/ad".into()),
                },
                workspaces: vec![a::Scope {
                    kind: "unit".into(),
                    id: f.unit.to_string(),
                    channel: Some(channel.into()),
                    workspace_id: Some(workspace),
                }],
                reason: "发布".into(),
            }),
        )
        .await
        .unwrap();
    let id = serde_json::from_value(ad["id"].clone()).unwrap();
    ads(f)
        .execute(
            &f.actor,
            a::Command::Configure(
                f.route.clone(),
                a::Configuration {
                    expected_revision: 0,
                    reason: "选用".into(),
                    config: a::Config {
                        mode: "fixed".into(),
                        fixed_ad_id: Some(id),
                        ..Default::default()
                    },
                },
            ),
        )
        .await
        .unwrap();
    id
}
async fn disable_ad(f: &Fixture, id: Uuid) {
    use wekeeper_api::advertising::domain as a;
    let detail = ads(f)
        .execute(&f.actor, a::Command::Detail(id))
        .await
        .unwrap();
    let mut policy: a::Policy = serde_json::from_value(detail["policy"].clone()).unwrap();
    policy.enabled = false;
    ads(f)
        .execute(
            &f.actor,
            a::Command::Policy(
                id,
                a::PolicyPublication {
                    expected_revision: detail["revision"].as_i64().unwrap(),
                    reason: "撤销投放".into(),
                    policy,
                },
            ),
        )
        .await
        .unwrap();
}
async fn composition(f: &Fixture, r: Uuid) -> Value {
    sqlx::query_scalar("SELECT to_jsonb(c) FROM source_execution_tasks l JOIN source_message_compositions c ON c.id=l.composition_id WHERE l.receipt_id=$1").bind(r).fetch_one(&f.pool).await.unwrap()
}
#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn advertisement_revoke_replaces_only_unstarted_generation_atomically() {
    use wekeeper_api::advertising::domain as a;
    let f = Fixture::new().await;
    f.configure(f.targets[..3].to_vec()).await;
    let ad = select_ad(&f, "mobile", f.workspace).await;
    f.lifecycle(LifecycleAction::Enable).await;
    let receipt = inbox(&f, "advertised", 1, "content.upsert").await;
    let worker = RoutingRepository(f.pool.clone());
    worker.run_once().await.unwrap();
    let old = composition(&f, receipt).await;
    let deadline:chrono::DateTime<chrono::Utc>=sqlx::query_scalar("SELECT t.confirmed_until FROM source_execution_tasks l JOIN mobile_v9_tasks t ON t.id=l.task_id WHERE l.receipt_id=$1").bind(receipt).fetch_one(&f.pool).await.unwrap();
    sqlx::query("UPDATE mobile_v9_dispatch_state ds SET completed_rounds=1 FROM mobile_v9_task_items i JOIN source_execution_tasks l ON l.task_id=i.task_id WHERE ds.item_id=i.id AND l.receipt_id=$1").bind(receipt).execute(&f.pool).await.unwrap();

    assert_eq!(old["composer"], "source-ad-text/1");
    assert!(
        old["content"]
            .as_str()
            .unwrap()
            .contains("【广告】\n广告正文")
    );
    let snapshots:i64=sqlx::query_scalar("SELECT count(DISTINCT p.snapshot_id) FROM source_execution_tasks l JOIN mobile_v9_task_items i ON i.task_id=l.task_id JOIN mobile_v9_plans p ON p.item_id=i.id WHERE l.receipt_id=$1").bind(receipt).fetch_one(&f.pool).await.unwrap();
    assert_eq!(snapshots, 1);
    // A content publication must not rewrite a frozen task, even before execution.
    ads(&f)
        .execute(
            &f.actor,
            a::Command::Content(
                ad,
                a::ContentPublication {
                    expected_revision: 2,
                    reason: "新素材".into(),
                    content: a::Content {
                        name: "更新".into(),
                        body: "新版广告".into(),
                        link: None,
                    },
                },
            ),
        )
        .await
        .unwrap();
    sqlx::query(
        "UPDATE source_inbox SET state='waiting',next_attempt_at=now() WHERE receipt_id=$1",
    )
    .bind(receipt)
    .execute(&f.pool)
    .await
    .unwrap();
    worker.run_once().await.unwrap();
    assert_eq!(composition(&f, receipt).await["id"], old["id"]);
    disable_ad(&f, ad).await;
    // Revocation blocks new submissions before any recheck worker runs.
    let mut tx = f.pool.begin().await.unwrap();
    assert_eq!(
        wekeeper_api::source_access::south::adapter::postgres::routing::guard_submission(
            &mut tx,
            receipt,
            f.targets[0]
        )
        .await
        .unwrap_err()
        .0,
        "ad_frozen_unavailable"
    );
    tx.rollback().await.unwrap();
    for _ in 0..5 {
        worker.run_once().await.unwrap();
    }
    let current = composition(&f, receipt).await;
    assert_eq!(current["generation"], 1);
    let inherited:bool=sqlx::query_scalar("SELECT t.confirmed_until=$2 AND bool_and(ds.completed_rounds=1) FROM source_execution_tasks l JOIN mobile_v9_tasks t ON t.id=l.task_id JOIN mobile_v9_task_items i ON i.task_id=t.id JOIN mobile_v9_dispatch_state ds ON ds.item_id=i.id WHERE l.receipt_id=$1 GROUP BY t.id").bind(receipt).bind(deadline).fetch_one(&f.pool).await.unwrap();
    assert!(inherited);

    assert_eq!(current["supersedes"], old["id"]);
    assert_eq!(current["composer"], "source-text/1");
    assert!(!current["content"].as_str().unwrap().contains("【广告】"));
    let history: i64 =
        sqlx::query_scalar("SELECT count(*) FROM source_execution_generations WHERE receipt_id=$1")
            .bind(receipt)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(history, 2);
    let old_stopped:bool=sqlx::query_scalar("SELECT t.stopped FROM source_execution_generations l JOIN mobile_v9_tasks t ON t.id=l.task_id WHERE l.receipt_id=$1 AND l.generation=0").bind(receipt).fetch_one(&f.pool).await.unwrap();
    assert!(old_stopped);
    let unchanged: Value =
        sqlx::query_scalar("SELECT to_jsonb(c) FROM source_message_compositions c WHERE id=$1")
            .bind(old["id"].as_str().unwrap().parse::<Uuid>().unwrap())
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(old, unchanged);
    assert_eq!(receipt_value(&f, receipt).await["summary_total"], 3);
    f.close().await;
}
#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn advertisement_unknown_freezes_all_targets_and_none_ignores_asset_store_failure() {
    let f = Fixture::new().await;
    f.configure(f.targets[..2].to_vec()).await;
    let ad = select_ad(&f, "mobile", f.workspace).await;
    f.lifecycle(LifecycleAction::Enable).await;
    let worker = RoutingRepository(f.pool.clone());
    let receipt = inbox(&f, "unknown-ad", 1, "content.upsert").await;
    worker.run_once().await.unwrap();
    let old = composition(&f, receipt).await;
    sqlx::query("UPDATE mobile_v9_attempts SET state='unknown' WHERE item_id=(SELECT i.id FROM source_execution_tasks l JOIN mobile_v9_task_items i ON i.task_id=l.task_id WHERE l.receipt_id=$1 LIMIT 1)").bind(receipt).execute(&f.pool).await.unwrap();
    disable_ad(&f, ad).await;
    for _ in 0..4 {
        worker.run_once().await.unwrap();
    }
    assert_eq!(composition(&f, receipt).await["id"], old["id"]);
    let evidence = receipt_value(&f, receipt).await;
    assert_eq!(evidence["sending"]["unknown"], 1);
    assert_eq!(evidence["sending"]["failed"], 1);
    use wekeeper_api::advertising::domain as a;
    ads(&f)
        .execute(
            &f.actor,
            a::Command::Configure(
                f.route.clone(),
                a::Configuration {
                    expected_revision: 1,
                    config: a::Config::default(),
                    reason: "新消息无广告".into(),
                },
            ),
        )
        .await
        .unwrap();
    sqlx::query("ALTER TABLE advertisements RENAME COLUMN policy_version TO unavailable_policy")
        .execute(&f.pool)
        .await
        .unwrap();
    let no_ad = inbox(&f, "none-ad", 1, "content.upsert").await;
    worker.run_once().await.unwrap();
    assert_eq!(composition(&f, no_ad).await["composer"], "source-text/1");
    // Broken asset storage cannot prevent withdrawal of an existing advertised message.
    let withdrawn = inbox(&f, "unknown-ad", 2, "content.withdrawn").await;
    worker.run_once().await.unwrap();
    assert_eq!(
        receipt_value(&f, withdrawn).await["processing_stage"],
        "withdrawn"
    );
    f.close().await;
}
#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn advertisement_private_authorization_cas_preview_and_keyword_fallback() {
    use wekeeper_api::advertising::domain as a;
    use wekeeper_api::source_access::{
        domain::keyword_filter as k, south::adapter::postgres::keyword_filter::KeywordRepository,
    };
    let f = Fixture::new().await;
    f.configure(vec![f.targets[0]]).await;
    let ad = select_ad(&f, "mobile", f.workspace).await;
    // Version publication is CAS and never overwrites another operator's update.
    let input = || a::ContentPublication {
        expected_revision: 1,
        reason: "过期修订".into(),
        content: a::Content {
            name: "名称".into(),
            body: "正文".into(),
            link: None,
        },
    };
    assert_eq!(
        ads(&f)
            .execute(&f.actor, a::Command::Content(ad, input()))
            .await
            .unwrap_err()
            .0,
        "revision_conflict"
    );
    let service = wekeeper_api::source_access::north::local::keyword_filter::Service(
        KeywordRepository(f.pool.clone()),
    );
    service
        .execute(
            &f.actor,
            k::Command::Publish {
                scope: "global".into(),
                input: k::Publication {
                    expected_revision: 1,
                    reason: "过滤广告".into(),
                    entries: vec![k::Entry {
                        word: "广告正文".into(),
                        enabled: true,
                    }],
                },
            },
        )
        .await
        .unwrap();
    let preview = ads(&f)
        .execute(
            &f.actor,
            a::Command::Preview(
                f.route.clone(),
                a::Preview {
                    title: Some("通知".into()),
                    content: Some("正文".into()),
                    detail_url: "https://example.com/notice/1".into(),
                },
            ),
        )
        .await
        .unwrap();
    assert_eq!(preview["selection"]["composer"], "source-ad-text/1");
    assert_eq!(preview["deliverable"], true);
    assert!(preview["checks"]["final"].is_null());
    assert!(preview["content"].as_str().unwrap().contains("广告正文"));
    f.lifecycle(LifecycleAction::Enable).await;
    let r = inbox(&f, "keyword-ad", 1, "content.upsert").await;
    RoutingRepository(f.pool.clone()).run_once().await.unwrap();
    assert_eq!(composition(&f, r).await["content"], preview["content"]);
    // A unit author without route/workspace authority cannot read receipt content.
    let author = Uuid::new_v4();
    sqlx::query("INSERT INTO accounts(id,username,display_name) VALUES($1,'ad-author','Author')")
        .bind(author)
        .execute(&f.pool)
        .await
        .unwrap();
    ads(&f)
        .execute(
            &f.actor,
            a::Command::Manager(
                f.unit,
                a::Manager {
                    username: "ad-author".into(),
                    expected_revision: 0,
                    can_edit: true,
                    can_share: false,
                },
            ),
        )
        .await
        .unwrap();
    let actor:Actor=serde_json::from_value(json!({"id":author,"username":"ad-author","display_name":"Author","platform_admin":false,"grants":[],"wecom_authenticated":false})).unwrap();
    assert_eq!(
        ads(&f)
            .execute(&actor, a::Command::Detail(ad))
            .await
            .unwrap()["can_edit"],
        true
    );
    assert_eq!(
        ads(&f)
            .execute(&actor, a::Command::Compositions(f.route.clone(), r, 1))
            .await
            .unwrap_err()
            .0,
        "forbidden"
    );
    let detail = ads(&f)
        .execute(&actor, a::Command::Detail(ad))
        .await
        .unwrap();
    let policy = serde_json::from_value(detail["policy"].clone()).unwrap();
    assert_eq!(
        ads(&f)
            .execute(
                &actor,
                a::Command::Policy(
                    ad,
                    a::PolicyPublication {
                        expected_revision: 2,
                        policy,
                        reason: "越权共享".into()
                    }
                )
            )
            .await
            .unwrap_err()
            .0,
        "forbidden"
    );
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn advertisement_cross_unit_read_filter_deny_expiry_and_receiver_permissions() {
    use wekeeper_api::advertising::domain as a;
    let f = Fixture::new().await;
    f.configure(vec![f.targets[0]]).await;
    let other = f
        .service()
        .execute(
            &f.actor,
            Command::Entity {
                kind: "routing-units".into(),
                id: None,
                input: EntityInput {
                    name: "同名单位".into(),
                    description: "另一个稳定身份".into(),
                    active: true,
                    revision: None,
                    unit_id: None,
                },
            },
        )
        .await
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .parse::<Uuid>()
        .unwrap();
    let created = ads(&f)
        .execute(
            &f.actor,
            a::Command::Create(a::Create {
                owner_unit_id: other,
                content: a::Content {
                    name: "外单位广告".into(),
                    body: "只在明确授权后使用".into(),
                    link: None,
                },
                workspaces: vec![a::Scope {
                    kind: "unit".into(),
                    id: other.to_string(),
                    channel: Some("mobile".into()),
                    workspace_id: Some(f.workspace),
                }],
                reason: "私有默认".into(),
            }),
        )
        .await
        .unwrap();
    let id = created["id"].as_str().unwrap().parse::<Uuid>().unwrap();
    let candidates = ads(&f)
        .execute(&f.actor, a::Command::Candidates(f.route.clone(), 1))
        .await
        .unwrap();
    assert!(candidates["items"].as_array().unwrap().is_empty());
    let receiver = Uuid::new_v4();
    sqlx::query("INSERT INTO accounts(id,username,display_name) VALUES($1,'receiver','Receiver')")
        .bind(receiver)
        .execute(&f.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO mobile_workspace_members VALUES($1,$2,'admin')")
        .bind(f.workspace)
        .bind(receiver)
        .execute(&f.pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO source_route_managers(route_id,account_id,granted_by) VALUES($1,$2,$3)",
    )
    .bind(&f.route)
    .bind(receiver)
    .bind(f.actor.id)
    .execute(&f.pool)
    .await
    .unwrap();
    let actor:Actor=serde_json::from_value(json!({"id":receiver,"username":"receiver","display_name":"Receiver","platform_admin":false,"grants":[],"wecom_authenticated":false})).unwrap();
    assert_eq!(
        ads(&f)
            .execute(&actor, a::Command::Detail(id))
            .await
            .unwrap_err()
            .0,
        "not_found"
    );
    let mut policy = a::Policy {
        enabled: true,
        valid_from: chrono::Utc::now(),
        valid_until: None,
        allows: vec![a::Scope {
            kind: "route".into(),
            id: f.route.clone(),
            channel: Some("mobile".into()),
            workspace_id: Some(f.workspace),
        }],
        denies: vec![],
    };
    ads(&f)
        .execute(
            &f.actor,
            a::Command::Policy(
                id,
                a::PolicyPublication {
                    expected_revision: 2,
                    reason: "向指定路由共享".into(),
                    policy: policy.clone(),
                },
            ),
        )
        .await
        .unwrap();
    let list = ads(&f)
        .execute(
            &actor,
            a::Command::List {
                page: 1,
                filter: "shared".into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(list["items"].as_array().unwrap().len(), 1);
    assert_eq!(list["items"][0]["can_edit"], false);
    assert_eq!(list["items"][0]["policy"], Value::Null);
    let denied = ads(&f)
        .execute(
            &actor,
            a::Command::Content(
                id,
                a::ContentPublication {
                    expected_revision: 3,
                    reason: "不应修改".into(),
                    content: a::Content {
                        name: "覆盖".into(),
                        body: "覆盖".into(),
                        link: None,
                    },
                },
            ),
        )
        .await
        .unwrap_err();
    assert_eq!(denied.0, "forbidden");
    policy.denies.push(a::Scope {
        kind: "group".into(),
        id: f.group.to_string(),
        channel: None,
        workspace_id: None,
    });
    ads(&f)
        .execute(
            &f.actor,
            a::Command::Policy(
                id,
                a::PolicyPublication {
                    expected_revision: 3,
                    reason: "禁投优先".into(),
                    policy: policy.clone(),
                },
            ),
        )
        .await
        .unwrap();
    assert!(
        ads(&f)
            .execute(&actor, a::Command::Candidates(f.route.clone(), 1))
            .await
            .unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    policy.denies.clear();
    policy.valid_from = chrono::Utc::now() - chrono::Duration::hours(2);
    policy.valid_until = Some(chrono::Utc::now() - chrono::Duration::hours(1));
    ads(&f)
        .execute(
            &f.actor,
            a::Command::Policy(
                id,
                a::PolicyPublication {
                    expected_revision: 4,
                    reason: "已过期".into(),
                    policy,
                },
            ),
        )
        .await
        .unwrap();
    assert!(
        ads(&f)
            .execute(
                &actor,
                a::Command::List {
                    page: 1,
                    filter: "all".into()
                }
            )
            .await
            .unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    f.close().await;
}
#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn advertisement_configuration_wait_records_idempotent_decision_and_original_blocks_first() {
    use wekeeper_api::advertising::domain as a;
    use wekeeper_api::source_access::{
        domain::keyword_filter as k, south::adapter::postgres::keyword_filter::KeywordRepository,
    };
    let f = Fixture::new().await;
    f.configure(vec![f.targets[0]]).await;
    let ad = select_ad(&f, "mobile", f.workspace).await;
    let config = a::Config {
        mode: "fixed".into(),
        fixed_ad_id: Some(ad),
        unavailable_policy: "configuration_error".into(),
        ..Default::default()
    };
    ads(&f)
        .execute(
            &f.actor,
            a::Command::Configure(
                f.route.clone(),
                a::Configuration {
                    expected_revision: 1,
                    reason: "不可用时等待".into(),
                    config,
                },
            ),
        )
        .await
        .unwrap();
    disable_ad(&f, ad).await;
    f.lifecycle(LifecycleAction::Enable).await;
    let worker = RoutingRepository(f.pool.clone());
    let r = inbox(&f, "config-wait", 1, "content.upsert").await;
    worker.run_once().await.unwrap();
    assert_eq!(receipt_value(&f, r).await["reason_code"], "ad_unavailable");
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM source_execution_tasks WHERE receipt_id=$1")
            .bind(r)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(count, 0);
    for _ in 0..3 {
        sqlx::query("UPDATE source_inbox SET next_attempt_at=now() WHERE receipt_id=$1")
            .bind(r)
            .execute(&f.pool)
            .await
            .unwrap();
        worker.run_once().await.unwrap();
    }
    let decisions = ads(&f)
        .execute(&f.actor, a::Command::Compositions(f.route.clone(), r, 1))
        .await
        .unwrap();
    assert_eq!(decisions["decisions"].as_array().unwrap().len(), 1);
    assert_eq!(
        decisions["decisions"][0]["selection"]["exclusions"][0],
        "ad_disabled"
    );
    let service = wekeeper_api::source_access::north::local::keyword_filter::Service(
        KeywordRepository(f.pool.clone()),
    );
    service
        .execute(
            &f.actor,
            k::Command::Publish {
                scope: "global".into(),
                input: k::Publication {
                    expected_revision: 1,
                    reason: "原文先拦截".into(),
                    entries: vec![k::Entry {
                        word: "正文".into(),
                        enabled: true,
                    }],
                },
            },
        )
        .await
        .unwrap();
    sqlx::query("ALTER TABLE advertisements RENAME COLUMN policy_version TO unavailable_policy")
        .execute(&f.pool)
        .await
        .unwrap();
    let blocked = inbox(&f, "blocked-raw", 1, "content.upsert").await;
    worker.run_once().await.unwrap();
    assert_eq!(
        receipt_value(&f, blocked).await["reason_code"],
        "keyword_blocked"
    );
    let none: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM source_advertisement_decisions WHERE receipt_id=$1",
    )
    .bind(blocked)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(none, 0);
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn advertisement_http_identity_csrf_and_strict_publication_contract() {
    use axum::{
        body::{Body, to_bytes},
        http::Request,
    };
    use tower::ServiceExt;
    let f = Fixture::new().await;
    let app = wekeeper_api::bootstrap::build_application(f.pool.clone());
    let (token, actor) = app
        .identity
        .0
        .session(f.actor.id, "browser", false)
        .await
        .unwrap();
    let router = wekeeper_api::http::app_router(app);
    let input = json!({"data":{"type":"advertisements","attributes":{"owner_unit_id":f.unit,"content":{"name":"HTTP 广告","body":"正文","link":null},"workspaces":[],"reason":"创建"}}});
    let request = |csrf: bool, body: Value| {
        let mut b = Request::builder()
            .method("POST")
            .uri("/api/v2/admin/advertisements")
            .header("cookie", format!("wekeeper_session={token}"))
            .header("origin", "http://localhost:5173")
            .header("content-type", "application/vnd.api+json");
        if csrf {
            b = b.header("x-csrf-token", &actor.csrf);
        }
        b.body(Body::from(body.to_string())).unwrap()
    };
    let no_auth = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v2/admin/advertisements")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(no_auth.status(), 401);
    let no_csrf = router
        .clone()
        .oneshot(request(false, input.clone()))
        .await
        .unwrap();
    assert_eq!(no_csrf.status(), 403);
    let mut extra = input.clone();
    extra["data"]["attributes"]["force_pass"] = json!(true);
    let invalid = router.clone().oneshot(request(true, extra)).await.unwrap();
    assert_eq!(invalid.status(), 400);
    let response = router.clone().oneshot(request(true, input)).await.unwrap();
    assert_eq!(response.status(), 200);
    let doc: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 100000).await.unwrap()).unwrap();
    let id = doc["data"]["id"].as_str().unwrap().parse::<Uuid>().unwrap();
    assert_ne!(id, Uuid::nil());
    assert_eq!(doc["data"]["attributes"]["revision"], 2);
    assert!(doc["data"]["attributes"].get("id").is_none());
    let detail = router
        .oneshot(
            Request::builder()
                .uri(format!("/api/v2/admin/advertisements/{id}"))
                .header("cookie", format!("wekeeper_session={token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(detail.status(), 200);
    let doc: Value =
        serde_json::from_slice(&to_bytes(detail.into_body(), 100000).await.unwrap()).unwrap();
    assert_eq!(doc["data"]["id"], id.to_string());
    assert_eq!(doc["data"]["attributes"]["content"]["body"], "正文");
    f.close().await;
}
#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn advertisement_child_failure_rolls_back_composition_snapshot_and_decision() {
    let f = Fixture::new().await;
    f.configure(vec![f.targets[0]]).await;
    select_ad(&f, "mobile", f.workspace).await;
    f.lifecycle(LifecycleAction::Enable).await;
    sqlx::query("ALTER TABLE mobile_v9_task_drafts RENAME COLUMN message TO unavailable_message")
        .execute(&f.pool)
        .await
        .unwrap();
    let r = inbox(&f, "atomic-ad", 1, "content.upsert").await;
    let worker = RoutingRepository(f.pool.clone());
    worker.run_once().await.unwrap();
    for table in [
        "source_message_compositions",
        "source_advertisement_decisions",
        "source_execution_generations",
        "distribution_message_snapshots",
        "mobile_v9_tasks",
    ] {
        let count: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
            .fetch_one(&f.pool)
            .await
            .unwrap();
        assert_eq!(count, 0, "{table} must roll back with channel child");
    }
    assert_eq!(
        receipt_value(&f, r).await["reason_code"],
        "temporarily_unavailable"
    );
    sqlx::query("ALTER TABLE mobile_v9_task_drafts RENAME COLUMN unavailable_message TO message")
        .execute(&f.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE source_inbox SET next_attempt_at=now() WHERE receipt_id=$1")
        .bind(r)
        .execute(&f.pool)
        .await
        .unwrap();
    worker.run_once().await.unwrap();
    assert_eq!(composition(&f, r).await["composer"], "source-ad-text/1");
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn advertisement_deletion_removes_active_listing_and_retains_version_history() {
    use wekeeper_api::advertising::domain as a;
    let f = Fixture::new().await;
    f.configure(vec![f.targets[0]]).await;
    let id = select_ad(&f, "mobile", f.workspace).await;
    let active = ads(&f)
        .execute(
            &f.actor,
            a::Command::List {
                page: 1,
                filter: "all".into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(active["items"].as_array().unwrap().len(), 1);
    ads(&f)
        .execute(
            &f.actor,
            a::Command::Archive(
                id,
                a::Revision {
                    expected_revision: 2,
                    reason: "删除不再使用的广告".into(),
                },
            ),
        )
        .await
        .unwrap();
    for filter in ["all", "owned", "shared"] {
        let active = ads(&f)
            .execute(
                &f.actor,
                a::Command::List {
                    page: 1,
                    filter: filter.into(),
                },
            )
            .await
            .unwrap();
        assert!(active["items"].as_array().unwrap().is_empty());
    }
    let archived = ads(&f)
        .execute(
            &f.actor,
            a::Command::List {
                page: 1,
                filter: "archived".into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(archived["items"][0]["id"], id.to_string());
    assert_eq!(archived["items"][0]["archived"], true);
    let versions = ads(&f)
        .execute(&f.actor, a::Command::Versions(id, 1))
        .await
        .unwrap();
    assert_eq!(versions["items"].as_array().unwrap().len(), 2);
    let candidates = ads(&f)
        .execute(&f.actor, a::Command::Candidates(f.route.clone(), 1))
        .await
        .unwrap();
    assert!(candidates["items"].as_array().unwrap().is_empty());
    f.close().await;
}

async fn ordered_pool(f: &Fixture) -> (Uuid, Uuid, Value) {
    use wekeeper_api::advertising::domain as a;
    let one = select_ad(f, "mobile", f.workspace).await;
    let second = ads(f)
        .execute(
            &f.actor,
            a::Command::Create(a::Create {
                owner_unit_id: f.unit,
                content: a::Content {
                    name: "第二广告".into(),
                    body: "第二条广告正文".into(),
                    link: None,
                },
                workspaces: vec![a::Scope {
                    kind: "unit".into(),
                    id: f.unit.to_string(),
                    channel: Some("mobile".into()),
                    workspace_id: Some(f.workspace),
                }],
                reason: "第二素材".into(),
            }),
        )
        .await
        .unwrap();
    let two: Uuid = serde_json::from_value(second["id"].clone()).unwrap();
    let pool = ads(f)
        .execute(
            &f.actor,
            a::Command::PublishPool(
                f.route.clone(),
                a::pool::Publication {
                    expected_revision: 0,
                    reason: "重复序列".into(),
                    slots: [one, two, two, one]
                        .into_iter()
                        .map(|ad_id| a::pool::SlotInput {
                            slot_id: None,
                            ad_id,
                        })
                        .collect(),
                },
            ),
        )
        .await
        .unwrap();
    ads(f)
        .execute(
            &f.actor,
            a::Command::Configure(
                f.route.clone(),
                a::Configuration {
                    expected_revision: 1,
                    reason: "启用池".into(),
                    config: a::Config {
                        mode: "pool".into(),
                        ..Default::default()
                    },
                },
            ),
        )
        .await
        .unwrap();
    (one, two, pool)
}
#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn ordered_pool_exact_repeats_concurrent_workers_restarts_edits_and_reset() {
    use wekeeper_api::advertising::domain as a;
    let f = Fixture::new().await;
    f.configure(vec![f.targets[0]]).await;
    let (one, two, initial) = ordered_pool(&f).await;
    assert_ne!(
        initial["slots"][1]["slot_id"],
        initial["slots"][2]["slot_id"]
    );
    f.lifecycle(LifecycleAction::Enable).await;
    let mut receipts = vec![];
    for n in 0..20 {
        receipts.push(inbox(&f, &format!("pool-{n}"), 1, "content.upsert").await);
    }
    let w = RoutingRepository(f.pool.clone());
    let other = RoutingRepository(f.pool.clone());
    for _ in 0..3 {
        let (x, y) = tokio::join!(w.run_once(), other.run_once());
        x.unwrap();
        y.unwrap();
    }
    let chosen:Vec<Uuid>=sqlx::query_scalar("SELECT (selection->>'ad_id')::uuid FROM advertisement_pool_consumptions ORDER BY selection_order").fetch_all(&f.pool).await.unwrap();
    assert_eq!(
        chosen,
        (0..5)
            .flat_map(|_| [one, two, two, one])
            .collect::<Vec<_>>()
    );
    let cursor: Value = ads(&f)
        .execute(&f.actor, a::Command::Pool(f.route.clone()))
        .await
        .unwrap();
    assert_eq!(cursor["cursor"]["selection_order"], 20);
    assert_eq!(cursor["cursor"]["cycle"], 5);
    let counts = ads(&f)
        .execute(&f.actor, a::Command::Consumptions(f.route.clone(), 1))
        .await
        .unwrap();
    assert_eq!(counts["statistics"]["selections"], 20);
    assert_eq!(counts["statistics"]["valid_targets"], 20);
    assert_eq!(counts["statistics"]["device_attempts"], 20);
    assert_eq!(counts["statistics"]["sent_targets"], 0);
    assert_eq!(counts["statistics"]["pending_targets"], 20);
    assert_eq!(counts["slot_statistics"].as_array().unwrap().len(), 4);
    assert!(
        counts["slot_statistics"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c["selections"] == 5)
    );
    assert_eq!(counts["ad_statistics"].as_array().unwrap().len(), 2);
    assert!(
        counts["ad_statistics"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c["selections"] == 10)
    );
    // A restarted worker cannot consume the existing 20 events again.
    RoutingRepository(f.pool.clone()).run_once().await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM advertisement_pool_consumptions")
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        20
    );
    let before = composition(&f, receipts[0]).await;
    let reset = ads(&f)
        .execute(
            &f.actor,
            a::Command::ResetPool(
                f.route.clone(),
                a::pool::Reset {
                    expected_revision: 1,
                    expected_cursor_version: 21,
                    reason: "重新开始".into(),
                },
            ),
        )
        .await
        .unwrap();
    assert_eq!(reset["cursor"]["selection_order"], 20);
    assert_eq!(reset["cursor"]["reset_epoch"], 1);
    let conflict = ads(&f)
        .execute(
            &f.actor,
            a::Command::ResetPool(
                f.route.clone(),
                a::pool::Reset {
                    expected_revision: 1,
                    expected_cursor_version: 21,
                    reason: "过时重置".into(),
                },
            ),
        )
        .await
        .unwrap_err();
    assert_eq!(conflict.0, "revision_conflict");
    let new = inbox(&f, "after-reset", 1, "content.upsert").await;
    w.run_once().await.unwrap();
    assert_eq!(composition(&f, new).await["selection"]["ad_id"], json!(one));
    assert_eq!(composition(&f, receipts[0]).await, before);
    // Moving the consumed slot keeps its identity and continues from its new successor.
    let slots: Vec<a::pool::Slot> = serde_json::from_value(initial["slots"].clone()).unwrap();
    let order = [
        slots[1].clone(),
        slots[0].clone(),
        slots[2].clone(),
        slots[3].clone(),
    ];
    let published = ads(&f)
        .execute(
            &f.actor,
            a::Command::PublishPool(
                f.route.clone(),
                a::pool::Publication {
                    expected_revision: 1,
                    reason: "移动首槽".into(),
                    slots: order
                        .into_iter()
                        .map(|s| a::pool::SlotInput {
                            slot_id: Some(s.slot_id),
                            ad_id: s.ad_id,
                        })
                        .collect(),
                },
            ),
        )
        .await
        .unwrap();
    assert_eq!(published["pool_id"], initial["pool_id"]);
    assert_eq!(published["cursor"]["next_slot_id"], json!(slots[2].slot_id));
    f.close().await;
}
#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn ordered_pool_bounded_skips_no_candidate_no_consumption_and_preview() {
    use wekeeper_api::advertising::domain as a;
    let f = Fixture::new().await;
    f.configure(vec![f.targets[0]]).await;
    let (one, two, _) = ordered_pool(&f).await;
    f.lifecycle(LifecycleAction::Enable).await;
    disable_ad(&f, two).await;
    for _ in 0..10 {
        ads(&f)
            .execute(
                &f.actor,
                a::Command::Preview(
                    f.route.clone(),
                    a::Preview {
                        title: Some("通知".into()),
                        content: Some("正文".into()),
                        detail_url: "https://example.com".into(),
                    },
                ),
            )
            .await
            .unwrap();
    }
    let first = inbox(&f, "skip-1", 1, "content.upsert").await;
    let second = inbox(&f, "skip-2", 1, "content.upsert").await;
    RoutingRepository(f.pool.clone()).run_once().await.unwrap();
    assert_eq!(
        composition(&f, first).await["selection"]["ad_id"],
        json!(one)
    );
    let c = composition(&f, second).await;
    assert_eq!(c["selection"]["pool"]["selected_position"], 4);
    assert_eq!(c["selection"]["pool"]["scans"].as_array().unwrap().len(), 3);
    disable_ad(&f, one).await;
    let none = inbox(&f, "all-disabled", 1, "content.upsert").await;
    RoutingRepository(f.pool.clone()).run_once().await.unwrap();
    let c = composition(&f, none).await;
    assert!(c["selection"]["ad_id"].is_null());
    assert!(c["selection"]["pool"]["selected_slot_id"].is_null());
    assert_eq!(
        c["selection"]["pool"]["before"],
        c["selection"]["pool"]["after"]
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM advertisement_pool_consumptions")
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        2
    );
    f.close().await;
}
#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn ordered_pool_reservation_survives_restart_without_later_events_overtaking() {
    let f = Fixture::new().await;
    f.configure(vec![f.targets[0]]).await;
    let (one, two, _) = ordered_pool(&f).await;
    f.lifecycle(LifecycleAction::Enable).await;
    let first = inbox(&f, "reserved-first", 1, "content.upsert").await;
    let second = inbox(&f, "reserved-second", 1, "content.upsert").await;
    sqlx::query("UPDATE advertisement_pool_queue SET reservation_version=10,reservation_expiry=now()+interval '2 minutes' WHERE receipt_id=$1")
        .bind(first).execute(&f.pool).await.unwrap();
    RoutingRepository(f.pool.clone()).run_once().await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM advertisement_pool_consumptions")
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        0
    );
    // Simulate a crashed owner's expiration without waiting on wall-clock sleeps.
    sqlx::query("UPDATE advertisement_pool_queue SET reservation_expiry=now()-interval '1 second' WHERE receipt_id=$1")
        .bind(first).execute(&f.pool).await.unwrap();
    RoutingRepository(f.pool.clone()).run_once().await.unwrap();
    assert_eq!(
        composition(&f, first).await["selection"]["ad_id"],
        json!(one)
    );
    assert_eq!(
        composition(&f, second).await["selection"]["ad_id"],
        json!(two)
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT reservation_version FROM advertisement_pool_queue WHERE receipt_id=$1"
        )
        .bind(first)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        11
    );
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn ordered_pool_task_failure_rolls_back_cursor_and_decision() {
    use wekeeper_api::advertising::domain as a;
    let f = Fixture::new().await;
    f.configure(vec![f.targets[0]]).await;
    let (one, _, _) = ordered_pool(&f).await;
    f.lifecycle(LifecycleAction::Enable).await;
    let r = inbox(&f, "fault", 1, "content.upsert").await;
    sqlx::query("CREATE FUNCTION fail_pool_task() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected task failure'; END $$").execute(&f.pool).await.unwrap();
    let w = RoutingRepository(f.pool.clone());
    for table in [
        "distribution_message_snapshots",
        "source_message_compositions",
        "advertisement_pool_consumptions",
        "mobile_v9_task_items",
        "source_execution_tasks",
        "source_access_audit",
    ] {
        let condition = if table == "source_access_audit" {
            "WHEN (NEW.action='source.task.created')"
        } else {
            ""
        };
        sqlx::query(&format!("CREATE TRIGGER fail_pool_task BEFORE INSERT ON {table} FOR EACH ROW {condition} EXECUTE FUNCTION fail_pool_task()"))
            .execute(&f.pool).await.unwrap();
        sqlx::query("UPDATE source_inbox SET next_attempt_at=now() WHERE receipt_id=$1")
            .bind(r)
            .execute(&f.pool)
            .await
            .unwrap();
        w.run_once().await.unwrap();
        let p = ads(&f)
            .execute(&f.actor, a::Command::Pool(f.route.clone()))
            .await
            .unwrap();
        assert_eq!(p["cursor"]["selection_order"], 0, "{table}");
        let leftovers: i64 = sqlx::query_scalar("SELECT (SELECT count(*) FROM source_message_compositions)+(SELECT count(*) FROM advertisement_pool_consumptions)+(SELECT count(*) FROM source_execution_tasks)+(SELECT count(*) FROM distribution_message_snapshots)+(SELECT count(*) FROM mobile_v9_task_items)")
            .fetch_one(&f.pool).await.unwrap();
        assert_eq!(leftovers, 0, "partial commit after failure on {table}");
        sqlx::query(&format!("DROP TRIGGER fail_pool_task ON {table}"))
            .execute(&f.pool)
            .await
            .unwrap();
    }
    sqlx::query("UPDATE source_inbox SET next_attempt_at=now() WHERE receipt_id=$1")
        .bind(r)
        .execute(&f.pool)
        .await
        .unwrap();
    w.run_once().await.unwrap();
    assert_eq!(composition(&f, r).await["selection"]["ad_id"], json!(one));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM advertisement_pool_consumptions")
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        1
    );
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn ordered_pools_commit_independently_while_another_full_task_transaction_is_blocked() {
    let test_base = PgPool::connect(
        &std::env::var("WEKEEPER_TEST_DATABASE_URL").expect("isolated database required"),
    )
    .await
    .unwrap();
    let mut isolation = test_base.begin().await.unwrap();
    sqlx::query("SELECT pg_advisory_xact_lock(74182318)")
        .execute(&mut *isolation)
        .await
        .unwrap();
    let a = Fixture::new_with_guard(false).await;
    let b = Fixture::new_with_guard(false).await;
    a.configure(vec![a.targets[0]]).await;
    b.configure(vec![b.targets[0]]).await;
    ordered_pool(&a).await;
    ordered_pool(&b).await;
    a.lifecycle(LifecycleAction::Enable).await;
    b.lifecycle(LifecycleAction::Enable).await;
    let ar = inbox(&a, "pool-a", 1, "content.upsert").await;
    let br = inbox(&b, "pool-b", 1, "content.upsert").await;
    let gate = 74182319_i64;
    let mut blocker = a.pool.acquire().await.unwrap();
    sqlx::query("SELECT pg_advisory_lock($1)")
        .bind(gate)
        .execute(&mut *blocker)
        .await
        .unwrap();
    sqlx::query("CREATE FUNCTION pause_pool_commit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_advisory_xact_lock(74182319::bigint); RETURN NEW; END $$").execute(&a.pool).await.unwrap();
    sqlx::query("CREATE TRIGGER pause_pool_commit BEFORE INSERT ON source_execution_tasks FOR EACH ROW EXECUTE FUNCTION pause_pool_commit()").execute(&a.pool).await.unwrap();
    let aw = RoutingRepository(a.pool.clone());
    let worker = tokio::spawn(async move { aw.run_once().await });
    // Observe the actual blocked task insert, rather than relying on a timing sleep.
    tokio::time::timeout(std::time::Duration::from_secs(15),async {
        loop {
            let waiting:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE locktype='advisory' AND objid=74182319 AND NOT granted)").fetch_one(&a.pool).await.unwrap();
            if waiting {break} tokio::task::yield_now().await;
        }
    }).await.unwrap();
    let independent = tokio::time::timeout(
        std::time::Duration::from_secs(15),
        RoutingRepository(b.pool.clone()).run_once(),
    )
    .await;
    sqlx::query("SELECT pg_advisory_unlock($1)")
        .bind(gate)
        .execute(&mut *blocker)
        .await
        .unwrap();
    drop(blocker);
    worker.await.unwrap().unwrap();
    independent
        .expect("unrelated pool must not wait on global source transaction lock")
        .unwrap();
    assert!(composition(&a, ar).await["selection"]["ad_id"].is_string());
    assert!(composition(&b, br).await["selection"]["ad_id"].is_string());
    a.close().await;
    b.close().await;
    isolation.rollback().await.unwrap();
    test_base.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn ordered_pool_does_not_filter_advertising_text() {
    let f = Fixture::new().await;
    f.configure(vec![f.targets[0]]).await;
    let (_, two, _) = ordered_pool(&f).await;
    publish_keywords(&f, "global", &["第二条广告"]).await;
    f.lifecycle(LifecycleAction::Enable).await;
    let worker = RoutingRepository(f.pool.clone());
    let first = inbox(&f, "ad-first", 1, "content.upsert").await;
    worker.run_once().await.unwrap();
    let second = inbox(&f, "ad-second", 1, "content.upsert").await;
    worker.run_once().await.unwrap();
    let result = composition(&f, second).await;
    assert_eq!(result["selection"]["ad_id"], two.to_string());
    assert!(result["content"].as_str().unwrap().contains("第二条广告"));
    assert_eq!(
        receipt_value(&f, first).await["processing_stage"],
        "task_created"
    );
    assert_eq!(
        receipt_value(&f, second).await["processing_stage"],
        "task_created"
    );
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn ordered_pool_copy_has_new_progress_and_management_rejects_foreign_slots() {
    use wekeeper_api::advertising::domain as a;
    let f = Fixture::new().await;
    f.configure(vec![f.targets[0]]).await;
    let (_, _, original) = ordered_pool(&f).await;
    let target = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO source_routes(route_id,source_id,route_name,status) VALUES($1,$2,'Copy','active')").bind(&target).bind(f.source).execute(&f.pool).await.unwrap();
    sqlx::query(
        "INSERT INTO source_key_routes(key_id,route_id,source_id,status) VALUES($1,$2,$3,'active')",
    )
    .bind(&f.key)
    .bind(&target)
    .bind(f.source)
    .execute(&f.pool)
    .await
    .unwrap();
    f.service()
        .execute(
            &f.actor,
            Command::Configure {
                route: target.clone(),
                input: ConfigurationInput {
                    revision: 1,
                    unit_id: f.unit,
                    group_id: f.group,
                    execution: Some(ExecutionConfig {
                        channel: Channel::Mobile,
                        workspace_id: f.workspace,
                        target_ids: vec![f.targets[0]],
                    }),
                },
            },
        )
        .await
        .unwrap();
    let copied = ads(&f)
        .execute(
            &f.actor,
            a::Command::CopyPool(
                f.route.clone(),
                a::pool::Copy {
                    target_route_id: target.clone(),
                    expected_revision: 0,
                    reason: "复制从头开始".into(),
                },
            ),
        )
        .await
        .unwrap();
    assert_ne!(copied["pool_id"], original["pool_id"]);
    assert_eq!(copied["cursor"]["selection_order"], 0);
    for i in 0..4 {
        assert_eq!(copied["slots"][i]["ad_id"], original["slots"][i]["ad_id"]);
        assert_ne!(
            copied["slots"][i]["slot_id"],
            original["slots"][i]["slot_id"]
        );
    }
    let foreign: a::pool::Slot = serde_json::from_value(original["slots"][0].clone()).unwrap();
    let error = ads(&f)
        .execute(
            &f.actor,
            a::Command::PublishPool(
                target.clone(),
                a::pool::Publication {
                    expected_revision: 1,
                    reason: "不允许外池槽位".into(),
                    slots: vec![a::pool::SlotInput {
                        slot_id: Some(foreign.slot_id),
                        ad_id: foreign.ad_id,
                    }],
                },
            ),
        )
        .await
        .unwrap_err();
    assert_eq!(error.0, "ad_pool_invalid_slot");
    let uid = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO accounts(id,username,display_name) VALUES($1,'pool-outsider','Outsider')",
    )
    .bind(uid)
    .execute(&f.pool)
    .await
    .unwrap();
    let outsider:Actor=serde_json::from_value(json!({"id":uid,"username":"pool-outsider","display_name":"Outsider","platform_admin":false,"grants":[],"wecom_authenticated":false})).unwrap();
    for command in [
        a::Command::Pool(target.clone()),
        a::Command::Consumptions(target.clone(), 1),
        a::Command::ResetPool(
            target,
            a::pool::Reset {
                expected_revision: 1,
                expected_cursor_version: 1,
                reason: "越权重置".into(),
            },
        ),
    ] {
        assert_eq!(
            ads(&f).execute(&outsider, command).await.unwrap_err().0,
            "forbidden"
        );
    }
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn automation_tasks_freeze_subset_authorize_stop_and_never_resurrect() {
    use wekeeper_api::source_access::{
        domain::orchestration::{Command as Auto, Control, Filter},
        north::local::orchestration::Service as AutoService,
        south::adapter::postgres::orchestration::Repository,
    };
    let f = Fixture::new().await;
    f.configure(f.targets.clone()).await;
    f.lifecycle(LifecycleAction::Enable).await;
    // One business-disabled target must not block three valid targets with offline phones.
    sqlx::query("UPDATE mobile_targets SET enabled=false WHERE id=$1")
        .bind(f.targets[3])
        .execute(&f.pool)
        .await
        .unwrap();
    let receipt = inbox(&f, "automation", 1, "content.upsert").await;
    let worker = RoutingRepository(f.pool.clone());
    worker.run_once().await.unwrap();
    let service = AutoService(Repository(f.pool.clone()));
    let list = service
        .execute(&f.actor, Auto::List(Filter::default()))
        .await
        .unwrap();
    assert_eq!(list["items"].as_array().unwrap().len(), 1, "{list}");
    assert_eq!(list["items"][0]["counts"]["total"], 3);
    let detail = service
        .execute(&f.actor, Auto::Detail(receipt))
        .await
        .unwrap();
    assert_eq!(detail["frozen"]["executed_by"], "source-routing");
    let targets = service
        .execute(&f.actor, Auto::Targets(receipt))
        .await
        .unwrap();
    assert_eq!(targets["excluded"].as_array().unwrap().len(), 1);
    let attempts = service
        .execute(&f.actor, Auto::Attempts(receipt, f.targets[0]))
        .await
        .unwrap();
    assert!(!attempts["items"].as_array().unwrap().is_empty());
    let outsider = Uuid::new_v4();
    sqlx::query("INSERT INTO accounts(id,username,display_name) VALUES($1,'outsider','outsider')")
        .bind(outsider)
        .execute(&f.pool)
        .await
        .unwrap();
    let mut actor = f.actor.clone();
    actor.id = outsider;
    actor.platform_admin = false;
    assert!(
        service
            .execute(&actor, Auto::Detail(receipt))
            .await
            .is_err()
    );
    sqlx::query("INSERT INTO mobile_workspace_members(workspace_id,account_id,role) VALUES($1,$2,'operator')").bind(f.workspace).bind(outsider).execute(&f.pool).await.unwrap();
    assert!(service.execute(&actor, Auto::Detail(receipt)).await.is_ok());
    let cmd = || Control {
        expected_revision: 1,
        idempotency_key: Uuid::new_v4(),
        reason: "停止业务".into(),
        target_ids: vec![],
    };
    assert!(
        service
            .execute(&actor, Auto::Stop(receipt, cmd()))
            .await
            .is_err()
    );
    let key = Uuid::new_v4();
    for _ in 0..2 {
        service
            .execute(
                &f.actor,
                Auto::Stop(
                    receipt,
                    Control {
                        expected_revision: 1,
                        idempotency_key: key,
                        reason: "停止业务".into(),
                        target_ids: vec![],
                    },
                ),
            )
            .await
            .unwrap();
    }
    worker.run_once().await.unwrap();
    let stopped = service
        .execute(&f.actor, Auto::Detail(receipt))
        .await
        .unwrap();
    assert_eq!(stopped["task"]["stopped"], true);
    assert_eq!(stopped["task"]["state"], "stopped");
    assert!(
        service
            .execute(&f.actor, Auto::Retry(receipt, cmd()))
            .await
            .is_err()
    );
    f.configure(vec![f.targets[1]]).await;
    worker.run_once().await.unwrap();
    let frozen = service
        .execute(&f.actor, Auto::Detail(receipt))
        .await
        .unwrap();
    assert_eq!(frozen["frozen"]["snapshot"], detail["frozen"]["snapshot"]);
    let events = service
        .execute(&f.actor, Auto::Events(receipt, 0))
        .await
        .unwrap();
    assert!(
        events["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["kind"] == "task.stop_requested")
    );
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn automation_version_policy_only_resends_changed_future_versions_with_known_results() {
    use wekeeper_api::source_access::{
        domain::orchestration::{Command as Auto, PolicyInput, VersionPolicy},
        north::local::orchestration::Service as AutoService,
        south::adapter::postgres::orchestration::Repository,
    };
    let f = Fixture::new().await;
    f.configure(vec![f.targets[0]]).await;
    f.lifecycle(LifecycleAction::Enable).await;
    let service = AutoService(Repository(f.pool.clone()));
    service
        .execute(
            &f.actor,
            Auto::Policy(
                f.route.clone(),
                Some(PolicyInput {
                    revision: 0,
                    policy: VersionPolicy::ResendNewVersion,
                    reason: "明确授权新版本".into(),
                }),
            ),
        )
        .await
        .unwrap();
    let r1 = inbox(&f, "version-policy", 1, "content.upsert").await;
    let worker = RoutingRepository(f.pool.clone());
    worker.run_once().await.unwrap();
    sqlx::query("UPDATE mobile_v9_attempts SET state='observed_sent' WHERE item_id IN (SELECT i.id FROM mobile_v9_task_items i JOIN source_execution_tasks e ON e.task_id=i.task_id WHERE e.receipt_id=$1)").bind(r1).execute(&f.pool).await.unwrap();
    let r2 = inbox(&f, "version-policy", 2, "content.upsert").await;
    change_title(&f, r2, "第二版新内容").await;
    worker.run_once().await.unwrap();
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM source_automation_tasks WHERE id=$1)")
            .bind(r2)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert!(exists, "explicit policy must create a new version task");
    // Unknown second-version evidence cannot be bypassed by a third content edit.
    sqlx::query("UPDATE mobile_v9_attempts a SET state='unknown' FROM mobile_v9_task_items i JOIN source_execution_tasks e ON e.task_id=i.task_id WHERE a.item_id=i.id AND e.receipt_id=$1").bind(r2).execute(&f.pool).await.unwrap();
    sqlx::query("UPDATE mobile_v9_dispatch_state d SET state='dispatch_failed',completed_rounds=3 FROM mobile_v9_task_items i JOIN source_execution_tasks e ON e.task_id=i.task_id WHERE d.item_id=i.id AND e.receipt_id=$1").bind(r2).execute(&f.pool).await.unwrap();
    let r3 = inbox(&f, "version-policy", 3, "content.upsert").await;
    change_title(&f, r3, "第三版内容").await;
    worker.run_once().await.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM source_automation_tasks WHERE id=$1")
        .bind(r3)
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(count, 0, "unknown version must not be bypassed");
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn automation_aggregate_preserves_partial_unknown_and_late_results() {
    use wekeeper_api::source_access::{
        domain::orchestration::Command as Auto,
        north::local::orchestration::Service as AutoService,
        south::adapter::postgres::orchestration::Repository,
    };
    let f = Fixture::new().await;
    f.configure(f.targets[..3].to_vec()).await;
    f.lifecycle(LifecycleAction::Enable).await;
    let receipt = inbox(&f, "aggregate", 1, "content.upsert").await;
    let worker = RoutingRepository(f.pool.clone());
    worker.run_once().await.unwrap();
    let service = AutoService(Repository(f.pool.clone()));
    for (target, state) in [
        (f.targets[0], "observed_sent"),
        (f.targets[1], "unknown"),
        (f.targets[2], "failed_before_call"),
    ] {
        sqlx::query("UPDATE mobile_v9_attempts a SET state=$3 FROM mobile_v9_task_items i JOIN source_execution_tasks e ON e.task_id=i.task_id WHERE a.item_id=i.id AND e.receipt_id=$1 AND i.target_id=$2").bind(receipt).bind(target).bind(state).execute(&f.pool).await.unwrap();
    }
    sqlx::query("UPDATE mobile_v9_dispatch_state d SET state='dispatch_failed',completed_rounds=3 FROM mobile_v9_task_items i JOIN source_execution_tasks e ON e.task_id=i.task_id WHERE d.item_id=i.id AND e.receipt_id=$1").bind(receipt).execute(&f.pool).await.unwrap();
    let detail = service
        .execute(&f.actor, Auto::Detail(receipt))
        .await
        .unwrap();
    assert_eq!(detail["task"]["state"], "unknown");
    assert_eq!(detail["task"]["counts"]["sent"], 1);
    assert_eq!(detail["task"]["counts"]["unknown"], 1);
    sqlx::query("UPDATE mobile_v9_attempts a SET state='verified_not_sent' FROM mobile_v9_task_items i JOIN source_execution_tasks e ON e.task_id=i.task_id WHERE a.item_id=i.id AND e.receipt_id=$1 AND i.target_id=$2").bind(receipt).bind(f.targets[1]).execute(&f.pool).await.unwrap();
    let detail = service
        .execute(&f.actor, Auto::Detail(receipt))
        .await
        .unwrap();
    assert_eq!(detail["task"]["state"], "partially_succeeded");
    for t in &f.targets[..3] {
        sqlx::query("UPDATE mobile_v9_attempts a SET state='observed_sent' FROM mobile_v9_task_items i JOIN source_execution_tasks e ON e.task_id=i.task_id WHERE a.item_id=i.id AND e.receipt_id=$1 AND i.target_id=$2").bind(receipt).bind(t).execute(&f.pool).await.unwrap();
    }
    worker.aggregate().await.unwrap();
    let detail = service
        .execute(&f.actor, Auto::Detail(receipt))
        .await
        .unwrap();
    assert_eq!(detail["task"]["state"], "succeeded");
    assert_eq!(detail["task"]["counts"]["total"], 3);
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn automation_recovery_pages_are_durable_scoped_and_exclude_stopped() {
    let f = Fixture::new().await;
    f.configure(vec![f.targets[0]]).await;
    sqlx::query("DELETE FROM source_orchestration_recovery_jobs")
        .execute(&f.pool)
        .await
        .unwrap();
    let mut receipts = Vec::new();
    for n in 0..111 {
        let id = inbox(&f, &format!("waiting-{n}"), 1, "content.upsert").await;
        sqlx::query("UPDATE source_inbox SET state='waiting',next_attempt_at=now()+interval '1 day' WHERE receipt_id=$1").bind(id).execute(&f.pool).await.unwrap();
        sqlx::query("INSERT INTO source_content_versions(source_id,route_id,external_content_id,sequence,version,receipt_id,withdrawn) VALUES($1,$2,$3,1,'1',$4,false)").bind(f.source).bind(&f.route).bind(format!("waiting-{n}")).bind(id).execute(&f.pool).await.unwrap();
        receipts.push(id);
    }
    sqlx::query("INSERT INTO source_routing_waits(receipt_id,route_id,reason_code,wake_key,stopped) VALUES($1,$2,'message_stopped',$2,true)").bind(receipts[0]).bind(&f.route).execute(&f.pool).await.unwrap();
    sqlx::query("INSERT INTO source_orchestration_recovery_jobs(route_id) VALUES($1)")
        .bind(&f.route)
        .execute(&f.pool)
        .await
        .unwrap();
    RoutingRepository(f.pool.clone())
        .recover_orchestration()
        .await
        .unwrap();
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM source_inbox WHERE next_attempt_at<=now()")
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(count, 50);
    RoutingRepository(f.pool.clone())
        .recover_orchestration()
        .await
        .unwrap();
    RoutingRepository(f.pool.clone())
        .recover_orchestration()
        .await
        .unwrap();
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM source_inbox WHERE next_attempt_at<=now()")
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(count, 110);
    let complete: bool =
        sqlx::query_scalar("SELECT bool_and(complete) FROM source_orchestration_recovery_jobs")
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert!(complete);
    f.close().await;
}

async fn configure_official(f: &Fixture) -> (Uuid, Vec<Uuid>) {
    let workspace = Uuid::new_v4();
    let member = Uuid::new_v4();
    let corp = "source-corp";
    sqlx::query("INSERT INTO wecom_workspaces(id,name) VALUES($1,'Official')")
        .bind(workspace)
        .execute(&f.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO wecom_directory_state(singleton,corp_id,credential_scope) VALUES(true,$1,'scope')").bind(corp).execute(&f.pool).await.unwrap();
    sqlx::query("INSERT INTO wecom_members(id,userid,name,status,verification,checked_at) VALUES($1,'owner','Owner',1,'verified',now())").bind(member).execute(&f.pool).await.unwrap();
    sqlx::query("UPDATE accounts SET wecom_member_id=$2 WHERE id=$1")
        .bind(f.actor.id)
        .bind(member)
        .execute(&f.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO wecom_workspace_members VALUES($1,$2,'admin')")
        .bind(workspace)
        .bind(f.actor.id)
        .execute(&f.pool)
        .await
        .unwrap();
    let mut groups = Vec::new();
    for n in 0..4 {
        let id = Uuid::new_v4();
        let directory = Uuid::new_v4();
        let chat = format!("chat-{n}");
        sqlx::query("INSERT INTO wecom_asset_objects(id,kind,corp_id,key,data,checked_at,credential_generation,visibility,verification) VALUES($1,'groups',$2,$3,$4,now(),1,'visible','verified')").bind(directory).bind(corp).bind(&chat).bind(json!({"owner":"owner"})).execute(&f.pool).await.unwrap();
        sqlx::query("INSERT INTO wecom_groups(id,school_id,corp_id,chat_id,name,directory_group_id) VALUES($1,$2,$3,$4,$4,$5)").bind(id).bind(workspace).bind(corp).bind(chat).bind(directory).execute(&f.pool).await.unwrap();
        groups.push(id);
    }
    f.service()
        .execute(
            &f.actor,
            Command::Configure {
                route: f.route.clone(),
                input: ConfigurationInput {
                    revision: f.revision().await,
                    unit_id: f.unit,
                    group_id: f.group,
                    execution: Some(ExecutionConfig {
                        channel: Channel::Wecom,
                        workspace_id: workspace,
                        target_ids: groups[..3].to_vec(),
                    }),
                },
            },
        )
        .await
        .unwrap();
    (workspace, groups)
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn automation_official_stop_persists_existing_executor_operation_and_preserves_unknown() {
    use wekeeper_api::source_access::{
        domain::orchestration::{Command as Auto, Control},
        north::local::orchestration::Service as AutoService,
        south::adapter::postgres::orchestration::Repository,
    };
    let f = Fixture::new().await;
    configure_official(&f).await;
    f.lifecycle(LifecycleAction::Enable).await;
    let id = inbox(&f, "official-stop", 1, "content.upsert").await;
    RoutingRepository(f.pool.clone()).run_once().await.unwrap();
    sqlx::query("UPDATE broadcast_batches b SET state='created',msgid='existing-external-message' FROM source_execution_tasks e WHERE b.task_id=e.task_id AND e.receipt_id=$1 AND b.ordinal=0").bind(id).execute(&f.pool).await.unwrap();
    sqlx::query("UPDATE broadcast_batches b SET state='unknown',invocation_id=$2 FROM source_execution_tasks e WHERE b.task_id=e.task_id AND e.receipt_id=$1 AND b.ordinal=1").bind(id).bind(Uuid::new_v4()).execute(&f.pool).await.unwrap();
    let service = AutoService(Repository(f.pool.clone()));
    service
        .execute(
            &f.actor,
            Auto::Stop(
                id,
                Control {
                    expected_revision: 1,
                    idempotency_key: Uuid::new_v4(),
                    reason: "停止后续提交".into(),
                    target_ids: vec![],
                },
            ),
        )
        .await
        .unwrap();
    let states:Vec<String>=sqlx::query_scalar("SELECT b.state FROM broadcast_batches b JOIN source_execution_tasks e ON e.task_id=b.task_id WHERE e.receipt_id=$1 ORDER BY ordinal").bind(id).fetch_all(&f.pool).await.unwrap();
    assert_eq!(states, vec!["created", "unknown", "cancelled"]);
    let operations:i64=sqlx::query_scalar("SELECT count(*) FROM broadcast_operations o JOIN source_execution_tasks e ON e.task_id=o.task_id WHERE e.receipt_id=$1 AND o.operation='stop' AND o.batch_id IS NULL AND o.state='done'").bind(id).fetch_one(&f.pool).await.unwrap();
    assert_eq!(operations, 1);
    let detail = service.execute(&f.actor, Auto::Detail(id)).await.unwrap();
    assert_eq!(detail["task"]["stopped"], true);
    assert_eq!(detail["task"]["counts"]["unknown"], 1);
    assert_eq!(detail["task"]["counts"]["cancelled"], 1);
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn automatic_official_execution_needs_no_platform_confirmation_and_preserves_evidence() {
    use wekeeper_api::distribution::{
        domain::broadcast::{Observation, Outcome, Results},
        south::{adapter::postgres::broadcast::Repository as Broadcast, port::broadcast::Store},
    };
    use wekeeper_api::source_access::{
        domain::orchestration::Command as Auto,
        north::local::orchestration::Service as AutoService,
        south::adapter::postgres::orchestration::Repository,
    };
    let f = Fixture::new().await;
    let (workspace, groups) = configure_official(&f).await;
    sqlx::query("INSERT INTO official_channel_health(corp_id,generation,credential_scope,available,checked_at,last_success_at) VALUES('source-corp',1,'scope',true,now(),now())").execute(&f.pool).await.unwrap();
    f.lifecycle(LifecycleAction::Enable).await;
    let receipt = inbox(&f, "official-execution", 1, "content.upsert").await;
    let worker = RoutingRepository(f.pool.clone());
    worker.run_once().await.unwrap();
    let (task,): (Uuid,) = sqlx::query_as(
        "SELECT task_id FROM source_execution_tasks WHERE receipt_id=$1 AND channel='wecom'",
    )
    .bind(receipt)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    let (kind, confirmed): (String, Option<Uuid>) =
        sqlx::query_as("SELECT authorization_kind,confirmed_by FROM broadcast_tasks WHERE id=$1")
            .bind(task)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!((kind, confirmed), ("source_route".into(), None));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM mobile_v9_tasks")
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        0
    );
    let store = Broadcast(f.pool.clone());
    for (index, mut w) in store
        .work(Some(task))
        .await
        .unwrap()
        .into_iter()
        .enumerate()
    {
        assert!(store.claim(&w).await.unwrap());
        let msgid = format!("automatic-{}", w.id);
        store
            .finish(
                &w,
                Outcome::Created {
                    msgid: msgid.clone(),
                    failed: vec![],
                },
            )
            .await
            .unwrap();
        w.msgid = Some(msgid);
        w.operation = "read".into();
        store
            .observed(
                &w,
                Ok(Results {
                    member_status: Some(2),
                    groups: vec![Observation {
                        chat_id: w.chats[0].clone(),
                        status: match index {
                            0 => 1,
                            1 => 99,
                            _ => 0,
                        },
                        send_time: Some(1700000000),
                    }],
                }),
            )
            .await
            .unwrap();
    }
    worker.aggregate().await.unwrap();
    let auto = AutoService(Repository(f.pool.clone()));
    let detail = auto.execute(&f.actor, Auto::Detail(receipt)).await.unwrap();
    assert_eq!(detail["execution"]["platform_interaction"], "none");
    assert_eq!(
        detail["execution"]["external_confirmation"],
        "member_required"
    );
    assert_eq!(detail["task"]["counts"]["sent"], 1);
    assert_eq!(detail["task"]["counts"]["unknown"], 1);
    let facts = auto
        .execute(&f.actor, Auto::Observations(receipt, 0))
        .await
        .unwrap();
    assert!(
        facts["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["evidence_type"] == "official_task_created"
                && v["normalized_result"] == "pending")
    );
    let targets = auto
        .execute(&f.actor, Auto::Targets(receipt))
        .await
        .unwrap();
    assert_eq!(targets["items"].as_array().unwrap().len(), 3);
    let attempts = auto
        .execute(&f.actor, Auto::Attempts(receipt, groups[0]))
        .await
        .unwrap();
    assert_eq!(attempts["items"][0]["member_status"], 2);
    let mut outsider = f.actor.clone();
    outsider.id = Uuid::new_v4();
    outsider.platform_admin = false;
    assert!(
        auto.execute(&outsider, Auto::Observations(receipt, 0))
            .await
            .is_err()
    );
    let stored = store
        .rows(
            workspace,
            task,
            "observations",
            wekeeper_api::business::Page::default(),
        )
        .await
        .unwrap();
    assert!(!stored.is_empty());
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn automatic_mobile_expiration_precedes_lost_candidate_resources() {
    use wekeeper_api::distribution::south::adapter::postgres::shared_send_v2::Repository as Mobile;
    for remove_binding in [false, true] {
        let f = Fixture::new().await;
        f.configure(vec![f.targets[0]]).await;
        f.lifecycle(LifecycleAction::Enable).await;
        let receipt = inbox(&f, "expired-resource", 1, "content.upsert").await;
        RoutingRepository(f.pool.clone()).run_once().await.unwrap();
        let task: Uuid =
            sqlx::query_scalar("SELECT task_id FROM source_execution_tasks WHERE receipt_id=$1")
                .bind(receipt)
                .fetch_one(&f.pool)
                .await
                .unwrap();
        if remove_binding {
            sqlx::query("DELETE FROM mobile_device_target_bindings")
                .execute(&f.pool)
                .await
                .unwrap();
        } else {
            sqlx::query("UPDATE execution_devices SET enabled=false")
                .execute(&f.pool)
                .await
                .unwrap();
        }
        sqlx::query("UPDATE mobile_v9_tasks SET confirmed_until=clock_timestamp()-interval '1 second' WHERE id=$1")
            .bind(task).execute(&f.pool).await.unwrap();
        for _ in 0..3 {
            // Even an already selected or explicitly woken item must retain its terminal state.
            sqlx::query("UPDATE mobile_v9_dispatch_state SET next_retry_at=clock_timestamp()-interval '1 second'")
                .execute(&f.pool).await.unwrap();
            Mobile(f.pool.clone()).reconcile_dispatch().await.unwrap();
            let result: Value = sqlx::query_scalar("SELECT to_jsonb(ds) FROM mobile_v9_dispatch_state ds JOIN mobile_v9_task_items i ON i.id=ds.item_id WHERE i.task_id=$1")
                .bind(task).fetch_one(&f.pool).await.unwrap();
            assert_eq!(result["state"], "confirmation_expired");
            assert!(!result["finished_at"].is_null());
        }
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM mobile_execution_grants")
                .fetch_one(&f.pool)
                .await
                .unwrap(),
            0
        );
        f.close().await;
    }
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn automatic_mobile_obsolete_candidate_scope_stops_without_waiting_for_expiry() {
    use wekeeper_api::distribution::south::adapter::postgres::shared_send_v2::Repository as Mobile;
    let f = Fixture::new().await;
    f.configure(vec![f.targets[0]]).await;
    f.lifecycle(LifecycleAction::Enable).await;
    inbox(&f, "obsolete-resource", 1, "content.upsert").await;
    RoutingRepository(f.pool.clone()).run_once().await.unwrap();
    sqlx::query("DELETE FROM mobile_device_target_bindings")
        .execute(&f.pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE mobile_v9_dispatch_state SET next_retry_at=clock_timestamp()-interval '1 second'",
    )
    .execute(&f.pool)
    .await
    .unwrap();
    Mobile(f.pool.clone()).reconcile_dispatch().await.unwrap();
    let result: Value = sqlx::query_scalar("SELECT to_jsonb(ds) FROM mobile_v9_dispatch_state ds")
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(result["state"], "stopped");
    assert_eq!(result["reason"], "plan_dependency_changed");
    assert!(result["next_retry_at"].is_null());
    assert!(!result["finished_at"].is_null());
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn automatic_official_completed_batches_do_not_starve_creates_or_unresolved_reads() {
    use wekeeper_api::distribution::south::{
        adapter::postgres::broadcast::Repository as Broadcast, port::broadcast::Store,
    };
    let f = Fixture::new().await;
    configure_official(&f).await;
    f.lifecycle(LifecycleAction::Enable).await;
    let worker = RoutingRepository(f.pool.clone());
    for n in 0..7 {
        inbox(&f, &format!("old-official-{n}"), 1, "content.upsert").await;
    }
    worker.run_once().await.unwrap();
    sqlx::query("UPDATE broadcast_batches SET state='created',msgid='test-'||id::text,next_sync_at=clock_timestamp()-interval '1 second',member_status=2")
        .execute(&f.pool).await.unwrap();
    sqlx::query("UPDATE broadcast_targets SET official_status=1")
        .execute(&f.pool)
        .await
        .unwrap();
    inbox(&f, "new-official", 1, "content.upsert").await;
    worker.run_once().await.unwrap();
    let store = Broadcast(f.pool.clone());
    let work = store.work(None).await.unwrap();
    assert_eq!(work.len(), 3);
    assert!(work.iter().all(|w| w.operation == "create"));

    // Reclassify the history as pending, unknown and missing results.
    // Every unresolved kind must retain a result-read opportunity.
    sqlx::query("UPDATE broadcast_targets x SET official_status=CASE b.ordinal WHEN 0 THEN 0 WHEN 1 THEN 99 ELSE NULL END FROM broadcast_batches b WHERE b.id=x.batch_id AND b.msgid IS NOT NULL")
        .execute(&f.pool).await.unwrap();
    let work = store.work(None).await.unwrap();
    assert_eq!(work.len(), 20);
    assert_eq!(work.iter().filter(|w| w.operation == "create").count(), 3);
    assert_eq!(work.iter().filter(|w| w.operation == "read").count(), 17);
    for n in 0..6 {
        inbox(&f, &format!("new-more-{n}"), 1, "content.upsert").await;
    }
    worker.run_once().await.unwrap();
    let work = store.work(None).await.unwrap();
    assert_eq!(work.len(), 20);
    assert_eq!(work.iter().filter(|w| w.operation == "create").count(), 10);
    assert_eq!(work.iter().filter(|w| w.operation == "read").count(), 10);
    assert_eq!(work[0].operation, "create");
    assert_eq!(work[1].operation, "read");
    f.close().await;
}
