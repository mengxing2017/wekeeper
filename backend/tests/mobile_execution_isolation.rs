use serde_json::{Value, json};
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
async fn targets_work_without_official_config_and_bind_atomically() {
    let (pool, a, school, device) = setup().await;
    let service = Service(Repository(pool.clone()));
    let input = json!({"search_text":"手机备注  群🦦","device_ids":[device]});
    assert!(
        service
            .write(
                &a,
                Some(school),
                "target",
                None,
                "before-grant",
                input.clone()
            )
            .await
            .is_err()
    );
    let targets = service
        .list(
            &a,
            Some(school),
            Resource::Targets,
            Page::default(),
            String::new(),
            None,
        )
        .await
        .unwrap();
    assert!(
        targets.is_empty(),
        "failed bind must roll back target insert"
    );
    let grant = service
        .write(
            &a,
            None,
            "grant",
            None,
            "grant",
            json!({"school_id":school,"device_id":device,"purpose":"校园通知"}),
        )
        .await
        .unwrap();
    let target = service
        .write(&a, Some(school), "target", None, "create", input.clone())
        .await
        .unwrap();
    assert_eq!(target["search_text"], input["search_text"]);
    assert_eq!(
        service
            .write(&a, Some(school), "target", None, "create", input)
            .await
            .unwrap(),
        target
    );
    assert!(
        service
            .write(
                &a,
                Some(school),
                "target",
                None,
                "create",
                json!({"search_text":"different"})
            )
            .await
            .is_err()
    );
    let targets = service
        .list(
            &a,
            Some(school),
            Resource::Targets,
            Page::default(),
            String::new(),
            Some(device),
        )
        .await
        .unwrap();
    assert_eq!(targets.len(), 1);
    let grant_id = grant["id"].as_str().unwrap().parse().unwrap();
    service
        .write(
            &a,
            None,
            "revoke-grant",
            Some(grant_id),
            "revoke",
            json!({"expected_revision":1,"reason":"停用"}),
        )
        .await
        .unwrap();
    assert!(
        service
            .list(
                &a,
                Some(school),
                Resource::Targets,
                Page::default(),
                String::new(),
                Some(device)
            )
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM broadcast_tasks")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM wecom_groups")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
}
#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn platform_role_does_not_replace_school_authority_and_duplicate_text_needs_confirmation() {
    let (pool, a, school, _) = setup().await;
    let service = Service(Repository(pool.clone()));
    service
        .write(
            &a,
            Some(school),
            "target",
            None,
            "one",
            json!({"search_text":"群"}),
        )
        .await
        .unwrap();
    let second = service
        .write(
            &a,
            Some(school),
            "target",
            None,
            "two",
            json!({"search_text":"群"}),
        )
        .await
        .unwrap();
    assert_eq!(second["confirmation_required"], true);
    assert_eq!(
        service
            .write(
                &a,
                Some(school),
                "target",
                None,
                "two",
                json!({"search_text":"群"})
            )
            .await
            .unwrap(),
        second
    );
    service
        .write(
            &a,
            Some(school),
            "target",
            None,
            "two-confirmed",
            json!({"search_text":"群","duplicate_confirmation":second["duplicate_confirmation"]}),
        )
        .await
        .unwrap();
    assert_eq!(
        service
            .list(
                &a,
                Some(school),
                Resource::Targets,
                Page::default(),
                String::new(),
                None
            )
            .await
            .unwrap()
            .len(),
        2
    );
    sqlx::query("DELETE FROM school_memberships WHERE account_id=$1")
        .bind(a.id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        service
            .write(
                &a,
                Some(school),
                "target",
                None,
                "forbidden",
                json!({"search_text":"其他"})
            )
            .await
            .is_err()
    );
    assert!(
        service
            .list(
                &a,
                Some(school),
                Resource::Targets,
                Page::default(),
                String::new(),
                None
            )
            .await
            .is_err()
    );
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn device_consent_requires_live_device_and_exact_current_school_scope() {
    use wekeeper_api::distribution::domain::{
        device_control::Credential, mobile_execution::ConsentInput,
    };
    let (pool, a, school, device) = setup().await;
    let service = Service(Repository(pool.clone()));
    let grant = service
        .write(
            &a,
            None,
            "grant",
            None,
            "grant",
            json!({"school_id":school,"device_id":device,"purpose":"通知"}),
        )
        .await
        .unwrap();
    let token_hash = wekeeper_api::business::hash("wk_device_test_consent");
    sqlx::query("INSERT INTO device_credentials(token_hash,device_id,generation,expires_at) VALUES($1,$2,0,clock_timestamp()+interval '1 hour')").bind(&token_hash).bind(device).execute(&pool).await.unwrap();
    let c = Credential {
        device_id: device,
        generation: 0,
        token_hash,
    };
    let context = service.consent_context(&c).await.unwrap();
    assert_eq!(context.len(), 1);
    assert_eq!(context[0]["accepted"], false);
    let input = ConsentInput {
        grant_id: grant["id"].as_str().unwrap().parse().unwrap(),
        grant_revision: 1,
        scope_digest: context[0]["scope_digest"].as_str().unwrap().into(),
        expected_revision: 0,
        accepted: true,
    };
    let accepted = service.consent(&c, "accept", input.clone()).await.unwrap();
    assert_eq!(accepted["revision"], 1);
    assert_eq!(
        service.consent(&c, "accept", input.clone()).await.unwrap(),
        accepted
    );
    assert_eq!(
        service.consent_context(&c).await.unwrap()[0]["accepted"],
        true
    );
    let mut revoke = input.clone();
    revoke.accepted = false;
    revoke.expected_revision = 1;
    assert!(service.consent(&c, "accept", revoke.clone()).await.is_err());
    service.consent(&c, "revoke", revoke).await.unwrap();
    assert_eq!(
        service.consent_context(&c).await.unwrap()[0]["accepted"],
        false
    );
    service
        .write(
            &a,
            None,
            "grant",
            None,
            "new-purpose",
            json!({"school_id":school,"device_id":device,"purpose":"变更用途"}),
        )
        .await
        .unwrap();
    assert!(service.consent(&c, "stale", input).await.is_err());
    let changed = service.consent_context(&c).await.unwrap();
    assert_eq!(changed[0]["accepted"], false);
    assert_eq!(changed[0]["scope"]["grant_revision"], 2);
    let mut forged = c.clone();
    forged.device_id = Uuid::new_v4();
    assert!(service.consent_context(&forged).await.is_err());
    sqlx::query("UPDATE device_credentials SET revoked_at=clock_timestamp() WHERE token_hash=$1")
        .bind(&c.token_hash)
        .execute(&pool)
        .await
        .unwrap();
    assert!(service.consent_context(&c).await.is_err());
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn flow_release_and_assignment_require_current_device_validation_without_official_configuration()
 {
    use wekeeper_api::distribution::domain::{
        device_control::Credential,
        mobile_execution::{ConsentInput, FLOW_CHECKS, ValidationResultInput},
    };
    let (pool, a, school, device) = setup().await;
    let service = Service(Repository(pool.clone()));
    service
        .write(
            &a,
            None,
            "grant",
            None,
            "flow-grant",
            json!({"school_id":school,"device_id":device,"purpose":"通知"}),
        )
        .await
        .unwrap();
    let flow: Value = serde_json::from_str(include_str!(
        "../../api-contract/fixtures/mobile-execution-flow-v2.json"
    ))
    .unwrap();
    let version = service
        .write(
            &a,
            Some(school),
            "flow",
            None,
            "register",
            json!({"name":"独立手机流程","flow":flow}),
        )
        .await
        .unwrap();
    let fid: Uuid = version["id"].as_str().unwrap().parse().unwrap();
    assert!(
        service
            .write(
                &a,
                Some(school),
                "validation",
                None,
                "without-consent",
                json!({"device_id":device,"flow_version_id":fid,"search_text":"现场验证群"})
            )
            .await
            .is_err()
    );
    let token_hash = wekeeper_api::business::hash("wk_device_flow_test");
    sqlx::query("INSERT INTO device_credentials(token_hash,device_id,generation,expires_at) VALUES($1,$2,0,clock_timestamp()+interval '1 hour')").bind(&token_hash).bind(device).execute(&pool).await.unwrap();
    let c = Credential {
        device_id: device,
        generation: 0,
        token_hash,
    };
    let context = service.consent_context(&c).await.unwrap();
    service
        .consent(
            &c,
            "flow-consent",
            ConsentInput {
                grant_id: context[0]["id"].as_str().unwrap().parse().unwrap(),
                grant_revision: 1,
                scope_digest: context[0]["scope_digest"].as_str().unwrap().into(),
                expected_revision: 0,
                accepted: true,
            },
        )
        .await
        .unwrap();
    let validation = service
        .write(
            &a,
            Some(school),
            "validation",
            None,
            "validate",
            json!({"device_id":device,"flow_version_id":fid,"search_text":"现场验证群"}),
        )
        .await
        .unwrap();
    assert!(validation.get("challenge").is_none());
    let vid: Uuid = validation["id"].as_str().unwrap().parse().unwrap();
    assert!(
        service
            .write(
                &a,
                Some(school),
                "release",
                None,
                "before-pass",
                json!({"flow_version_id":fid,"validation_id":vid,"name":"发布"})
            )
            .await
            .is_err()
    );
    let jobs = service.validations(&c).await.unwrap();
    assert_eq!(jobs.len(), 1);
    let result = ValidationResultInput {
        challenge: jobs[0]["challenge"].as_str().unwrap().into(),
        flow_digest: version["content_digest"].as_str().unwrap().into(),
        protocol_version: 6,
        engine_version: "otter-ui/2".into(),
        package_name: "com.tencent.wework".into(),
        version_code: 100,
        checks: FLOW_CHECKS.iter().map(|v| (v.to_string(), true)).collect(),
        evidence: "仅观察首群和发送控件，未点击发送".into(),
    };
    let passed = service
        .validation_result(&c, vid, "result", result.clone())
        .await
        .unwrap();
    assert_eq!(passed["state"], "passed");
    assert!(passed["result"].get("challenge").is_none());
    assert_eq!(
        passed,
        service
            .validation_result(&c, vid, "result", result)
            .await
            .unwrap()
    );
    let release = service
        .write(
            &a,
            Some(school),
            "release",
            None,
            "release",
            json!({"flow_version_id":fid,"validation_id":vid,"name":"已验证发布"}),
        )
        .await
        .unwrap();
    let rid: Uuid = release["id"].as_str().unwrap().parse().unwrap();
    let assignment = service
        .write(
            &a,
            Some(school),
            "assignment",
            None,
            "assign",
            json!({"device_id":device,"release_id":rid,"expected_revision":0,"enabled":true}),
        )
        .await
        .unwrap();
    let aid: Uuid = assignment["id"].as_str().unwrap().parse().unwrap();
    let list = service
        .list(
            &a,
            Some(school),
            Resource::Assignments,
            Page::default(),
            String::new(),
            None,
        )
        .await
        .unwrap();
    assert_eq!(list[0]["available"], true);
    assert!(
        service
            .write(
                &a,
                Some(school),
                "update-assignment",
                Some(aid),
                "stale",
                json!({"device_id":device,"release_id":rid,"expected_revision":0,"enabled":false})
            )
            .await
            .is_err()
    );
    service
        .write(
            &a,
            Some(school),
            "revoke-release",
            Some(rid),
            "revoke-flow",
            json!({"expected_revision":1,"reason":"停用发布"}),
        )
        .await
        .unwrap();
    let list = service
        .list(
            &a,
            Some(school),
            Resource::Assignments,
            Page::default(),
            String::new(),
            None,
        )
        .await
        .unwrap();
    assert_eq!(list[0]["available"], false);
    // An invalidated release must still allow stopping its configuration.
    service
        .write(
            &a,
            Some(school),
            "update-assignment",
            Some(aid),
            "stop-assignment",
            json!({"device_id":device,"release_id":rid,"expected_revision":1,"enabled":false}),
        )
        .await
        .unwrap();
    assert!(
        service
            .write(
                &a,
                Some(school),
                "update-assignment",
                Some(aid),
                "enable-revoked",
                json!({"device_id":device,"release_id":rid,"expected_revision":2,"enabled":true})
            )
            .await
            .is_err()
    );
}
