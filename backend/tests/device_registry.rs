//! Device identity survives removal of the old sending channel.
use serde_json::{Value, json};
use sqlx::{PgPool, postgres::PgPoolOptions};
use uuid::Uuid;
use wekeeper_api::{
    business::Page,
    database,
    distribution::{
        domain::device_registry::{Command, Principal, Query, Scope},
        north::local::device_registry::Service,
        south::adapter::postgres::device_registry::Repository,
    },
};
fn scope(id: Option<Uuid>) -> Scope {
    Scope { school: None, id }
}
fn uid(v: &Value) -> Uuid {
    v["id"].as_str().unwrap().parse().unwrap()
}
#[test]
fn registry_contract_has_no_task_or_account_commands() {
    for operation in [
        "claim",
        "permit",
        "heartbeat",
        "create_account",
        "automation",
        "dispatch",
    ] {
        assert!(serde_json::from_value::<Command>(json!({"operation":operation})).is_err());
    }
    let value = json!({"operation":"enroll","code":"01234567","client_instance_id":"instance","protocol_version":9});
    let input: Command = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(input).unwrap(), value);
    assert!(
        serde_json::from_value::<Command>(
            json!({"operation":"create_device","name":"手机","sender_account_id":Uuid::new_v4()})
        )
        .is_err()
    );
}
#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn pairing_rotation_revocation_and_idempotency_use_only_registry() {
    let url = std::env::var("WEKEEPER_TEST_DATABASE_URL").expect("isolated database required");
    let base = PgPool::connect(&url).await.unwrap();
    let schema = format!("device_registry_{}", Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE SCHEMA {schema}"))
        .execute(&base)
        .await
        .unwrap();
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .after_connect(move |c, _| {
            let sql = format!("SET search_path TO {schema},public");
            Box::pin(async move {
                sqlx::query(&sql).execute(c).await?;
                Ok(())
            })
        })
        .connect(&url)
        .await
        .unwrap();
    database::migrate(&pool).await.unwrap();
    let actor = Uuid::new_v4();
    let outsider = Uuid::new_v4();
    sqlx::query("INSERT INTO accounts(id,username,display_name,platform_admin) VALUES($1,'platform','Platform',true),($2,'outsider','Outsider',false)").bind(actor).bind(outsider).execute(&pool).await.unwrap();
    let service = Service {
        store: Repository(pool.clone()),
    };
    let created = service
        .execute(
            Principal::Actor(actor),
            scope(None),
            "create",
            Command::CreateDevice {
                name: "手机".into(),
            },
        )
        .await
        .unwrap();
    let device = uid(&created);
    assert!(created["enabled"].as_bool().unwrap());
    assert_eq!(
        created,
        service
            .execute(
                Principal::Actor(actor),
                scope(None),
                "create",
                Command::CreateDevice {
                    name: "手机".into()
                }
            )
            .await
            .unwrap()
    );
    assert!(
        service
            .query(
                Principal::Actor(outsider),
                scope(None),
                Query::Devices,
                Page::default()
            )
            .await
            .is_err()
    );
    let enrollment = service
        .execute(
            Principal::Actor(actor),
            scope(Some(device)),
            "code",
            Command::Enrollment {
                expected_revision: created["revision"].as_i64().unwrap(),
            },
        )
        .await
        .unwrap();
    let code = enrollment["code"].as_str().unwrap();
    assert_eq!(code.len(), 8);
    let enroll = Command::Enroll {
        device_id: None,
        code: code.into(),
        client_instance_id: "instance".into(),
        protocol_version: 9,
    };
    let credential = service
        .execute(Principal::Enrollment, scope(None), "pair", enroll.clone())
        .await
        .unwrap();
    assert_eq!(uid(&credential), device);
    assert_eq!(credential["protocol_version"], 9);
    let replay = service
        .execute(Principal::Enrollment, scope(None), "pair", enroll)
        .await
        .unwrap();
    assert_eq!(replay["secret_replay_unavailable"], true);
    assert!(replay.get("device_token").is_none());
    assert!(
        service
            .execute(
                Principal::Enrollment,
                scope(None),
                "different-pair",
                Command::Enroll {
                    device_id: None,
                    code: code.into(),
                    client_instance_id: "instance".into(),
                    protocol_version: 9
                }
            )
            .await
            .is_err()
    );
    let generation = credential["credential_generation"].as_i64().unwrap();
    let rotated = service
        .execute(
            Principal::Device {
                id: device,
                generation,
            },
            scope(None),
            "rotate",
            Command::Rotate {},
        )
        .await
        .unwrap();
    assert_eq!(rotated["credential_generation"], generation + 1);
    let current = service
        .query(
            Principal::Actor(actor),
            scope(Some(device)),
            Query::Device,
            Page::default(),
        )
        .await
        .unwrap();
    service
        .execute(
            Principal::Actor(actor),
            scope(Some(device)),
            "revoke",
            Command::Revoke {
                expected_revision: current["revision"].as_i64().unwrap(),
                reason: "测试撤销".into(),
            },
        )
        .await
        .unwrap();
    assert!(
        service
            .execute(
                Principal::Device {
                    id: device,
                    generation: generation + 1
                },
                scope(None),
                "rejected-rotate",
                Command::Rotate {}
            )
            .await
            .is_err()
    );
    let live: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM device_credentials WHERE device_id=$1 AND revoked_at IS NULL",
    )
    .bind(device)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(live, 0);
    let old:bool=sqlx::query_scalar("SELECT to_regclass('mobile_delivery_items') IS NULL AND to_regclass('sending_accounts') IS NULL").fetch_one(&pool).await.unwrap();
    assert!(old);
    pool.close().await;
    base.close().await;
}
