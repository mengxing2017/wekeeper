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
