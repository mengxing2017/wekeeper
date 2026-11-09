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
        let url = std::env::var("WEKEEPER_TEST_DATABASE_URL")
            .expect("explicit isolated database required");
        let base = PgPool::connect(&url).await.unwrap();
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
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM source_routing_decisions WHERE receipt_id=$1")
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
    // A later configuration never rewrites a prior immutable decision.
    f.configure(vec![f.targets[3]]).await;
    match f.service().resolve(receipt).await.unwrap() {
        Resolution::Ready { snapshot } => assert_eq!(snapshot.targets, a.targets),
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
    f.lifecycle(LifecycleAction::Enable).await;
    let receipt = inbox(&f, "official", 1, "content.upsert").await;
    let worker = RoutingRepository(f.pool.clone());
    worker.run_once().await.unwrap();
    let detail:Value=sqlx::query_scalar("SELECT jsonb_build_object('kind',t.authorization_kind,'human_confirmation',t.confirmed_by,'count',count(x.id)) FROM source_execution_tasks l JOIN broadcast_tasks t ON t.id=l.task_id JOIN broadcast_targets x ON x.task_id=t.id WHERE l.receipt_id=$1 GROUP BY t.id").bind(receipt).fetch_one(&f.pool).await.unwrap();
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
async fn real_policy_recheck_keeps_identity_history_and_fails_closed() {
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
    assert_eq!(
        receipt_value(&f, receipt).await["reason_code"],
        "keyword_blocked"
    );
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
    assert_eq!(sqlx::query_scalar::<_,String>("SELECT outcome FROM keyword_check_results WHERE receipt_id=$1 ORDER BY created_at LIMIT 1").bind(receipt).fetch_one(&f.pool).await.unwrap(), "blocked");
    let operator_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO accounts(id,username,display_name) VALUES($1,'receipt-operator','Operator')",
    )
    .bind(operator_id)
    .execute(&f.pool)
    .await
    .unwrap();
    let mut operator = f.actor.clone();
    operator.id = operator_id;
    operator.platform_admin = false;
    let read = || P::Detail {
        route: f.route.clone(),
        receipt,
    };
    assert_eq!(
        service.execute(&operator, read()).await.unwrap_err().0,
        "forbidden"
    );
    sqlx::query(
        "INSERT INTO source_route_managers(route_id,account_id,granted_by) VALUES($1,$2,$3)",
    )
    .bind(&f.route)
    .bind(operator_id)
    .bind(f.actor.id)
    .execute(&f.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO mobile_workspace_members(workspace_id,account_id,role) VALUES($1,$2,'admin')",
    )
    .bind(f.workspace)
    .bind(operator_id)
    .execute(&f.pool)
    .await
    .unwrap();
    assert_eq!(
        service.execute(&operator, read()).await.unwrap()["can_recheck"],
        false
    );
    let denied = P::Recheck {
        route: f.route.clone(),
        receipt,
        input: RecheckInput {
            revision: detail["revision"].as_i64().unwrap(),
            reason: "正常理由".into(),
        },
    };
    assert_eq!(
        service.execute(&operator, denied).await.unwrap_err().0,
        "forbidden"
    );
    sqlx::query("UPDATE source_route_managers SET can_recheck=true WHERE account_id=$1")
        .bind(operator_id)
        .execute(&f.pool)
        .await
        .unwrap();
    assert_eq!(
        service.execute(&operator, read()).await.unwrap()["can_recheck"],
        true
    );
    sqlx::query("DELETE FROM mobile_workspace_members WHERE account_id=$1")
        .bind(operator_id)
        .execute(&f.pool)
        .await
        .unwrap();
    assert_eq!(
        service.execute(&operator, read()).await.unwrap_err().0,
        "forbidden"
    );
    let recheck = |revision| P::Recheck {
        route: f.route.clone(),
        receipt,
        input: RecheckInput {
            revision,
            reason: "复核原内容".into(),
        },
    };
    assert_eq!(
        service.execute(&f.actor, recheck(-1)).await.unwrap_err().0,
        "invalid_request"
    );
    service
        .execute(&f.actor, recheck(detail["revision"].as_i64().unwrap()))
        .await
        .unwrap();
    // A second operator using a stale revision cannot enqueue another plan.
    assert_eq!(
        service
            .execute(&f.actor, recheck(detail["revision"].as_i64().unwrap()))
            .await
            .unwrap_err()
            .0,
        "revision_conflict"
    );
    worker.run_once().await.unwrap();
    assert_eq!(receipt_value(&f, receipt).await["status"], "rejected");
    let plans: i64 = sqlx::query_scalar("SELECT count(*) FROM source_recheck_requests")
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(plans, 0);
    // Removing the persisted policy is a storage failure, never a fabricated empty pass.
    sqlx::query("ALTER TABLE keyword_scope_heads RENAME COLUMN version_id TO unavailable_version")
        .execute(&f.pool)
        .await
        .unwrap();
    let revision = receipt_value(&f, receipt).await["processing_revision"]
        .as_i64()
        .unwrap();
    service.execute(&f.actor, recheck(revision)).await.unwrap();
    worker.run_once().await.unwrap();
    assert_eq!(receipt_value(&f, receipt).await["status"], "received");
    assert_eq!(
        receipt_value(&f, receipt).await["processing_stage"],
        "processing_retry"
    );
    let tasks: i64 = sqlx::query_scalar("SELECT count(*) FROM source_execution_tasks")
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(tasks, 0);
    sqlx::query("ALTER TABLE keyword_scope_heads RENAME COLUMN unavailable_version TO version_id")
        .execute(&f.pool)
        .await
        .unwrap();
    publish_keywords(&f, "global", &[]).await;
    // Recovery after a new process requires no source resubmission.
    RoutingRepository(f.pool.clone()).run_once().await.unwrap();
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
    assert_eq!(detail["status"], "applied");
    assert_eq!(detail["processing_stage"], "task_created");
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM keyword_check_results WHERE receipt_id=$1"
        )
        .bind(receipt)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        4
    );
    assert!(
        detail["history"]
            .as_array()
            .unwrap()
            .iter()
            .any(|h| h["reason_code"] == "keyword_blocked")
    );
    let tasks: i64 =
        sqlx::query_scalar("SELECT count(*) FROM source_execution_tasks WHERE receipt_id=$1")
            .bind(receipt)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(tasks, 1);
    // A rule revision invalidates old qualification immediately, before the next worker tick.
    publish_keywords(&f, "global", &["正文"]).await;
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
        "content_check_pending"
    );
    tx.rollback().await.unwrap();
    worker.run_once().await.unwrap();
    assert_eq!(receipt_value(&f, receipt).await["status"], "rejected");
    let newer = inbox(&f, "recheck", 2, "content.upsert").await;
    worker.run_once().await.unwrap();
    assert_eq!(
        receipt_value(&f, newer).await["reason_code"],
        "keyword_blocked"
    );
    let revision = receipt_value(&f, receipt).await["processing_revision"]
        .as_i64()
        .unwrap();
    assert_eq!(
        service
            .execute(&f.actor, recheck(revision))
            .await
            .unwrap_err()
            .0,
        "content_version_superseded"
    );
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn policy_revision_recovery_uses_persistent_bounded_pages() {
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
    assert_eq!(first, 50);
    RoutingRepository(f.pool.clone()).run_once().await.unwrap();
    let all: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM source_content_versions WHERE check_state='blocked'",
    )
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(all, 80);
    let complete: bool =
        sqlx::query_scalar("SELECT bool_and(complete) FROM keyword_recheck_batches")
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert!(complete);
    f.close().await;
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
async fn keyword_scopes_final_text_recovery_and_immutable_evidence() {
    let f = Fixture::new().await;
    f.configure(vec![f.targets[0]]).await;
    f.lifecycle(LifecycleAction::Enable).await;
    let worker = RoutingRepository(f.pool.clone());
    let r = inbox(&f, "final-url", 1, "content.upsert").await;
    worker.run_once().await.unwrap();
    let task: Uuid =
        sqlx::query_scalar("SELECT task_id FROM source_execution_tasks WHERE receipt_id=$1")
            .bind(r)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM keyword_check_results WHERE receipt_id=$1"
        )
        .bind(r)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        3
    );
    // First restriction on a formerly empty workspace invalidates its old qualification immediately.
    publish_keywords(&f, &format!("mobile:{}", f.workspace), &["EXAMPLE.COM"]).await;
    let mut tx = f.pool.begin().await.unwrap();
    assert_eq!(
        wekeeper_api::source_access::south::adapter::postgres::routing::guard_submission(
            &mut tx,
            r,
            f.targets[0]
        )
        .await
        .unwrap_err()
        .0,
        "content_check_pending"
    );
    tx.rollback().await.unwrap();
    worker.run_once().await.unwrap();
    assert_eq!(
        receipt_value(&f, r).await["reason_code"],
        "final_keyword_blocked"
    );
    assert_eq!(receipt_value(&f, r).await["sending"]["failed"], 1);
    let check: Uuid = sqlx::query_scalar(
        "SELECT id FROM keyword_check_results WHERE receipt_id=$1 AND outcome='blocked'",
    )
    .bind(r)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert!(
        sqlx::query("UPDATE keyword_check_results SET outcome='passed' WHERE id=$1")
            .bind(check)
            .execute(&f.pool)
            .await
            .is_err()
    );
    assert!(
        !sqlx::query_scalar::<_, bool>("SELECT stopped FROM mobile_v9_tasks WHERE id=$1")
            .bind(task)
            .fetch_one(&f.pool)
            .await
            .unwrap()
    );
    // Loosening resumes the same task; no duplicate intent or task is created.
    publish_keywords(&f, &format!("mobile:{}", f.workspace), &[]).await;
    worker.run_once().await.unwrap();
    assert_eq!(
        receipt_value(&f, r).await["processing_stage"],
        "task_created"
    );
    assert_eq!(
        sqlx::query_scalar::<_, Uuid>(
            "SELECT task_id FROM source_execution_tasks WHERE receipt_id=$1"
        )
        .bind(r)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        task
    );
    let mut tx = f.pool.begin().await.unwrap();
    wekeeper_api::source_access::south::adapter::postgres::routing::guard_submission(
        &mut tx,
        r,
        f.targets[0],
    )
    .await
    .unwrap();
    tx.rollback().await.unwrap();
    // Upstream restrictions still apply when a workspace has no local words.
    publish_keywords(&f, &format!("unit:{}", f.unit), &["ＳＴＲＡＳＳＥ"]).await;
    let raw = inbox(&f, "raw-fullfold", 1, "content.upsert").await;
    change_title(&f, raw, "Straße").await;
    worker.run_once().await.unwrap();
    assert_eq!(
        receipt_value(&f, raw).await["reason_code"],
        "keyword_blocked"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM source_routing_decisions WHERE receipt_id=$1"
        )
        .bind(raw)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM source_execution_tasks WHERE receipt_id=$1"
        )
        .bind(raw)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        0
    );
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
    assert_eq!(scopes["items"].as_array().unwrap().len(), 1);
    assert_eq!(scopes["items"][0]["id"], scope);
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
    service.execute(&actor, publication()).await.unwrap();
    assert_eq!(
        service.execute(&actor, publication()).await.unwrap_err().0,
        "revision_conflict"
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
async fn keyword_scope_union_workspace_original_and_real_composition_boundaries() {
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
    publish_keywords(&f, &format!("wecom:{}", f.workspace), &["正文"]).await;
    let free = inbox(&f, "other-channel", 1, "content.upsert").await;
    worker.run_once().await.unwrap();
    assert_eq!(
        receipt_value(&f, free).await["processing_stage"],
        "task_created"
    );
    // Original fields never cross-match; the actual newline in final composition can match.
    publish_keywords(&f, &format!("route:{}", f.route), &["通知\n正文"]).await;
    let composed = inbox(&f, "composed-boundary", 1, "content.upsert").await;
    worker.run_once().await.unwrap();
    assert_eq!(receipt_value(&f, composed).await["status"], "applied");
    assert_eq!(
        receipt_value(&f, composed).await["reason_code"],
        "final_keyword_blocked"
    );
    assert_eq!(sqlx::query_scalar::<_,String>("SELECT outcome FROM keyword_check_results WHERE receipt_id=$1 AND stage='original_common'").bind(composed).fetch_one(&f.pool).await.unwrap(),"passed");
    publish_keywords(&f, &format!("route:{}", f.route), &[]).await;
    for (n, scope) in [
        "global".to_owned(),
        format!("source:{}", f.source),
        format!("unit:{}", f.unit),
        format!("route:{}", f.route),
    ]
    .iter()
    .enumerate()
    {
        publish_keywords(&f, scope, &["正文"]).await;
        let r = inbox(&f, &format!("union-{n}"), 1, "content.upsert").await;
        worker.run_once().await.unwrap();
        assert_eq!(receipt_value(&f, r).await["reason_code"], "keyword_blocked");
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM source_execution_tasks WHERE receipt_id=$1"
            )
            .bind(r)
            .fetch_one(&f.pool)
            .await
            .unwrap(),
            0
        );
        publish_keywords(&f, scope, &[]).await;
    }
    publish_keywords(&f, &format!("mobile:{}", f.workspace), &["正文"]).await;
    let local = inbox(&f, "workspace-original", 1, "content.upsert").await;
    worker.run_once().await.unwrap();
    assert_eq!(receipt_value(&f, local).await["status"], "applied");
    assert_eq!(
        receipt_value(&f, local).await["reason_code"],
        "workspace_keyword_blocked"
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
