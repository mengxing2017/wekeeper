use serde_json::json;
use sqlx::{PgPool, postgres::PgPoolOptions};
use uuid::Uuid;
use wekeeper_api::{
    business::{Actor, Page},
    database,
    distribution::{
        north::local::mobile_execution::Service,
        south::{
            adapter::postgres::mobile_execution::Repository, port::mobile_execution::Resource,
        },
    },
};

async fn setup() -> (PgPool, Actor, Uuid, Uuid) {
    let url =
        std::env::var("WEKEEPER_TEST_DATABASE_URL").expect("explicit isolated database required");
    let base = PgPool::connect(&url).await.unwrap();
    let schema = format!("phone_isolation_{}", Uuid::new_v4().simple());
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
    let school = Uuid::new_v4();
    let actor = Uuid::new_v4();
    let device = Uuid::new_v4();
    sqlx::query("INSERT INTO schools(id,name) VALUES($1,'School')")
        .bind(school)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO accounts(id,username,display_name,platform_admin) VALUES($1,'admin','Admin',true)").bind(actor).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO school_memberships(school_id,account_id,role) VALUES($1,$2,'admin')")
        .bind(school)
        .bind(actor)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO execution_devices(id,name,enabled) VALUES($1,'Phone',true)")
        .bind(device)
        .execute(&pool)
        .await
        .unwrap();
    let a:Actor=serde_json::from_value(json!({"id":actor,"username":"admin","display_name":"Admin","platform_admin":true,"grants":[],"csrf":"","wecom_authenticated":false})).unwrap();
    (pool, a, school, device)
}
#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn shared_bindings_require_platform_authority_and_preserve_effective_locator() {
    let (pool, a, school, device) = setup().await;
    let service = Service(Repository(pool.clone()));
    let target = service
        .write(
            &a,
            Some(school),
            "target",
            None,
            "target",
            json!({"search_text":"默认  群🦦"}),
        )
        .await
        .unwrap();
    let target_id: Uuid = serde_json::from_value(target["id"].clone()).unwrap();
    assert!(
        service
            .write(
                &a,
                Some(school),
                "bind",
                None,
                "school-bind",
                json!({"target_id":target_id,"device_ids":[device],"search_text_override":null})
            )
            .await
            .is_err()
    );
    let input =
        json!({"target_id":target_id,"device_ids":[device],"search_text_override":"手机  备注🦦"});
    let bound = service
        .write(&a, None, "bind", None, "bind", input.clone())
        .await
        .unwrap();
    assert_eq!(
        service
            .write(&a, None, "bind", None, "bind", input)
            .await
            .unwrap(),
        bound
    );
    let rows = service
        .list(
            &a,
            Some(school),
            Resource::Bindings,
            Page::default(),
            String::new(),
            None,
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["search_text"], "手机  备注🦦");
    assert_eq!(rows[0]["search_source"], "binding_override");
    let source_revision = rows[0]["source_locator_revision"].clone();
    service.write(&a,Some(school),"update-target",Some(target_id),"rename",json!({"expected_revision":1,"search_text":"新默认名","alias":"别名","remark":"备注","enabled":true})).await.unwrap();
    let rows = service
        .list(
            &a,
            Some(school),
            Resource::Bindings,
            Page::default(),
            String::new(),
            None,
        )
        .await
        .unwrap();
    assert_eq!(rows[0]["search_text"], "手机  备注🦦");
    assert_eq!(rows[0]["source_locator_revision"], source_revision);
    let binding: Uuid = serde_json::from_value(rows[0]["id"].clone()).unwrap();
    service
        .write(
            &a,
            None,
            "update-binding",
            Some(binding),
            "inherit",
            json!({"expected_revision":1,"enabled":true,"search_text_override":null}),
        )
        .await
        .unwrap();
    let rows = service
        .list(
            &a,
            Some(school),
            Resource::Bindings,
            Page::default(),
            String::new(),
            None,
        )
        .await
        .unwrap();
    assert_eq!(rows[0]["search_text"], "新默认名");
    assert_eq!(rows[0]["search_source"], "target_default");
    assert_eq!(rows[0]["revision"], 2);
    assert!(rows[0].get("grant_id").is_none());
    pool.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn one_shared_device_serves_two_schools_without_leaking_global_notes() {
    let (pool, a, school, device) = setup().await;
    let other = Uuid::now_v7();
    sqlx::query("INSERT INTO schools(id,name) VALUES($1,'Other')")
        .bind(other)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO school_memberships(school_id,account_id,role) VALUES($1,$2,'admin')")
        .bind(other)
        .bind(a.id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE execution_devices SET remark='private-account-hint' WHERE id=$1")
        .bind(device)
        .execute(&pool)
        .await
        .unwrap();
    let service = Service(Repository(pool.clone()));
    let mut bindings = Vec::new();
    for (index, s) in [school, other].iter().enumerate() {
        let target = service
            .write(
                &a,
                Some(*s),
                "target",
                None,
                &format!("target-{index}"),
                json!({"search_text":format!("群{index}")}),
            )
            .await
            .unwrap();
        let result = service
            .write(
                &a,
                None,
                "bind",
                None,
                &format!("bind-{index}"),
                json!({"target_id":target["id"],"device_ids":[device],"search_text_override":null}),
            )
            .await
            .unwrap();
        bindings.push(result["bindings"][0].clone());
    }
    for s in [school, other] {
        let devices = service
            .list(
                &a,
                Some(s),
                Resource::Devices,
                Page::default(),
                String::new(),
                None,
            )
            .await
            .unwrap();
        assert_eq!(devices.len(), 1);
        for private in ["remark", "custodian_actor_id", "school_id", "grant_id"] {
            assert!(devices[0].get(private).is_none());
        }
        let rows = service
            .list(
                &a,
                Some(s),
                Resource::Bindings,
                Page::default(),
                String::new(),
                None,
            )
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["school_id"], s.to_string());
    }
    let id: Uuid = serde_json::from_value(bindings[0]["id"].clone()).unwrap();
    service
        .write(
            &a,
            None,
            "revoke-binding",
            Some(id),
            "unbind",
            json!({"expected_revision":1,"reason":"停用甲群"}),
        )
        .await
        .unwrap();
    assert!(
        service
            .list(
                &a,
                Some(school),
                Resource::Devices,
                Page::default(),
                String::new(),
                None
            )
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        service
            .list(
                &a,
                Some(other),
                Resource::Devices,
                Page::default(),
                String::new(),
                None
            )
            .await
            .unwrap()
            .len(),
        1
    );
    pool.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn batch_bind_is_atomic_and_never_infers_school_from_client() {
    let (pool, a, school, device) = setup().await;
    let service = Service(Repository(pool.clone()));
    let target = service
        .write(
            &a,
            Some(school),
            "target",
            None,
            "target",
            json!({"search_text":"群"}),
        )
        .await
        .unwrap();
    for (key, input) in [
        (
            "missing-device",
            json!({"target_id":target["id"],"device_ids":[device,Uuid::now_v7()],"search_text_override":null}),
        ),
        (
            "spoof-school",
            json!({"target_id":target["id"],"device_ids":[device],"search_text_override":null,"school_id":Uuid::now_v7()}),
        ),
    ] {
        assert!(
            service
                .write(&a, None, "bind", None, key, input)
                .await
                .is_err()
        );
    }
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM mobile_device_target_bindings")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    assert!(
        service
            .write(
                &a,
                Some(school),
                "target",
                None,
                "old-target",
                json!({"search_text":"群","device_ids":[device]})
            )
            .await
            .is_err()
    );
    assert!(
        service
            .write(
                &a,
                None,
                "grant",
                None,
                "old-grant",
                json!({"school_id":school,"device_id":device,"purpose":"old"})
            )
            .await
            .is_err()
    );
    assert!(
        service
            .write(
                &a,
                Some(school),
                "task-preview",
                None,
                "old-preview",
                json!({"device_id":device,"target_ids":[target["id"]],"message":"消息"})
            )
            .await
            .is_err()
    );
    pool.close().await;
}

