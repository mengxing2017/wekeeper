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
    school: Uuid,
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
        let school = Uuid::new_v4();
        let device = Uuid::new_v4();
        sqlx::query("INSERT INTO accounts(id,username,display_name,platform_admin) VALUES($1,'v9-admin','Admin',true)").bind(actor_id).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO schools(id,name) VALUES($1,'V9 test')")
            .bind(school)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO school_memberships(school_id,account_id,role) VALUES($1,$2,'admin')",
        )
        .bind(school)
        .bind(actor_id)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO execution_devices(id,name,enabled,credential_generation) VALUES($1,'Offline phone',true,1)").bind(device).execute(&pool).await.unwrap();
        let targets = vec![Uuid::new_v4(), Uuid::new_v4()];
        for target in &targets {
            sqlx::query("INSERT INTO mobile_targets(id,school_id,search_text,created_by,updated_by) VALUES($1,$2,'测试群',$3,$3)").bind(target).bind(school).bind(actor_id).execute(&pool).await.unwrap();
            sqlx::query("INSERT INTO mobile_device_target_bindings(id,school_id,device_id,target_id,created_by,updated_by) VALUES($1,$2,$3,$4,$5,$5)").bind(Uuid::new_v4()).bind(school).bind(device).bind(target).bind(actor_id).execute(&pool).await.unwrap();
        }
        let release = Uuid::new_v4();
        let acceptance = Uuid::new_v4();
        if seed_adapter {
            Self::seed_adapter(&pool, actor_id, device, release, acceptance).await;
        }
        let actor=serde_json::from_value(json!({"id":actor_id,"username":"v9-admin","display_name":"Admin","platform_admin":true,"grants":[],"wecom_authenticated":false})).unwrap();
        Self {
            pool,
            base,
            schema,
            actor,
            school,
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

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn offline_binding_is_selectable_and_confirmable_without_environment_or_consent() {
    let f = Fixture::new().await;
    f.activate("activate").await.unwrap();
    let r = f.repo();
    let policy:PolicyInput=serde_json::from_value(json!({"expected_revision":0,"mode":"enabled","confirmation_seconds":600,"inspection_seconds":600,"retry_seconds":300,"retry_limit":3,"timezone":"Asia/Shanghai","allowed_windows":[{"weekdays":[1,2,3,4,5,6,7],"start_minute":0,"end_minute":1440}],"limits":ObjectLimits::value(),"reason":"Synthetic test"})).unwrap();
    r.platform_write(&f.actor, "policy", PlatformCommand::Policy(policy))
        .await
        .unwrap();
    r.platform_write(
        &f.actor,
        "scope",
        PlatformCommand::Manage(ManagementCommand::Scopes(ScopeInput {
            acceptance_id: f.acceptance,
            school_id: f.school,
            device_ids: vec![f.device],
            target_ids: f.targets.clone(),
            auto_targets: false,
            max_calls: 2,
            expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
            expected_access: [(f.device, 0)].into(),
            reason: "Synthetic test".into(),
        })),
    )
    .await
    .unwrap();
    let status = r.school_read(&f.actor, f.school, None).await.unwrap();
    assert_eq!(status["readiness"][0]["plan_selectable"], true);
    assert_eq!(status["readiness"][0]["confirmable"], true);
    assert_eq!(status["readiness"][0]["execution_ready"], false);
    let input = DraftInput {
        target_ids: f.targets.clone(),
        message: "  测试\n🙂  ".into(),
        content_type: "text/plain".into(),
        expected_revision: None,
        between_targets_seconds: 30,
    };
    let draft = r
        .school_write(
            &f.actor,
            f.school,
            "draft",
            SchoolCommand::Draft(None, input),
        )
        .await
        .unwrap();
    let preview = r
        .school_write(
            &f.actor,
            f.school,
            "preview",
            SchoolCommand::Preview(PreviewInput {
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
        .school_write(
            &f.actor,
            f.school,
            "confirm",
            SchoolCommand::Confirm(confirm.clone()),
        )
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
            .all(|i| i["attempt_state"] == "queued")
    );
    let receipt_detail = r.school_read(&f.actor, f.school, Some(id)).await.unwrap();
    assert!(
        receipt_detail["items"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|i| i["attempts"].as_array().unwrap())
            .any(|a| a["id"] == json!(aid) && a["device_received_at"].is_string())
    );
    r.dispatch_task_wakes().await.unwrap();
    r.dispatch_task_wakes().await.unwrap();
    let command_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM device_control_commands WHERE device_id=$1 AND kind='task_available'",
    )
    .bind(f.device)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert!(command_count > 0);
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

    let detail = r.school_read(&f.actor, f.school, Some(id)).await.unwrap();
    let status = &detail["items"][0]["attempts"][0]["execution_status"];
    assert_eq!(status["phase"], "waiting");
    assert_eq!(status["conditions"][0]["code"], "waiting_resources");
    assert_eq!(status["conditions"][0]["scope_type"], "item");
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
    assert_eq!(task["cursor_position"], 0);
    assert_eq!(task["items"].as_array().unwrap().len(), 2);
    assert_eq!(
        task["items"][0]["attempts"][0]["plan"]["message"],
        "  测试\n🙂  "
    );
    assert_eq!(
        r.school_write(
            &f.actor,
            f.school,
            "confirm",
            SchoolCommand::Confirm(confirm)
        )
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

struct ObjectLimits;
impl ObjectLimits {
    fn value() -> Value {
        let l = json!({"interval_seconds":60,"window_seconds":3600,"max_calls":10,"daily_max_calls":20});
        json!({"global":l,"school":l,"device":l,"target":l})
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
async fn business_usage_is_atomic_scoped_idempotent_and_survives_delivery_changes() {
    use wekeeper_api::distribution::domain::shared_mobile::usage::*;
    let f = Fixture::new().await;
    let r = f.repo();
    let input = UsageInput {
        device_ids: vec![f.device],
        targets: vec![UsageTarget {
            id: Some(f.targets[0]),
            name: "测试群".into(),
            search_text: "测试群".into(),
        }],
        max_calls: 5,
        expires_at: chrono::Utc::now() + chrono::Duration::hours(2),
        auto_targets: false,
        renewal_of: None,
        expected_revision: None,
    };
    let preview = r
        .usage_write(
            &f.actor,
            f.school,
            "usage-preview",
            UsageCommand::Preview(input.clone()),
        )
        .await
        .unwrap();
    let confirm = UsageCommand::Confirm(UsageConfirm {
        preview_id: serde_json::from_value(preview["id"].clone()).unwrap(),
        digest: preview["digest"].as_str().unwrap().into(),
    });
    let approved = r
        .usage_write(&f.actor, f.school, "usage-confirm", confirm.clone())
        .await
        .unwrap();
    assert_eq!(approved["items"][0]["approval"], "approved");
    assert_eq!(approved["items"][0]["delivery"], "available");
    assert_eq!(
        r.usage_write(&f.actor, f.school, "usage-confirm", confirm.clone())
            .await
            .unwrap(),
        approved
    );
    assert_eq!(
        r.usage_write(&f.actor, f.school, "same-summary-new-key", confirm)
            .await
            .unwrap(),
        approved
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM mobile_usages")
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        1
    );
    let id: Uuid = serde_json::from_value(approved["items"][0]["id"].clone()).unwrap();
    let school_actor_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO accounts(id,username,display_name) VALUES($1,'school-only','School admin')",
    )
    .bind(school_actor_id)
    .execute(&f.pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO school_memberships(school_id,account_id,role) VALUES($1,$2,'admin')")
        .bind(f.school)
        .bind(school_actor_id)
        .execute(&f.pool)
        .await
        .unwrap();
    let school_actor:Actor=serde_json::from_value(json!({"id":school_actor_id,"username":"school-only","display_name":"School admin","platform_admin":false,"grants":[],"wecom_authenticated":false})).unwrap();
    assert!(
        r.usage_read(&school_actor, None, None, false)
            .await
            .is_err()
    );
    assert!(
        r.usage_read(&school_actor, Some(Uuid::new_v4()), None, false)
            .await
            .is_err()
    );
    // The school must reuse the approved budget rather than mint a second allowance.
    let p = r
        .usage_write(
            &school_actor,
            f.school,
            "school-preview",
            UsageCommand::Preview(input.clone()),
        )
        .await
        .unwrap();
    let reused = r
        .usage_write(
            &school_actor,
            f.school,
            "school-confirm",
            UsageCommand::Confirm(UsageConfirm {
                preview_id: serde_json::from_value(p["id"].clone()).unwrap(),
                digest: p["digest"].as_str().unwrap().into(),
            }),
        )
        .await
        .unwrap();
    assert_eq!(reused["items"][0]["id"], json!(id));
    // Renewal retains the original budget, requires platform approval, and never mutates old scopes.
    let mut renewal = input;
    renewal.renewal_of = Some(id);
    renewal.expected_revision = Some(1);
    renewal.expires_at += chrono::Duration::hours(1);
    let p = r
        .usage_write(
            &school_actor,
            f.school,
            "renew-preview",
            UsageCommand::Preview(renewal),
        )
        .await
        .unwrap();
    let pending = r
        .usage_write(
            &school_actor,
            f.school,
            "renew-confirm",
            UsageCommand::Confirm(UsageConfirm {
                preview_id: serde_json::from_value(p["id"].clone()).unwrap(),
                digest: p["digest"].as_str().unwrap().into(),
            }),
        )
        .await
        .unwrap();
    assert_eq!(pending["items"][0]["approval"], "pending");
    let next: Uuid = serde_json::from_value(pending["items"][0]["id"].clone()).unwrap();
    assert!(
        r.usage_write(
            &school_actor,
            f.school,
            "forged-approve",
            UsageCommand::Mutate {
                id: next,
                operation: "approve".into(),
                mutation: UsageMutation {
                    expected_revision: 1,
                    reason: "越权".into()
                }
            }
        )
        .await
        .is_err()
    );
    let roots: Vec<Uuid> = sqlx::query_scalar("SELECT budget_root FROM mobile_usages")
        .fetch_all(&f.pool)
        .await
        .unwrap();
    assert_eq!(roots[0], roots[1]);
    let paused = r
        .usage_write(
            &f.actor,
            f.school,
            "pause-usage",
            UsageCommand::Mutate {
                id,
                operation: "pause".into(),
                mutation: UsageMutation {
                    expected_revision: 1,
                    reason: "暂停使用".into(),
                },
            },
        )
        .await
        .unwrap();
    assert_eq!(paused["items"][0]["state"], "paused");
    assert!(
        r.usage_write(
            &f.actor,
            f.school,
            "stale-revoke",
            UsageCommand::Mutate {
                id,
                operation: "revoke".into(),
                mutation: UsageMutation {
                    expected_revision: 1,
                    reason: "过期修订".into()
                }
            }
        )
        .await
        .is_err()
    );
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn business_scope_materializes_without_delivery_acceptance_or_activation_and_pause_blocks_plans()
 {
    use wekeeper_api::distribution::domain::shared_mobile::usage::*;
    let f = Fixture::new().await;
    let r = f.repo();
    // The actual executable rule is a synthetic fixture; no acceptance/delivery/activation
    // records are required to authorize resources or confirm a business task.
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM mobile_capability_deliveries")
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM mobile_v9_activations")
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        0
    );
    let p:PolicyInput=serde_json::from_value(json!({"expected_revision":0,"mode":"enabled","confirmation_seconds":600,"inspection_seconds":600,"retry_seconds":300,"retry_limit":3,"timezone":"Asia/Shanghai","allowed_windows":[{"weekdays":[1,2,3,4,5,6,7],"start_minute":0,"end_minute":1440}],"limits":ObjectLimits::value(),"reason":"Synthetic test"})).unwrap();
    r.platform_write(&f.actor, "business-policy", PlatformCommand::Policy(p))
        .await
        .unwrap();
    let input = UsageInput {
        device_ids: vec![f.device],
        targets: vec![UsageTarget {
            id: None,
            name: "同页新增群".into(),
            search_text: "  同页新增群  ".into(),
        }],
        max_calls: 5,
        expires_at: chrono::Utc::now() + chrono::Duration::hours(2),
        auto_targets: false,
        renewal_of: None,
        expected_revision: None,
    };
    let p = r
        .usage_write(
            &f.actor,
            f.school,
            "production-preview",
            UsageCommand::Preview(input),
        )
        .await
        .unwrap();
    let u = r
        .usage_write(
            &f.actor,
            f.school,
            "production-confirm",
            UsageCommand::Confirm(UsageConfirm {
                preview_id: serde_json::from_value(p["id"].clone()).unwrap(),
                digest: p["digest"].as_str().unwrap().into(),
            }),
        )
        .await
        .unwrap();
    assert_eq!(u["items"][0]["delivery"], "available");
    r.reconcile_usages().await.unwrap();
    r.reconcile_usages().await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM mobile_usage_scopes")
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        1
    );
    let target: Uuid =
        serde_json::from_value(u["items"][0]["body"]["targets"][0]["id"].clone()).unwrap();
    let send = UsageTaskInput {
        draft_id: None,
        expected_revision: None,
        target_ids: vec![target],
        message: "  一步预览\n🙂  ".into(),
        between_targets_seconds: 30,
        selections: vec![],
    };
    let preview = r
        .usage_write(
            &f.actor,
            f.school,
            "auto-save-preview",
            UsageCommand::TaskPreview(send),
        )
        .await
        .unwrap();
    assert!(preview["draft"].get("stage").is_none());
    assert_eq!(preview["preview"]["plan"]["message"], "  一步预览\n🙂  ");
    assert!(preview["preview"]["plan"]["items"][0]["blocker"].is_null());
    let task = r
        .usage_write(
            &f.actor,
            f.school,
            "confirm-without-delivery",
            UsageCommand::TaskConfirm(Confirmation {
                preview_id: serde_json::from_value(preview["preview"]["id"].clone()).unwrap(),
                preview_digest: preview["preview"]["digest"].as_str().unwrap().into(),
                confirmed: true,
            }),
        )
        .await
        .unwrap();
    assert_eq!(task["items"][0]["attempts"][0]["state"], "queued");
    assert_eq!(
        task["items"][0]["attempts"][0]["plan"]["device_id"],
        json!(f.device)
    );

    let id: Uuid = serde_json::from_value(u["items"][0]["id"].clone()).unwrap();
    r.usage_write(
        &f.actor,
        f.school,
        "pause-production",
        UsageCommand::Mutate {
            id,
            operation: "pause".into(),
            mutation: UsageMutation {
                expected_revision: 1,
                reason: "暂停".into(),
            },
        },
    )
    .await
    .unwrap();
    let overview = r.school_read(&f.actor, f.school, None).await.unwrap();
    let row = overview["readiness"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["target_id"] == json!(target))
        .unwrap();
    assert_eq!(row["confirmable"], false);
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn business_configuration_does_not_require_technical_delivery() {
    use wekeeper_api::distribution::domain::shared_mobile::usage::*;
    let f = Fixture::with_adapter(false).await;
    let r = f.repo();
    let options = r
        .usage_read(&f.actor, Some(f.school), None, true)
        .await
        .unwrap();
    assert!(options["sending_status"].get("delivery").is_none());
    assert_eq!(options["sending_status"]["platform"], "default");
    let result = r
        .usage_write(
            &f.actor,
            f.school,
            "blocked-preview",
            UsageCommand::TaskPreview(UsageTaskInput {
                draft_id: None,
                expected_revision: None,
                target_ids: vec![f.targets[0]],
                message: "保存但不发送".into(),
                between_targets_seconds: 30,
                selections: vec![],
            }),
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
        .usage_write(
            &f.actor,
            f.school,
            "binding-only-confirm",
            UsageCommand::TaskConfirm(Confirmation {
                preview_id: Uuid::parse_str(p["id"].as_str().unwrap()).unwrap(),
                preview_digest: p["digest"].as_str().unwrap().into(),
                confirmed: true,
            }),
        )
        .await;
    let task = task.unwrap();
    assert_eq!(task["items"][0]["attempts"][0]["state"], "queued");
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM mobile_v9_scope_grants")
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        0
    );
    let mut input: BusinessSchemeInput = serde_json::from_str(include_str!(
        "../../api-contract/fixtures/mobile-business-scheme.json"
    ))
    .unwrap();
    r.platform_write(
        &f.actor,
        "closed-scheme",
        PlatformCommand::Policy(input.clone().policy().unwrap()),
    )
    .await
    .unwrap();
    assert!(
        r.platform_write(
            &f.actor,
            "stale-scheme",
            PlatformCommand::Policy(input.clone().policy().unwrap())
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
    r.platform_write(
        &f.actor,
        "open-no-delivery",
        PlatformCommand::Policy(input.policy().unwrap()),
    )
    .await
    .unwrap();
    let token = "wk_device_device-owned-regression";
    sqlx::query("INSERT INTO device_credentials(token_hash,device_id,generation,expires_at) VALUES($1,$2,1,clock_timestamp()+interval '1 hour')").bind(wekeeper_api::business::hash(token)).bind(f.device).execute(&f.pool).await.unwrap();
    let c = r.credential(token).await.unwrap();
    let context = r.delegation_read(&c).await.unwrap();
    r.device_write(
        &c,
        "device-owned-consent",
        DeviceCommand::Consent(ConsentInput {
            delegation_id: serde_json::from_value(context["delegation"]["id"].clone()).unwrap(),
            scope_digest: context["delegation"]["scope_digest"]
                .as_str()
                .unwrap()
                .into(),
            expected_revision: 0,
            accepted: true,
        }),
    )
    .await
    .unwrap();
    let epoch = Uuid::new_v4();
    sqlx::query("INSERT INTO device_control_sessions(device_id,connection_id,transport,credential_generation,last_heartbeat_at) VALUES($1,$2,'https',1,clock_timestamp())").bind(f.device).bind(Uuid::new_v4()).execute(&f.pool).await.unwrap();
    sqlx::query("INSERT INTO mobile_shared_runtime(device_id,credential_generation,runtime_epoch,ui_ready,paused,quiescent,observed_at) VALUES($1,1,$2,true,false,true,clock_timestamp())").bind(f.device).bind(epoch).execute(&f.pool).await.unwrap();
    sqlx::query("INSERT INTO mobile_v9_runtime_details(device_id,credential_generation,ui_context_epoch,conditions,apk_sha256) VALUES($1,1,'local','{}',repeat('a',64))").bind(f.device).execute(&f.pool).await.unwrap();
    let aid: Uuid = serde_json::from_value(task["items"][0]["attempts"][0]["id"].clone()).unwrap();
    let grant = r
        .device_write(
            &c,
            "device-owned-grant",
            DeviceCommand::Grant(
                wekeeper_api::distribution::domain::shared_mobile::execution::GrantRequest {
                    runtime_epoch: epoch,
                    eligible_attempt_ids: vec![aid],
                    inspection_id: None,
                },
            ),
        )
        .await
        .unwrap();
    assert_eq!(grant["state"], "granted", "{grant}");
    assert_eq!(grant["plan"]["execution"]["contract"], "device_owned/1");
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
async fn approved_business_resources_exist_without_any_adapter_or_delivery_records() {
    use wekeeper_api::distribution::domain::shared_mobile::usage::*;
    let f = Fixture::with_adapter(false).await;
    let r = f.repo();
    // No policy row, activation, acceptance or runtime is seeded. Default business
    // settings must be sufficient for authorization, preview and confirmation.
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM mobile_v9_send_policy_versions")
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        0
    );
    let input = UsageInput {
        device_ids: vec![f.device],
        targets: vec![UsageTarget {
            id: None,
            name: "新增目标".into(),
            search_text: "新增目标".into(),
        }],
        max_calls: 5,
        expires_at: chrono::Utc::now() + chrono::Duration::days(3),
        auto_targets: false,
        renewal_of: None,
        expected_revision: None,
    };
    let p = r
        .usage_write(
            &f.actor,
            f.school,
            "business-only-preview",
            UsageCommand::Preview(input),
        )
        .await
        .unwrap();
    let u = r
        .usage_write(
            &f.actor,
            f.school,
            "business-only-confirm",
            UsageCommand::Confirm(UsageConfirm {
                preview_id: serde_json::from_value(p["id"].clone()).unwrap(),
                digest: p["digest"].as_str().unwrap().into(),
            }),
        )
        .await
        .unwrap();
    assert_eq!(u["items"][0]["delivery"], "available");
    let id: Uuid = serde_json::from_value(u["items"][0]["id"].clone()).unwrap();
    sqlx::query(
        "UPDATE mobile_usages SET confirmed_until=clock_timestamp()-interval '1 day' WHERE id=$1",
    )
    .bind(id)
    .execute(&f.pool)
    .await
    .unwrap();
    r.reconcile_usages().await.unwrap();
    r.reconcile_usages().await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM mobile_usage_scopes WHERE usage_id=$1 AND release_id IS NULL"
        )
        .bind(id)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        1
    );
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM mobile_v9_scope_grants WHERE release_id IS NULL AND acceptance_id IS NULL").fetch_one(&f.pool).await.unwrap(),1);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM mobile_v9_adapter_releases")
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM mobile_v9_acceptance_runs")
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM mobile_shared_school_access WHERE enabled"
        )
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        1
    );
    let mut stopped: BusinessSchemeInput = serde_json::from_str(include_str!(
        "../../api-contract/fixtures/mobile-business-scheme.json"
    ))
    .unwrap();
    stopped.enabled = false;
    r.platform_write(
        &f.actor,
        "pause-execution-only",
        PlatformCommand::Policy(stopped.policy().unwrap()),
    )
    .await
    .unwrap();
    r.reconcile_usages().await.unwrap();
    let options = r
        .usage_read(&f.actor, Some(f.school), None, true)
        .await
        .unwrap();
    assert_eq!(options["sending_status"]["can_confirm"], true);
    let target: Uuid =
        serde_json::from_value(u["items"][0]["body"]["targets"][0]["id"].clone()).unwrap();
    let overview = r.school_read(&f.actor, f.school, None).await.unwrap();
    let row = overview["readiness"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["target_id"] == json!(target))
        .unwrap();
    assert_eq!(row["plan_selectable"], true);
    // Platform accepts business intent without any downstream UI configuration.
    let p = r
        .usage_write(
            &f.actor,
            f.school,
            "no-rules-task-preview",
            UsageCommand::TaskPreview(UsageTaskInput {
                draft_id: None,
                expected_revision: None,
                target_ids: vec![target],
                message: "正常下发".into(),
                between_targets_seconds: 30,
                selections: vec![],
            }),
        )
        .await
        .unwrap();
    assert!(p["preview"]["plan"]["items"][0]["blocker"].is_null());
    let task = r
        .usage_write(
            &f.actor,
            f.school,
            "no-rules-task-confirm",
            UsageCommand::TaskConfirm(Confirmation {
                preview_id: serde_json::from_value(p["preview"]["id"].clone()).unwrap(),
                preview_digest: p["preview"]["digest"].as_str().unwrap().into(),
                confirmed: true,
            }),
        )
        .await
        .unwrap();
    let plan = &task["items"][0]["attempts"][0]["plan"];
    assert_eq!(plan["execution"]["contract"], "device_owned/1");
    assert!(plan.get("flow").is_none());
    assert!(plan.get("flow_digest").is_none());
    assert_eq!(task["items"][0]["attempts"][0]["state"], "queued");
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn durable_grant_replay_and_duplicate_receipt_do_not_extend_deadline() {
    let f = Fixture::with_adapter(false).await;
    let b = pool_phone(&f, "B").await;
    let (ca, ea) = pool_ready(&f, f.device).await;
    let (_cb, _eb) = pool_ready(&f, b).await;
    let task = pool_confirm(&f, vec![f.targets[0]]).await;
    pool_tick(&f).await;
    let detail = pool_detail(&f, &task).await;
    let aid = serde_json::from_value(pool_current(&detail["items"][0])["id"].clone()).unwrap();
    let grant = pool_claim(&f, &ca, ea, aid).await;
    assert_eq!(grant["state"], "granted", "{grant}");

    let request = wekeeper_api::distribution::domain::shared_mobile::execution::GrantRequest {
        runtime_epoch: ea,
        eligible_attempt_ids: vec![aid],
        inspection_id: None,
    };
    let replay = f
        .repo()
        .device_write(&ca, "pool-grant", DeviceCommand::Grant(request))
        .await
        .unwrap();
    assert_eq!(grant, replay);
    let receipt = wekeeper_api::distribution::domain::shared_mobile::execution::ExecutionReceipt {
        execution_id: serde_json::from_value(grant["id"].clone()).unwrap(),
    };
    f.repo()
        .device_write(&ca, "received", DeviceCommand::Receipt(receipt.clone()))
        .await
        .unwrap();
    let first: chrono::DateTime<chrono::Utc> = sqlx::query_scalar(
        "SELECT result_deadline_at FROM mobile_execution_grants WHERE attempt_id=$1",
    )
    .bind(aid)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    f.repo()
        .device_write(&ca, "received-again", DeviceCommand::Receipt(receipt))
        .await
        .unwrap();
    let second: chrono::DateTime<chrono::Utc> = sqlx::query_scalar(
        "SELECT result_deadline_at FROM mobile_execution_grants WHERE attempt_id=$1",
    )
    .bind(aid)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(first, second);
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn usage_overrides_reuse_and_renewal_keep_one_business_family() {
    use wekeeper_api::distribution::domain::shared_mobile::usage::*;
    let f = Fixture::with_adapter(false).await;
    let r = f.repo();
    let second = Uuid::now_v7();
    sqlx::query("INSERT INTO execution_devices(id,name,enabled,credential_generation) VALUES($1,'第二台手机',true,1)").bind(second).execute(&f.pool).await.unwrap();
    sqlx::query("INSERT INTO mobile_device_target_bindings(id,school_id,device_id,target_id,search_text_override,created_by,updated_by) VALUES($1,$2,$3,$4,'第二台专用群',$5,$5)").bind(Uuid::now_v7()).bind(f.school).bind(second).bind(f.targets[0]).bind(f.actor.id).execute(&f.pool).await.unwrap();
    sqlx::query("UPDATE mobile_device_target_bindings SET search_text_override='手机专用群',locator_revision=locator_revision+1 WHERE device_id=$1 AND target_id=$2").bind(f.device).bind(f.targets[0]).execute(&f.pool).await.unwrap();
    let input = UsageInput {
        device_ids: vec![f.device, second],
        targets: vec![UsageTarget {
            id: Some(f.targets[0]),
            name: "测试群".into(),
            search_text: "测试群".into(),
        }],
        max_calls: 10,
        expires_at: chrono::Utc::now() + chrono::Duration::days(10),
        auto_targets: false,
        renewal_of: None,
        expected_revision: None,
    };
    let p = r
        .usage_write(
            &f.actor,
            f.school,
            "override-preview",
            UsageCommand::Preview(input.clone()),
        )
        .await
        .unwrap();
    assert_eq!(p["body"]["locators"][0]["search_text"], "手机专用群");
    assert_eq!(p["body"]["locators"][1]["search_text"], "第二台专用群");
    assert!(p["body"]["locators"][0]["target_locator_revision"].is_null());
    // Display aliases and unused default names are not the phone's location.
    sqlx::query("UPDATE mobile_targets SET alias='显示名已更新',search_text='无关默认搜索名',locator_revision=locator_revision+1 WHERE id=$1").bind(f.targets[0]).execute(&f.pool).await.unwrap();
    let opened = r
        .usage_write(
            &f.actor,
            f.school,
            "override-confirm",
            UsageCommand::Confirm(UsageConfirm {
                preview_id: serde_json::from_value(p["id"].clone()).unwrap(),
                digest: p["digest"].as_str().unwrap().into(),
            }),
        )
        .await
        .unwrap();
    assert_eq!(opened["items"][0]["delivery"], "available");
    let id: Uuid = serde_json::from_value(opened["items"][0]["id"].clone()).unwrap();
    let root = opened["items"][0]["budget_root"].clone();
    let p = r
        .usage_write(
            &f.actor,
            f.school,
            "repeat-preview",
            UsageCommand::Preview(input.clone()),
        )
        .await
        .unwrap();
    let reused = r
        .usage_write(
            &f.actor,
            f.school,
            "repeat-confirm",
            UsageCommand::Confirm(UsageConfirm {
                preview_id: serde_json::from_value(p["id"].clone()).unwrap(),
                digest: p["digest"].as_str().unwrap().into(),
            }),
        )
        .await
        .unwrap();
    assert_eq!(reused["items"][0]["id"], json!(id));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM mobile_usages")
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        1
    );
    r.usage_write(
        &f.actor,
        f.school,
        "pause-family",
        UsageCommand::Mutate {
            id,
            operation: "pause".into(),
            mutation: UsageMutation {
                expected_revision: 1,
                reason: "暂停使用".into(),
            },
        },
    )
    .await
    .unwrap();
    let mut renewal = input;
    renewal.renewal_of = Some(id);
    renewal.expected_revision = Some(2);
    renewal.expires_at += chrono::Duration::days(30);
    let p = r
        .usage_write(
            &f.actor,
            f.school,
            "renew-override-preview",
            UsageCommand::Preview(renewal.clone()),
        )
        .await
        .unwrap();
    let renewed = r
        .usage_write(
            &f.actor,
            f.school,
            "renew-override-confirm",
            UsageCommand::Confirm(UsageConfirm {
                preview_id: serde_json::from_value(p["id"].clone()).unwrap(),
                digest: p["digest"].as_str().unwrap().into(),
            }),
        )
        .await
        .unwrap();
    assert_eq!(renewed["items"][0]["supersedes"], json!(id));
    assert_eq!(renewed["items"][0]["budget_root"], root);
    assert_eq!(renewed["items"][0]["state"], "paused");
    assert!(
        r.usage_write(
            &f.actor,
            f.school,
            "fork-preview",
            UsageCommand::Preview(renewal)
        )
        .await
        .is_err()
    );
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn delegation_discovery_and_consent_ignore_the_global_housekeeping_lock() {
    use wekeeper_api::distribution::domain::device_control::Credential;
    let f = Fixture::with_adapter(false).await;
    let token = "wk_device_synthetic-discovery-test";
    let hash = wekeeper_api::business::hash(token);
    sqlx::query("INSERT INTO device_credentials(token_hash,device_id,generation,expires_at) VALUES($1,$2,1,clock_timestamp()+interval '1 hour')").bind(&hash).bind(f.device).execute(&f.pool).await.unwrap();
    let r = f.repo();
    let c = Credential {
        device_id: f.device,
        generation: 1,
        token_hash: hash,
    };
    let mut held = f.pool.begin().await.unwrap();
    sqlx::query("SELECT pg_advisory_xact_lock(78234610243)")
        .execute(&mut *held)
        .await
        .unwrap();
    let one = tokio::time::timeout(std::time::Duration::from_secs(2), r.delegation_read(&c))
        .await
        .expect("discovery must not wait for unrelated housekeeping")
        .unwrap();
    let two = r.delegation_read(&c).await.unwrap();
    assert_eq!(one["delegation"]["id"], two["delegation"]["id"]);
    let input = ConsentInput {
        delegation_id: serde_json::from_value(one["delegation"]["id"].clone()).unwrap(),
        scope_digest: one["delegation"]["scope_digest"].as_str().unwrap().into(),
        expected_revision: 0,
        accepted: true,
    };
    let decision = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        r.device_write(&c, "stable-consent", DeviceCommand::Consent(input.clone())),
    )
    .await
    .expect("consent must not wait for global housekeeping")
    .unwrap();
    assert_eq!(decision["accepted"], true);
    assert_eq!(
        decision,
        r.device_write(&c, "stable-consent", DeviceCommand::Consent(input))
            .await
            .unwrap()
    );
    held.rollback().await.unwrap();
    sqlx::query("UPDATE execution_devices SET enabled=false WHERE id=$1")
        .bind(f.device)
        .execute(&f.pool)
        .await
        .unwrap();
    assert!(
        r.credential(token).await.is_ok(),
        "valid credential must remain usable for read/legitimate finalization"
    );
    assert_eq!(
        r.delegation_read(&c).await.unwrap()["delegation"]["decision"]["accepted"],
        true
    );
    sqlx::query("UPDATE device_credentials SET revoked_at=clock_timestamp() WHERE device_id=$1")
        .bind(f.device)
        .execute(&f.pool)
        .await
        .unwrap();
    assert!(r.credential(token).await.is_err());
    assert!(r.delegation_read(&c).await.is_err());
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn reconciliation_never_schedules_downstream_ui_checks() {
    let f = Fixture::new().await;
    let r = f.repo();
    let token = "wk_device_synthetic-check-isolation";
    sqlx::query("INSERT INTO device_credentials(token_hash,device_id,generation,expires_at) VALUES($1,$2,1,clock_timestamp()+interval '1 hour')").bind(wekeeper_api::business::hash(token)).bind(f.device).execute(&f.pool).await.unwrap();
    let c = r.credential(token).await.unwrap();
    let context = r.delegation_read(&c).await.unwrap();
    r.device_write(
        &c,
        "consent",
        DeviceCommand::Consent(ConsentInput {
            delegation_id: serde_json::from_value(context["delegation"]["id"].clone()).unwrap(),
            scope_digest: context["delegation"]["scope_digest"]
                .as_str()
                .unwrap()
                .into(),
            expected_revision: 0,
            accepted: true,
        }),
    )
    .await
    .unwrap();
    sqlx::query(&format!("CREATE FUNCTION reject_one_check() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.target_id='{}'::uuid THEN RAISE EXCEPTION 'synthetic check insert failure'; END IF; RETURN NEW; END $$",f.targets[0])).execute(&f.pool).await.unwrap();
    sqlx::query("CREATE TRIGGER reject_one_check BEFORE INSERT ON mobile_v9_send_inspections FOR EACH ROW EXECUTE FUNCTION reject_one_check()").execute(&f.pool).await.unwrap();
    let mut global = f.pool.begin().await.unwrap();
    sqlx::query("SELECT pg_advisory_xact_lock(78234610243)")
        .execute(&mut *global)
        .await
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), r.reconcile_usages())
        .await
        .expect("check scheduling must not need the global execution lock")
        .unwrap();
    global.rollback().await.unwrap();
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
    r.reconcile_usages().await.unwrap();
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

// ---- §9 regression acceptance: business control state is resolved by version ----
// These cover the false-block review: history/pending/superseded records and generic
// technical revision drift must never act as an implicit disable, while an explicit
// pause, revocation or expiry on the operative version must still block accurately.

/// Insert a synthetic business usage row covering exactly (device,target) so a test can
/// drive the operative-version resolution without depending on reconcile internals.
#[allow(clippy::too_many_arguments)] // One row versions a single business usage.
async fn seed_usage(
    f: &Fixture,
    id: Uuid,
    approval: &str,
    state: &str,
    device: Uuid,
    target: Uuid,
    budget_root: Uuid,
    created_at: chrono::DateTime<chrono::Utc>,
    expires_at: chrono::DateTime<chrono::Utc>,
    supersedes: Option<Uuid>,
) {
    let body = json!({
        "device_ids": [device],
        "auto_targets": false,
        "targets": [{"id": target}],
        "max_calls": 5,
    });
    sqlx::query("INSERT INTO mobile_usages(id,school_id,actor_id,approval,state,revision,body,confirmed_until,expires_at,created_at,budget_root,approved_by,supersedes) VALUES($1,$2,$3,$4,$5,1,$6,$7,$8,$9,$10,$11,$12)")
        .bind(id).bind(f.school).bind(f.actor.id).bind(approval).bind(state).bind(&body)
        .bind(expires_at).bind(expires_at).bind(created_at).bind(budget_root).bind(f.actor.id).bind(supersedes)
        .execute(&f.pool).await.unwrap();
}

/// The readiness row for one device/target pair.
fn readiness_row(overview: &Value, device: Uuid, target: Uuid) -> Value {
    overview["readiness"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["device_id"] == json!(device) && v["target_id"] == json!(target))
        .expect("readiness row for the seeded binding")
        .clone()
}

fn scope_blocker(row: &Value) -> Option<String> {
    row["blockers"].as_array().unwrap().iter().find_map(|b| {
        b["code"]
            .as_str()
            .filter(|c| c.starts_with("business_scope_"))
            .map(|c| c.to_string())
    })
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn pending_declined_history_does_not_implicitly_disable_a_valid_binding() {
    let f = Fixture::new().await;
    let r = f.repo();
    let (device, target) = (f.device, f.targets[0]);
    // A pending application and a declined one both mention this device/target, but
    // neither is an approved operative version, so they are history — not a disable.
    seed_usage(
        &f,
        Uuid::now_v7(),
        "pending",
        "active",
        device,
        target,
        Uuid::new_v4(),
        chrono::Utc::now() - chrono::Duration::hours(3),
        chrono::Utc::now() + chrono::Duration::days(1),
        None,
    )
    .await;
    seed_usage(
        &f,
        Uuid::now_v7(),
        "declined",
        "active",
        device,
        target,
        Uuid::new_v4(),
        chrono::Utc::now() - chrono::Duration::hours(2),
        chrono::Utc::now() + chrono::Duration::days(1),
        None,
    )
    .await;
    let overview = r.school_read(&f.actor, f.school, None).await.unwrap();
    let row = readiness_row(&overview, device, target);
    assert_eq!(row["plan_selectable"], true);
    assert_eq!(
        row["confirmable"], true,
        "history must never manufacture an implicit disable"
    );
    assert_eq!(scope_blocker(&row), None);
    // Exercise the failing path as well as its readiness explanation: a pending
    // application must not prevent preview and confirmation of the bound task.
    let draft = r
        .school_write(
            &f.actor,
            f.school,
            "history-draft",
            SchoolCommand::Draft(
                None,
                DraftInput {
                    target_ids: vec![target],
                    message: "历史申请不影响下发".into(),
                    content_type: "text/plain".into(),
                    expected_revision: None,
                    between_targets_seconds: 30,
                },
            ),
        )
        .await
        .unwrap();
    let preview = r
        .school_write(
            &f.actor,
            f.school,
            "history-preview",
            SchoolCommand::Preview(PreviewInput {
                draft_id: serde_json::from_value(draft["id"].clone()).unwrap(),
                expected_revision: 1,
                selections: vec![],
            }),
        )
        .await
        .unwrap();
    assert!(preview["plan"]["items"][0]["blocker"].is_null());
    let task = r
        .school_write(
            &f.actor,
            f.school,
            "history-confirm",
            SchoolCommand::Confirm(Confirmation {
                preview_id: serde_json::from_value(preview["id"].clone()).unwrap(),
                preview_digest: preview["digest"].as_str().unwrap().into(),
                confirmed: true,
            }),
        )
        .await
        .unwrap();
    assert_eq!(task["items"].as_array().unwrap().len(), 1);
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn paused_revoked_and_expired_operative_usage_block_with_accurate_reasons() {
    let f = Fixture::new().await;
    let r = f.repo();
    let (device, target) = (f.device, f.targets[0]);
    let id = Uuid::now_v7();
    seed_usage(
        &f,
        id,
        "approved",
        "active",
        device,
        target,
        Uuid::new_v4(),
        chrono::Utc::now(),
        chrono::Utc::now() + chrono::Duration::days(1),
        None,
    )
    .await;
    let overview = r.school_read(&f.actor, f.school, None).await.unwrap();
    assert_eq!(
        readiness_row(&overview, device, target)["confirmable"],
        true
    );

    for (state, expires, expected) in [
        ("paused", chrono::Duration::days(1), "business_scope_paused"),
        (
            "revoked",
            chrono::Duration::days(1),
            "business_scope_revoked",
        ),
        (
            "active",
            chrono::Duration::days(-1),
            "business_scope_expired",
        ),
    ] {
        sqlx::query("UPDATE mobile_usages SET state=$2,expires_at=clock_timestamp()+$3::interval WHERE id=$1")
            .bind(id)
            .bind(state)
            .bind(format!("{} days", expires.num_days()))
            .execute(&f.pool)
            .await
            .unwrap();
        let overview = r.school_read(&f.actor, f.school, None).await.unwrap();
        let row = readiness_row(&overview, device, target);
        assert_eq!(
            row["confirmable"], false,
            "{state} operative usage must block confirmation"
        );
        assert_eq!(scope_blocker(&row).as_deref(), Some(expected));
    }
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn superseded_lineage_resolves_to_the_operative_version() {
    let f = Fixture::new().await;
    let r = f.repo();
    let (device, target) = (f.device, f.targets[0]);
    let root = Uuid::new_v4();
    let older = Uuid::now_v7();
    // The older version already expired, but it is superseded within the same budget
    // root, so its expiry is history and must not leak into the current decision.
    seed_usage(
        &f,
        older,
        "approved",
        "active",
        device,
        target,
        root,
        chrono::Utc::now() - chrono::Duration::hours(2),
        chrono::Utc::now() - chrono::Duration::hours(1),
        None,
    )
    .await;
    let newer = Uuid::now_v7();
    seed_usage(
        &f,
        newer,
        "approved",
        "active",
        device,
        target,
        root,
        chrono::Utc::now(),
        chrono::Utc::now() + chrono::Duration::days(1),
        Some(older),
    )
    .await;
    let overview = r.school_read(&f.actor, f.school, None).await.unwrap();
    let row = readiness_row(&overview, device, target);
    assert_eq!(row["confirmable"], true, "the live renewal is operative");
    assert_eq!(scope_blocker(&row), None);

    // Revoking only the operative version blocks; the expired ancestor stays history.
    sqlx::query("UPDATE mobile_usages SET state='revoked' WHERE id=$1")
        .bind(newer)
        .execute(&f.pool)
        .await
        .unwrap();
    let overview = r.school_read(&f.actor, f.school, None).await.unwrap();
    let row = readiness_row(&overview, device, target);
    assert_eq!(row["confirmable"], false);
    assert_eq!(
        scope_blocker(&row).as_deref(),
        Some("business_scope_revoked")
    );
    // Revoked roots can be opened again as a new business family. Old revocation
    // history must not make that explicit reopening unusable forever.
    seed_usage(
        &f,
        Uuid::now_v7(),
        "approved",
        "active",
        device,
        target,
        Uuid::new_v4(),
        chrono::Utc::now(),
        chrono::Utc::now() + chrono::Duration::days(1),
        None,
    )
    .await;
    let overview = r.school_read(&f.actor, f.school, None).await.unwrap();
    assert_eq!(
        readiness_row(&overview, device, target)["confirmable"],
        true
    );
    // An explicit newer revoke must still win, including an action on an ancestor.
    sqlx::query("INSERT INTO mobile_usage_events(id,usage_id,actor_id,action,body) VALUES($1,$2,$3,'revoke','{}')")
        .bind(Uuid::now_v7()).bind(older).bind(f.actor.id).execute(&f.pool).await.unwrap();
    let overview = r.school_read(&f.actor, f.school, None).await.unwrap();
    assert_eq!(
        scope_blocker(&readiness_row(&overview, device, target)).as_deref(),
        Some("business_scope_revoked")
    );
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn repeated_identical_pool_binding_save_does_not_advance_locator_revision() {
    let f = Fixture::with_adapter(false).await;
    let r = f.repo();
    // A school-pool device shared into the school, bound to an existing target.
    let pool_device = Uuid::now_v7();
    let target = f.targets[0];
    sqlx::query("INSERT INTO execution_devices(id,name,enabled,credential_generation) VALUES($1,'共享池手机',true,1)").bind(pool_device).execute(&f.pool).await.unwrap();
    sqlx::query("INSERT INTO mobile_shared_school_access(device_id,school_id,enabled,revision,updated_by) VALUES($1,$2,true,1,$3)").bind(pool_device).bind(f.school).bind(f.actor.id).execute(&f.pool).await.unwrap();

    async fn locator(pool: &PgPool, device: Uuid, target: Uuid) -> i64 {
        sqlx::query_scalar::<_, i64>(
            "SELECT locator_revision FROM mobile_device_target_bindings WHERE device_id=$1 AND target_id=$2",
        )
        .bind(device)
        .bind(target)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    // First save establishes the locator.
    r.school_write(
        &f.actor,
        f.school,
        "pool-bind-1",
        SchoolCommand::PoolBinding(PoolBindingInput {
            target_id: target,
            device_id: pool_device,
            expected_access_revision: 1,
            search_text_override: Some("共享池群".into()),
        }),
    )
    .await
    .unwrap();
    assert_eq!(locator(&f.pool, pool_device, target).await, 1);

    // Re-saving identical content must not advance locator_revision.
    r.school_write(
        &f.actor,
        f.school,
        "pool-bind-2",
        SchoolCommand::PoolBinding(PoolBindingInput {
            target_id: target,
            device_id: pool_device,
            expected_access_revision: 1,
            search_text_override: Some("共享池群".into()),
        }),
    )
    .await
    .unwrap();
    assert_eq!(
        locator(&f.pool, pool_device, target).await,
        1,
        "re-saving identical content must not advance locator_revision"
    );

    // A genuine effective-locator change does advance it.
    r.school_write(
        &f.actor,
        f.school,
        "pool-bind-3",
        SchoolCommand::PoolBinding(PoolBindingInput {
            target_id: target,
            device_id: pool_device,
            expected_access_revision: 1,
            search_text_override: Some("共享池群改名".into()),
        }),
    )
    .await
    .unwrap();
    assert_eq!(
        locator(&f.pool, pool_device, target).await,
        2,
        "a genuine effective-locator change advances locator_revision"
    );
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn device_reenable_and_projection_drift_do_not_permanently_invalidate_scope() {
    use wekeeper_api::distribution::domain::shared_mobile::usage::*;
    let f = Fixture::new().await;
    let r = f.repo();
    // Establish a real approved business usage through the normal preview/confirm flow.
    let input = UsageInput {
        device_ids: vec![f.device],
        targets: vec![UsageTarget {
            id: None,
            name: "漂移测试群".into(),
            search_text: "漂移测试群".into(),
        }],
        max_calls: 5,
        expires_at: chrono::Utc::now() + chrono::Duration::days(3),
        auto_targets: false,
        renewal_of: None,
        expected_revision: None,
    };
    let p = r
        .usage_write(
            &f.actor,
            f.school,
            "drift-preview",
            UsageCommand::Preview(input),
        )
        .await
        .unwrap();
    let u = r
        .usage_write(
            &f.actor,
            f.school,
            "drift-confirm",
            UsageCommand::Confirm(UsageConfirm {
                preview_id: serde_json::from_value(p["id"].clone()).unwrap(),
                digest: p["digest"].as_str().unwrap().into(),
            }),
        )
        .await
        .unwrap();
    let usage_id: Uuid = serde_json::from_value(u["items"][0]["id"].clone()).unwrap();
    let target: Uuid =
        serde_json::from_value(u["items"][0]["body"]["targets"][0]["id"].clone()).unwrap();
    r.reconcile_usages().await.unwrap();
    r.reconcile_usages().await.unwrap();
    let scope_before: Uuid = sqlx::query_scalar(
        "SELECT scope_id FROM mobile_usage_scopes WHERE usage_id=$1 AND release_id IS NULL AND device_id=$2 AND target_id=$3",
    )
    .bind(usage_id).bind(f.device).bind(target).fetch_one(&f.pool).await.unwrap();

    // Simulate generic technical churn: a device credential/enable revision bump and a
    // locator revision drift with no effective search-text change. Neither is a disable.
    sqlx::query("UPDATE execution_devices SET revision=revision+1 WHERE id=$1")
        .bind(f.device)
        .execute(&f.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE mobile_device_target_bindings SET locator_revision=locator_revision+1 WHERE device_id=$1 AND target_id=$2")
        .bind(f.device)
        .bind(target)
        .execute(&f.pool)
        .await
        .unwrap();
    r.reconcile_usages().await.unwrap();
    r.reconcile_usages().await.unwrap();

    // The projection healed in place: same scope_id (budget root preserved), snapshot
    // refreshed, and still exactly one business scope.
    let scope_after: Uuid = sqlx::query_scalar(
        "SELECT scope_id FROM mobile_usage_scopes WHERE usage_id=$1 AND release_id IS NULL AND device_id=$2 AND target_id=$3",
    )
    .bind(usage_id).bind(f.device).bind(target).fetch_one(&f.pool).await.unwrap();
    assert_eq!(
        scope_before, scope_after,
        "healing must keep the same scope_id"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM mobile_usage_scopes WHERE usage_id=$1")
            .bind(usage_id)
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        1
    );
    let snapshot: Value =
        sqlx::query_scalar("SELECT resource_snapshot FROM mobile_usage_scopes WHERE scope_id=$1")
            .bind(scope_after)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(snapshot["binding_locator_revision"].as_i64(), Some(2));

    let overview = r.school_read(&f.actor, f.school, None).await.unwrap();
    let row = readiness_row(&overview, f.device, target);
    assert_eq!(
        row["confirmable"], true,
        "technical drift must not permanently invalidate a valid scope"
    );
    assert_eq!(scope_blocker(&row), None);

    // A matching old grant must never override another current explicit disable.
    seed_usage(
        &f,
        Uuid::now_v7(),
        "approved",
        "paused",
        f.device,
        target,
        Uuid::new_v4(),
        chrono::Utc::now(),
        chrono::Utc::now() + chrono::Duration::days(1),
        None,
    )
    .await;
    let overview = r.school_read(&f.actor, f.school, None).await.unwrap();
    let row = readiness_row(&overview, f.device, target);
    assert_eq!(
        row["confirmable"], false,
        "an old valid grant must not bypass a pause"
    );
    assert_eq!(
        scope_blocker(&row).as_deref(),
        Some("business_scope_paused")
    );
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn runtime_configuration_is_independent_and_invalid_update_does_not_replace_current() {
    use wekeeper_api::distribution::{
        domain::{
            shared_mobile::runtime_configuration::RuntimeConfiguration, shared_mobile::usage::*,
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
        .usage_write(
            &f.actor,
            f.school,
            "runtime-only-preview",
            UsageCommand::TaskPreview(UsageTaskInput {
                draft_id: None,
                expected_revision: None,
                target_ids: vec![f.targets[0]],
                message: "Synthetic no send".into(),
                between_targets_seconds: 30,
                selections: vec![],
            }),
        )
        .await
        .unwrap();
    assert!(preview["preview"]["plan"]["items"][0]["blocker"].is_null());
    assert_eq!(
        preview["preview"]["plan"]["items"][0]["selected"]["execution"]["contract"],
        "device_owned/1"
    );
    assert_eq!(preview["preview"]["plan"]["configuration"], Value::Null);
    sqlx::query("UPDATE execution_devices SET enabled=false WHERE id=$1")
        .bind(f.device)
        .execute(&f.pool)
        .await
        .unwrap();
    let overview = f
        .repo()
        .school_read(&f.actor, f.school, None)
        .await
        .unwrap();
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
        sqlx::query("INSERT INTO mobile_device_target_bindings(id,school_id,device_id,target_id,search_text_override,created_by,updated_by) VALUES($1,$2,$3,$4,$5,$6,$6)").bind(Uuid::new_v4()).bind(f.school).bind(id).bind(target).bind(format!("{name}专用群名")).bind(f.actor.id).execute(&f.pool).await.unwrap();
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
    let context = r.delegation_read(&c).await.unwrap();
    r.device_write(
        &c,
        "pool-consent",
        DeviceCommand::Consent(ConsentInput {
            delegation_id: serde_json::from_value(context["delegation"]["id"].clone()).unwrap(),
            scope_digest: context["delegation"]["scope_digest"]
                .as_str()
                .unwrap()
                .into(),
            expected_revision: 0,
            accepted: true,
        }),
    )
    .await
    .unwrap();
    let epoch = Uuid::new_v4();
    sqlx::query("INSERT INTO device_control_sessions(device_id,connection_id,transport,credential_generation,last_heartbeat_at) VALUES($1,$2,'https',1,clock_timestamp()) ON CONFLICT(device_id) DO UPDATE SET connected=true,last_heartbeat_at=clock_timestamp()").bind(device).bind(Uuid::new_v4()).execute(&f.pool).await.unwrap();
    sqlx::query("INSERT INTO mobile_shared_runtime(device_id,credential_generation,runtime_epoch,ui_ready,paused,quiescent,observed_at) VALUES($1,1,$2,true,false,true,clock_timestamp()) ON CONFLICT(device_id) DO UPDATE SET observed_at=clock_timestamp(),ui_ready=true,paused=false,quiescent=true").bind(device).bind(epoch).execute(&f.pool).await.unwrap();
    sqlx::query("INSERT INTO mobile_v9_runtime_details(device_id,credential_generation,ui_context_epoch,conditions,apk_sha256) VALUES($1,1,'local','{\"interactive\":true,\"keyguard_locked\":false,\"device_locked\":false,\"keyguard_secure\":false,\"accessibility\":true,\"notification\":true}',repeat('a',64)) ON CONFLICT(device_id) DO UPDATE SET observed_at=clock_timestamp()").bind(device).execute(&f.pool).await.unwrap();
    (c, epoch)
}
async fn pool_confirm(f: &Fixture, targets: Vec<Uuid>) -> Value {
    use wekeeper_api::distribution::domain::shared_mobile::usage::*;
    let r = f.repo();
    let p = r
        .usage_write(
            &f.actor,
            f.school,
            "pool-preview",
            UsageCommand::TaskPreview(UsageTaskInput {
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
            }),
        )
        .await
        .unwrap();
    r.usage_write(
        &f.actor,
        f.school,
        "pool-confirm",
        UsageCommand::TaskConfirm(Confirmation {
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
        "UPDATE mobile_v9_dispatch_state SET next_retry_at=clock_timestamp()-interval '1 second'",
    )
    .execute(&f.pool)
    .await
    .unwrap();
    f.repo().reconcile_dispatch().await.unwrap();
}
async fn pool_detail(f: &Fixture, t: &Value) -> Value {
    f.repo()
        .school_read(
            &f.actor,
            f.school,
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
async fn pool_claim(
    f: &Fixture,
    c: &wekeeper_api::distribution::domain::device_control::Credential,
    epoch: Uuid,
    aid: Uuid,
) -> Value {
    f.repo()
        .device_write(
            c,
            "pool-grant",
            DeviceCommand::Grant(
                wekeeper_api::distribution::domain::shared_mobile::execution::GrantRequest {
                    runtime_epoch: epoch,
                    eligible_attempt_ids: vec![aid],
                    inspection_id: None,
                },
            ),
        )
        .await
        .unwrap()
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn offline_preference_dispatches_authorized_phone_and_never_expands_scope() {
    let f = Fixture::with_adapter(false).await;
    let b = pool_phone(&f, "B").await;
    let (_cb, _eb) = pool_ready(&f, b).await;
    let task = pool_confirm(&f, vec![f.targets[0]]).await;
    assert_eq!(task["dispatch_mode"], "candidate_pool");
    assert_eq!(
        pool_current(&task["items"][0])["plan"]["device_id"],
        json!(b)
    );
    assert_eq!(
        pool_current(&task["items"][0])["plan"]["search_text"],
        "B专用群名"
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
    sqlx::query("UPDATE device_control_sessions SET connected=false WHERE device_id=$1")
        .bind(b)
        .execute(&f.pool)
        .await
        .unwrap();
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
async fn offline_granted_phone_moves_to_next_without_receipt_or_idle_proof() {
    let f = Fixture::with_adapter(false).await;
    let b = pool_phone(&f, "B").await;
    let (ca, ea) = pool_ready(&f, f.device).await;
    let (_cb, _eb) = pool_ready(&f, b).await;
    let task = pool_confirm(&f, vec![f.targets[0]]).await;
    pool_tick(&f).await;
    let detail = pool_detail(&f, &task).await;
    let aid = serde_json::from_value(pool_current(&detail["items"][0])["id"].clone()).unwrap();
    let grant = pool_claim(&f, &ca, ea, aid).await;
    assert_eq!(grant["state"], "granted", "{grant}");

    sqlx::query("UPDATE device_control_sessions SET connected=false WHERE device_id=$1")
        .bind(f.device)
        .execute(&f.pool)
        .await
        .unwrap();
    pool_tick(&f).await;
    let detail = pool_detail(&f, &task).await;
    assert_eq!(
        pool_current(&detail["items"][0])["plan"]["device_id"],
        json!(b)
    );
    let request = wekeeper_api::distribution::domain::shared_mobile::execution::GrantRequest {
        runtime_epoch: ea,
        eligible_attempt_ids: vec![aid],
        inspection_id: None,
    };
    let replay = f
        .repo()
        .device_write(&ca, "pool-grant", DeviceCommand::Grant(request))
        .await
        .unwrap();
    assert_eq!(replay["state"], "superseded");
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
    let grant = pool_claim(&f, &ca, ea, aid).await;
    assert_eq!(grant["state"], "granted", "{grant}");

    let completion = completion(&grant, "unknown");
    f.repo().complete_execution(completion).await.unwrap();
    pool_tick(&f).await;
    let before = pool_detail(&f, &task).await;
    assert_eq!(pool_current(&before["items"][0])["id"], json!(aid));
    sqlx::query("UPDATE mobile_execution_grants SET result_deadline_at=clock_timestamp()-interval '1 second' WHERE attempt_id=$1").bind(aid).execute(&f.pool).await.unwrap();
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
    let grant = pool_claim(&f, &ca, ea, aid).await;
    assert_eq!(grant["state"], "granted", "{grant}");

    let failed = completion(&grant, "not_called");
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
    let (cb, eb) = pool_ready(&f, b).await;
    let task = pool_confirm(&f, vec![f.targets[0]]).await;
    pool_tick(&f).await;
    let detail = pool_detail(&f, &task).await;
    let aid = serde_json::from_value(pool_current(&detail["items"][0])["id"].clone()).unwrap();
    let grant = pool_claim(&f, &ca, ea, aid).await;
    assert_eq!(grant["state"], "granted", "{grant}");

    sqlx::query("UPDATE mobile_execution_grants SET result_deadline_at=clock_timestamp()-interval '1 second' WHERE attempt_id=$1").bind(aid).execute(&f.pool).await.unwrap();
    pool_tick(&f).await;
    let before = pool_detail(&f, &task).await;
    let next = pool_current(&before["items"][0])["id"].clone();
    assert_ne!(next, json!(aid));
    f.repo()
        .complete_execution(completion(&grant, "observed_sent"))
        .await
        .unwrap();
    // A persisted success fences grants before its asynchronous projection is refreshed.
    let denied = pool_claim(&f, &cb, eb, serde_json::from_value(next.clone()).unwrap()).await;
    assert_eq!(denied["state"], "idle");
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
    let grant = pool_claim(&f, &ca, ea, aid).await;
    assert_eq!(grant["state"], "granted", "{grant}");

    sqlx::query("UPDATE mobile_execution_grants SET result_deadline_at=clock_timestamp()-interval '1 second' WHERE attempt_id=$1").bind(aid).execute(&f.pool).await.unwrap();
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
        reason: "synthetic".into(),
        evidence: "synthetic evidence".into(),
        consumed: matches!(outcome, "observed_sent" | "unknown"),
        calling_marked: matches!(outcome, "observed_sent" | "unknown"),
    }
}
