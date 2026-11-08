//! Presence survives transport repair and uses one complete silence window.
use serde_json::{Value, json};
use sqlx::{PgPool, postgres::PgPoolOptions};
use uuid::Uuid;
use wekeeper_api::{
    business::hash,
    database,
    distribution::{
        domain::device_control::{Credential, Frame, HeartbeatPolicy, Operation, Pulse},
        north::local::device_control::Service,
        south::adapter::postgres::device_control::Repository,
    },
};

#[test]
fn policy_requires_three_complete_cycles_and_response_grace() {
    for (h, suspect, offline, valid) in [
        (60, 130, 190, true),
        (300, 610, 910, true),
        (20, 45, 90, false),
        (60, 75, 90, false),
        (60, 130, 189, false),
        (301, 612, 913, false),
    ] {
        assert_eq!(
            HeartbeatPolicy {
                heartbeat_interval_seconds: h,
                suspect_after_seconds: suspect,
                offline_after_seconds: offline
            }
            .valid(),
            valid
        );
    }
}

async fn snapshot(pool: &PgPool, device: Uuid) -> Value {
    sqlx::query_scalar("SELECT to_jsonb(p) FROM device_control_presence p WHERE id=$1")
        .bind(device)
        .fetch_one(pool)
        .await
        .unwrap()
}
async fn age(pool: &PgPool, device: Uuid, seconds: f64) {
    sqlx::query("UPDATE device_control_sessions SET last_heartbeat_at=clock_timestamp()-make_interval(secs=>$2) WHERE device_id=$1")
        .bind(device).bind(seconds).execute(pool).await.unwrap();
}
fn pulse(frame: &Frame, id: Uuid, sequence: i64) -> Operation {
    Operation::Pulse(Pulse {
        connection_id: frame.connection_id,
        epoch: frame.epoch,
        heartbeat_id: Some(id),
        heartbeat_sequence: sequence,
        probe_id: None,
        readiness: "blocked".into(),
        block_reason: Some("device_locked".into()),
        receipts: vec![],
    })
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn heartbeat_window_survives_close_reopen_replay_and_generation_changes() {
    let url = std::env::var("WEKEEPER_TEST_DATABASE_URL").expect("isolated database required");
    let base = PgPool::connect(&url).await.unwrap();
    let schema = format!("control_presence_{}", Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE SCHEMA {schema}"))
        .execute(&base)
        .await
        .unwrap();
    let schema_path = schema.clone();
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .after_connect(move |c, _| {
            let sql = format!("SET search_path TO {schema_path},public");
            Box::pin(async move {
                sqlx::query(&sql).execute(c).await?;
                Ok(())
            })
        })
        .connect(&url)
        .await
        .unwrap();
    database::migrate(&pool).await.unwrap();
    let device = Uuid::new_v4();
    let token = format!("wk_device_{}", Uuid::new_v4().simple());
    sqlx::query("INSERT INTO execution_devices(id,name,enabled,credential_generation) VALUES($1,'Presence phone',true,1)")
        .bind(device).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO device_credentials(token_hash,device_id,generation,expires_at) VALUES($1,$2,1,clock_timestamp()+interval '1 day')")
        .bind(hash(&token)).bind(device).execute(&pool).await.unwrap();
    let credential = Credential {
        device_id: device,
        generation: 1,
        token_hash: hash(&token),
    };
    let service = Service(Repository(pool.clone()));
    let first = service
        .device(
            &credential,
            Operation::Open {
                transport: "https".into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(
        (
            first.heartbeat_interval_seconds,
            first.offline_after_seconds
        ),
        (60, 190)
    );
    assert_eq!(snapshot(&pool, device).await["state"], "connecting");
    let heartbeat = Uuid::new_v4();
    let ack = service
        .device(&credential, pulse(&first, heartbeat, 1))
        .await
        .unwrap();
    assert_eq!(ack.heartbeat_id, Some(heartbeat));
    let online = snapshot(&pool, device).await;
    assert_eq!(online["state"], "online");
    assert_eq!(online["block_reason"], "device_locked"); // execution readiness is separate

    for (seconds, state) in [
        (35., "online"),
        (70., "online"),
        (129., "online"),
        (131., "suspect"),
        (181., "suspect"),
        (189., "suspect"),
        (191., "offline"),
    ] {
        age(&pool, device, seconds).await;
        assert_eq!(snapshot(&pool, device).await["state"], state);
    }
    // A current, new heartbeat immediately recovers an expired device.
    service
        .device(&credential, pulse(&first, Uuid::new_v4(), 2))
        .await
        .unwrap();
    assert_eq!(snapshot(&pool, device).await["state"], "online");
    age(&pool, device, 70.).await;
    let before_close = snapshot(&pool, device).await;
    service
        .device(
            &credential,
            Operation::Close {
                connection_id: first.connection_id,
                epoch: first.epoch,
            },
        )
        .await
        .unwrap();
    let closed = snapshot(&pool, device).await;
    assert_eq!(closed["state"], "online");
    assert_eq!(closed["transport_connected"], false);
    assert_eq!(
        closed["last_control_heartbeat_at"],
        before_close["last_control_heartbeat_at"]
    );
    let second = service
        .device(
            &credential,
            Operation::Open {
                transport: "https".into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(
        snapshot(&pool, device).await["last_control_heartbeat_at"],
        before_close["last_control_heartbeat_at"]
    );
    assert!(
        service
            .device(&credential, pulse(&first, Uuid::new_v4(), 2))
            .await
            .is_err()
    );
    service
        .device(
            &credential,
            Operation::Close {
                connection_id: first.connection_id,
                epoch: first.epoch,
            },
        )
        .await
        .unwrap();
    assert_eq!(snapshot(&pool, device).await["transport_connected"], true);

    let id = Uuid::new_v4();
    service
        .device(&credential, pulse(&second, id, 1))
        .await
        .unwrap();
    service
        .device(&credential, pulse(&second, Uuid::new_v4(), 2))
        .await
        .unwrap();
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM device_control_pulse_receipts WHERE device_id=$1")
            .bind(device)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 1); // latest state only, even after multiple heartbeats
    age(&pool, device, 191.).await;
    let expired = snapshot(&pool, device).await;
    service
        .device(&credential, pulse(&second, id, 1))
        .await
        .unwrap(); // nonconsecutive replay
    assert_eq!(
        snapshot(&pool, device).await["last_control_heartbeat_at"],
        expired["last_control_heartbeat_at"]
    );
    assert_eq!(snapshot(&pool, device).await["state"], "offline");
    // Polling and reconnecting an expired device do not renew it or prevent recovery.
    service
        .device(
            &credential,
            Operation::Poll {
                connection_id: second.connection_id,
                epoch: second.epoch,
            },
        )
        .await
        .unwrap();
    let third = service
        .device(
            &credential,
            Operation::Open {
                transport: "https".into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(snapshot(&pool, device).await["state"], "offline");
    service
        .device(&credential, pulse(&third, Uuid::new_v4(), 1))
        .await
        .unwrap();
    assert_eq!(snapshot(&pool, device).await["state"], "online");

    // Even a device that never completed its first heartbeat cannot extend its
    // initial connecting window by repeatedly reopening the transport.
    sqlx::query("UPDATE device_control_sessions SET last_heartbeat_at=NULL,presence_started_at=clock_timestamp()-interval '191 seconds' WHERE device_id=$1")
        .bind(device).execute(&pool).await.unwrap();
    service
        .device(
            &credential,
            Operation::Open {
                transport: "https".into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(snapshot(&pool, device).await["state"], "offline");
    sqlx::query("UPDATE execution_devices SET credential_generation=2 WHERE id=$1")
        .bind(device)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(snapshot(&pool, device).await["state"], "unauthorized");
    assert!(
        service
            .device(&credential, pulse(&third, Uuid::new_v4(), 1))
            .await
            .is_err()
    );
    sqlx::query("INSERT INTO device_credentials(token_hash,device_id,generation,expires_at) VALUES($1,$2,2,clock_timestamp()+interval '1 day')")
        .bind(hash("wk_device_replacement")).bind(device).execute(&pool).await.unwrap();
    let next = Credential {
        device_id: device,
        generation: 2,
        token_hash: hash("wk_device_replacement"),
    };
    let fresh = service
        .device(
            &next,
            Operation::Open {
                transport: "https".into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(snapshot(&pool, device).await["state"], "connecting");
    service
        .device(&next, pulse(&fresh, Uuid::new_v4(), 1))
        .await
        .unwrap();
    sqlx::query("UPDATE execution_devices SET enabled=false WHERE id=$1")
        .bind(device)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(snapshot(&pool, device).await["state"], json!("disabled"));
    pool.close().await;
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&base)
        .await
        .unwrap();
    base.close().await;
}