#[tokio::test]
#[ignore = "requires WEKEEPER_TEST_DATABASE_URL pointing to an isolated PostgreSQL"]
async fn shared_capability_consent_is_device_scoped_versioned_and_never_execution_ready() {
    use wekeeper_api::distribution::domain::{
        device_control::Credential, mobile_execution::shared_capability::DecisionInput,
    };
    let (pool, actor, school, device) = setup().await;
    let service = Service(Repository(pool.clone()));
    sqlx::query("UPDATE execution_devices SET credential_generation=1 WHERE id=$1")
        .bind(device)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO device_credentials(token_hash,device_id,generation,expires_at) VALUES('cap-test', $1,1,clock_timestamp()+interval '1 hour')").bind(device).execute(&pool).await.unwrap();
    let credential = Credential {
        device_id: device,
        generation: 1,
        token_hash: "cap-test".into(),
    };
    assert!(
        service
            .capability_context(&credential)
            .await
            .unwrap()
            .is_empty()
    );
    let flow: serde_json::Value = serde_json::from_str(include_str!(
        "../../api-contract/fixtures/shared-mobile-flow-v7.json"
    ))
    .unwrap();
    let config = json!({"device_id":device,"expected_revision":0,"name":"全局配置","enabled":true,"flow":flow});
    assert!(
        service
            .write(
                &actor,
                Some(school),
                "capability",
                None,
                "school-forbidden",
                config.clone()
            )
            .await
            .is_err()
    );
    let result = service
        .write(&actor, None, "capability", None, "create", config.clone())
        .await
        .unwrap();
    assert_eq!(result["execution_ready"], false);
    assert_eq!(
        service
            .write(&actor, None, "capability", None, "create", config.clone())
            .await
            .unwrap(),
        result
    );
    assert!(
        service
            .write(
                &actor,
                None,
                "capability",
                None,
                "duplicate",
                config.clone()
            )
            .await
            .is_err()
    );
    let cap: Uuid = serde_json::from_value(result["id"].clone()).unwrap();
    let scope = service
        .capability_context(&credential)
        .await
        .unwrap()
        .remove(0);
    assert!(scope["scope"].get("school_id").is_none());
    let input:DecisionInput=serde_json::from_value(json!({"capability_id":cap,"configuration_revision":1,"scope_digest":scope["scope_digest"],"expected_revision":0,"accepted":true})).unwrap();
    let saved = service
        .capability_decision(&credential, "accept", input.clone())
        .await
        .unwrap();
    assert_eq!(saved["accepted"], true);
    assert_eq!(
        service
            .capability_decision(&credential, "accept", input.clone())
            .await
            .unwrap(),
        saved
    );
    let mut different = input.clone();
    different.accepted = false;
    assert!(
        service
            .capability_decision(&credential, "accept", different)
            .await
            .is_err()
    );
    assert!(
        service
            .capability_decision(&credential, "stale", input.clone())
            .await
            .is_err()
    );
    let mut next = input.clone();
    next.expected_revision = 1;
    next.accepted = false;
    let (a, b) = tokio::join!(
        service.capability_decision(&credential, "race-a", next.clone()),
        service.capability_decision(&credential, "race-b", next)
    );
    assert_ne!(a.is_ok(), b.is_ok());
    assert_eq!(
        service.capability_context(&credential).await.unwrap()[0]["accepted"],
        false
    );
    // Another valid device cannot query this consent by copying a capability UUID or idempotency key.
    let other = Uuid::new_v4();
    sqlx::query("INSERT INTO execution_devices(id,name,enabled,credential_generation) VALUES($1,'Other',true,1)").bind(other).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO device_credentials(token_hash,device_id,generation,expires_at) VALUES('cap-other',$1,1,clock_timestamp()+interval '1 hour')").bind(other).execute(&pool).await.unwrap();
    let other = Credential {
        device_id: other,
        generation: 1,
        token_hash: "cap-other".into(),
    };
    assert!(service.capability_context(&other).await.unwrap().is_empty());
    assert!(
        service
            .capability_decision(&other, "accept", input.clone())
            .await
            .is_err()
    );
    let mut updated = config;
    updated["expected_revision"] = json!(1);
    updated["name"] = json!("配置二");
    service
        .write(
            &actor,
            None,
            "update-capability",
            Some(cap),
            "update",
            updated.clone(),
        )
        .await
        .unwrap();
    assert!(
        service
            .write(
                &actor,
                None,
                "update-capability",
                Some(cap),
                "stale-update",
                updated.clone()
            )
            .await
            .is_err()
    );
    assert!(
        service
            .capability_decision(&credential, "old-config", input)
            .await
            .is_err()
    );
    let offers = service.capability_context(&credential).await.unwrap();
    assert_eq!(offers[0]["accepted"], false);
    assert_eq!(offers[0]["scope"]["configuration_revision"], 2);
    // Rotation cannot replay an agreement from the prior credential generation.
    sqlx::query("UPDATE execution_devices SET credential_generation=2 WHERE id=$1")
        .bind(device)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO device_credentials(token_hash,device_id,generation,expires_at) VALUES('cap-rotated',$1,2,clock_timestamp()+interval '1 hour')").bind(device).execute(&pool).await.unwrap();
    assert!(service.capability_context(&credential).await.is_err());
    let rotated = Credential {
        device_id: device,
        generation: 2,
        token_hash: "cap-rotated".into(),
    };
    let mut new_input:DecisionInput=serde_json::from_value(json!({"capability_id":cap,"configuration_revision":2,"scope_digest":offers[0]["scope_digest"],"expected_revision":2,"accepted":true})).unwrap();
    assert!(
        service
            .capability_decision(&rotated, "accept", new_input.clone())
            .await
            .is_err()
    );
    let current = service.capability_context(&rotated).await.unwrap();
    new_input.scope_digest = current[0]["scope_digest"].as_str().unwrap().into();
    service
        .capability_decision(&rotated, "accept", new_input)
        .await
        .unwrap();
    assert_eq!(
        service.capability_context(&rotated).await.unwrap()[0]["accepted"],
        true
    );
    assert!(
        sqlx::query(
            "UPDATE mobile_device_capability_versions SET name='changed' WHERE capability_id=$1"
        )
        .bind(cap)
        .execute(&pool)
        .await
        .is_err()
    );
    updated["expected_revision"] = json!(2);
    updated["enabled"] = json!(false);
    service
        .write(
            &actor,
            None,
            "update-capability",
            Some(cap),
            "disable",
            updated,
        )
        .await
        .unwrap();
    assert!(
        service
            .capability_context(&rotated)
            .await
            .unwrap()
            .is_empty()
    );
    let list = service
        .list(
            &actor,
            None,
            Resource::Capabilities,
            Page::default(),
            String::new(),
            Some(device),
        )
        .await
        .unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["execution_ready"], false);
    pool.close().await;
}
