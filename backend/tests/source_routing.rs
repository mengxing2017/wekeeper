//! Current business-unit automatic dispatch. Historical version/pool semantics are retired.
#[path = "support/business_dispatch.rs"]
mod support;
use serde_json::Value;
use support::Fixture;
use uuid::Uuid;

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn scoped_advertising_rotates_once_with_inactive_storage_partition() {
    let f = Fixture::new().await;
    f.configure(vec![f.target]).await;
    f.ad("全局", "global").await;
    f.ad("平台", "platform").await;
    let a = f.ad("单位甲", "unit").await;
    let b = f.ad("单位乙", "unit").await;
    let (one, _) = f.message().await;
    f.run().await;
    f.run().await;
    let (two, _) = f.message().await;
    f.run().await;
    let decisions:Vec<Value>=sqlx::query_scalar("SELECT decision FROM orchestration_dispatches WHERE message_id=ANY($1) ORDER BY created_at").bind(vec![one,two]).fetch_all(&f.pool).await.unwrap();
    assert_eq!(decisions.len(), 2);
    assert_ne!(
        decisions[0]["selected_ad"]["material_id"],
        decisions[1]["selected_ad"]["material_id"]
    );
    for d in decisions {
        assert!(
            [a.to_string(), b.to_string()].contains(
                &d["selected_ad"]["material_id"]
                    .as_str()
                    .unwrap()
                    .to_string()
            )
        );
        assert_eq!(
            d["content"],
            format!(
                "原消息\n{}",
                d["selected_ad"]["content"]["body"].as_str().unwrap()
            )
        );
    }
    let count: i64 =
        sqlx::query_scalar("SELECT selected_count FROM ad_unit_counters WHERE unit_id=$1")
            .bind(f.unit)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(count, 2);
    let tasks: i64 = sqlx::query_scalar("SELECT count(*) FROM mobile_v9_tasks")
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(tasks, 2);
    let units:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('unit',business_unit_id,'scheme_unit',scheme_snapshot->'business_unit_id','source',scheme_snapshot->'source') FROM mobile_v9_tasks").fetch_all(&f.pool).await.unwrap();
    for context in units {
        assert_eq!(context["unit"], serde_json::json!(f.unit));
        assert_eq!(context["scheme_unit"], serde_json::json!(f.unit));
        assert_eq!(context["source"], "builtin");
    }
    let retired: bool = sqlx::query_scalar("SELECT to_regclass('source_execution_tasks') IS NULL")
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert!(retired);
    let _ = f.scope;
    f.close().await;
}
#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn no_targets_is_terminal_and_new_messages_continue() {
    let f = Fixture::new().await;
    let (id, receipt) = f.message().await;
    f.run().await;
    let reason: String =
        sqlx::query_scalar("SELECT reason_code FROM orchestration_dispatches WHERE message_id=$1")
            .bind(id)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(reason, "route_no_targets");
    let receipts = wekeeper_api::source_access::north::local::projection::Service(
        wekeeper_api::source_access::south::adapter::postgres::projection::ReceiptsRepository(
            f.pool.clone(),
        ),
    )
    .execute(
        &f.actor,
        wekeeper_api::source_access::domain::projection::Command::List {
            route: f.route.clone(),
            page: wekeeper_api::business::Page::default(),
        },
    )
    .await
    .unwrap();
    let row = &receipts["items"][0];
    assert_eq!(row["id"], receipt.to_string());
    assert_eq!(row["reason_code"], "route_no_targets");
    assert!(row.get("automation_task_id").is_none());

    f.configure(vec![f.target]).await;
    f.run().await;
    let tasks: i64 =
        sqlx::query_scalar("SELECT count(*) FROM orchestration_delegates WHERE message_id=$1")
            .bind(id)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(tasks, 0);
    let stage: String =
        sqlx::query_scalar("SELECT processing_stage FROM source_receipts WHERE receipt_id=$1")
            .bind(receipt)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(stage, "not_dispatched");
    f.message().await;
    f.run().await;
    let tasks: i64 = sqlx::query_scalar("SELECT count(*) FROM orchestration_delegates")
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(tasks, 1);
    f.close().await;
}
#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn failed_child_reuses_frozen_ad_and_candidate_plan() {
    let f = Fixture::new().await;
    let config = f.configure(vec![f.target]).await;
    f.ad("一次广告", "unit").await;
    sqlx::query("CREATE FUNCTION fail_child() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected transient failure'; END $$").execute(&f.pool).await.unwrap();
    sqlx::query("CREATE TRIGGER fail_child BEFORE INSERT ON mobile_v9_tasks FOR EACH ROW EXECUTE FUNCTION fail_child()").execute(&f.pool).await.unwrap();
    let (id, _) = f.message().await;
    f.run().await;
    let state: String =
        sqlx::query_scalar("SELECT state FROM orchestration_dispatches WHERE message_id=$1")
            .bind(id)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(state, "ready");
    sqlx::query("UPDATE ad_placements SET enabled=false")
        .execute(&f.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE channel_receiving_configurations SET configuration=jsonb_set(configuration,'{target_ids}','[]') WHERE id=$1").bind(config).execute(&f.pool).await.unwrap();
    sqlx::query("DROP TRIGGER fail_child ON mobile_v9_tasks")
        .execute(&f.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE orchestration_dispatches SET next_attempt_at=now() WHERE message_id=$1")
        .bind(id)
        .execute(&f.pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE mobile_dispatch_intakes SET next_retry_at=clock_timestamp() WHERE message_id=$1",
    )
    .bind(id)
    .execute(&f.pool)
    .await
    .unwrap();
    f.run().await;
    f.run().await;
    let count: i64 =
        sqlx::query_scalar("SELECT selected_count FROM ad_unit_counters WHERE unit_id=$1")
            .bind(f.unit)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(count, 1);
    let content: String =
        sqlx::query_scalar("SELECT content FROM orchestration_dispatches WHERE message_id=$1")
            .bind(id)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert!(content.contains("一次广告"));
    let tasks: i64 =
        sqlx::query_scalar("SELECT count(*) FROM orchestration_delegates WHERE message_id=$1")
            .bind(id)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(tasks, 1);
    f.close().await;
}
#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn shared_target_is_delivered_once_and_missing_resources_do_not_wait() {
    let f = Fixture::new().await;
    f.configure(vec![f.target]).await;
    f.configure(vec![f.target]).await;
    f.message().await;
    f.run().await;
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM mobile_v9_task_items")
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(n, 1);
    sqlx::query("UPDATE channel_receiving_configurations SET configuration=jsonb_set(configuration,'{device_ids}','[]')").execute(&f.pool).await.unwrap();
    let (id, _) = f.message().await;
    f.run().await;
    let reason: String =
        sqlx::query_scalar("SELECT reason_code FROM orchestration_dispatches WHERE message_id=$1")
            .bind(id)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(reason, "channel_resources_missing");
    f.close().await;
}
#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn unavailable_ad_sends_original_without_count_and_results_are_visible() {
    let f = Fixture::new().await;
    f.configure(vec![f.target]).await;
    f.ad("停用广告", "unit").await;
    sqlx::query("UPDATE ad_placements SET enabled=false")
        .execute(&f.pool)
        .await
        .unwrap();
    let (id, receipt) = f.message().await;
    f.run().await;
    let d: Value =
        sqlx::query_scalar("SELECT decision FROM orchestration_dispatches WHERE message_id=$1")
            .bind(id)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert!(d["selected_ad"].is_null());
    assert_eq!(d["content"], "原消息");
    assert!(d["selection_count"].is_null());
    let list =
        wekeeper_api::message_orchestration::south::adapter::postgres::Repository(f.pool.clone())
            .list(
                &f.actor,
                None,
                Some(receipt),
                wekeeper_api::business::Page::default(),
            )
            .await
            .unwrap();
    assert_eq!(list["items"][0]["delegates"].as_array().unwrap().len(), 1);
    assert_eq!(list["items"][0]["sending"]["pending"], 1);
    let outsider = Uuid::new_v4();
    sqlx::query("INSERT INTO accounts(id,username,display_name) VALUES($1,'stranger','Stranger')")
        .bind(outsider)
        .execute(&f.pool)
        .await
        .unwrap();
    let mut actor = f.actor.clone();
    actor.id = outsider;
    actor.platform_admin = false;
    let result =
        wekeeper_api::message_orchestration::south::adapter::postgres::Repository(f.pool.clone())
            .list(&actor, None, None, wekeeper_api::business::Page::default())
            .await;
    assert!(matches!(
        result,
        Err(wekeeper_api::business::Error::Forbidden)
    ));
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn interrupted_freeze_rolls_back_ad_count_and_reuses_frozen_targets() {
    let f = Fixture::new().await;
    let config = f.configure(vec![f.target]).await;
    f.ad("冻结广告", "unit").await;
    sqlx::query("CREATE FUNCTION fail_ready() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF OLD.state='preparing' AND NEW.state='ready' THEN RAISE EXCEPTION 'injected interruption before atomic content commit'; END IF; RETURN NEW; END $$").execute(&f.pool).await.unwrap();
    sqlx::query("CREATE TRIGGER fail_ready BEFORE UPDATE ON orchestration_dispatches FOR EACH ROW EXECUTE FUNCTION fail_ready()").execute(&f.pool).await.unwrap();
    let (id, _) = f.message().await;
    f.run().await;
    let state: String =
        sqlx::query_scalar("SELECT state FROM orchestration_dispatches WHERE message_id=$1")
            .bind(id)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(state, "preparing");
    sqlx::query("UPDATE channel_receiving_configurations SET configuration=jsonb_set(configuration,'{target_ids}','[]') WHERE id=$1").bind(config).execute(&f.pool).await.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM ad_unit_counters WHERE unit_id=$1")
        .bind(f.unit)
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(
        count, 0,
        "failed outer commit must roll back the ad counter"
    );
    sqlx::query("DROP TRIGGER fail_ready ON orchestration_dispatches")
        .execute(&f.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE orchestration_dispatches SET next_attempt_at=now() WHERE message_id=$1")
        .bind(id)
        .execute(&f.pool)
        .await
        .unwrap();
    let repo =
        wekeeper_api::message_orchestration::south::adapter::postgres::Repository(f.pool.clone());
    let (a, b) = tokio::join!(repo.run_once(), repo.run_once());
    a.unwrap();
    b.unwrap();
    // Both scanners may skip a row held by the other's planning/finalization
    // transaction. The next periodic scan must finish the same frozen input.
    f.run().await;
    let n: i64 =
        sqlx::query_scalar("SELECT count(*) FROM orchestration_delegates WHERE message_id=$1")
            .bind(id)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(n, 1);
    let n: i64 = sqlx::query_scalar("SELECT selected_count FROM ad_unit_counters WHERE unit_id=$1")
        .bind(f.unit)
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(n, 1);
    let content: String =
        sqlx::query_scalar("SELECT content FROM orchestration_dispatches WHERE message_id=$1")
            .bind(id)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert!(content.contains("冻结广告"));
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn unavailable_target_does_not_block_other_targets() {
    let f = Fixture::new().await;
    let missing = Uuid::new_v4();
    sqlx::query("INSERT INTO mobile_targets(client_kind,id,search_text,created_by,updated_by) VALUES('wecom',$1,'没有手机的群',$2,$2)").bind(missing).bind(f.actor.id).execute(&f.pool).await.unwrap();
    f.configure(vec![missing, f.target]).await;
    sqlx::query("DELETE FROM mobile_device_target_bindings WHERE target_id=$1")
        .bind(missing)
        .execute(&f.pool)
        .await
        .unwrap();
    let (_, receipt) = f.message().await;
    f.run().await;
    let sending: Value =
        sqlx::query_scalar("SELECT sending FROM source_receipts WHERE receipt_id=$1")
            .bind(receipt)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(
        sending,
        serde_json::json!({"sent":0,"failed":1,"pending":1,"unknown":0})
    );
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM mobile_v9_task_items")
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(n, 1);
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn both_channels_share_one_ad_and_aggregate_official_results_without_membership() {
    use wekeeper_api::distribution::{
        domain::receiving::{Channel, ConfigurationInput},
        north::local::receiving::Service,
        south::adapter::postgres::receiving::Repository,
    };
    let f = Fixture::new().await;
    f.configure(vec![f.target]).await;
    let _workspace = Uuid::new_v4();
    let member = Uuid::new_v4();
    let target = Uuid::new_v4();
    let directory = Uuid::new_v4();
    sqlx::query("INSERT INTO wecom_directory_state(singleton,corp_id,credential_scope) VALUES(true,'test-corp','scope')").execute(&f.pool).await.unwrap();
    sqlx::query("INSERT INTO wecom_members(id,userid,name,status,verification,checked_at) VALUES($1,'owner','Owner',1,'verified',now())").bind(member).execute(&f.pool).await.unwrap();
    sqlx::query("UPDATE accounts SET wecom_member_id=$2 WHERE id=$1")
        .bind(f.actor.id)
        .bind(member)
        .execute(&f.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO wecom_asset_objects(id,kind,corp_id,key,data,checked_at,credential_generation,visibility,verification) VALUES($1,'groups','test-corp','chat',jsonb_build_object('owner','owner'),now(),1,'visible','verified')").bind(directory).execute(&f.pool).await.unwrap();
    sqlx::query("INSERT INTO wecom_groups(id,corp_id,chat_id,name,directory_group_id) VALUES($1,'test-corp','chat','客户群',$2)").bind(target).bind(directory).execute(&f.pool).await.unwrap();
    Service(Repository(f.pool.clone()))
        .save(
            &f.actor,
            Channel::Wecom,
            f.unit,
            Uuid::new_v4(),
            ConfigurationInput {
                client_kind: None,
                target_selection_mode:
                    wekeeper_api::distribution::domain::receiving::SelectionMode::All,
                device_dispatch_mode: None,
                name: "企业微信配置".into(),
                enabled: true,
                target_ids: vec![target],
                device_ids: vec![],
                sender_ids: vec![f.actor.id],
            },
        )
        .await
        .unwrap();
    sqlx::query("INSERT INTO wecom_sender_qualifications(account_id,enabled,updated_by) VALUES($1,false,$1)").bind(f.actor.id).execute(&f.pool).await.unwrap();
    f.ad("共同广告", "unit").await;
    let (id, receipt) = f.message().await;
    f.run().await;
    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM orchestration_delegates WHERE message_id=$1 AND state='delegated'",
    )
    .bind(id)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(n, 2);
    let contents:Vec<String>=sqlx::query_scalar("SELECT s.content FROM orchestration_delegates d JOIN distribution_message_snapshots s ON s.id=d.snapshot_id WHERE d.message_id=$1").bind(id).fetch_all(&f.pool).await.unwrap();
    assert_eq!(contents[0], contents[1]);
    assert_eq!(contents[0], "原消息\n共同广告");
    let n: i64 = sqlx::query_scalar("SELECT selected_count FROM ad_unit_counters WHERE unit_id=$1")
        .bind(f.unit)
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(n, 1);
    // New automatic tasks pass channel validation through their durable delegation.
    let task: Uuid = sqlx::query_scalar(
        "SELECT task_id FROM orchestration_delegates WHERE message_id=$1 AND channel='wecom'",
    )
    .bind(id)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    let diagnostics =
        wekeeper_api::message_orchestration::south::adapter::postgres::Repository(f.pool.clone());
    sqlx::query("UPDATE broadcast_targets SET official_status=97 WHERE task_id=$1")
        .bind(task)
        .execute(&f.pool)
        .await
        .unwrap();
    let unknown = diagnostics
        .deliveries(&f.actor, receipt, Default::default())
        .await
        .unwrap();
    assert!(
        unknown["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["channel"] == "wecom" && v["outcome"] == "unknown")
    );
    sqlx::query("UPDATE broadcast_targets SET official_status=NULL WHERE task_id=$1")
        .bind(task)
        .execute(&f.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE broadcast_batches SET stop_state='stopped' WHERE task_id=$1")
        .bind(task)
        .execute(&f.pool)
        .await
        .unwrap();
    let stopped = diagnostics
        .deliveries(&f.actor, receipt, Default::default())
        .await
        .unwrap();
    assert!(
        stopped["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["channel"] == "wecom" && v["outcome"] == "failed")
    );
    sqlx::query("UPDATE broadcast_batches SET stop_state='none' WHERE task_id=$1")
        .bind(task)
        .execute(&f.pool)
        .await
        .unwrap();
    let mut tx = f.pool.begin().await.unwrap();
    wekeeper_api::distribution::south::adapter::postgres::content::validate_task(&mut tx, task)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    use wekeeper_api::distribution::south::port::broadcast::Store;
    sqlx::query("INSERT INTO official_channel_health(credential_scope,corp_id,generation,available,checked_at) VALUES('scope','test-corp',1,true,now())").execute(&f.pool).await.unwrap();
    let channel =
        wekeeper_api::distribution::south::adapter::postgres::broadcast::Repository(f.pool.clone());
    let work = channel.work(Some(task)).await.unwrap();
    assert_eq!(work.len(), 1);
    assert!(channel.claim(&work[0]).await.unwrap());
    assert!(!channel.claim(&work[0]).await.unwrap());
    sqlx::query("UPDATE broadcast_targets SET official_status=1 WHERE task_id=$1")
        .bind(task)
        .execute(&f.pool)
        .await
        .unwrap();
    f.run().await;
    let sending: Value =
        sqlx::query_scalar("SELECT sending FROM source_receipts WHERE receipt_id=$1")
            .bind(receipt)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(
        sending,
        serde_json::json!({"sent":1,"failed":0,"pending":1,"unknown":0})
    );
    // A long but channel-valid original must reach both channels without an ad.
    let (long, _) = f.message_with_content(&"原".repeat(250)).await;
    f.run().await;
    f.run().await;
    let contents: Vec<String> = sqlx::query_scalar("SELECT s.content FROM orchestration_delegates d JOIN distribution_message_snapshots s ON s.id=d.snapshot_id WHERE d.message_id=$1 AND d.state='delegated'")
        .bind(long).fetch_all(&f.pool).await.unwrap();
    assert_eq!(contents.len(), 2);
    assert_eq!(contents[0], contents[1]);
    assert_eq!(contents[0], "原".repeat(250));
    let count: i64 =
        sqlx::query_scalar("SELECT selected_count FROM ad_unit_counters WHERE unit_id=$1")
            .bind(f.unit)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(count, 1);

    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn first_dependency_failure_backs_off_fifty_messages_and_allows_new_work() {
    let f = Fixture::new().await;
    let config = f.configure(vec![f.target]).await;
    let saved: Value = sqlx::query_scalar(
        "SELECT configuration FROM channel_receiving_configurations WHERE id=$1",
    )
    .bind(config)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    // Fault injection: configuration cannot be decoded before any plan is frozen.
    sqlx::query("UPDATE channel_receiving_configurations SET configuration=configuration-'target_ids' WHERE id=$1")
        .bind(config).execute(&f.pool).await.unwrap();
    let mut failed = Vec::new();
    for _ in 1..=50 {
        failed.push(f.message().await.0);
    }
    f.run().await;
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM orchestration_dispatches WHERE message_id=ANY($1) AND state='pending' AND failures=1 AND next_attempt_at>clock_timestamp() AND plans='[]'")
        .bind(&failed).fetch_one(&f.pool).await.unwrap();
    assert_eq!(count, 50);
    sqlx::query("UPDATE channel_receiving_configurations SET configuration=$2 WHERE id=$1")
        .bind(config)
        .bind(saved)
        .execute(&f.pool)
        .await
        .unwrap();
    let (new, _) = f.message().await;
    f.run().await;
    let state: String =
        sqlx::query_scalar("SELECT state FROM orchestration_dispatches WHERE message_id=$1")
            .bind(new)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(state, "delegated");
    sqlx::query("UPDATE orchestration_dispatches SET next_attempt_at=clock_timestamp() WHERE message_id=ANY($1)")
        .bind(&failed).execute(&f.pool).await.unwrap();
    f.run().await;
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM orchestration_dispatches WHERE message_id=ANY($1) AND state='delegated'")
        .bind(&failed).fetch_one(&f.pool).await.unwrap();
    assert_eq!(count, 50);
    assert!(
        sqlx::query("UPDATE orchestration_dispatches SET plans='[]' WHERE message_id=$1")
            .bind(new)
            .execute(&f.pool)
            .await
            .is_err()
    );
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn admin_list_http_pagination_and_message_authorization() {
    use axum::{
        body::{Body, to_bytes},
        http::Request,
    };
    use tower::ServiceExt;
    use wekeeper_api::{
        advertising::{
            domain::{
                Content,
                targeting::{CreateAdPlacement, CreateMaterial, PublishMaterial, TargetScope},
            },
            north::local::{AdPlacementService, MaterialService},
            south::adapter::postgres::Repository as Ads,
        },
        bootstrap, http,
    };
    async fn read(app: &axum::Router, path: &str, token: Option<&str>, status: u16) -> Value {
        let mut request = Request::builder().uri(path);
        if let Some(token) = token {
            request = request.header("cookie", format!("wekeeper_session={token}"));
        }
        let response = app
            .clone()
            .oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), status, "{path}");
        assert_eq!(
            response.headers()["content-type"],
            "application/vnd.api+json"
        );
        serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap()).unwrap()
    }
    let f = Fixture::new().await;
    let (token, _) = wekeeper_api::identity::south::adapter::postgres::Repository(f.pool.clone())
        .session(f.actor.id, "browser", false)
        .await
        .unwrap();
    let app = http::app_router(bootstrap::build_application(f.pool.clone()));
    let path = "/api/v2/admin/messages?page%5Bnumber%5D=1&page%5Bsize%5D=30";
    let mut protected_paths = vec![
        "/api/v2/admin/source-route-assignments".to_string(),
        format!("/api/v2/admin/source-routes/{}/configuration", f.route),
        format!("/api/v2/admin/source-routes/{}/receipts", f.route),
    ];
    protected_paths.extend([
        "/api/v2/admin/mobile-dispatch/intakes".to_string(),
        "/api/v2/admin/mobile-dispatch/delivery-exchanges".to_string(),
    ]);
    for protected in &protected_paths {
        read(&app, protected, Some(&token), 200).await;
        read(&app, protected, None, 401).await;
    }
    let configuration = read(&app, &protected_paths[1], Some(&token), 200).await;
    let mut expected: Value = serde_json::from_str(include_str!(
        "../../api-contract/fixtures/source-route-configuration.json"
    ))
    .unwrap();
    expected["data"]["attributes"]["business_unit_id"] = serde_json::json!(f.unit);
    assert_eq!(configuration, expected);
    let managers = format!("/api/v2/admin/source-routes/{}/managers", f.route);
    for method in ["GET", "PUT"] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(&managers)
                    .header("cookie", format!("wekeeper_session={token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), 404);
    }

    let empty = read(&app, path, Some(&token), 200).await;
    assert_eq!(empty["data"]["type"], "messages");
    assert_eq!(empty["data"]["id"], "result");
    assert_eq!(
        empty["data"]["attributes"],
        serde_json::json!({"items":[],"has_more":false})
    );
    let mut receipt = Uuid::nil();
    for n in 1..=31 {
        receipt = f.message().await.1;
        if [1, 30, 31].contains(&n) {
            let page = read(&app, path, Some(&token), 200).await;
            assert_eq!(
                page["data"]["attributes"]["items"]
                    .as_array()
                    .unwrap()
                    .len(),
                n.min(30) as usize
            );
            assert_eq!(page["data"]["attributes"]["has_more"], n > 30);
        }
    }
    let delivery_path = format!("/api/v2/admin/messages/{receipt}/deliveries");
    read(&app, &delivery_path, Some(&token), 200).await;
    read(&app, &delivery_path, None, 401).await;
    protected_paths.push(delivery_path);
    let first = read(&app, path, Some(&token), 200).await;
    let second = read(
        &app,
        "/api/v2/admin/messages?page[number]=2&page[size]=30",
        Some(&token),
        200,
    )
    .await;
    let last = &second["data"]["attributes"]["items"];
    assert_eq!(last.as_array().unwrap().len(), 1);
    assert_eq!(second["data"]["attributes"]["has_more"], false);
    assert!(
        first["data"]["attributes"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["id"] != last[0]["id"])
    );
    let filtered = read(
        &app,
        &format!(
            "/api/v2/admin/messages?route_id={}&receipt_id={receipt}&page[number]=1&page[size]=1",
            f.route
        ),
        Some(&token),
        200,
    )
    .await;
    assert_eq!(
        filtered["data"]["attributes"]["items"][0]["receipt_id"],
        receipt.to_string()
    );
    assert_eq!(
        filtered["data"]["attributes"]["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(filtered["data"]["attributes"]["has_more"], false);
    let missing = read(
        &app,
        "/api/v2/admin/messages?route_id=missing",
        Some(&token),
        200,
    )
    .await;
    assert_eq!(
        missing["data"]["attributes"]["items"],
        serde_json::json!([])
    );
    // Seed at least two entries per related list to exercise SQL size and offset.
    let key = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO source_credentials(key_id,source_id,unit_name,encryption_version,status,valid_from,created_by) VALUES($1,$2,'pagination','v1','active',now(),$3)").bind(&key).bind(f.platform).bind(f.actor.id).execute(&f.pool).await.unwrap();
    for n in 0..2 {
        let platform = Uuid::new_v4();
        sqlx::query("INSERT INTO source_callers(source_id) VALUES($1)")
            .bind(platform)
            .execute(&f.pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO source_credentials(key_id,source_id,unit_name,encryption_version,status,valid_from,created_by) VALUES($1,$2,'pagination','v1','active',now(),$3)").bind(Uuid::new_v4().to_string()).bind(platform).bind(f.actor.id).execute(&f.pool).await.unwrap();
        let unit = Uuid::new_v4();
        sqlx::query("INSERT INTO business_units(id,platform_id,name,created_by,updated_by) VALUES($1,$2,$3,$4,$4)").bind(unit).bind(f.platform).bind(format!("unit-{n}")).bind(f.actor.id).execute(&f.pool).await.unwrap();
        let route = format!("{}-{n}", f.route);
        sqlx::query("INSERT INTO source_routes(route_id,source_id,business_unit_id,route_name,status) VALUES($1,$2,$3,'pagination','active')").bind(&route).bind(f.platform).bind(unit).execute(&f.pool).await.unwrap();
        sqlx::query("INSERT INTO source_key_routes(key_id,route_id,source_id,status) VALUES($1,$2,$3,'active')").bind(&key).bind(&route).bind(f.platform).execute(&f.pool).await.unwrap();
    }
    let material_service = MaterialService(Ads(f.pool.clone()));
    let content = Content {
        name: "pagination".into(),
        body: "正文".into(),
        link: None,
    };
    let mut material = material_service
        .create(
            &f.actor,
            CreateMaterial {
                content: content.clone(),
            },
        )
        .await
        .unwrap();
    let material_id = material.id;
    for _ in 0..2 {
        material = material_service
            .publish(
                &f.actor,
                material.id,
                PublishMaterial {
                    expected_revision: material.revision,
                    content: content.clone(),
                },
            )
            .await
            .unwrap();
        let other = material_service
            .create(
                &f.actor,
                CreateMaterial {
                    content: content.clone(),
                },
            )
            .await
            .unwrap();
        AdPlacementService(Ads(f.pool.clone()))
            .create(
                &f.actor,
                CreateAdPlacement {
                    material_id: other.id,
                    scope: TargetScope::Global {},
                    enabled: true,
                    valid_from: chrono::Utc::now(),
                    valid_until: None,
                },
            )
            .await
            .unwrap();
    }
    let paths = [
        "/api/v2/admin/ad-materials".into(),
        format!("/api/v2/admin/ad-materials/{material_id}/versions"),
        "/api/v2/admin/ad-placements".into(),
        "/api/v2/admin/business-platforms".into(),
        format!("/api/v2/admin/business-platforms/{}/units", f.platform),
        "/api/v2/admin/source-credentials".into(),
        "/api/v2/admin/source-routes".into(),
        format!("/api/v2/admin/source-credentials/{key}/routes"),
        format!("/api/v2/admin/source-routes/{}/receipts", f.route),
        "/api/v2/admin/messages".into(),
    ];
    for base in &paths {
        let one = read(
            &app,
            &format!("{base}?page%5Bnumber%5D=1&page%5Bsize%5D=1"),
            Some(&token),
            200,
        )
        .await;
        let two = read(
            &app,
            &format!("{base}?page[number]=2&page[size]=1"),
            Some(&token),
            200,
        )
        .await;
        let a = &one["data"]["attributes"];
        let b = &two["data"]["attributes"];
        assert_eq!(a["items"].as_array().unwrap().len(), 1, "{base}");
        assert_eq!(b["items"].as_array().unwrap().len(), 1, "{base}");
        assert_eq!(a["has_more"], true, "{base}");
        assert_ne!(a["items"][0], b["items"][0], "{base}");
        for query in [
            "page=1",
            "page_size=1",
            "page[number]=abc",
            "page[number]=1&page[number]=2",
            "unknown=x",
        ] {
            let error = read(&app, &format!("{base}?{query}"), Some(&token), 400).await;
            assert_eq!(error["errors"][0]["code"], "invalid_request");
        }
        for query in ["page[number]=0", "page[size]=101"] {
            read(&app, &format!("{base}?{query}"), Some(&token), 422).await;
        }
    }
    read(&app, path, None, 401).await;
    let ordinary = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO accounts(id,username,display_name) VALUES($1,'pagination-manager','Manager')",
    )
    .bind(ordinary)
    .execute(&f.pool)
    .await
    .unwrap();
    let (ordinary_token, _) =
        wekeeper_api::identity::south::adapter::postgres::Repository(f.pool.clone())
            .session(ordinary, "browser", false)
            .await
            .unwrap();
    read(&app, path, Some(&ordinary_token), 403).await;
    for protected in &protected_paths {
        read(&app, protected, Some(&ordinary_token), 403).await;
    }
    // A stale in-memory admin flag must not bypass the transactional account check.
    let mut stale_admin = f.actor.clone();
    stale_admin.id = ordinary;
    use wekeeper_api::source_access::{
        domain::{projection, routing},
        south::{
            adapter::postgres::{projection::ReceiptsRepository, routing::RoutingRepository},
            port::{ProjectionStore, RoutingStore},
        },
    };
    let routing = RoutingRepository(f.pool.clone());
    let receipts = ReceiptsRepository(f.pool.clone());
    for command in [
        routing::Command::Assigned,
        routing::Command::Configuration {
            route: f.route.clone(),
        },
    ] {
        assert_eq!(
            routing.execute(&stale_admin, command).await.unwrap_err().0,
            "forbidden"
        );
    }
    assert_eq!(
        receipts
            .execute(
                &stale_admin,
                projection::Command::List {
                    route: f.route.clone(),
                    page: Default::default()
                }
            )
            .await
            .unwrap_err()
            .0,
        "forbidden"
    );

    let mut stale = f.actor.clone();
    stale.id = ordinary;
    assert!(matches!(
        wekeeper_api::message_orchestration::south::adapter::postgres::Repository(f.pool.clone())
            .list(&stale, None, None, Default::default())
            .await,
        Err(wekeeper_api::business::Error::Forbidden)
    ));
    sqlx::query("UPDATE accounts SET active=false WHERE id=$1")
        .bind(f.actor.id)
        .execute(&f.pool)
        .await
        .unwrap();
    read(&app, path, Some(&token), 401).await;
    for protected in &protected_paths {
        read(&app, protected, Some(&token), 401).await;
    }
    assert_eq!(
        routing
            .execute(&f.actor, routing::Command::Assigned)
            .await
            .unwrap_err()
            .0,
        "forbidden"
    );
    assert_eq!(
        receipts
            .execute(
                &f.actor,
                projection::Command::List {
                    route: f.route.clone(),
                    page: Default::default()
                }
            )
            .await
            .unwrap_err()
            .0,
        "forbidden"
    );

    assert!(matches!(
        wekeeper_api::message_orchestration::south::adapter::postgres::Repository(f.pool.clone())
            .list(&f.actor, None, None, Default::default())
            .await,
        Err(wekeeper_api::business::Error::Forbidden)
    ));
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn complete_text_is_preserved_and_internal_pause_only_blocks_dispatch() {
    let f = Fixture::new().await;
    f.configure(vec![f.target]).await;
    let text = " 完整标题\r\n正文\n#小程序://Linjian/示例 ";
    let (id, _) = f.message_with_content(text).await;
    f.run().await;
    let content: String =
        sqlx::query_scalar("SELECT content FROM orchestration_dispatches WHERE message_id=$1")
            .bind(id)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(content, text);
    sqlx::query("UPDATE source_routes SET status='disabled' WHERE route_id=$1")
        .bind(&f.route)
        .execute(&f.pool)
        .await
        .unwrap();
    let (paused, receipt) = f.message().await;
    f.run().await;
    let reason: String =
        sqlx::query_scalar("SELECT reason_code FROM orchestration_dispatches WHERE message_id=$1")
            .bind(paused)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(reason, "route_execution_disabled");
    let delegated: i64 =
        sqlx::query_scalar("SELECT count(*) FROM orchestration_delegates WHERE message_id=$1")
            .bind(paused)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(delegated, 0);
    assert!(
        sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM source_inbox WHERE receipt_id=$1)"
        )
        .bind(receipt)
        .fetch_one(&f.pool)
        .await
        .unwrap()
    );
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn message_progress_tracks_delivery_receipt_successor_and_live_results() {
    use wekeeper_api::message_orchestration::south::adapter::postgres::{
        Repository, progress::ProgressFilter,
    };
    let f = Fixture::new().await;
    f.configure(vec![f.target]).await;
    let (mid, receipt) = f.message().await;
    f.run().await;
    let repo = Repository(f.pool.clone());
    let detail = repo
        .deliveries(&f.actor, receipt, Default::default())
        .await
        .unwrap();
    assert_eq!(detail["items"].as_array().unwrap().len(), 1);
    assert_eq!(detail["items"][0]["target_name"], "群名");
    assert_eq!(detail["items"][0]["diagnostic"]["stage"], "scheduling");
    let attempt: Uuid = serde_json::from_value(detail["items"][0]["attempt_id"].clone()).unwrap();
    let item: Uuid = sqlx::query_scalar("SELECT item_id FROM mobile_v9_attempts WHERE id=$1")
        .bind(attempt)
        .fetch_one(&f.pool)
        .await
        .unwrap();
    // The message projection must agree with the phone task domain's terminal precedence.
    use wekeeper_api::distribution::domain::shared_mobile::shared_send_v2::{
        TaskStatus, TaskTargetProgress,
    };
    for (dispatch, expected) in [
        ("assigned", "pending"),
        ("needs_review", "unknown"),
        ("dispatch_failed", "failed"),
        ("stopped", "failed"),
        ("completed", "sent"),
    ] {
        sqlx::query("UPDATE mobile_v9_dispatch_state SET state=$2 WHERE item_id=$1")
            .bind(item)
            .bind(dispatch)
            .execute(&f.pool)
            .await
            .unwrap();
        let status = TaskStatus::project(
            false,
            &[TaskTargetProgress {
                dispatch_state: Some(dispatch.into()),
                attempt_state: "queued".into(),
                sent: false,
            }],
        );
        let outcome = match status {
            TaskStatus::Completed => "sent",
            TaskStatus::NeedsReview => "unknown",
            TaskStatus::DispatchFailed | TaskStatus::Stopped | TaskStatus::ConfirmationExpired => {
                "failed"
            }
            _ => "pending",
        };
        assert_eq!(outcome, expected);
        let projected = repo
            .deliveries(&f.actor, receipt, Default::default())
            .await
            .unwrap();
        assert_eq!(projected["items"][0]["outcome"], outcome);
    }
    sqlx::query("UPDATE mobile_v9_dispatch_state SET state='assigned' WHERE item_id=$1")
        .bind(item)
        .execute(&f.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO mobile_execution_grants(id,attempt_id,device_id,credential_generation,kind,result_credential_hash,envelope) VALUES($1,$2,$3,1,'send','public-test-hash',$4)").bind(Uuid::new_v4()).bind(attempt).bind(f.device).bind(Vec::<u8>::new()).execute(&f.pool).await.unwrap();
    let detail = repo
        .deliveries(&f.actor, receipt, Default::default())
        .await
        .unwrap();
    assert_eq!(detail["items"][0]["diagnostic"]["stage"], "device_receipt");
    let list = repo
        .list(&f.actor, None, Some(receipt), Default::default())
        .await
        .unwrap();
    assert_eq!(list["items"][0]["state"], "dispatching");
    assert_eq!(list["items"][0]["progress"]["version"], detail["version"]);
    // A newer message in a different phase must not hide this older match.
    f.message().await;
    // Filter before pagination, not just on the displayed page.
    let filtered = repo
        .list_progress(
            &f.actor,
            None,
            None,
            wekeeper_api::business::Page {
                page_size: Some(1),
                ..Default::default()
            },
            ProgressFilter {
                channel: Some("mobile".into()),
                stage: Some("device_receipt".into()),
                attention: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(filtered["items"][0]["receipt_id"], receipt.to_string());
    sqlx::query("UPDATE mobile_execution_grants SET accepted_at=clock_timestamp(),result_deadline_at=clock_timestamp()+interval '5 minutes' WHERE attempt_id=$1").bind(attempt).execute(&f.pool).await.unwrap();
    let accepted = repo
        .deliveries(&f.actor, receipt, Default::default())
        .await
        .unwrap();
    assert_eq!(
        accepted["items"][0]["diagnostic"]["stage"],
        "device_execution"
    );
    assert_ne!(accepted["version"], detail["version"]);
    sqlx::query("UPDATE mobile_execution_grants SET result_deadline_at=clock_timestamp()-interval '2 days' WHERE attempt_id=$1").bind(attempt).execute(&f.pool).await.unwrap();
    let overdue = repo
        .deliveries(&f.actor, receipt, Default::default())
        .await
        .unwrap();
    assert_eq!(overdue["items"][0]["diagnostic"]["severity"], "attention");
    assert!(
        overdue["items"][0]["diagnostic"]["next_action"]
            .as_str()
            .unwrap()
            .contains("无需等待原手机恢复")
    );
    // A historical unknown cannot hide an active authorized successor.
    sqlx::query("UPDATE mobile_execution_grants SET state='superseded' WHERE attempt_id=$1")
        .bind(attempt)
        .execute(&f.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE mobile_v9_attempts SET state='unknown',current=false WHERE id=$1")
        .bind(attempt)
        .execute(&f.pool)
        .await
        .unwrap();
    let successor = Uuid::new_v4();
    sqlx::query("INSERT INTO mobile_v9_attempts(id,item_id,plan_id,confirmed_by,attempt_no,state,current) SELECT $2,item_id,plan_id,confirmed_by,attempt_no+1,'queued',true FROM mobile_v9_attempts WHERE id=$1").bind(attempt).bind(successor).execute(&f.pool).await.unwrap();
    sqlx::query("UPDATE mobile_v9_dispatch_state SET state='assigned',reason='candidate_ready',next_retry_at=clock_timestamp()+interval '5 seconds' WHERE item_id=$1").bind(item).execute(&f.pool).await.unwrap();
    let list = repo
        .list(&f.actor, None, Some(receipt), Default::default())
        .await
        .unwrap();
    assert_eq!(list["items"][0]["sending"]["pending"], 1);
    assert_eq!(list["items"][0]["sending"]["unknown"], 0);
    assert_eq!(list["items"][0]["progress"]["total"], 1);
    assert_eq!(
        list["items"][0]["progress"]["diagnostics"][0]["historical_risk"],
        true
    );
    sqlx::query("UPDATE mobile_v9_attempts SET state='observed_sent' WHERE id=$1")
        .bind(successor)
        .execute(&f.pool)
        .await
        .unwrap();
    let completed = repo
        .list(&f.actor, None, Some(receipt), Default::default())
        .await
        .unwrap();
    assert_eq!(completed["items"][0]["state"], "completed");
    assert_eq!(completed["items"][0]["sending"]["sent"], 1);
    assert_eq!(completed["items"][0]["progress"]["projection_lag"], true);
    let dispatch: String =
        sqlx::query_scalar("SELECT state FROM orchestration_dispatches WHERE message_id=$1")
            .bind(mid)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(dispatch, "delegated");
    // Lose the notification deliberately: bounded reconciliation must repair it.
    sqlx::query("DELETE FROM orchestration_result_queue")
        .execute(&f.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE source_receipts SET summary_updated_at=clock_timestamp()-interval '61 seconds' WHERE receipt_id=$1").bind(receipt).execute(&f.pool).await.unwrap();
    repo.aggregate().await.unwrap();
    let completed = repo
        .list(&f.actor, None, Some(receipt), Default::default())
        .await
        .unwrap();
    assert_eq!(completed["items"][0]["progress"]["projection_lag"], false);
    let mut unauthorized = f.actor.clone();
    unauthorized.platform_admin = false;
    assert!(matches!(
        repo.deliveries(&unauthorized, receipt, Default::default())
            .await,
        Err(wekeeper_api::business::Error::Forbidden)
    ));
    assert!(matches!(
        repo.deliveries(&f.actor, Uuid::new_v4(), Default::default())
            .await,
        Err(wekeeper_api::business::Error::NotFound)
    ));
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn received_messages_are_visible_before_internal_handoff() {
    use wekeeper_api::message_orchestration::south::adapter::postgres::{
        Repository, progress::ProgressFilter,
    };
    let f = Fixture::new().await;
    let receipt = Uuid::new_v4();
    let event = Uuid::new_v4();
    let body =
        serde_json::to_vec(&serde_json::json!({"data":{"attributes":{"content":"尚待交接"}}}))
            .unwrap();
    sqlx::query("INSERT INTO source_receipts(receipt_id,source_id,business_unit_id,event_id,route_id,body_sha256,schema_version) VALUES($1,$2,$3,$4,$5,$6,2)").bind(receipt).bind(f.platform).bind(f.unit).bind(event).bind(&f.route).bind(wekeeper_api::source_access::domain::digest(&body)).execute(&f.pool).await.unwrap();
    sqlx::query("INSERT INTO source_inbox(receipt_id,request_body) VALUES($1,$2)")
        .bind(receipt)
        .bind(body)
        .execute(&f.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE source_message_handoffs SET last_error='internal_processing_unavailable',next_attempt_at=clock_timestamp()+interval '30 seconds' WHERE receipt_id=$1").bind(receipt).execute(&f.pool).await.unwrap();
    let repo = Repository(f.pool.clone());
    let result = repo
        .list_progress(
            &f.actor,
            None,
            Some(receipt),
            Default::default(),
            ProgressFilter {
                channel: None,
                stage: Some("handoff".into()),
                attention: Some("retrying".into()),
            },
        )
        .await
        .unwrap();
    let row = &result["items"][0];
    assert_eq!(row["id"], receipt.to_string());
    assert!(row["message_id"].is_null());
    assert_eq!(row["state"], "handoff");
    assert!(row["progress"]["total"].is_null());
    assert_eq!(row["progress"]["diagnostics"][0]["severity"], "retrying");
    assert_eq!(
        repo.deliveries(&f.actor, receipt, Default::default())
            .await
            .unwrap()["items"],
        serde_json::json!([])
    );
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn two_groups_have_independent_progress_and_one_message_outcome() {
    use wekeeper_api::message_orchestration::south::adapter::postgres::Repository;
    let f = Fixture::new().await;
    let second = Uuid::new_v4();
    sqlx::query("INSERT INTO mobile_targets(client_kind,id,search_text,created_by,updated_by) VALUES('wecom',$1,'第二个群',$2,$2)").bind(second).bind(f.actor.id).execute(&f.pool).await.unwrap();
    f.configure(vec![f.target, second]).await;
    let (_, receipt) = f.message().await;
    f.run().await;
    let repo = Repository(f.pool.clone());
    let list = repo
        .list(&f.actor, None, Some(receipt), Default::default())
        .await
        .unwrap();
    assert_eq!(list["items"][0]["sending"]["pending"], 2);
    assert_eq!(list["items"][0]["progress"]["total"], 2);
    let details = repo
        .deliveries(
            &f.actor,
            receipt,
            wekeeper_api::business::Page {
                page_size: Some(1),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(details["items"].as_array().unwrap().len(), 1);
    assert_eq!(details["has_more"], true);
    let attempt: Uuid = serde_json::from_value(details["items"][0]["attempt_id"].clone()).unwrap();
    sqlx::query("UPDATE mobile_v9_attempts SET state='observed_sent' WHERE id=$1")
        .bind(attempt)
        .execute(&f.pool)
        .await
        .unwrap();
    let partial = repo
        .list(&f.actor, None, Some(receipt), Default::default())
        .await
        .unwrap();
    assert_eq!(partial["items"][0]["state"], "partial");
    assert_eq!(partial["items"][0]["sending"]["sent"], 1);
    assert_eq!(partial["items"][0]["sending"]["pending"], 1);
    sqlx::query("UPDATE mobile_v9_attempts SET state='observed_sent' WHERE current")
        .execute(&f.pool)
        .await
        .unwrap();
    let done = repo
        .list(&f.actor, None, Some(receipt), Default::default())
        .await
        .unwrap();
    assert_eq!(done["items"][0]["state"], "completed");
    assert_eq!(done["items"][0]["sending"]["sent"], 2);
    assert_eq!(done["items"][0]["sending"]["pending"], 0);
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn phone_intake_failure_survives_rollback_and_deduplicates_creation() {
    use wekeeper_api::distribution::{
        domain::{receiving::DeliveryPlan, shared_mobile::dispatch_records::RecordQuery},
        south::adapter::postgres::shared_send_v2::Repository,
    };
    let f = Fixture::new().await;
    f.configure(vec![f.target]).await;
    let (message, receipt) = f.message().await;
    f.run().await;
    let repo = Repository(f.pool.clone());
    let original: Value =
        sqlx::query_scalar("SELECT plan FROM mobile_dispatch_intakes WHERE message_id=$1")
            .bind(message)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    let plan: DeliveryPlan = serde_json::from_value(original).unwrap();
    let id = repo
        .submit_mobile(message, receipt, plan.clone(), "原消息".into())
        .await
        .unwrap();
    let again = repo
        .submit_mobile(message, receipt, plan.clone(), "原消息".into())
        .await
        .unwrap();
    assert_eq!(id, again);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM mobile_v9_tasks")
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        1
    );
    let list = repo
        .read_records(&f.actor, "intakes", None, RecordQuery::default())
        .await
        .unwrap();
    assert_eq!(list["items"].as_array().unwrap().len(), 1);
    let ops = repo
        .read_records(&f.actor, "operations", Some(id), RecordQuery::default())
        .await
        .unwrap();
    assert!(
        ops["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|o| o["kind"] == "evaluate")
    );
    let checks = sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM mobile_dispatch_checks WHERE intake_id=$1",
    )
    .bind(id)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert!(checks > 0);
    assert!(
        repo.submit_mobile(message, receipt, plan.clone(), "不同正文".into())
            .await
            .is_err()
    );
    let (new_message, new_receipt) = f.message().await;
    // An old responsibility cannot be reused for a different message.
    assert!(
        repo.submit_mobile(new_message, new_receipt, plan.clone(), "原消息".into())
            .await
            .is_err()
    );
    // Corrupt a newly allocated frozen input at the persistence boundary to exercise
    // durable pre-task rejection, rather than inventing an unowned delivery identity.
    sqlx::raw_sql("CREATE FUNCTION invalid_frozen_candidates() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN NEW.plan=jsonb_set(NEW.plan,'{targets,0,identity,candidates}','[]'); RETURN NEW; END $$; CREATE TRIGGER invalid_frozen_candidates BEFORE INSERT ON mobile_dispatch_intakes FOR EACH ROW EXECUTE FUNCTION invalid_frozen_candidates();").execute(&f.pool).await.unwrap();
    f.run().await;
    sqlx::query("DROP TRIGGER invalid_frozen_candidates ON mobile_dispatch_intakes")
        .execute(&f.pool)
        .await
        .unwrap();
    let rejected: Uuid =
        sqlx::query_scalar("SELECT id FROM mobile_dispatch_intakes WHERE message_id=$1")
            .bind(new_message)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    let detail = repo
        .read_records(&f.actor, "detail", Some(rejected), RecordQuery::default())
        .await
        .unwrap();
    assert!(detail["task_id"].is_null());
    assert_eq!(detail["creation_state"], "rejected");
    assert_eq!(detail["reason_code"], "mobile_candidates_missing");
    let events = repo
        .read_records(&f.actor, "events", Some(rejected), RecordQuery::default())
        .await
        .unwrap();
    assert!(
        events["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["kind"] == "creation_failed")
    );
    let q = RecordQuery {
        reason: Some("mobile_candidates_missing".into()),
        reason_scope: Some("history".into()),
        page_size: Some(1),
        ..Default::default()
    };
    let filtered = repo
        .read_records(&f.actor, "intakes", None, q)
        .await
        .unwrap();
    assert_eq!(filtered["items"][0]["id"], rejected.to_string());
    let mut ordinary = f.actor.clone();
    ordinary.platform_admin = false;
    assert!(
        repo.read_records(&ordinary, "intakes", None, RecordQuery::default())
            .await
            .is_err()
    );
    assert!(
        repo.read_records(
            &f.actor,
            "detail",
            Some(Uuid::new_v4()),
            RecordQuery::default()
        )
        .await
        .is_err()
    );
    // Retryable creation failures preserve started identity and are recovered from the original frozen input.
    let (retry_message, retry_receipt) = f.message().await;
    // Force an infrastructure failure after intake commit, inside task creation.
    sqlx::query("CREATE FUNCTION test_fail_task() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'test infrastructure failure'; END $$").execute(&f.pool).await.unwrap();
    sqlx::query("CREATE TRIGGER test_fail_task BEFORE INSERT ON mobile_v9_tasks FOR EACH ROW EXECUTE FUNCTION test_fail_task()").execute(&f.pool).await.unwrap();
    f.run().await;
    let _ = retry_receipt;
    let retry_id: Uuid =
        sqlx::query_scalar("SELECT id FROM mobile_dispatch_intakes WHERE message_id=$1")
            .bind(retry_message)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    let detail = repo
        .read_records(&f.actor, "detail", Some(retry_id), RecordQuery::default())
        .await
        .unwrap();
    assert_eq!(detail["creation_state"], "retrying");
    assert!(detail["task_id"].is_null());
    sqlx::query("DROP TRIGGER test_fail_task ON mobile_v9_tasks")
        .execute(&f.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE mobile_dispatch_intakes SET next_retry_at=clock_timestamp() WHERE id=$1")
        .bind(retry_id)
        .execute(&f.pool)
        .await
        .unwrap();
    repo.recover_mobile_intakes().await.unwrap();
    let detail = repo
        .read_records(&f.actor, "detail", Some(retry_id), RecordQuery::default())
        .await
        .unwrap();
    assert_eq!(detail["creation_state"], "created");
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM mobile_dispatch_intakes WHERE message_id=$1"
        )
        .bind(retry_message)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        1
    );
    // Equal timestamps and more than 100 records must not truncate or repeat pages.
    sqlx::query("INSERT INTO mobile_dispatch_intakes(id,source_kind,message_id,target_name,snapshot_id,plan,content,payload_hash,creation_state,reason_code,created_at) SELECT gen_random_uuid(),'manual',NULL,'分页测试',gen_random_uuid(),'{}','分页正文','test','rejected','pagination_fixture',now() FROM generate_series(1,105)")
        .execute(&f.pool).await.unwrap();
    let mut seen = std::collections::HashSet::new();
    let mut cursor = None;
    loop {
        let page = repo
            .read_records(
                &f.actor,
                "intakes",
                None,
                RecordQuery {
                    cursor,
                    page_size: Some(30),
                    reason: Some("pagination_fixture".into()),
                    source_kind: Some("all".into()),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        for item in page["items"].as_array().unwrap() {
            assert!(seen.insert(item["id"].as_str().unwrap().to_owned()));
        }
        if page["has_more"] == false {
            break;
        }
        cursor = Some(page["next_cursor"].as_str().unwrap().to_owned());
    }
    assert_eq!(seen.len(), 105);
    f.close().await;
}

async fn ad_decision(f: &Fixture, id: Uuid) -> Value {
    sqlx::query_scalar("SELECT decision FROM orchestration_dispatches WHERE message_id=$1")
        .bind(id)
        .fetch_one(&f.pool)
        .await
        .unwrap()
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn ad_storage_error_rolls_back_then_freezes_original_and_never_backfills() {
    let f = Fixture::new().await;
    f.configure(vec![f.target]).await;
    f.ad("广告", "unit").await;
    sqlx::query("CREATE FUNCTION fail_ad_counter() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected ad counter failure'; END $$")
        .execute(&f.pool).await.unwrap();
    sqlx::query("CREATE TRIGGER fail_ad_counter BEFORE UPDATE ON ad_unit_counters FOR EACH ROW EXECUTE FUNCTION fail_ad_counter()")
        .execute(&f.pool).await.unwrap();
    let (id, _) = f.message().await;
    f.run().await;
    let decision = ad_decision(&f, id).await;
    assert_eq!(decision["reason"], "ad_dependency_unavailable");
    assert_eq!(decision["content"], "原消息");
    assert_eq!(
        decision["selection_count"], 0,
        "the initial count was read before rollback"
    );
    let counters: i64 = sqlx::query_scalar("SELECT count(*) FROM ad_unit_counters")
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(counters, 0);
    let tasks: i64 =
        sqlx::query_scalar("SELECT count(*) FROM orchestration_delegates WHERE message_id=$1")
            .bind(id)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(tasks, 1);
    sqlx::query("DROP TRIGGER fail_ad_counter ON ad_unit_counters")
        .execute(&f.pool)
        .await
        .unwrap();
    f.run().await;
    assert_eq!(ad_decision(&f, id).await, decision);
    let (new_id, _) = f.message().await;
    f.run().await;
    assert_eq!(ad_decision(&f, new_id).await["content"], "原消息\n广告");
    let changed =
        sqlx::query("UPDATE orchestration_dispatches SET content='changed' WHERE message_id=$1")
            .bind(new_id)
            .execute(&f.pool)
            .await;
    assert!(changed.is_err(), "database must reject frozen body changes");
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn locked_ad_tables_are_bounded_and_do_not_block_original_dispatch() {
    use std::time::{Duration, Instant};
    let f = Fixture::new().await;
    f.configure(vec![f.target]).await;
    f.ad("广告", "unit").await;
    let mut lock = f.pool.begin().await.unwrap();
    sqlx::query("LOCK TABLE ad_placements IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *lock)
        .await
        .unwrap();
    let (id, _) = f.message().await;
    let start = Instant::now();
    tokio::time::timeout(Duration::from_secs(5), f.run())
        .await
        .unwrap();
    assert!(start.elapsed() < Duration::from_secs(5));
    let decision = ad_decision(&f, id).await;
    assert_eq!(decision["reason"], "ad_lock_timeout");
    assert_eq!(decision["content"], "原消息");
    lock.rollback().await.unwrap();
    f.run().await;
    assert_eq!(ad_decision(&f, id).await, decision);
    let state: String =
        sqlx::query_scalar("SELECT state FROM orchestration_dispatches WHERE message_id=$1")
            .bind(id)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(state, "delegated");
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn no_ad_does_not_wait_for_counter_lock_and_replay_does_not_query_ads() {
    let f = Fixture::new().await;
    f.configure(vec![f.target]).await;
    let mut lock = f.pool.begin().await.unwrap();
    sqlx::query("LOCK TABLE ad_unit_counters IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *lock)
        .await
        .unwrap();
    let (id, _) = f.message().await;
    tokio::time::timeout(std::time::Duration::from_secs(5), f.run())
        .await
        .unwrap();
    assert_eq!(ad_decision(&f, id).await["reason"], "no_available_ad");
    lock.rollback().await.unwrap();
    let mut lock = f.pool.begin().await.unwrap();
    sqlx::query("LOCK TABLE ad_placements IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *lock)
        .await
        .unwrap();
    let repo =
        wekeeper_api::message_orchestration::south::adapter::postgres::Repository(f.pool.clone());
    // Reproduce restart after a successful COMMIT whose acknowledgement was lost.
    tokio::time::timeout(
        std::time::Duration::from_millis(500),
        repo.freeze_content(id),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(ad_decision(&f, id).await["reason"], "no_available_ad");
    lock.rollback().await.unwrap();
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn counter_lock_and_slow_statement_skip_ads_and_restore_connection_settings() {
    use wekeeper_api::advertising::{
        domain::{decision::DecisionInput, targeting::SourceIdentity},
        south::adapter::postgres::decisions,
    };
    let f = Fixture::new().await;
    f.ad("广告", "unit").await;
    sqlx::query("INSERT INTO ad_unit_counters(platform_id,unit_id) VALUES($1,$2)")
        .bind(f.platform)
        .bind(f.unit)
        .execute(&f.pool)
        .await
        .unwrap();
    let input = DecisionInput {
        source: SourceIdentity {
            platform_id: f.platform,
            unit_id: f.unit,
        },
        original: "原文".into(),
        channels: vec!["mobile".into(), "wecom".into()],
    };
    let mut lock = f.pool.begin().await.unwrap();
    sqlx::query("SELECT selected_count FROM ad_unit_counters WHERE unit_id=$1 FOR UPDATE")
        .bind(f.unit)
        .execute(&mut *lock)
        .await
        .unwrap();
    let mut tx = f.pool.begin().await.unwrap();
    sqlx::query("SET LOCAL statement_timeout='3s'")
        .execute(&mut *tx)
        .await
        .unwrap();
    let skipped = decisions::decide(&mut tx, &input).await.unwrap();
    assert_eq!(skipped.reason, "ad_lock_timeout");
    let setting: String = sqlx::query_scalar("SHOW statement_timeout")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(setting, "3s");
    tx.commit().await.unwrap();
    lock.rollback().await.unwrap();
    sqlx::query("CREATE FUNCTION slow_ad_counter() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_sleep(2); RETURN NEW; END $$")
        .execute(&f.pool).await.unwrap();
    sqlx::query("CREATE TRIGGER slow_ad_counter BEFORE UPDATE ON ad_unit_counters FOR EACH ROW EXECUTE FUNCTION slow_ad_counter()")
        .execute(&f.pool).await.unwrap();
    let mut tx = f.pool.begin().await.unwrap();
    let skipped = decisions::decide(&mut tx, &input).await.unwrap();
    assert_eq!(skipped.reason, "ad_decision_timeout");
    tx.commit().await.unwrap();
    sqlx::query("DROP TRIGGER slow_ad_counter ON ad_unit_counters")
        .execute(&f.pool)
        .await
        .unwrap();
    let mut tx = f.pool.begin().await.unwrap();
    sqlx::query("SET LOCAL statement_timeout='3s'")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("SET LOCAL lock_timeout='2s'")
        .execute(&mut *tx)
        .await
        .unwrap();
    assert_eq!(
        decisions::decide(&mut tx, &input)
            .await
            .unwrap()
            .selection_count,
        Some(1)
    );
    let settings: (String, String) = sqlx::query_as(
        "SELECT current_setting('statement_timeout'),current_setting('lock_timeout')",
    )
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(settings, ("3s".into(), "2s".into()));
    tx.commit().await.unwrap();
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn failed_ad_query_dependency_skips_without_waiting_for_repair() {
    let f = Fixture::new().await;
    f.configure(vec![f.target]).await;
    f.ad("广告", "unit").await;
    sqlx::query("ALTER TABLE ad_placements RENAME TO unavailable_ad_placements")
        .execute(&f.pool)
        .await
        .unwrap();
    // Keep an explicit failing object in this test schema so search_path
    // cannot resolve an unrelated public-schema table from other test suites.
    sqlx::query("CREATE VIEW ad_placements AS SELECT * FROM unavailable_ad_placements WHERE 1/0=0")
        .execute(&f.pool)
        .await
        .unwrap();
    let (id, _) = f.message().await;
    f.run().await;
    let decision = ad_decision(&f, id).await;
    assert_eq!(decision["reason"], "ad_dependency_unavailable");
    assert_eq!(decision["content"], "原消息");
    let state: String =
        sqlx::query_scalar("SELECT state FROM orchestration_dispatches WHERE message_id=$1")
            .bind(id)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(state, "delegated");
    sqlx::query("DROP VIEW ad_placements")
        .execute(&f.pool)
        .await
        .unwrap();
    sqlx::query("ALTER TABLE unavailable_ad_placements RENAME TO ad_placements")
        .execute(&f.pool)
        .await
        .unwrap();
    f.run().await;
    assert_eq!(ad_decision(&f, id).await, decision);
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn lost_database_connection_never_dispatches_an_uncommitted_fallback() {
    let f = Fixture::new().await;
    f.configure(vec![f.target]).await;
    f.ad("广告", "unit").await;
    sqlx::query("CREATE FUNCTION disconnect_ad_counter() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_terminate_backend(pg_backend_pid()); RETURN NEW; END $$")
        .execute(&f.pool).await.unwrap();
    sqlx::query("CREATE TRIGGER disconnect_ad_counter BEFORE UPDATE ON ad_unit_counters FOR EACH ROW EXECUTE FUNCTION disconnect_ad_counter()")
        .execute(&f.pool).await.unwrap();
    let (id, _) = f.message().await;
    f.run().await;
    let facts: (String, bool, i64) = sqlx::query_as("SELECT state,content_frozen_at IS NULL,(SELECT count(*) FROM orchestration_delegates WHERE message_id=$1) FROM orchestration_dispatches WHERE message_id=$1")
        .bind(id).fetch_one(&f.pool).await.unwrap();
    assert_eq!(facts, ("preparing".into(), true, 0));
    sqlx::query("DROP TRIGGER disconnect_ad_counter ON ad_unit_counters")
        .execute(&f.pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE orchestration_dispatches SET next_attempt_at=clock_timestamp() WHERE message_id=$1",
    )
    .bind(id)
    .execute(&f.pool)
    .await
    .unwrap();
    f.run().await;
    assert_eq!(ad_decision(&f, id).await["selection_count"], 1);
    f.close().await;
}
