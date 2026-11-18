#[path = "support/business_dispatch.rs"]
mod support;
use support::Fixture;
use uuid::Uuid;
use wekeeper_api::{
    business::Page,
    distribution::{
        domain::receiving::{Channel, ConfigurationInput},
        north::local::receiving::Service,
        south::adapter::postgres::receiving::Repository,
    },
};
#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn unit_configuration_has_no_workspace_version_or_target_exclusivity() {
    let f = Fixture::new().await;
    let a = f.configure(vec![f.target]).await;
    let b = f.configure(vec![f.target]).await;
    assert_ne!(a, b);
    let service = Service(Repository(f.pool.clone()));
    let c = service
        .get(&f.actor, Channel::Mobile, f.unit, a)
        .await
        .unwrap();
    assert_eq!(c.business_unit_id, f.unit);
    let value = serde_json::to_value(c).unwrap();
    assert!(value.get("workspace_id").is_none());
    assert_eq!(value["revision"], 1);
    service
        .save(
            &f.actor,
            Channel::Mobile,
            f.unit,
            a,
            ConfigurationInput {
                target_selection_mode:
                    wekeeper_api::distribution::domain::receiving::SelectionMode::All,
                device_dispatch_mode: Some(
                    wekeeper_api::distribution::domain::receiving::SelectionMode::RoundRobin,
                ),
                name: "最新配置".into(),
                enabled: true,
                target_ids: vec![],
                device_ids: vec![f.device],
                sender_ids: vec![],
            },
        )
        .await
        .unwrap();
    assert_eq!(
        service
            .list(&f.actor, Channel::Mobile, f.unit, Page::default())
            .await
            .unwrap()
            .len(),
        2
    );
    let mut outsider = f.actor.clone();
    outsider.id = Uuid::new_v4();
    outsider.platform_admin = false;
    assert!(
        service
            .get(&outsider, Channel::Mobile, f.unit, a)
            .await
            .is_err()
    );
    let _ = (&f.route, f.platform, f.scope);
    f.message().await;
    f.run().await;
    f.ad("广告", "global").await;
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn unit_creates_target_and_binds_phone_without_workspace_setup() {
    use wekeeper_api::distribution::domain::receiving::TargetInput;
    let f = Fixture::new().await;
    let service = Service(Repository(f.pool.clone()));
    let id = Uuid::new_v4();
    let target = service
        .target(
            &f.actor,
            Channel::Mobile,
            f.unit,
            id,
            TargetInput {
                name: "只填写一个群名".into(),
                directory_group_id: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(target.id, id);
    let replay = service
        .target(
            &f.actor,
            Channel::Mobile,
            f.unit,
            id,
            TargetInput {
                name: "只填写一个群名".into(),
                directory_group_id: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(replay.id, id);
    f.configure(vec![target.id]).await;
    let (message, _) = f.message().await;
    f.run().await;
    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM orchestration_delegates WHERE message_id=$1 AND state='delegated'",
    )
    .bind(message)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(n, 1);
    f.close().await;
}

use serde_json::Value;
use wekeeper_api::distribution::domain::receiving::SelectionMode;
async fn devices_and_targets(f: &Fixture, n: usize) -> (Vec<Uuid>, Vec<Uuid>) {
    let mut devices = vec![f.device];
    let mut targets = vec![f.target];
    for i in 1..n {
        let device = Uuid::new_v4();
        let target = Uuid::new_v4();
        sqlx::query("INSERT INTO execution_devices(id,name,enabled,credential_generation) VALUES($1,$2,true,1)").bind(device).bind(format!("Phone {i}")).execute(&f.pool).await.unwrap();
        sqlx::query(
            "INSERT INTO mobile_targets(id,search_text,created_by,updated_by) VALUES($1,$2,$3,$3)",
        )
        .bind(target)
        .bind(format!("群 {i}"))
        .bind(f.actor.id)
        .execute(&f.pool)
        .await
        .unwrap();
        devices.push(device);
        targets.push(target);
    }
    (devices, targets)
}
async fn policy(
    f: &Fixture,
    id: Uuid,
    devices: Vec<Uuid>,
    targets: Vec<Uuid>,
    group_mode: SelectionMode,
    phone_mode: SelectionMode,
) {
    Service(Repository(f.pool.clone()))
        .save(
            &f.actor,
            Channel::Mobile,
            f.unit,
            id,
            ConfigurationInput {
                name: "策略".into(),
                enabled: true,
                target_ids: targets,
                device_ids: devices,
                sender_ids: vec![],
                target_selection_mode: group_mode,
                device_dispatch_mode: Some(phone_mode),
            },
        )
        .await
        .unwrap();
}
#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn persistent_round_robin_moves_both_dimensions_once_per_new_message() {
    let f = Fixture::new().await;
    let (devices, targets) = devices_and_targets(&f, 3).await;
    let config = Uuid::new_v4();
    policy(
        &f,
        config,
        devices.clone(),
        targets.clone(),
        SelectionMode::RoundRobin,
        SelectionMode::RoundRobin,
    )
    .await;
    for i in 0..4 {
        let (message, _) = f.message().await;
        f.run().await;
        f.run().await;
        let plan: Value =
            sqlx::query_scalar("SELECT plan FROM mobile_dispatch_intakes WHERE message_id=$1")
                .bind(message)
                .fetch_one(&f.pool)
                .await
                .unwrap();
        assert_eq!(plan["targets"][0]["id"], targets[i % 3].to_string());
        assert_eq!(
            plan["targets"][0]["identity"]["candidates"][0]["device_id"],
            devices[i % 3].to_string()
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM receiving_dispatch_decisions WHERE message_id=$1"
            )
            .bind(message)
            .fetch_one(&f.pool)
            .await
            .unwrap(),
            1
        );
    }
    let current = Service(Repository(f.pool.clone()))
        .get(&f.actor, Channel::Mobile, f.unit, config)
        .await
        .unwrap();
    assert_eq!(current.next_target_id, Some(targets[1]));
    assert_eq!(current.next_device_id, Some(devices[1]));
    policy(
        &f,
        config,
        vec![devices[0], devices[2]],
        vec![targets[0], targets[2]],
        SelectionMode::RoundRobin,
        SelectionMode::RoundRobin,
    )
    .await;
    let current = Service(Repository(f.pool.clone()))
        .get(&f.actor, Channel::Mobile, f.unit, config)
        .await
        .unwrap();
    assert_eq!(current.next_target_id, Some(targets[2]));
    assert_eq!(current.next_device_id, Some(devices[2]));
    f.close().await;
}
#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn all_phones_have_independent_tasks_but_upstream_counts_one_group() {
    let f = Fixture::new().await;
    let (devices, _) = devices_and_targets(&f, 3).await;
    policy(
        &f,
        Uuid::new_v4(),
        devices.clone(),
        vec![f.target],
        SelectionMode::All,
        SelectionMode::All,
    )
    .await;
    let (message, receipt) = f.message().await;
    f.run().await;
    let tasks: Vec<Uuid> = sqlx::query_scalar(
        "SELECT task_id FROM mobile_dispatch_intakes WHERE message_id=$1 ORDER BY task_id",
    )
    .bind(message)
    .fetch_all(&f.pool)
    .await
    .unwrap();
    assert_eq!(tasks.len(), 3);
    assert_eq!(
        tasks.iter().collect::<std::collections::HashSet<_>>().len(),
        3
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM orchestration_delivery_progress WHERE message_id=$1"
        )
        .bind(message)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        1
    );
    sqlx::query("UPDATE mobile_v9_attempts SET state='observed_sent' WHERE item_id IN (SELECT id FROM mobile_v9_task_items WHERE task_id=$1)").bind(tasks[0]).execute(&f.pool).await.unwrap();
    sqlx::query("UPDATE mobile_v9_dispatch_state SET state='dispatch_failed' WHERE item_id IN (SELECT id FROM mobile_v9_task_items WHERE task_id=ANY($1))").bind(&tasks[1..]).execute(&f.pool).await.unwrap();
    sqlx::query("UPDATE mobile_v9_attempts SET state='unknown' WHERE item_id IN (SELECT id FROM mobile_v9_task_items WHERE task_id=$1)").bind(tasks[2]).execute(&f.pool).await.unwrap();
    // An uncertain attempt without an execution grant remains visible as historical risk.
    sqlx::query("UPDATE mobile_v9_dispatch_state SET state='needs_review' WHERE item_id IN (SELECT id FROM mobile_v9_task_items WHERE task_id=$1)").bind(tasks[2]).execute(&f.pool).await.unwrap();
    let view = Service(Repository(f.pool.clone()))
        .phone_progress(&f.actor, receipt, Page::default())
        .await
        .unwrap();
    assert_eq!(view["summary"]["expected_count"], 3);
    assert_eq!(view["summary"]["sent"], 1);
    assert_eq!(view["summary"]["failed"], 1);
    assert_eq!(view["summary"]["unknown"], 1);
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT outcome FROM orchestration_delivery_progress WHERE message_id=$1"
        )
        .bind(message)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        "unknown"
    );
    f.run().await;
    let total: i64 =
        sqlx::query_scalar("SELECT summary_total FROM source_receipts WHERE receipt_id=$1")
            .bind(receipt)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(total, 1);
    f.close().await;
}
#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn concurrent_messages_and_replays_reserve_distinct_positions() {
    let f = Fixture::new().await;
    let (devices, targets) = devices_and_targets(&f, 2).await;
    policy(
        &f,
        Uuid::new_v4(),
        devices,
        targets.clone(),
        SelectionMode::RoundRobin,
        SelectionMode::RoundRobin,
    )
    .await;
    let (one, _) = f.message().await;
    let (two, _) = f.message().await;
    let a =
        wekeeper_api::message_orchestration::south::adapter::postgres::Repository(f.pool.clone());
    let b = a.clone();
    let (x, y) = tokio::join!(a.run_once(), b.run_once());
    x.unwrap();
    y.unwrap();
    let assigned: Vec<Uuid> = sqlx::query_scalar(
        "SELECT target_id FROM receiving_dispatch_targets WHERE message_id=ANY($1)",
    )
    .bind(vec![one, two])
    .fetch_all(&f.pool)
    .await
    .unwrap();
    assert_eq!(assigned.len(), 2);
    assert_ne!(assigned[0], assigned[1]);
    f.close().await;
}
#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn all_phone_failure_isolated_and_stopping_prevents_remaining_materialization_from_sending() {
    let f = Fixture::new().await;
    let (devices, targets) = devices_and_targets(&f, 11).await;
    policy(
        &f,
        Uuid::new_v4(),
        devices.clone(),
        targets,
        SelectionMode::All,
        SelectionMode::All,
    )
    .await;
    // Disabled phone remains an expected responsibility, but must never receive a task.
    sqlx::query("UPDATE execution_devices SET enabled=false WHERE id=$1")
        .bind(devices[0])
        .execute(&f.pool)
        .await
        .unwrap();
    let (message, receipt) = f.message().await;
    f.run().await;
    let before: i64 = sqlx::query_scalar("SELECT count(*) FROM mobile_v9_tasks")
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert!(before > 0 && before < 121);
    Service(Repository(f.pool.clone()))
        .stop_phone(&f.actor, receipt, None)
        .await
        .unwrap();
    f.run().await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM mobile_v9_tasks")
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        before
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM receiving_dispatch_targets WHERE message_id=$1 AND NOT stopped"
        )
        .bind(message)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        0
    );
    let view = Service(Repository(f.pool.clone()))
        .phone_progress(&f.actor, receipt, Page::default())
        .await
        .unwrap();
    assert_eq!(view["summary"]["expected_count"], 121);
    assert!(view["summary"]["failed"].as_i64().unwrap() > 0);
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn allocation_rollback_does_not_consume_rotation_and_failed_message_can_resume() {
    let f = Fixture::new().await;
    let (devices, targets) = devices_and_targets(&f, 2).await;
    let config = Uuid::new_v4();
    policy(
        &f,
        config,
        devices.clone(),
        targets.clone(),
        SelectionMode::RoundRobin,
        SelectionMode::RoundRobin,
    )
    .await;
    sqlx::raw_sql("CREATE FUNCTION fail_decision() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'simulated transaction failure'; END $$; CREATE TRIGGER fail_decision BEFORE INSERT ON receiving_dispatch_decisions FOR EACH ROW EXECUTE FUNCTION fail_decision();").execute(&f.pool).await.unwrap();
    let (message, _) = f.message().await;
    f.run().await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM receiving_dispatch_decisions")
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        0
    );
    let c = Service(Repository(f.pool.clone()))
        .get(&f.actor, Channel::Mobile, f.unit, config)
        .await
        .unwrap();
    assert_eq!(c.next_device_id, Some(devices[0]));
    sqlx::query("DROP TRIGGER fail_decision ON receiving_dispatch_decisions")
        .execute(&f.pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE orchestration_dispatches SET next_attempt_at=clock_timestamp() WHERE message_id=$1",
    )
    .bind(message)
    .execute(&f.pool)
    .await
    .unwrap();
    f.run().await;
    let target: Uuid =
        sqlx::query_scalar("SELECT target_id FROM receiving_dispatch_targets WHERE message_id=$1")
            .bind(message)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(target, targets[0]);
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn materialization_resumes_after_configuration_change_without_expanding_or_losing_responsibilities()
 {
    let f = Fixture::new().await;
    let (devices, targets) = devices_and_targets(&f, 11).await;
    let config = Uuid::new_v4();
    policy(
        &f,
        config,
        devices.clone(),
        targets.clone(),
        SelectionMode::All,
        SelectionMode::All,
    )
    .await;
    let (message, receipt) = f.message().await;
    f.run().await;
    let initial: i64 =
        sqlx::query_scalar("SELECT count(*) FROM mobile_dispatch_intakes WHERE message_id=$1")
            .bind(message)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(initial, 100);
    policy(
        &f,
        config,
        vec![devices[0]],
        vec![targets[0]],
        SelectionMode::All,
        SelectionMode::RoundRobin,
    )
    .await;
    f.run().await;
    f.run().await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM mobile_dispatch_intakes WHERE message_id=$1"
        )
        .bind(message)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        121
    );
    let view = Service(Repository(f.pool.clone()))
        .phone_progress(
            &f.actor,
            receipt,
            Page {
                page: Some(2),
                page_size: Some(100),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(view["summary"]["materialized_count"], 121);
    assert_eq!(view["items"].as_array().unwrap().len(), 21);
    let mut outsider = f.actor.clone();
    outsider.platform_admin = false;
    outsider.id = Uuid::new_v4();
    assert!(
        Service(Repository(f.pool.clone()))
            .phone_progress(&outsider, receipt, Page::default())
            .await
            .is_err()
    );
    f.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn empty_all_phone_selection_never_falls_back_to_system_devices() {
    let f = Fixture::new().await;
    policy(
        &f,
        Uuid::new_v4(),
        vec![],
        vec![f.target],
        SelectionMode::All,
        SelectionMode::All,
    )
    .await;
    let (message, receipt) = f.message().await;
    f.run().await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM mobile_v9_tasks")
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT outcome FROM orchestration_delivery_progress WHERE message_id=$1"
        )
        .bind(message)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        "failed"
    );
    let view = Service(Repository(f.pool.clone()))
        .phone_progress(&f.actor, receipt, Page::default())
        .await
        .unwrap();
    assert_eq!(view["summary"]["expected_count"], 0);
    assert_eq!(view["summary"]["target_count"], 1);
    f.close().await;
}
