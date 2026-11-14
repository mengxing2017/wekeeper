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
    assert!(value.get("revision").is_none());
    service
        .save(
            &f.actor,
            Channel::Mobile,
            f.unit,
            a,
            ConfigurationInput {
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
    f.message(1).await;
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
    let (message, _) = f.message(1).await;
    f.run().await;
    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM orchestration_delegates WHERE message_id=$1 AND state='delegated'",
    )
    .bind(message)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(n, 1);
    let memberships: i64 = sqlx::query_scalar("SELECT count(*) FROM mobile_workspace_members")
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(memberships, 0);
    f.close().await;
}
