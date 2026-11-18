//! Synthetic records in a unique schema, never production acceptance evidence.
use serde_json::{Value, json};
use sqlx::{PgPool, postgres::PgPoolOptions};
use uuid::Uuid;
use wekeeper_api::{
    business::Actor,
    database,
    distribution::{
        domain::shared_mobile::shared_send_v2::*,
        south::{adapter::postgres::shared_send_v2::Repository, port::shared_send_v2::*},
    },
};

struct Fixture {
    pool: PgPool,
    base: PgPool,
    schema: String,
    actor: Actor,
    device: Uuid,
    targets: Vec<Uuid>,
    release: Uuid,
    acceptance: Uuid,
}
impl Fixture {
    async fn new() -> Self {
        Self::with_adapter(true).await
    }
    async fn with_adapter(seed_adapter: bool) -> Self {
        let url = std::env::var("WEKEEPER_TEST_DATABASE_URL")
            .expect("explicit isolated database required");
        let base = PgPool::connect(&url).await.unwrap();
        let schema = format!("shared_v9_{}", Uuid::new_v4().simple());
        sqlx::query(&format!("CREATE SCHEMA {schema}"))
            .execute(&base)
            .await
            .unwrap();
        let search = format!("SET search_path TO {schema},public");
        let pool = PgPoolOptions::new()
            .max_connections(4)
            .after_connect(move |c, _| {
                let search = search.clone();
                Box::pin(async move {
                    sqlx::query(&search).execute(c).await?;
                    Ok(())
                })
            })
            .connect(&url)
            .await
            .unwrap();
        database::migrate(&pool).await.unwrap();
        let actor_id = Uuid::new_v4();
        let device = Uuid::new_v4();
        sqlx::query("INSERT INTO accounts(id,username,display_name,platform_admin) VALUES($1,'v9-admin','Admin',true)").bind(actor_id).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO execution_devices(id,name,enabled,credential_generation) VALUES($1,'Offline phone',true,1)").bind(device).execute(&pool).await.unwrap();
        let targets = vec![Uuid::new_v4(), Uuid::new_v4()];
        for target in &targets {
            sqlx::query("INSERT INTO mobile_targets(client_kind,id,search_text,created_by,updated_by) VALUES('wecom',$1,'测试群',$2,$2)").bind(target).bind(actor_id).execute(&pool).await.unwrap();
            sqlx::query("INSERT INTO mobile_device_target_bindings(id,device_id,target_id,created_by,updated_by) VALUES($1,$2,$3,$4,$4)").bind(Uuid::new_v4()).bind(device).bind(target).bind(actor_id).execute(&pool).await.unwrap();
        }
        let release = Uuid::new_v4();
        let acceptance = Uuid::new_v4();
        if seed_adapter {
            Self::seed_adapter(&pool, actor_id, device, release, acceptance).await;
        }
        let actor=serde_json::from_value(json!({"id":actor_id,"username":"v9-admin","display_name":"Admin","platform_admin":true,"wecom_authenticated":false})).unwrap();
        Self {
            pool,
            base,
            schema,
            actor,
            device,
            targets,
            release,
            acceptance,
        }
    }
    async fn seed_adapter(
        pool: &PgPool,
        actor_id: Uuid,
        device: Uuid,
        release: Uuid,
        acceptance: Uuid,
    ) {
        Self::seed_runtime_configuration(pool, release).await;
        let flow = Uuid::new_v4();
        let artifact = Uuid::new_v4();
        let baseline = Uuid::new_v4();
        let fixture: Value = serde_json::from_str(include_str!(
            "../../api-contract/fixtures/local-group-text-flow.json"
        ))
        .unwrap();
        sqlx::query("INSERT INTO mobile_v9_flows(id,name,flow,digest,created_by) VALUES($1,'Synthetic',$2,$3,$4)").bind(flow).bind(&fixture["flow"]).bind(fixture["flow_digest"].as_str()).bind(actor_id).execute(pool).await.unwrap();
        sqlx::query("INSERT INTO mobile_v9_artifacts(id,kind,name,sha256,content,created_by) VALUES($1,'apk','Synthetic',repeat('a',64),$2,$3)").bind(artifact).bind(vec![1u8]).bind(actor_id).execute(pool).await.unwrap();
        sqlx::query("INSERT INTO mobile_v9_baselines(id,flow_id,device_id,samples,evidence_ids,min_interval_ms,recorded_by) VALUES($1,$2,$3,'[]',ARRAY[$4]::uuid[],1000,$5)").bind(baseline).bind(flow).bind(device).bind(artifact).bind(actor_id).execute(pool).await.unwrap();
        sqlx::query("INSERT INTO mobile_v9_adapter_releases(id,name,version,flow_id,flow,content_digest,baseline_id,evidence_ids,created_by) VALUES($1,'Synthetic','1',$2,$3,$4,$5,ARRAY[$6]::uuid[],$7)").bind(release).bind(flow).bind(&fixture["flow"]).bind(fixture["flow_digest"].as_str()).bind(baseline).bind(artifact).bind(actor_id).execute(pool).await.unwrap();
        sqlx::query("INSERT INTO mobile_v9_acceptance_runs(id,release_id,stage,apk_id,evidence_ids,validation_ids,cases,soak_hours,soak_passed,historical_debt_closed,reviewer_id,reason) VALUES($1,$2,'test_send',$3,ARRAY[$3]::uuid[],'{}','{}',0,false,false,$4,'Synthetic test only')").bind(acceptance).bind(release).bind(artifact).bind(actor_id).execute(pool).await.unwrap();
    }
    async fn seed_runtime_configuration(pool: &PgPool, id: Uuid) {
        use wekeeper_api::distribution::{
            domain::shared_mobile::runtime_configuration::RuntimeConfiguration,
            south::adapter::postgres::shared_send_v2::configuration,
        };
        let fixture: Value = serde_json::from_str(include_str!(
            "../../api-contract/fixtures/local-group-text-flow.json"
        ))
        .unwrap();
        configuration::install(
            pool,
            &RuntimeConfiguration {
                id,
                version: "Synthetic test only".into(),
                capability: "wecom.group_text".into(),
                flow: fixture["flow"].clone(),
                flow_digest: fixture["flow_digest"].as_str().unwrap().into(),
            },
        )
        .await
        .unwrap();
    }
    async fn close(self) {
        self.pool.close().await;
        sqlx::query(&format!("DROP SCHEMA {} CASCADE", self.schema))
            .execute(&self.base)
            .await
            .unwrap();
        self.base.close().await;
    }
    fn repo(&self) -> Repository {
        Repository(self.pool.clone())
    }
    async fn activate(&self, key: &str) -> wekeeper_api::business::Result<Value> {
        self.repo()
            .platform_write(
                &self.actor,
                key,
                PlatformCommand::Manage(ManagementCommand::Activations(ActivationInput {
                    release_id: self.release,
                    expected_revision: 0,
                    reason: "Synthetic test".into(),
                })),
            )
            .await
    }
}

