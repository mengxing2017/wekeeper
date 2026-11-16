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
    let (one, _) = f.message(2).await;
    f.run().await;
    f.run().await;
    let (two, _) = f.message(1).await;
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
            d["content"].as_str().unwrap().matches("【广告】").count(),
            1
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
    let (id, receipt) = f.message(1).await;
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
    f.message(1).await;
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
    let (id, _) = f.message(1).await;
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
    f.message(1).await;
    f.run().await;
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM mobile_v9_task_items")
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(n, 1);
    sqlx::query("UPDATE channel_receiving_configurations SET configuration=jsonb_set(configuration,'{device_ids}','[]')").execute(&f.pool).await.unwrap();
    let (id, _) = f.message(1).await;
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
    let (id, receipt) = f.message(1).await;
    f.run().await;
    let d: Value =
        sqlx::query_scalar("SELECT decision FROM orchestration_dispatches WHERE message_id=$1")
            .bind(id)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert!(d["selected_ad"].is_null());
    assert!(!d["content"].as_str().unwrap().contains("【广告】"));
    assert_eq!(d["selection_count"], 0);
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
async fn interrupted_ad_commit_uses_original_frozen_targets_and_decision() {
    let f = Fixture::new().await;
    let config = f.configure(vec![f.target]).await;
    f.ad("冻结广告", "unit").await;
    sqlx::query("CREATE FUNCTION fail_ready() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF OLD.state='preparing' AND NEW.state='ready' THEN RAISE EXCEPTION 'injected interruption after ad commit'; END IF; RETURN NEW; END $$").execute(&f.pool).await.unwrap();
    sqlx::query("CREATE TRIGGER fail_ready BEFORE UPDATE ON orchestration_dispatches FOR EACH ROW EXECUTE FUNCTION fail_ready()").execute(&f.pool).await.unwrap();
    let (id, _) = f.message(1).await;
    f.run().await;
    let state: String =
        sqlx::query_scalar("SELECT state FROM orchestration_dispatches WHERE message_id=$1")
            .bind(id)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(state, "preparing");
    sqlx::query("UPDATE channel_receiving_configurations SET configuration=jsonb_set(configuration,'{target_ids}','[]') WHERE id=$1").bind(config).execute(&f.pool).await.unwrap();
    sqlx::query("UPDATE ad_placements SET enabled=false")
        .execute(&f.pool)
        .await
        .unwrap();
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
    sqlx::query("INSERT INTO mobile_targets(id,search_text,created_by,updated_by) VALUES($1,'没有手机的群',$2,$2)").bind(missing).bind(f.actor.id).execute(&f.pool).await.unwrap();
    f.configure(vec![missing, f.target]).await;
    sqlx::query("DELETE FROM mobile_device_target_bindings WHERE target_id=$1")
        .bind(missing)
        .execute(&f.pool)
        .await
        .unwrap();
    let (_, receipt) = f.message(1).await;
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
    let (id, receipt) = f.message(1).await;
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
    assert_eq!(contents[0].matches("【广告】").count(), 1);
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
    let (long, _) = f.message_with_content(2, &"原".repeat(250)).await;
    f.run().await;
    f.run().await;
    let contents: Vec<String> = sqlx::query_scalar("SELECT s.content FROM orchestration_delegates d JOIN distribution_message_snapshots s ON s.id=d.snapshot_id WHERE d.message_id=$1 AND d.state='delegated'")
        .bind(long).fetch_all(&f.pool).await.unwrap();
    assert_eq!(contents.len(), 2);
    assert_eq!(contents[0], contents[1]);
    assert!(contents[0].contains(&"原".repeat(250)));
    assert!(!contents[0].contains("【广告】"));
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
    for sequence in 1..=50 {
        failed.push(f.message(sequence).await.0);
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
    let (new, _) = f.message(51).await;
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
    let protected_paths = [
        "/api/v2/admin/source-route-assignments".to_string(),
        format!("/api/v2/admin/source-routes/{}/configuration", f.route),
        format!("/api/v2/admin/source-routes/{}/receipts", f.route),
    ];
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
        receipt = f.message(n).await.1;
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
