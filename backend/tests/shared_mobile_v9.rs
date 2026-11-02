//! Synthetic records in a unique schema, never production acceptance evidence.
use serde_json::{Value, json};
use sqlx::{PgPool, postgres::PgPoolOptions};
use uuid::Uuid;
use wekeeper_api::{
    business::Actor,
    database,
    distribution::{
        domain::mobile_execution::shared_send_v2::*,
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
        let flow = Uuid::new_v4();
        let artifact = Uuid::new_v4();
        let baseline = Uuid::new_v4();
        let release = Uuid::new_v4();
        let acceptance = Uuid::new_v4();
        let fixture: Value = serde_json::from_str(include_str!(
            "../../api-contract/fixtures/shared-mobile-send-v9.json"
        ))
        .unwrap();
        sqlx::query("INSERT INTO mobile_v9_flows(id,name,flow,digest,created_by) VALUES($1,'Synthetic',$2,$3,$4)").bind(flow).bind(&fixture["flow"]).bind(fixture["flow_digest"].as_str()).bind(actor_id).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO mobile_v9_artifacts(id,kind,name,sha256,content,created_by) VALUES($1,'apk','Synthetic',repeat('a',64),$2,$3)").bind(artifact).bind(vec![1u8]).bind(actor_id).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO mobile_v9_baselines(id,flow_id,device_id,samples,evidence_ids,min_interval_ms,recorded_by) VALUES($1,$2,$3,'[]',ARRAY[$4]::uuid[],1000,$5)").bind(baseline).bind(flow).bind(device).bind(artifact).bind(actor_id).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO mobile_v9_adapter_releases(id,name,version,flow_id,flow,content_digest,baseline_id,evidence_ids,created_by) VALUES($1,'Synthetic','1',$2,$3,$4,$5,ARRAY[$6]::uuid[],$7)").bind(release).bind(flow).bind(&fixture["flow"]).bind(fixture["flow_digest"].as_str()).bind(baseline).bind(artifact).bind(actor_id).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO mobile_v9_acceptance_runs(id,release_id,stage,apk_id,evidence_ids,validation_ids,cases,soak_hours,soak_passed,historical_debt_closed,reviewer_id,reason) VALUES($1,$2,'test_send',$3,ARRAY[$3]::uuid[],'{}','{}',0,false,false,$4,'Synthetic test only')").bind(acceptance).bind(release).bind(artifact).bind(actor_id).execute(&pool).await.unwrap();
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
    let policy:PolicyInput=serde_json::from_value(json!({"expected_revision":0,"mode":"enabled","allowed_stages":["test_send"],"confirmation_seconds":600,"inspection_seconds":600,"retry_seconds":300,"retry_limit":3,"timezone":"Asia/Shanghai","allowed_windows":[{"weekdays":[1,2,3,4,5,6,7],"start_minute":0,"end_minute":1440}],"limits":ObjectLimits::value(),"reason":"Synthetic test"})).unwrap();
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
        stage: "test_send".into(),
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
    use wekeeper_api::distribution::domain::mobile_execution::usage::*;
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
    assert_eq!(approved["items"][0]["delivery"], "waiting");
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
async fn business_production_delivery_materializes_once_and_pausing_blocks_plans() {
    use wekeeper_api::distribution::domain::mobile_execution::usage::*;
    let f = Fixture::new().await;
    let r = f.repo();
    f.activate("activate-usage").await.unwrap();
    // All evidence in this test is synthetic and isolated; never used by delivery tooling.
    sqlx::query("INSERT INTO mobile_v9_acceptance_runs(id,release_id,stage,apk_id,evidence_ids,validation_ids,cases,soak_hours,soak_passed,historical_debt_closed,reviewer_id,reason) SELECT $1,release_id,'production_allowlist',apk_id,evidence_ids,validation_ids,cases,72,true,true,reviewer_id,'Synthetic business test only' FROM mobile_v9_acceptance_runs WHERE id=$2").bind(Uuid::now_v7()).bind(f.acceptance).execute(&f.pool).await.unwrap();
    sqlx::query("INSERT INTO mobile_capability_deliveries(release_id,acceptance_id,supported_apks,provenance) SELECT release_id,id,ARRAY[(SELECT sha256 FROM mobile_v9_artifacts WHERE id=apk_id)], '{\"synthetic_test\":true}' FROM mobile_v9_acceptance_runs WHERE stage='production_allowlist'").execute(&f.pool).await.unwrap();
    let p:PolicyInput=serde_json::from_value(json!({"expected_revision":0,"mode":"enabled","allowed_stages":["production_allowlist"],"confirmation_seconds":600,"inspection_seconds":600,"retry_seconds":300,"retry_limit":3,"timezone":"Asia/Shanghai","allowed_windows":[{"weekdays":[1,2,3,4,5,6,7],"start_minute":0,"end_minute":1440}],"limits":ObjectLimits::value(),"reason":"Synthetic test"})).unwrap();
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
    assert_eq!(preview["draft"]["stage"], "production_allowlist");
    assert_eq!(preview["preview"]["plan"]["message"], "  一步预览\n🙂  ");
    assert!(preview["preview"]["plan"]["items"][0]["blocker"].is_null());
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
    use wekeeper_api::distribution::domain::mobile_execution::usage::*;
    let f = Fixture::new().await;
    let r = f.repo();
    let options = r
        .usage_read(&f.actor, Some(f.school), None, true)
        .await
        .unwrap();
    assert_eq!(options["sending_status"]["delivery"], "missing");
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
        false
    );
    assert!(!result["preview"]["plan"]["items"][0]["blocker"].is_null());
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
    r.platform_write(
        &f.actor,
        "open-no-delivery",
        PlatformCommand::Policy(input.policy().unwrap()),
    )
    .await
    .unwrap();
    f.close().await;
}