struct ObjectLimits;
impl ObjectLimits {
    fn value() -> Value {
        let l = json!({"interval_seconds":60,"window_seconds":3600,"max_calls":10,"daily_max_calls":20});
        json!({"total":l,"device":l,"target":l})
    }
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn concurrent_activation_has_one_winner_and_immutable_audit() {
    let f = Fixture::new().await;
    let (a, b) = tokio::join!(f.activate("left"), f.activate("right"));
    assert_ne!(a.is_ok(), b.is_ok());
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM mobile_v9_activation_events")
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    assert!(
        sqlx::query("DELETE FROM mobile_v9_activation_events")
            .execute(&f.pool)
            .await
            .is_err()
    );
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn business_configuration_does_not_require_technical_delivery() {
    use wekeeper_api::distribution::domain::shared_mobile::business_scheme::*;
    let f = Fixture::with_adapter(false).await;
    let r = f.repo();
    let options = r.scheme_read(&f.actor, None).await.unwrap();
    assert!(options.get("delivery").is_none());
    assert_eq!(options["source"], "builtin");
    let result = r
        .preview_task(
            &f.actor,
            "blocked-preview",
            TaskPreviewInput {
                client_kind: wekeeper_api::distribution::domain::shared_mobile::MobileClient::Wecom,
                business_unit_id: None,
                draft_id: None,
                expected_revision: None,
                target_ids: vec![f.targets[0]],
                message: "保存但不发送".into(),
                between_targets_seconds: 30,
                selections: vec![],
            },
        )
        .await
        .unwrap();
    assert!(!result["draft"]["id"].is_null());
    assert_eq!(
        result["preview"]["plan"]["sending_status"]["can_confirm"],
        true
    );
    assert_eq!(
        result["preview"]["plan"]["items"][0]["blocker"],
        Value::Null
    );
    assert!(
        result["preview"]["plan"]["items"][0]["selected"]
            .get("scope_id")
            .is_none()
    );
    let p = &result["preview"];
    let task = r
        .task_write(
            &f.actor,
            "binding-only-confirm",
            TaskCommand::Confirm(Confirmation {
                preview_id: Uuid::parse_str(p["id"].as_str().unwrap()).unwrap(),
                preview_digest: p["digest"].as_str().unwrap().into(),
                confirmed: true,
            }),
        )
        .await;
    let task = task.unwrap();
    // Acceptance is independent of phone readiness; the first round still waits five minutes.
    assert_eq!(task["items"][0]["dispatch"]["state"], "waiting_resources");
    assert_eq!(task["items"][0]["dispatch"]["completed_rounds"], 0);
    assert_eq!(task["items"][0]["attempts"][0]["state"], "invalidated");
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM information_schema.tables WHERE table_schema=current_schema() AND table_name='mobile_v9_scope_grants'")
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        0
    );
    let mut input: BusinessSchemeInput = serde_json::from_str(include_str!(
        "../../api-contract/fixtures/mobile-business-scheme.json"
    ))
    .unwrap();
    save_policy(
        &r,
        &f.actor,
        "closed-scheme",
        input.clone().policy().unwrap(),
    )
    .await
    .unwrap();
    assert!(
        save_policy(
            &r,
            &f.actor,
            "stale-scheme",
            input.clone().policy().unwrap()
        )
        .await
        .is_err()
    );
    input.expected_revision = 1;
    input.enabled = true;
    input.allowed_windows = serde_json::from_value(
        json!([{"weekdays":[1,2,3,4,5,6,7],"start_minute":0,"end_minute":1440}]),
    )
    .unwrap();
    save_policy(&r, &f.actor, "open-no-delivery", input.policy().unwrap())
        .await
        .unwrap();
    let token = "wk_device_device-owned-regression";
    sqlx::query("INSERT INTO device_credentials(token_hash,device_id,generation,expires_at) VALUES($1,$2,1,clock_timestamp()+interval '1 hour')").bind(wekeeper_api::business::hash(token)).bind(f.device).execute(&f.pool).await.unwrap();
    let c = r.credential(token).await.unwrap();
    let epoch = Uuid::new_v4();
    sqlx::query("INSERT INTO device_control_sessions(device_id,connection_id,transport,credential_generation,last_heartbeat_at,runtime_epoch) VALUES($1,$2,'wss',1,clock_timestamp(),$3)").bind(f.device).bind(Uuid::new_v4()).bind(epoch).execute(&f.pool).await.unwrap();
    pool_expire_round(&f).await;
    pool_tick(&f).await;
    let current_task = pool_detail(&f, &task).await;
    let aid: Uuid =
        serde_json::from_value(pool_current(&current_task["items"][0])["id"].clone()).unwrap();
    let grant = pool_delivery(&f, &c, epoch, aid).await;
    assert_eq!(grant["state"], "granted", "{grant}");
    assert_eq!(grant["plan"]["execution"]["contract"], "device_owned/2");
    assert!(grant["plan"].get("flow").is_none());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM mobile_v9_invocations")
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        0
    );
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn optional_environment_observation_ignores_global_housekeeping_and_is_idempotent() {
    let f = Fixture::with_adapter(false).await;
    let token = "wk_device_environment-observation-test";
    sqlx::query("INSERT INTO device_credentials(token_hash,device_id,generation,expires_at) VALUES($1,$2,1,clock_timestamp()+interval '1 hour')")
        .bind(wekeeper_api::business::hash(token)).bind(f.device).execute(&f.pool).await.unwrap();
    let r = f.repo();
    let c = r.credential(token).await.unwrap();
    let report: EnvironmentObservation = serde_json::from_str(include_str!(
        "../../api-contract/fixtures/device-environment.json"
    ))
    .unwrap();
    let mut held = f.pool.begin().await.unwrap();
    sqlx::query("SELECT pg_advisory_xact_lock(78234610243)")
        .execute(&mut *held)
        .await
        .unwrap();
    let first = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        r.device_write(
            &c,
            "environment",
            DeviceCommand::Environment(report.clone()),
        ),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        first,
        r.device_write(&c, "environment", DeviceCommand::Environment(report))
            .await
            .unwrap()
    );
    held.rollback().await.unwrap();
    let context = r.device_read(&c).await.unwrap();
    assert!(context.get("delegation").is_none());
    sqlx::query("UPDATE device_credentials SET revoked_at=clock_timestamp() WHERE device_id=$1")
        .bind(f.device)
        .execute(&f.pool)
        .await
        .unwrap();
    assert!(r.credential(token).await.is_err());
    assert!(r.device_read(&c).await.is_err());
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn reconciliation_never_schedules_downstream_ui_checks() {
    let f = Fixture::new().await;
    let r = f.repo();
    let token = "wk_device_synthetic-check-isolation";
    sqlx::query("INSERT INTO device_credentials(token_hash,device_id,generation,expires_at) VALUES($1,$2,1,clock_timestamp()+interval '1 hour')").bind(wekeeper_api::business::hash(token)).bind(f.device).execute(&f.pool).await.unwrap();
    assert!(r.credential(token).await.is_ok());
    sqlx::query(&format!("CREATE FUNCTION reject_one_check() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.target_id='{}'::uuid THEN RAISE EXCEPTION 'synthetic check insert failure'; END IF; RETURN NEW; END $$",f.targets[0])).execute(&f.pool).await.unwrap();
    sqlx::query("CREATE TRIGGER reject_one_check BEFORE INSERT ON mobile_v9_send_inspections FOR EACH ROW EXECUTE FUNCTION reject_one_check()").execute(&f.pool).await.unwrap();
    let _ = r.task_read(&f.actor, None).await.unwrap();
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM mobile_v9_send_inspections WHERE target_id=$1 AND state='pending'",
    )
    .bind(f.targets[1])
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(
        count, 0,
        "platform must never schedule downstream UI checks"
    );
    sqlx::query("DROP TRIGGER reject_one_check ON mobile_v9_send_inspections")
        .execute(&f.pool)
        .await
        .unwrap();
    let _ = r.task_read(&f.actor, None).await.unwrap();
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM mobile_v9_send_inspections WHERE target_id=$1 AND state='pending'",
    )
    .bind(f.targets[0])
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(
        count, 0,
        "reconciliation must not create downstream UI checks"
    );
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn runtime_configuration_is_independent_and_invalid_update_does_not_replace_current() {
    use wekeeper_api::distribution::{
        domain::{
            shared_mobile::business_scheme::*,
            shared_mobile::runtime_configuration::RuntimeConfiguration,
        },
        south::adapter::postgres::shared_send_v2::configuration,
    };
    let f = Fixture::with_adapter(false).await;
    assert_eq!(
        configuration::status(&f.pool).await.unwrap()["issue"]["code"],
        "configuration_missing"
    );
    Fixture::seed_runtime_configuration(&f.pool, f.release).await;
    assert_eq!(
        configuration::status(&f.pool).await.unwrap()["available"],
        true
    );
    // Reinstall is idempotent, without a service account or acceptance record.
    Fixture::seed_runtime_configuration(&f.pool, f.release).await;
    for table in [
        "mobile_v9_adapter_releases",
        "mobile_v9_acceptance_runs",
        "mobile_v9_artifacts",
    ] {
        assert_eq!(
            sqlx::query_scalar::<_, i64>(&format!("SELECT count(*) FROM {table}"))
                .fetch_one(&f.pool)
                .await
                .unwrap(),
            0
        );
    }
    let fixture: Value = serde_json::from_str(include_str!(
        "../../api-contract/fixtures/local-group-text-flow.json"
    ))
    .unwrap();
    let mut bad = RuntimeConfiguration {
        id: Uuid::new_v4(),
        version: "bad".into(),
        capability: "wecom.group_text".into(),
        flow: fixture["flow"].clone(),
        flow_digest: "invalid".into(),
    };
    assert!(configuration::install(&f.pool, &bad).await.is_err());
    bad.id = f.release;
    bad.flow_digest = fixture["flow_digest"].as_str().unwrap().into();
    assert!(configuration::install(&f.pool, &bad).await.is_err());
    bad.version = "Synthetic test only".into();
    assert!(configuration::verify(&f.pool, &bad).await.is_ok());
    bad.id = Uuid::new_v4();
    assert!(configuration::verify(&f.pool, &bad).await.is_err());
    assert_eq!(
        configuration::status(&f.pool).await.unwrap()["configuration_id"],
        json!(f.release)
    );
    let preview = f
        .repo()
        .preview_task(
            &f.actor,
            "runtime-only-preview",
            TaskPreviewInput {
                client_kind: wekeeper_api::distribution::domain::shared_mobile::MobileClient::Wecom,
                business_unit_id: None,
                draft_id: None,
                expected_revision: None,
                target_ids: vec![f.targets[0]],
                message: "Synthetic no send".into(),
                between_targets_seconds: 30,
                selections: vec![],
            },
        )
        .await
        .unwrap();
    assert!(preview["preview"]["plan"]["items"][0]["blocker"].is_null());
    assert_eq!(
        preview["preview"]["plan"]["items"][0]["selected"]["execution"]["contract"],
        "device_owned/2"
    );
    assert_eq!(preview["preview"]["plan"]["configuration"], Value::Null);
    sqlx::query("UPDATE execution_devices SET enabled=false WHERE id=$1")
        .bind(f.device)
        .execute(&f.pool)
        .await
        .unwrap();
    let overview = f.repo().task_read(&f.actor, None).await.unwrap();
    assert!(overview.get("configuration").is_none());
    for row in overview["readiness"].as_array().unwrap() {
        assert!(row["configuration_issue"].is_null());
        assert!(
            row["blockers"]
                .as_array()
                .unwrap()
                .iter()
                .any(|b| b["code"] == "resource_unavailable")
        );
        assert!(
            !row["blockers"]
                .as_array()
                .unwrap()
                .iter()
                .any(|b| b["code"] == "shared_send_execution_configuration_invalid")
        );
    }
    f.close().await;
}

async fn pool_phone(f: &Fixture, name: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO execution_devices(id,name,enabled,credential_generation) VALUES($1,$2,true,1)",
    )
    .bind(id)
    .bind(name)
    .execute(&f.pool)
    .await
    .unwrap();
    for target in &f.targets {
        sqlx::query("INSERT INTO mobile_device_target_bindings(id,device_id,target_id,created_by,updated_by) VALUES($1,$2,$3,$4,$4)").bind(Uuid::new_v4()).bind(id).bind(target).bind(f.actor.id).execute(&f.pool).await.unwrap();
    }
    id
}
async fn pool_ready(
    f: &Fixture,
    device: Uuid,
) -> (
    wekeeper_api::distribution::domain::device_control::Credential,
    Uuid,
) {
    let token = format!("wk_device_{}", Uuid::new_v4());
    sqlx::query("INSERT INTO device_credentials(token_hash,device_id,generation,expires_at) VALUES($1,$2,1,clock_timestamp()+interval '1 hour')").bind(wekeeper_api::business::hash(&token)).bind(device).execute(&f.pool).await.unwrap();
    let r = f.repo();
    let c = r.credential(&token).await.unwrap();
    let epoch = Uuid::new_v4();
    sqlx::query("INSERT INTO device_control_sessions(device_id,connection_id,transport,credential_generation,last_heartbeat_at,runtime_epoch) VALUES($1,$2,'wss',1,clock_timestamp(),$3) ON CONFLICT(device_id) DO UPDATE SET connected=true,last_heartbeat_at=clock_timestamp(),runtime_epoch=$3").bind(device).bind(Uuid::new_v4()).bind(epoch).execute(&f.pool).await.unwrap();
    (c, epoch)
}
async fn pool_confirm(f: &Fixture, targets: Vec<Uuid>) -> Value {
    pool_confirm_keyed(f, targets, "pool").await
}
async fn pool_confirm_keyed(f: &Fixture, targets: Vec<Uuid>, key: &str) -> Value {
    use wekeeper_api::distribution::domain::shared_mobile::business_scheme::*;
    let r = f.repo();
    let p = r
        .preview_task(
            &f.actor,
            &format!("{key}-preview"),
            TaskPreviewInput {
                client_kind: wekeeper_api::distribution::domain::shared_mobile::MobileClient::Wecom,
                business_unit_id: None,
                draft_id: None,
                expected_revision: None,
                selections: targets
                    .iter()
                    .map(|t| Selection {
                        target_id: *t,
                        device_id: f.device,
                    })
                    .collect(),
                target_ids: targets,
                message: "自动调度通知".into(),
                between_targets_seconds: 1,
            },
        )
        .await
        .unwrap();
    r.task_write(
        &f.actor,
        &format!("{key}-confirm"),
        TaskCommand::Confirm(Confirmation {
            preview_id: serde_json::from_value(p["preview"]["id"].clone()).unwrap(),
            preview_digest: p["preview"]["digest"].as_str().unwrap().into(),
            confirmed: true,
        }),
    )
    .await
    .unwrap()
}
async fn pool_tick(f: &Fixture) {
    sqlx::query(
        "UPDATE mobile_v9_dispatch_state SET next_retry_at=clock_timestamp()-interval '1 second' WHERE state NOT IN ('completed','stopped','confirmation_expired','dispatch_failed')",
    )
    .execute(&f.pool)
    .await
    .unwrap();
    // Advisory locks span schemas, so another parallel fixture can make a
    // nonblocking scan skip this fixture. Wait for its due work to be evaluated.
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            f.repo().reconcile_dispatch().await.unwrap();
            let due: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_v9_dispatch_state WHERE next_retry_at<=clock_timestamp() AND state NOT IN ('completed','stopped','confirmation_expired','dispatch_failed'))")
                .fetch_one(&f.pool).await.unwrap();
            if !due {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("fixture dispatch work remained due after repeated scans");
}
// Advance persisted round time explicitly; pool_tick itself never advances time.
async fn pool_expire_round(f: &Fixture) {
    sqlx::query("UPDATE mobile_v9_dispatch_state SET round_started_at=clock_timestamp()-interval '5 minutes 1 second' WHERE state NOT IN ('completed','stopped','confirmation_expired','dispatch_failed')")
        .execute(&f.pool).await.unwrap();
    // Advancing five minutes also advances the task's business interval.
    pool_business_due(f).await;
    pool_tick(f).await;
}
async fn pool_business_due(f: &Fixture) {
    sqlx::query(
        "UPDATE mobile_v9_tasks SET next_eligible_at=clock_timestamp()-interval '1 second'",
    )
    .execute(&f.pool)
    .await
    .unwrap();
}
async fn pool_detail(f: &Fixture, t: &Value) -> Value {
    f.repo()
        .task_read(
            &f.actor,
            Some(serde_json::from_value(t["id"].clone()).unwrap()),
        )
        .await
        .unwrap()
}
fn pool_current(item: &Value) -> &Value {
    item["attempts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["current"] == true)
        .unwrap()
}
async fn pool_delivery(
    f: &Fixture,
    c: &wekeeper_api::distribution::domain::device_control::Credential,
    _epoch: Uuid,
    aid: Uuid,
) -> Value {
    let (connection, epoch): (Uuid, i64) = sqlx::query_as(
        "SELECT connection_id,epoch FROM device_control_sessions WHERE device_id=$1",
    )
    .bind(c.device_id)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    let deliveries = f
        .repo()
        .pending_deliveries(c, connection, epoch)
        .await
        .unwrap();
    deliveries
        .into_iter()
        .map(|v| v.delivery.execution.unwrap())
        .find(|v| v["attempt_id"] == json!(aid))
        .expect("dispatcher must prepare the selected attempt before delivery")
}

// Internal scheduler columns must not enter the strict public DispatchSnapshot.
#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn task_confirmation_and_read_exclude_internal_dispatch_cursor() {
    let f = Fixture::with_adapter(false).await;
    // An offline phone must not prevent task acceptance or require an extra opening step.
    let task = pool_confirm(&f, vec![f.targets[0]]).await;
    let dispatch = &task["items"][0]["dispatch"];
    assert_eq!(dispatch["state"], "waiting_resources");
    assert!(dispatch.get("candidate_cursor").is_none());
    serde_json::from_value::<DispatchSnapshot>(dispatch.clone()).unwrap();
    let item: Uuid = serde_json::from_value(task["items"][0]["id"].clone()).unwrap();
    sqlx::query("UPDATE mobile_v9_dispatch_state SET candidate_cursor=7 WHERE item_id=$1")
        .bind(item)
        .execute(&f.pool)
        .await
        .unwrap();
    let detail = pool_detail(&f, &task).await;
    assert_eq!(detail["items"][0]["dispatch"], *dispatch);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM mobile_v9_tasks")
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(count, 1, "confirmation must commit the accepted task");
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn offline_preference_dispatches_authorized_phone_and_never_expands_scope() {
    let f = Fixture::with_adapter(false).await;
    let b = pool_phone(&f, "B").await;
    let (_cb, _eb) = pool_ready(&f, b).await;
    let task = pool_confirm(&f, vec![f.targets[0]]).await;
    assert_eq!(
        pool_current(&task["items"][0])["plan"]["device_id"],
        json!(b)
    );
    assert_eq!(
        pool_current(&task["items"][0])["plan"]["target"]["search_query"],
        "测试群"
    );
    assert_eq!(
        task["items"][0]["dispatch"]["candidates"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        task["items"][0]["dispatch"]["assignments"][0]["source"],
        "automatic"
    );
    let outsider = pool_phone(&f, "C").await;
    pool_ready(&f, outsider).await;
    sqlx::query("UPDATE device_control_sessions SET connected=false,last_heartbeat_at=clock_timestamp()-interval '191 seconds' WHERE device_id=$1")
        .bind(b)
        .execute(&f.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE mobile_execution_grants SET receipt_deadline_at=clock_timestamp()-interval '1 second'").execute(&f.pool).await.unwrap();
    pool_tick(&f).await;
    let detail = pool_detail(&f, &task).await;
    assert_eq!(detail["items"][0]["dispatch"]["state"], "waiting_resources");
    assert_ne!(
        pool_current(&detail["items"][0])["plan"]["device_id"],
        json!(outsider)
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM mobile_v9_attempts")
        .fetch_one(&f.pool)
        .await
        .unwrap();
    pool_tick(&f).await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM mobile_v9_attempts")
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        count
    );
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn unknown_waits_only_until_result_deadline_then_switches_phone() {
    let f = Fixture::with_adapter(false).await;
    let b = pool_phone(&f, "B").await;
    let (ca, ea) = pool_ready(&f, f.device).await;
    let (_cb, _eb) = pool_ready(&f, b).await;
    let task = pool_confirm(&f, vec![f.targets[0]]).await;
    pool_tick(&f).await;
    let detail = pool_detail(&f, &task).await;
    let aid = serde_json::from_value(pool_current(&detail["items"][0])["id"].clone()).unwrap();
    let grant = pool_delivery(&f, &ca, ea, aid).await;
    assert_eq!(grant["state"], "granted", "{grant}");

    let completion = completion(&grant, "unknown");
    f.repo().complete_execution(completion).await.unwrap();
    pool_tick(&f).await;
    let before = pool_detail(&f, &task).await;
    assert_eq!(pool_current(&before["items"][0])["id"], json!(aid));
    sqlx::query("UPDATE mobile_execution_grants SET receipt_deadline_at=clock_timestamp()-interval '1 second',result_deadline_at=clock_timestamp()-interval '1 second' WHERE attempt_id=$1").bind(aid).execute(&f.pool).await.unwrap();
    pool_tick(&f).await;
    let after = pool_detail(&f, &task).await;
    assert_eq!(
        pool_current(&after["items"][0])["plan"]["device_id"],
        json!(b)
    );
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn explicit_failure_switches_on_next_scan_without_five_minute_wait() {
    let f = Fixture::with_adapter(false).await;
    let b = pool_phone(&f, "B").await;
    let (ca, ea) = pool_ready(&f, f.device).await;
    let (_cb, _eb) = pool_ready(&f, b).await;
    let task = pool_confirm(&f, vec![f.targets[0]]).await;
    pool_tick(&f).await;
    let detail = pool_detail(&f, &task).await;
    let aid = serde_json::from_value(pool_current(&detail["items"][0])["id"].clone()).unwrap();
    let grant = pool_delivery(&f, &ca, ea, aid).await;
    assert_eq!(grant["state"], "granted", "{grant}");

    let mut failed = completion(&grant, "not_called");
    failed.reason = "device_unavailable".into();
    let first = f.repo().complete_execution(failed.clone()).await.unwrap();
    assert_eq!(first, f.repo().complete_execution(failed).await.unwrap());
    pool_tick(&f).await;
    let after = pool_detail(&f, &task).await;
    assert_eq!(
        pool_current(&after["items"][0])["plan"]["device_id"],
        json!(b)
    );
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated database"]
async fn late_success_keeps_successor_identity_and_completes_target() {
    let f = Fixture::with_adapter(false).await;
    let b = pool_phone(&f, "B").await;
    let (ca, ea) = pool_ready(&f, f.device).await;
    let (cb, _eb) = pool_ready(&f, b).await;
    let task = pool_confirm(&f, vec![f.targets[0]]).await;
    pool_tick(&f).await;
    let detail = pool_detail(&f, &task).await;
    let aid = serde_json::from_value(pool_current(&detail["items"][0])["id"].clone()).unwrap();
    let grant = pool_delivery(&f, &ca, ea, aid).await;
    assert_eq!(grant["state"], "granted", "{grant}");

    sqlx::query("UPDATE mobile_execution_grants SET receipt_deadline_at=clock_timestamp()-interval '1 second',result_deadline_at=clock_timestamp()-interval '1 second' WHERE attempt_id=$1").bind(aid).execute(&f.pool).await.unwrap();
    pool_tick(&f).await;
    let before = pool_detail(&f, &task).await;
    let next = pool_current(&before["items"][0])["id"].clone();
    assert_ne!(next, json!(aid));
    f.repo()
        .complete_execution(completion(&grant, "observed_sent"))
        .await
        .unwrap();
    // A persisted success fences grants before its asynchronous projection is refreshed.
    let (connection, connection_epoch): (Uuid, i64) = sqlx::query_as(
        "SELECT connection_id,epoch FROM device_control_sessions WHERE device_id=$1",
    )
    .bind(cb.device_id)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert!(
        f.repo()
            .pending_deliveries(&cb, connection, connection_epoch)
            .await
            .unwrap()
            .is_empty()
    );
    pool_tick(&f).await;
    let after = pool_detail(&f, &task).await;
    assert_eq!(after["items"][0]["dispatch"]["state"], "completed");
    assert_eq!(pool_current(&after["items"][0])["id"], next);
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated database"]
async fn concurrent_scanners_create_one_current_successor() {
    let f = Fixture::with_adapter(false).await;
    let b = pool_phone(&f, "B").await;
    let (ca, ea) = pool_ready(&f, f.device).await;
    let (_cb, _eb) = pool_ready(&f, b).await;
    let task = pool_confirm(&f, vec![f.targets[0]]).await;
    pool_tick(&f).await;
    let detail = pool_detail(&f, &task).await;
    let aid = serde_json::from_value(pool_current(&detail["items"][0])["id"].clone()).unwrap();
    let grant = pool_delivery(&f, &ca, ea, aid).await;
    assert_eq!(grant["state"], "granted", "{grant}");

    sqlx::query("UPDATE mobile_execution_grants SET receipt_deadline_at=clock_timestamp()-interval '1 second',result_deadline_at=clock_timestamp()-interval '1 second' WHERE attempt_id=$1").bind(aid).execute(&f.pool).await.unwrap();
    let r = f.repo();
    let (a, b_result) = tokio::join!(r.reconcile_dispatch(), r.reconcile_dispatch());
    a.unwrap();
    b_result.unwrap();
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM mobile_v9_attempts WHERE item_id=(SELECT item_id FROM mobile_v9_attempts WHERE id=$1) AND current").bind(aid).fetch_one(&f.pool).await.unwrap(),1);
    f.close().await;
}

fn completion(
    grant: &Value,
    outcome: &str,
) -> wekeeper_api::distribution::domain::shared_mobile::execution::Completion {
    wekeeper_api::distribution::domain::shared_mobile::execution::Completion {
        result_id: Uuid::new_v4(),
        execution_id: serde_json::from_value(grant["execution_id"].clone()).unwrap(),
        result_credential: grant["result_credential"].as_str().unwrap().into(),
        outcome: outcome.into(),
        reason: if outcome == "not_called" {
            "device_unavailable"
        } else {
            "synthetic"
        }
        .into(),
        evidence: "synthetic evidence".into(),
        consumed: matches!(outcome, "observed_sent" | "unknown"),
        calling_marked: matches!(outcome, "observed_sent" | "unknown"),
        device_started_at: None,
        device_finished_at: None,
    }
}

// Budget must survive fresh repository instances and terminal rows must stay off the queue.

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn dispatch_visits_every_candidate_exactly_three_times() {
    let f = Fixture::with_adapter(false).await;
    let b = pool_phone(&f, "B").await;
    let c = pool_phone(&f, "C").await;
    let mut phones = Vec::new();
    for device in [f.device, b, c] {
        let (credential, runtime) = pool_ready(&f, device).await;
        phones.push((device, credential, runtime));
    }
    let task = pool_confirm(&f, vec![f.targets[0]]).await;
    let mut seen = Vec::new();
    for _ in 0..9 {
        let detail = pool_detail(&f, &task).await;
        let current = pool_current(&detail["items"][0]);
        assert_eq!(current["state"], "leased");
        seen.push(current["plan"]["device_id"].clone());
        let device: Uuid = serde_json::from_value(current["plan"]["device_id"].clone()).unwrap();
        let (_, credential, runtime) = phones.iter().find(|(id, _, _)| *id == device).unwrap();
        let grant = pool_delivery(
            &f,
            credential,
            *runtime,
            serde_json::from_value(current["id"].clone()).unwrap(),
        )
        .await;
        f.repo()
            .complete_execution(completion(&grant, "not_called"))
            .await
            .unwrap();
        pool_business_due(&f).await;
        pool_tick(&f).await;
        let detail = pool_detail(&f, &task).await;
        if detail["items"][0]["dispatch"]["state"] == "waiting_resources" {
            pool_expire_round(&f).await;
            pool_tick(&f).await;
        }
    }
    let detail = pool_detail(&f, &task).await;
    assert_eq!(detail["items"][0]["dispatch"]["state"], "dispatch_failed");
    assert_eq!(&seen[..3], &seen[3..6]);
    assert_eq!(&seen[..3], &seen[6..9]);
    for device in [f.device, b, c] {
        assert_eq!(seen.iter().filter(|d| **d == json!(device)).count(), 3);
    }
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn dispatch_receipt_timeout_and_business_interval_do_not_refresh_budget() {
    let f = Fixture::with_adapter(false).await;
    pool_ready(&f, f.device).await;
    let task = pool_confirm(&f, vec![f.targets[0]]).await;
    let initial = pool_current(&task["items"][0])["id"].clone();
    pool_tick(&f).await;
    assert_eq!(
        pool_detail(&f, &task).await["items"][0]["dispatch"]["completed_rounds"],
        0
    );
    sqlx::query(
        "UPDATE mobile_v9_tasks SET next_eligible_at=clock_timestamp()+interval '10 minutes'",
    )
    .execute(&f.pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE mobile_v9_dispatch_state SET assigned_at=clock_timestamp()-interval '10 minutes'",
    )
    .execute(&f.pool)
    .await
    .unwrap();
    pool_tick(&f).await;
    let waiting = pool_detail(&f, &task).await;
    assert_eq!(waiting["items"][0]["dispatch"]["completed_rounds"], 0);
    assert_eq!(pool_current(&waiting["items"][0])["id"], initial);
    let timer: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT assigned_at FROM mobile_v9_dispatch_state")
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert!(
        timer.is_some(),
        "an issued execution retains its delivery timer across the business interval"
    );
    sqlx::query(
        "UPDATE mobile_v9_tasks SET next_eligible_at=clock_timestamp()-interval '1 second'",
    )
    .execute(&f.pool)
    .await
    .unwrap();
    pool_tick(&f).await;
    sqlx::query(
        "UPDATE mobile_v9_dispatch_state SET assigned_at=clock_timestamp()-interval '6 minutes',round_started_at=clock_timestamp()-interval '6 minutes'",
    )
    .execute(&f.pool)
    .await
    .unwrap();
    sqlx::query("UPDATE mobile_execution_grants SET receipt_deadline_at=clock_timestamp()-interval '1 second'").execute(&f.pool).await.unwrap();
    pool_tick(&f).await;
    let after = pool_detail(&f, &task).await;
    assert_eq!(after["items"][0]["dispatch"]["completed_rounds"], 1);
    assert_eq!(
        pool_current(&after["items"][0])["termination_reason"],
        "receipt_timeout"
    );
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn dispatch_third_round_success_completes_without_failure() {
    let f = Fixture::with_adapter(false).await;
    let (credential, epoch) = pool_ready(&f, f.device).await;
    let task = pool_confirm(&f, vec![f.targets[0]]).await;
    for _ in 0..2 {
        let detail = pool_detail(&f, &task).await;
        let aid = serde_json::from_value(pool_current(&detail["items"][0])["id"].clone()).unwrap();
        let grant = pool_delivery(&f, &credential, epoch, aid).await;
        f.repo()
            .complete_execution(completion(&grant, "not_called"))
            .await
            .unwrap();
        pool_expire_round(&f).await;
        pool_tick(&f).await;
    }
    let before = pool_detail(&f, &task).await;
    assert_eq!(before["items"][0]["dispatch"]["completed_rounds"], 2);
    let aid = serde_json::from_value(pool_current(&before["items"][0])["id"].clone()).unwrap();
    let grant = pool_delivery(&f, &credential, epoch, aid).await;
    assert_eq!(grant["state"], "granted");
    f.repo()
        .complete_execution(completion(&grant, "observed_sent"))
        .await
        .unwrap();
    pool_tick(&f).await;
    let after = pool_detail(&f, &task).await;
    assert_eq!(after["items"][0]["dispatch"]["state"], "completed");
    assert_eq!(after["items"][0]["dispatch"]["completed_rounds"], 2);
    assert!(after["items"][0]["dispatch"]["next_retry_at"].is_null());
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn dispatch_exhausted_target_does_not_block_other_targets() {
    let f = Fixture::with_adapter(false).await;
    let (credential, epoch) = pool_ready(&f, f.device).await;
    let task = pool_confirm(&f, f.targets.clone()).await;
    let first = serde_json::from_value::<Uuid>(task["items"][0]["id"].clone()).unwrap();
    sqlx::query("UPDATE mobile_v9_dispatch_state SET completed_rounds=2,round_started_at=clock_timestamp()-interval '5 minutes 1 second' WHERE item_id=$1")
        .bind(first)
        .execute(&f.pool)
        .await
        .unwrap();
    let attempt = serde_json::from_value(pool_current(&task["items"][0])["id"].clone()).unwrap();
    let grant = pool_delivery(&f, &credential, epoch, attempt).await;
    f.repo()
        .complete_execution(completion(&grant, "not_called"))
        .await
        .unwrap();
    pool_business_due(&f).await;
    pool_tick(&f).await;
    let detail = pool_detail(&f, &task).await;
    assert_eq!(detail["items"][0]["dispatch"]["state"], "dispatch_failed");
    assert_eq!(detail["items"][1]["dispatch"]["state"], "executing");
    assert_eq!(detail["items"][1]["dispatch"]["completed_rounds"], 0);
    f.close().await;
}

// A delivered singleton must get three independent five-minute result windows,
// even if its control channel disconnects while it executes locally.

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn delivered_phone_heartbeat_freshness_does_not_cancel_execution() {
    let f = Fixture::with_adapter(false).await;
    let b = pool_phone(&f, "B").await;
    let (credential, epoch) = pool_ready(&f, f.device).await;
    pool_ready(&f, b).await;
    let task = pool_confirm(&f, vec![f.targets[0]]).await;
    let aid = serde_json::from_value(pool_current(&task["items"][0])["id"].clone()).unwrap();
    let grant = pool_delivery(&f, &credential, epoch, aid).await;
    assert_eq!(grant["state"], "granted");
    acknowledge_execution(
        &f,
        &credential,
        serde_json::from_value(grant["id"].clone()).unwrap(),
    )
    .await;
    // Old dispatch used a hard-coded 30-second heartbeat cutoff, independently
    // of the shared heartbeat presence policy and the five-minute result wait.
    for age in [35, 60, 130, 180] {
        sqlx::query("UPDATE device_control_sessions SET last_heartbeat_at=clock_timestamp()-make_interval(secs=>$2::double precision) WHERE device_id=$1")
            .bind(f.device).bind(age as f64).execute(&f.pool).await.unwrap();
        pool_tick(&f).await;
        let detail = pool_detail(&f, &task).await;
        assert_eq!(pool_current(&detail["items"][0])["id"], json!(aid));
        assert_eq!(detail["items"][0]["dispatch"]["state"], "executing");
        assert_eq!(detail["items"][0]["dispatch"]["completed_rounds"], 0);
    }
    f.repo()
        .complete_execution(completion(&grant, "observed_sent"))
        .await
        .unwrap();
    pool_tick(&f).await;
    assert_eq!(
        pool_detail(&f, &task).await["items"][0]["dispatch"]["state"],
        "completed"
    );
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn offline_scans_cannot_consume_a_five_minute_round() {
    let f = Fixture::with_adapter(false).await;
    let task = pool_confirm(&f, vec![f.targets[0]]).await;
    for round in 0..3 {
        for _ in 0..3 {
            pool_tick(&f).await;
        }
        sqlx::query("UPDATE mobile_v9_dispatch_state SET round_started_at=clock_timestamp()-interval '4 minutes 50 seconds'")
            .execute(&f.pool).await.unwrap();
        pool_tick(&f).await;
        let before = pool_detail(&f, &task).await;
        assert_eq!(before["items"][0]["dispatch"]["completed_rounds"], round);
        assert_ne!(before["items"][0]["dispatch"]["state"], "dispatch_failed");
        pool_expire_round(&f).await;
        assert_eq!(
            pool_detail(&f, &task).await["items"][0]["dispatch"]["completed_rounds"],
            round + 1
        );
    }
    let failed = pool_detail(&f, &task).await;
    assert_eq!(failed["items"][0]["dispatch"]["state"], "dispatch_failed");
    assert!(failed["items"][0]["dispatch"]["next_retry_at"].is_null());
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn task_list_status_matches_detail_and_completion_survives_manual_stop() {
    let f = Fixture::with_adapter(false).await;
    let (credential, epoch) = pool_ready(&f, f.device).await;
    let task = pool_confirm(&f, vec![f.targets[0]]).await;
    let task_id = serde_json::from_value::<Uuid>(task["id"].clone()).unwrap();
    async fn check(f: &Fixture, task: &Value, expected: &str) {
        let overview = f.repo().task_read(&f.actor, None).await.unwrap();
        let listed = overview["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["id"] == task["id"])
            .unwrap();
        assert!(
            listed.get("items").is_none(),
            "list status must not require full details"
        );
        assert_eq!(listed["status"], expected);
        assert_eq!(pool_detail(f, task).await["status"], expected);
        serde_json::from_value::<TaskStatus>(listed["status"].clone()).unwrap();
    }
    let aid = serde_json::from_value(pool_current(&task["items"][0])["id"].clone()).unwrap();
    let grant = pool_delivery(&f, &credential, epoch, aid).await;
    assert_eq!(grant["state"], "granted");
    check(&f, &task, "dispatching").await;
    sqlx::query("UPDATE mobile_v9_dispatch_state SET state='dispatch_failed',completed_rounds=3,next_retry_at=NULL")
        .execute(&f.pool).await.unwrap();
    check(&f, &task, "dispatch_failed").await;
    // A late persisted success is authoritative before asynchronous projection.
    f.repo()
        .complete_execution(completion(&grant, "observed_sent"))
        .await
        .unwrap();
    check(&f, &task, "completed").await;
    let stopped = f
        .repo()
        .task_write(
            &f.actor,
            "stop-completed-task",
            TaskCommand::Stop(
                task_id,
                Mutation {
                    expected_revision: task["revision"].as_i64().unwrap(),
                    reason: "stop must preserve completed results".into(),
                },
            ),
        )
        .await
        .unwrap();
    assert_eq!(stopped["stopped"], true);
    assert_eq!(stopped["status"], "completed");
    check(&f, &task, "completed").await;
    let other = pool_confirm_keyed(&f, vec![f.targets[1]], "other-status").await;
    let other_id = serde_json::from_value::<Uuid>(other["id"].clone()).unwrap();
    let stopped = f
        .repo()
        .task_write(
            &f.actor,
            "stop-running-task",
            TaskCommand::Stop(
                other_id,
                Mutation {
                    expected_revision: other["revision"].as_i64().unwrap(),
                    reason: "operator stops remaining work".into(),
                },
            ),
        )
        .await
        .unwrap();
    assert_eq!(stopped["status"], "stopped");
    check(&f, &other, "stopped").await;
    check(&f, &task, "completed").await;
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn candidate_retains_assignment_through_two_lost_cycles_and_transport_repair() {
    let f = Fixture::with_adapter(false).await;
    let b = pool_phone(&f, "B").await;
    pool_ready(&f, f.device).await;
    pool_ready(&f, b).await;
    let task = pool_confirm(&f, vec![f.targets[0]]).await;
    let original = pool_current(&task["items"][0])["id"].clone();
    assert_eq!(
        pool_current(&task["items"][0])["plan"]["device_id"],
        json!(f.device)
    );
    for age in [35, 70, 131, 181] {
        sqlx::query("UPDATE device_control_sessions SET connected=false,last_heartbeat_at=clock_timestamp()-make_interval(secs=>$2::double precision) WHERE device_id=$1")
            .bind(f.device).bind(age as f64).execute(&f.pool).await.unwrap();
        pool_tick(&f).await;
        let detail = pool_detail(&f, &task).await;
        assert_eq!(
            pool_current(&detail["items"][0])["id"],
            original,
            "premature replacement at {age}s: {detail}"
        );
    }
    sqlx::query("UPDATE device_control_sessions SET last_heartbeat_at=clock_timestamp()-interval '191 seconds' WHERE device_id=$1")
        .bind(f.device).execute(&f.pool).await.unwrap();
    pool_tick(&f).await;
    let detail = pool_detail(&f, &task).await;
    assert_eq!(
        pool_current(&detail["items"][0])["plan"]["device_id"],
        json!(b)
    );
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn offline_binding_is_selectable_and_confirmable_without_environment_or_consent() {
    let f = Fixture::new().await;
    f.activate("activate").await.unwrap();
    let r = f.repo();
    let policy:PolicyInput=serde_json::from_value(json!({"expected_revision":0,"mode":"enabled","confirmation_seconds":600,"inspection_seconds":600,"retry_seconds":300,"retry_limit":3,"timezone":"Asia/Shanghai","allowed_windows":[{"weekdays":[1,2,3,4,5,6,7],"start_minute":0,"end_minute":1440}],"limits":ObjectLimits::value(),"reason":"Synthetic test"})).unwrap();
    save_policy(&r, &f.actor, "policy", policy).await.unwrap();

    let status = r.task_read(&f.actor, None).await.unwrap();
    assert_eq!(status["readiness"][0]["plan_selectable"], true);
    assert_eq!(status["readiness"][0]["confirmable"], true);
    assert_eq!(status["readiness"][0]["execution_ready"], false);
    let input = DraftInput {
        client_kind: wekeeper_api::distribution::domain::shared_mobile::MobileClient::Wecom,
        target_ids: f.targets.clone(),
        message: "  测试\n🙂  ".into(),
        content_type: "text/plain".into(),
        expected_revision: None,
        between_targets_seconds: 30,
    };
    let draft = r
        .task_write(&f.actor, "draft", TaskCommand::Draft(None, input))
        .await
        .unwrap();
    let preview = r
        .task_write(
            &f.actor,
            "preview",
            TaskCommand::Preview(PreviewInput {
                draft_id: serde_json::from_value(draft["id"].clone()).unwrap(),
                expected_revision: 1,
                selections: vec![],
            }),
        )
        .await
        .unwrap();
    assert!(
        preview["plan"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|i| i["blocker"].is_null() && i["candidates"].as_array().unwrap().len() == 1)
    );
    assert!(
        preview["plan"]["items"][0]["selected"]
            .get("environment_snapshot_id")
            .is_none()
    );
    let confirm = Confirmation {
        preview_id: serde_json::from_value(preview["id"].clone()).unwrap(),
        preview_digest: preview["digest"].as_str().unwrap().into(),
        confirmed: true,
    };
    let task = r
        .task_write(&f.actor, "confirm", TaskCommand::Confirm(confirm.clone()))
        .await
        .unwrap();
    let id: Uuid = serde_json::from_value(task["id"].clone()).unwrap();
    // The inbox must include future groups without consent, runtime or a control connection.
    let token_hash = wekeeper_api::business::hash("wk_device_inbox-synthetic");
    sqlx::query("INSERT INTO device_credentials(token_hash,device_id,generation,expires_at) VALUES($1,$2,1,clock_timestamp()+interval '1 hour')").bind(&token_hash).bind(f.device).execute(&f.pool).await.unwrap();
    let credential = wekeeper_api::distribution::domain::device_control::Credential {
        device_id: f.device,
        generation: 1,
        token_hash,
    };
    let inbox = r
        .inbox_read(
            &credential,
            InboxQuery {
                limit: Some(1),
                cursor: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(inbox["items"].as_array().unwrap().len(), 1);
    assert_eq!(inbox["items"][0]["target_name"], "测试群");
    assert_eq!(inbox["items"][0]["total"], 2);
    assert!(inbox["items"][0].get("message").is_none());
    let page2 = r
        .inbox_read(
            &credential,
            InboxQuery {
                limit: Some(1),
                cursor: Some(inbox["page"]["next_cursor"].as_str().unwrap().into()),
            },
        )
        .await
        .unwrap();
    assert_ne!(
        inbox["items"][0]["attempt_id"],
        page2["items"][0]["attempt_id"]
    );
    assert_eq!(page2["page"]["has_more"], false);
    let aid: Uuid = serde_json::from_value(inbox["items"][0]["attempt_id"].clone()).unwrap();
    let observed = InboxReceipt {
        receipt_id: Uuid::new_v4(),
        queue_revision: inbox["queue_revision"].as_i64().unwrap(),
        items: vec![
            InboxReceiptItem {
                attempt_id: aid,
                assignment_revision: 1,
            },
            InboxReceiptItem {
                attempt_id: Uuid::new_v4(),
                assignment_revision: 1,
            },
        ],
    };
    let receipt = r
        .inbox_receipt(&credential, "inbox-receipt", observed.clone())
        .await
        .unwrap();
    assert_eq!(receipt["accepted"].as_array().unwrap().len(), 1);
    assert_eq!(receipt["rejected"].as_array().unwrap().len(), 1);
    assert_eq!(
        receipt,
        r.inbox_receipt(&credential, "inbox-receipt", observed.clone())
            .await
            .unwrap()
    );
    let mut changed = observed;
    changed.queue_revision += 1;
    assert!(
        r.inbox_receipt(&credential, "inbox-receipt", changed)
            .await
            .is_err()
    );
    let after_receipt = r
        .inbox_read(&credential, InboxQuery::default())
        .await
        .unwrap();
    assert_eq!(after_receipt["queue_revision"], inbox["queue_revision"]);
    assert!(
        after_receipt["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|i| i["attempt_state"] == "invalidated")
    );
    let receipt_detail = r.task_read(&f.actor, Some(id)).await.unwrap();
    assert!(
        receipt_detail["items"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|i| i["attempts"].as_array().unwrap())
            .any(|a| a["id"] == json!(aid) && a["inbox_received_at"].is_string())
    );
    // A rolled-back state change cannot leak a wake or change the page version.
    let mut rolled = f.pool.begin().await.unwrap();
    sqlx::query("UPDATE mobile_v9_attempts SET state='cancelled' WHERE id=$1")
        .bind(aid)
        .execute(&mut *rolled)
        .await
        .unwrap();
    rolled.rollback().await.unwrap();
    let stable = r
        .inbox_read(&credential, InboxQuery::default())
        .await
        .unwrap();
    assert_eq!(stable["queue_revision"], inbox["queue_revision"]);
    let mut held = f.pool.begin().await.unwrap();
    sqlx::query("SELECT pg_advisory_xact_lock(78234610243)")
        .execute(&mut *held)
        .await
        .unwrap();
    tokio::time::timeout(
        std::time::Duration::from_secs(2),
        r.inbox_read(&credential, InboxQuery::default()),
    )
    .await
    .expect("read must not wait for execution recovery")
    .unwrap();
    held.rollback().await.unwrap();

    let detail = r.task_read(&f.actor, Some(id)).await.unwrap();
    let status = &detail["items"][0]["attempts"][0]["execution_status"];
    assert_eq!(
        status["phase"], "blocked",
        "the retired offline attempt cannot execute; its target retains the remaining round budget"
    );
    assert_eq!(status["conditions"][0]["code"], "awaiting_round_deadline");
    assert_eq!(status["conditions"][0]["scope_type"], "device");
    assert_eq!(status["conditions"].as_array().unwrap().len(), 1);
    assert!(status["device_state_observed_at"].is_null());
    let reservations: i64 = sqlx::query_scalar("SELECT count(*) FROM mobile_v9_quota_reservations")
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(
        reservations, 0,
        "reading live waiting status must never reserve quota"
    );
    assert!(task.get("cursor_position").is_none());
    assert_eq!(task["items"].as_array().unwrap().len(), 2);
    assert_eq!(
        task["items"][0]["attempts"][0]["plan"]["message"],
        "  测试\n🙂  "
    );
    assert_eq!(
        r.task_write(&f.actor, "confirm", TaskCommand::Confirm(confirm))
            .await
            .unwrap(),
        task
    );
    let artifact: Uuid =
        sqlx::query_scalar("SELECT apk_id FROM mobile_v9_acceptance_runs WHERE id=$1")
            .bind(f.acceptance)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(
        f.repo().artifact_content(&f.actor, artifact).await.unwrap(),
        vec![1u8]
    );
    let mut outsider = f.actor.clone();
    outsider.id = Uuid::new_v4();
    sqlx::query("INSERT INTO accounts(id,username,display_name,platform_admin) VALUES($1,'outsider','Outsider',false)").bind(outsider.id).execute(&f.pool).await.unwrap();
    assert!(
        f.repo()
            .artifact_content(&outsider, artifact)
            .await
            .is_err()
    );
    f.close().await;
}

async fn acknowledge_execution(
    f: &Fixture,
    c: &wekeeper_api::distribution::domain::device_control::Credential,
    id: Uuid,
) -> Value {
    use wekeeper_api::distribution::{
        domain::device_control::{Operation, Pulse},
        south::port::device_control::Store as _,
    };
    let (connection, epoch, runtime_epoch): (Uuid, i64, Uuid) = sqlx::query_as(
        "SELECT connection_id,epoch,runtime_epoch FROM device_control_sessions WHERE device_id=$1",
    )
    .bind(c.device_id)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    let frame = wekeeper_api::distribution::south::adapter::postgres::device_control::Repository(
        f.pool.clone(),
    )
    .device(
        c,
        Operation::Pulse(Pulse {
            protocol_version: 2,
            connection_id: connection,
            epoch,
            runtime_epoch,
            execution_receipts: vec![id],
            heartbeat_id: Some(Uuid::new_v4()),
            heartbeat_sequence: 0,
            probe_id: None,
            receipts: vec![],
        }),
    )
    .await
    .unwrap();
    assert_eq!(frame.execution_receipts, vec![id]);
    sqlx::query_scalar("SELECT to_jsonb(g)-'envelope' FROM mobile_execution_grants g WHERE id=$1")
        .bind(id)
        .fetch_one(&f.pool)
        .await
        .unwrap()
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn direct_delivery_signs_before_transport_and_reconnect_reuses_one_capability() {
    let f = Fixture::with_adapter(false).await;
    let (c, runtime) = pool_ready(&f, f.device).await;
    let task = pool_confirm(&f, vec![f.targets[0]]).await;
    pool_tick(&f).await;
    let detail = pool_detail(&f, &task).await;
    let attempt = serde_json::from_value(pool_current(&detail["items"][0])["id"].clone()).unwrap();
    let grant = pool_delivery(&f, &c, runtime, attempt).await;
    assert_eq!(grant["max_calls"], 1);
    assert_eq!(grant["plan"]["execution"]["contract"], "device_owned/2");
    assert!(grant["plan"].get("flow").is_none());
    let connection = Uuid::new_v4();
    sqlx::query(
        "UPDATE device_control_sessions SET connection_id=$2,epoch=epoch+1 WHERE device_id=$1",
    )
    .bind(f.device)
    .bind(connection)
    .execute(&f.pool)
    .await
    .unwrap();
    sqlx::query("UPDATE mobile_execution_delivery_outbox SET next_send_at=clock_timestamp()")
        .execute(&f.pool)
        .await
        .unwrap();
    let second = f
        .repo()
        .pending_deliveries(&c, connection, 2)
        .await
        .unwrap();
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].delivery.execution.as_ref().unwrap(), &grant);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM mobile_execution_grants WHERE attempt_id=$1"
        )
        .bind(attempt)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        1
    );
    let rows: Vec<Value> = sqlx::query_scalar(
        "SELECT to_jsonb(x) FROM mobile_delivery_exchanges x ORDER BY created_at,id",
    )
    .fetch_all(&f.pool)
    .await
    .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1]["replay_of"], rows[0]["id"]);
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn direct_timeouts_and_late_receipt_preserve_original_attempt_facts() {
    let f = Fixture::with_adapter(false).await;
    let b = pool_phone(&f, "B").await;
    let (c, e) = pool_ready(&f, f.device).await;
    let (cb, eb) = pool_ready(&f, b).await;
    let task = pool_confirm(&f, vec![f.targets[0]]).await;
    pool_tick(&f).await;
    let detail = pool_detail(&f, &task).await;
    let aid = serde_json::from_value(pool_current(&detail["items"][0])["id"].clone()).unwrap();
    let grant = pool_delivery(&f, &c, e, aid).await;
    let id: Uuid = serde_json::from_value(grant["id"].clone()).unwrap();
    sqlx::query("UPDATE mobile_execution_grants SET receipt_deadline_at=clock_timestamp()-interval '1 second' WHERE id=$1").bind(id).execute(&f.pool).await.unwrap();
    // Advance both frozen business interval and quota cooldown while forcing
    // the receipt deadline; an outstanding reservation still counts daily.
    pool_business_due(&f).await;
    sqlx::query("UPDATE mobile_v9_quota_reservations SET recorded_at=clock_timestamp()-interval '31 seconds'")
        .execute(&f.pool).await.unwrap();
    pool_tick(&f).await;
    let old: Value = sqlx::query_scalar(
        "SELECT to_jsonb(g)-'envelope' FROM mobile_execution_grants g WHERE id=$1",
    )
    .bind(id)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(old["superseded_reason"], "receipt_timeout");
    let detail = pool_detail(&f, &task).await;
    let aid2 = serde_json::from_value(pool_current(&detail["items"][0])["id"].clone()).unwrap();
    let next = pool_delivery(&f, &cb, eb, aid2).await;
    let next_id: Uuid = serde_json::from_value(next["id"].clone()).unwrap();
    let accepted = acknowledge_execution(&f, &cb, next_id).await;
    let duplicate = acknowledge_execution(&f, &cb, next_id).await;
    assert_eq!(
        accepted["result_deadline_at"],
        duplicate["result_deadline_at"]
    );
    let late = acknowledge_execution(&f, &c, id).await;
    assert_eq!(late["state"], "superseded");
    assert_eq!(late["result_deadline_at"], old["result_deadline_at"]);
    sqlx::query("UPDATE mobile_execution_grants SET result_deadline_at=clock_timestamp()-interval '1 second' WHERE id=$1").bind(next_id).execute(&f.pool).await.unwrap();
    pool_tick(&f).await;
    let reasons:Vec<String>=sqlx::query_scalar("SELECT superseded_reason FROM mobile_execution_grants WHERE id=ANY($1) ORDER BY assigned_at").bind(vec![id,next_id]).fetch_all(&f.pool).await.unwrap();
    assert_eq!(reasons, vec!["receipt_timeout", "result_timeout"]);
    let projection: Value = sqlx::query_scalar(
        "SELECT to_jsonb(v) FROM mobile_delivery_exchange_view v WHERE execution_id=$1 LIMIT 1",
    )
    .bind(id)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(projection["reception_state"], "receipt_timeout");
    assert!(projection["accepted_at"].is_string());
    f.repo()
        .complete_execution(completion(&grant, "not_called"))
        .await
        .unwrap();
    let detail = pool_detail(&f, &task).await;
    let attempts = detail["items"][0]["attempts"].as_array().unwrap();
    let first = attempts.iter().find(|a| a["id"] == json!(aid)).unwrap();
    let second = attempts.iter().find(|a| a["id"] == json!(aid2)).unwrap();
    assert_eq!(first["execution_id"], json!(id));
    assert_eq!(second["execution_id"], json!(next_id));
    assert!(first["result_received_at"].is_string());
    assert!(second["result_received_at"].is_null());
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn result_before_receipt_fences_redelivery_without_a_false_receipt_timeout() {
    let f = Fixture::with_adapter(false).await;
    let (c, e) = pool_ready(&f, f.device).await;
    let task = pool_confirm(&f, vec![f.targets[0]]).await;
    pool_tick(&f).await;
    let detail = pool_detail(&f, &task).await;
    let aid = serde_json::from_value(pool_current(&detail["items"][0])["id"].clone()).unwrap();
    let grant = pool_delivery(&f, &c, e, aid).await;
    let result = completion(&grant, "observed_sent");
    let receipt = f.repo().complete_execution(result.clone()).await.unwrap();
    assert_eq!(receipt, f.repo().complete_execution(result).await.unwrap());
    sqlx::query("UPDATE mobile_execution_delivery_outbox SET next_send_at=clock_timestamp()")
        .execute(&f.pool)
        .await
        .unwrap();
    let (connection, epoch): (Uuid, i64) = sqlx::query_as(
        "SELECT connection_id,epoch FROM device_control_sessions WHERE device_id=$1",
    )
    .bind(c.device_id)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert!(
        f.repo()
            .pending_deliveries(&c, connection, epoch)
            .await
            .unwrap()
            .is_empty()
    );
    pool_tick(&f).await;
    let value: Value = sqlx::query_scalar(
        "SELECT to_jsonb(g)-'envelope' FROM mobile_execution_grants g WHERE attempt_id=$1",
    )
    .bind(aid)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert!(value["accepted_at"].is_null());
    assert_ne!(value["superseded_reason"], "receipt_timeout");
    let detail = pool_detail(&f, &task).await;
    let attempt = detail["items"][0]["attempts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == json!(aid))
        .unwrap();
    assert_eq!(attempt["execution_id"], grant["execution_id"]);
    assert!(attempt["execution_accepted_at"].is_null());
    assert_eq!(
        serde_json::from_value::<chrono::DateTime<chrono::Utc>>(
            attempt["result_received_at"].clone()
        )
        .unwrap(),
        serde_json::from_value::<chrono::DateTime<chrono::Utc>>(
            value["result_received_at"].clone()
        )
        .unwrap()
    );
    assert!(attempt.get("device_received_at").is_none());
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn real_websocket_directly_delivers_signed_execution_and_old_routes_are_absent() {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::{Message, client::IntoClientRequest};
    let f = Fixture::with_adapter(false).await;
    let (c, runtime) = pool_ready(&f, f.device).await;
    let token = format!("wk_device_{}", Uuid::new_v4());
    sqlx::query("INSERT INTO device_credentials(token_hash,device_id,generation,expires_at) VALUES($1,$2,1,clock_timestamp()+interval '1 hour')").bind(wekeeper_api::business::hash(&token)).bind(c.device_id).execute(&f.pool).await.unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app =
        wekeeper_api::http::app_router(wekeeper_api::bootstrap::build_application(f.pool.clone()));
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let mut request = format!("ws://{address}/api/v1/device-control/channel")
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("Authorization", format!("Bearer {token}").parse().unwrap());
    let (mut socket, _) = tokio_tungstenite::connect_async(request).await.unwrap();
    let frame: Value =
        serde_json::from_str(socket.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
    let mut pulse = json!({"connection_id":frame["connection_id"],"epoch":frame["epoch"],"runtime_epoch":runtime,"execution_receipts":[],"heartbeat_id":Uuid::new_v4(),"heartbeat_sequence":1,"probe_id":null,"protocol_version":2,"receipts":[]});
    socket
        .send(Message::Text(pulse.to_string().into()))
        .await
        .unwrap();
    socket.next().await.unwrap().unwrap();
    let task = pool_confirm(&f, vec![f.targets[0]]).await;
    pool_tick(&f).await;
    let delivered = tokio::time::timeout(std::time::Duration::from_secs(12), async {
        loop {
            let v: Value =
                serde_json::from_str(socket.next().await.unwrap().unwrap().to_text().unwrap())
                    .unwrap();
            if !v["executions"].as_array().unwrap().is_empty() {
                break v;
            }
        }
    })
    .await
    .unwrap();
    let envelope = &delivered["executions"][0];
    assert_eq!(envelope["schema"], "mobile_execution_delivery/1");
    assert_eq!(envelope["delivery"]["execution"]["task_id"], task["id"]);
    assert!(envelope["delivery"]["execution"]["signature"].is_string());
    pulse["heartbeat_id"] = json!(Uuid::new_v4());
    pulse["heartbeat_sequence"] = json!(2);
    pulse["execution_receipts"] = json!([envelope["delivery_id"]]);
    socket
        .send(Message::Text(pulse.to_string().into()))
        .await
        .unwrap();
    let reply: Value =
        serde_json::from_str(socket.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
    assert_eq!(reply["execution_receipts"], pulse["execution_receipts"]);
    for route in ["task-deliveries", "execution-grants", "execution-receipts"] {
        let response = reqwest::Client::new()
            .post(format!("http://{address}/api/v10/device-execution/{route}"))
            .bearer_auth(&token)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 404);
    }
    socket.close(None).await.unwrap();
    server.abort();
    let _ = server.await;
    f.close().await;
}

async fn save_policy(
    r: &Repository,
    actor: &Actor,
    key: &str,
    p: PolicyInput,
) -> wekeeper_api::business::Result<Value> {
    use wekeeper_api::distribution::domain::shared_mobile::business_scheme::{
        BusinessSchemeInput, SchemeMutation,
    };
    let input = BusinessSchemeInput {
        expected_revision: p.expected_revision,
        enabled: p.mode == "enabled",
        confirmed: true,
        timezone: p.timezone,
        allowed_windows: p.allowed_windows,
        limits: p.limits,
        reason: p.reason,
    };
    let mutation = SchemeMutation {
        expected_revision: input.expected_revision,
        reason: input.reason.clone(),
        confirmed: true,
    };
    r.scheme_write(actor, key, None, Some(input), mutation)
        .await
}

async fn scheme_unit(f: &Fixture) -> Uuid {
    let unit = Uuid::new_v4();
    let platform = Uuid::new_v4();
    sqlx::query("INSERT INTO source_callers(source_id) VALUES($1)")
        .bind(platform)
        .execute(&f.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO business_units(id,platform_id,name,created_by,updated_by) VALUES($1,$2,'单位',$3,$3)").bind(unit).bind(platform).bind(f.actor.id).execute(&f.pool).await.unwrap();
    unit
}
fn scheme_input(
    enabled: bool,
    revision: i64,
) -> wekeeper_api::distribution::domain::shared_mobile::business_scheme::BusinessSchemeInput {
    let mut v:wekeeper_api::distribution::domain::shared_mobile::business_scheme::BusinessSchemeInput=serde_json::from_str(include_str!("../../api-contract/fixtures/mobile-business-scheme.json")).unwrap();
    v.enabled = enabled;
    v.expected_revision = revision;
    v.allowed_windows = serde_json::from_value(
        json!([{"weekdays":[1,2,3,4,5,6,7],"start_minute":0,"end_minute":1440}]),
    )
    .unwrap();
    for l in [
        &mut v.limits.total,
        &mut v.limits.device,
        &mut v.limits.target,
    ] {
        l.interval_seconds = 1;
        l.max_calls = 1000;
        l.daily_max_calls = 1000;
    }
    v
}
async fn scheme_save(
    f: &Fixture,
    unit: Option<Uuid>,
    input: wekeeper_api::distribution::domain::shared_mobile::business_scheme::BusinessSchemeInput,
    key: &str,
) -> wekeeper_api::business::Result<Value> {
    use wekeeper_api::distribution::domain::shared_mobile::business_scheme::SchemeMutation;
    let mutation = SchemeMutation {
        expected_revision: input.expected_revision,
        reason: input.reason.clone(),
        confirmed: input.confirmed,
    };
    f.repo()
        .scheme_write(&f.actor, key, unit, Some(input), mutation)
        .await
}
async fn unit_preview(f: &Fixture, unit: Option<Uuid>, device: Uuid, key: &str) -> Value {
    use wekeeper_api::distribution::domain::shared_mobile::business_scheme::TaskPreviewInput;
    f.repo()
        .preview_task(
            &f.actor,
            key,
            TaskPreviewInput {
                client_kind: wekeeper_api::distribution::domain::shared_mobile::MobileClient::Wecom,
                business_unit_id: unit,
                draft_id: None,
                expected_revision: None,
                target_ids: vec![f.targets[0]],
                message: "单位消息".into(),
                between_targets_seconds: 1,
                selections: vec![Selection {
                    target_id: f.targets[0],
                    device_id: device,
                }],
            },
        )
        .await
        .unwrap()["preview"]
        .clone()
}
async fn confirm_preview(
    f: &Fixture,
    p: &Value,
    key: &str,
) -> wekeeper_api::business::Result<Value> {
    f.repo()
        .task_write(
            &f.actor,
            key,
            TaskCommand::Confirm(Confirmation {
                preview_id: serde_json::from_value(p["id"].clone()).unwrap(),
                preview_digest: p["digest"].as_str().unwrap().into(),
                confirmed: true,
            }),
        )
        .await
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn unit_schemes_resolve_defaults_independently_and_restore_with_audit() {
    use wekeeper_api::distribution::domain::shared_mobile::business_scheme::SchemeMutation;
    let f = Fixture::with_adapter(false).await;
    let a = scheme_unit(&f).await;
    let b = scheme_unit(&f).await;
    assert_eq!(
        f.repo().scheme_read(&f.actor, Some(a)).await.unwrap()["source"],
        "builtin"
    );
    let default = scheme_save(&f, None, scheme_input(false, 0), "same-key")
        .await
        .unwrap();
    let own = scheme_save(&f, Some(a), scheme_input(true, 0), "same-key")
        .await
        .unwrap();
    assert_ne!(own["id"], default["id"]);
    assert_eq!(
        f.repo().scheme_read(&f.actor, Some(b)).await.unwrap()["mode"],
        "disabled"
    );
    let (left, right) = tokio::join!(
        scheme_save(&f, Some(a), scheme_input(true, 1), "left"),
        scheme_save(&f, Some(a), scheme_input(false, 1), "right")
    );
    assert_ne!(left.is_ok(), right.is_ok());
    let restored = f
        .repo()
        .scheme_write(
            &f.actor,
            "inherit",
            Some(a),
            None,
            SchemeMutation {
                expected_revision: 2,
                reason: "恢复默认".into(),
                confirmed: true,
            },
        )
        .await
        .unwrap();
    assert_eq!(restored["source"], "default");
    assert_eq!(restored["scope_revision"], 3);
    assert_eq!(restored["id"], default["id"]);
    assert_eq!(
        f.repo().scheme_read(&f.actor, Some(b)).await.unwrap()["scope_revision"],
        0
    );
    assert!(
        f.repo()
            .scheme_read(&f.actor, Some(Uuid::new_v4()))
            .await
            .is_err()
    );
    let audits: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_events WHERE action='mobile.business_scheme'",
    )
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(audits, 4);
    let obsolete:i64=sqlx::query_scalar("SELECT count(*) FROM information_schema.tables WHERE table_schema=current_schema() AND table_name IN ('mobile_usages','mobile_usage_scopes','mobile_v9_scope_grants','mobile_execution_usage_budgets','mobile_v9_send_policy_versions')").fetch_one(&f.pool).await.unwrap();
    assert_eq!(obsolete, 0);
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn unit_scheme_changes_invalidate_only_affected_previews_and_pause_without_spending_rounds() {
    let f = Fixture::with_adapter(false).await;
    let unit = scheme_unit(&f).await;
    scheme_save(&f, Some(unit), scheme_input(false, 0), "unit")
        .await
        .unwrap();
    let p = unit_preview(&f, Some(unit), f.device, "own-preview").await;
    scheme_save(&f, None, scheme_input(true, 0), "default")
        .await
        .unwrap();
    let t = confirm_preview(&f, &p, "confirm-paused").await.unwrap();
    assert_eq!(t["business_unit_id"], json!(unit));
    assert_eq!(t["scheme_snapshot"]["source"], "unit");
    for _ in 0..4 {
        pool_expire_round(&f).await;
        pool_tick(&f).await;
    }
    let t = pool_detail(&f, &t).await;
    assert_eq!(t["items"][0]["dispatch"]["completed_rounds"], 0);
    assert_eq!(t["items"][0]["dispatch"]["reason"], "business_scheme_wait");
    let (c, epoch) = pool_ready(&f, f.device).await;
    scheme_save(&f, Some(unit), scheme_input(true, 1), "open")
        .await
        .unwrap();
    pool_tick(&f).await;
    let t = pool_detail(&f, &t).await;
    let aid = serde_json::from_value(pool_current(&t["items"][0])["id"].clone()).unwrap();
    let packet = pool_delivery(&f, &c, epoch, aid).await;
    assert_eq!(
        packet["plan"]["policy_id"],
        p["plan"]["scheme_snapshot"]["id"]
    );
    assert!(packet["plan"].get("business_unit_id").is_none());
    assert!(packet["plan"].get("scheme_snapshot").is_none());
    assert_eq!(packet["schema"], "durable_execution/1");
    assert_eq!(packet["protocol_version"], 10);
    let inherited = unit_preview(&f, None, f.device, "inherited").await;
    scheme_save(&f, None, scheme_input(false, 1), "new-default")
        .await
        .unwrap();
    assert!(matches!(
        confirm_preview(&f, &inherited, "stale").await,
        Err(wekeeper_api::business::Error::Policy(
            "business_scheme_changed"
        ))
    ));
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn inherited_unit_quotas_are_independent_and_version_changes_do_not_reset_usage() {
    let f = Fixture::with_adapter(false).await;
    let a = scheme_unit(&f).await;
    let b = scheme_unit(&f).await;
    let phone_b = pool_phone(&f, "Phone B").await;
    pool_ready(&f, f.device).await;
    pool_ready(&f, phone_b).await;
    let mut input = scheme_input(true, 0);
    input.limits.total.daily_max_calls = 1;
    scheme_save(&f, None, input, "default-quota").await.unwrap();
    let first = unit_preview(&f, Some(a), f.device, "a1").await;
    let ta = confirm_preview(&f, &first, "ca1").await.unwrap();
    assert_eq!(ta["items"][0]["dispatch"]["state"], "executing");
    let second = unit_preview(&f, Some(a), phone_b, "a2").await;
    let waiting = confirm_preview(&f, &second, "ca2").await.unwrap();
    assert_eq!(
        waiting["items"][0]["dispatch"]["reason"],
        "business_scheme_wait"
    );
    let third = unit_preview(&f, Some(b), phone_b, "b1").await;
    let tb = confirm_preview(&f, &third, "cb1").await.unwrap();
    assert_eq!(tb["items"][0]["dispatch"]["state"], "executing");
    let mut own = scheme_input(true, 0);
    own.limits.total.daily_max_calls = 1;
    scheme_save(&f, Some(a), own, "own").await.unwrap();
    pool_tick(&f).await;
    let waiting = pool_detail(&f, &waiting).await;
    assert_eq!(waiting["items"][0]["dispatch"]["completed_rounds"], 0);
    let keys:Vec<String>=sqlx::query_scalar("SELECT dimension_key FROM mobile_v9_quota_reservations WHERE dimension_key LIKE '%:total' ORDER BY dimension_key").fetch_all(&f.pool).await.unwrap();
    assert_eq!(keys.len(), 2);
    assert!(keys.contains(&format!("unit:{a}:total")));
    assert!(keys.contains(&format!("unit:{b}:total")));
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn resource_budget_is_shared_across_units_and_uncalled_results_release_it() {
    let f = Fixture::with_adapter(false).await;
    let a = scheme_unit(&f).await;
    let b = scheme_unit(&f).await;
    scheme_save(&f, None, scheme_input(true, 0), "default")
        .await
        .unwrap();
    let phone_b = pool_phone(&f, "Phone B").await;
    let (c, epoch) = pool_ready(&f, f.device).await;
    pool_ready(&f, phone_b).await;
    let root:Uuid=sqlx::query_scalar("UPDATE mobile_device_target_bindings SET max_calls=1 WHERE device_id=$1 AND target_id=$2 RETURNING budget_root").bind(f.device).bind(f.targets[0]).fetch_one(&f.pool).await.unwrap();
    let preview = unit_preview(&f, Some(a), f.device, "a").await;
    let first = confirm_preview(&f, &preview, "ca").await.unwrap();
    let aid = serde_json::from_value(pool_current(&first["items"][0])["id"].clone()).unwrap();
    let packet = pool_delivery(&f, &c, epoch, aid).await;
    // Re-enabling or editing a real binding never creates a fresh budget identity.
    sqlx::query("UPDATE mobile_device_target_bindings SET enabled=false WHERE device_id=$1 AND target_id=$2").bind(f.device).bind(f.targets[0]).execute(&f.pool).await.unwrap();
    sqlx::query("UPDATE mobile_device_target_bindings SET enabled=true,valid_until=clock_timestamp()+interval '1 day' WHERE device_id=$1 AND target_id=$2").bind(f.device).bind(f.targets[0]).execute(&f.pool).await.unwrap();
    let preview = unit_preview(&f, Some(b), f.device, "b").await;
    let second = confirm_preview(&f, &preview, "cb").await.unwrap();
    assert_eq!(
        pool_current(&second["items"][0])["plan"]["device_id"],
        json!(phone_b)
    );
    let consumed: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM mobile_execution_resource_charges WHERE budget_root=$1",
    )
    .bind(root)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(consumed, 1);
    f.repo()
        .complete_execution(completion(&packet, "not_called"))
        .await
        .unwrap();
    // A definitive uncalled result frees the cumulative allowance, even for another unit.
    let third_unit = scheme_unit(&f).await;
    let preview = unit_preview(&f, Some(third_unit), f.device, "b-again").await;
    let third = confirm_preview(&f, &preview, "cb-again").await.unwrap();
    assert_eq!(third["items"][0]["dispatch"]["state"], "executing");
    assert_eq!(
        pool_current(&third["items"][0])["plan"]["device_id"],
        json!(f.device)
    );
    let consumed:i64=sqlx::query_scalar("SELECT count(*) FROM mobile_execution_resource_charges c JOIN mobile_execution_grants g ON g.id=c.execution_id WHERE c.budget_root=$1 AND coalesce(g.result_body->>'outcome','pending') NOT IN ('not_called','failed')").bind(root).fetch_one(&f.pool).await.unwrap();
    assert_eq!(consumed, 1);
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn singleton_phone_quota_wait_does_not_exhaust_dispatch_rounds() {
    let f = Fixture::with_adapter(false).await;
    let unit = scheme_unit(&f).await;
    let mut input = scheme_input(true, 0);
    input.limits.device.daily_max_calls = 1;
    scheme_save(&f, Some(unit), input, "device-limit")
        .await
        .unwrap();
    let (c, epoch) = pool_ready(&f, f.device).await;
    let p = unit_preview(&f, Some(unit), f.device, "first").await;
    let first = confirm_preview(&f, &p, "confirm-first").await.unwrap();
    let aid = serde_json::from_value(pool_current(&first["items"][0])["id"].clone()).unwrap();
    let packet = pool_delivery(&f, &c, epoch, aid).await;
    f.repo()
        .complete_execution(completion(&packet, "observed_sent"))
        .await
        .unwrap();
    sqlx::query("UPDATE mobile_v9_quota_reservations SET recorded_at=clock_timestamp()-interval '2 seconds'").execute(&f.pool).await.unwrap();
    // Only the device dimension is exhausted: total and target remain available.
    let p = unit_preview(&f, Some(unit), f.device, "second").await;
    let task = confirm_preview(&f, &p, "confirm-second").await.unwrap();
    for _ in 0..4 {
        pool_expire_round(&f).await;
        pool_tick(&f).await;
    }
    let task = pool_detail(&f, &task).await;
    assert_eq!(task["items"][0]["dispatch"]["completed_rounds"], 0);
    assert_eq!(task["items"][0]["dispatch"]["reason"], "quota_wait");
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn dual_client_same_phone_preserves_kind_and_rejects_mixed_targets() {
    use wekeeper_api::distribution::domain::shared_mobile::{
        MobileClient, business_scheme::TaskPreviewInput,
    };
    let f = Fixture::with_adapter(false).await;
    // Same display name, same physical device, separate application identities.
    sqlx::query("UPDATE mobile_targets SET client_kind='wechat' WHERE id=$1")
        .bind(f.targets[1])
        .execute(&f.pool)
        .await
        .unwrap();
    let request = |kind, targets| TaskPreviewInput {
        client_kind: kind,
        business_unit_id: None,
        draft_id: None,
        expected_revision: None,
        target_ids: targets,
        message: "双客户端通知".into(),
        between_targets_seconds: 30,
        selections: vec![],
    };
    let repo = f.repo();
    for (position, kind) in [MobileClient::Wecom, MobileClient::Wechat]
        .into_iter()
        .enumerate()
    {
        let result = repo
            .preview_task(
                &f.actor,
                &format!("client-{position}"),
                request(kind, vec![f.targets[position]]),
            )
            .await
            .unwrap();
        assert_eq!(result["draft"]["client_kind"], kind.as_str());
        let item = &result["preview"]["plan"]["items"][0];
        assert_eq!(item["target"]["client_kind"], kind.as_str());
        assert_eq!(
            item["candidates"][0]["execution"]["capability"],
            kind.capability()
        );
        assert_eq!(item["candidates"][0]["device_id"], json!(f.device));
    }
    assert!(
        repo.preview_task(
            &f.actor,
            "mixed",
            request(MobileClient::Wechat, f.targets.clone())
        )
        .await
        .is_err()
    );
    f.close().await;
}
