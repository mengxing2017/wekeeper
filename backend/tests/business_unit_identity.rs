//! Integration test; requires an explicitly configured isolated database.
use serde_json::json;
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;
use wekeeper_api::{business::Actor, source_access::north::local::Service};

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn route_creates_distinct_unit_and_database_enforces_scope() {
    let url = std::env::var("WEKEEPER_TEST_DATABASE_URL")
        .expect("explicit isolated test database required");
    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    wekeeper_api::database::migrate(&pool).await.unwrap();
    let removed: bool = sqlx::query_scalar("SELECT to_regclass('routing_group_targets') IS NULL")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(removed);
    let actor_id = Uuid::new_v4();
    let platform = Uuid::new_v4();
    let other_platform = Uuid::new_v4();
    let key = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO accounts(id,username,display_name,platform_admin) VALUES($1,$2,'admin',true)",
    )
    .bind(actor_id)
    .bind(actor_id.to_string())
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO source_callers(source_id) VALUES($1),($2)")
        .bind(platform)
        .bind(other_platform)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO source_credentials(key_id,source_id,unit_name,encryption_version,status,valid_from,created_by) VALUES($1,$2,'platform','v1','active',now(),$3)")
        .bind(&key).bind(platform).bind(actor_id).execute(&pool).await.unwrap();
    let duplicate_credential = sqlx::query("INSERT INTO source_credentials(key_id,source_id,unit_name,encryption_version,status,valid_from,created_by) VALUES($1,$2,'duplicate','v1','active',now(),$3)")
        .bind(Uuid::new_v4().to_string()).bind(platform).bind(actor_id).execute(&pool).await.unwrap_err();
    assert_eq!(
        duplicate_credential
            .as_database_error()
            .unwrap()
            .code()
            .as_deref(),
        Some("23505")
    );
    let actor = Actor {
        id: actor_id,
        username: "admin".into(),
        display_name: "admin".into(),
        platform_admin: true,
        bound_school_id: None,
        grants: vec![],
        csrf: String::new(),
        session_hash: String::new(),
        request_id: Uuid::new_v4().to_string(),
        browser: false,
        wecom_authenticated: false,
        wecom_binding_revision: 0,
    };
    let service = Service(pool.clone());
    let created = service
        .manage(
            &actor,
            "source-routes",
            None,
            "POST",
            Some(
                serde_json::from_value(
                    json!({"key_id":key,"business_unit_name":"单位一","status":"active"}),
                )
                .unwrap(),
            ),
            1,
            "",
            "",
            None,
        )
        .await
        .unwrap();
    let route = created["id"].as_str().unwrap();
    let (unit, source, name): (Uuid, Uuid, String) = sqlx::query_as("SELECT u.id,u.platform_id,u.name FROM business_units u JOIN source_routes r ON r.business_unit_id=u.id WHERE r.route_id=$1")
        .bind(route).fetch_one(&pool).await.unwrap();
    assert_ne!(unit.to_string(), route);
    assert_eq!(source, platform);
    assert_eq!(name, "单位一");
    let directory = wekeeper_api::business_directory::north::local::UnitDirectory(
        wekeeper_api::business_directory::south::adapter::postgres::Repository(pool.clone()),
    );
    let entry = directory.find(platform, unit).await.unwrap().unwrap();
    assert_eq!(entry.id, unit);
    assert_eq!(entry.platform_id, platform);
    assert_eq!(entry.name, "单位一");
    assert!(entry.active);
    assert!(
        directory
            .find(other_platform, unit)
            .await
            .unwrap()
            .is_none()
    );
    let platforms = directory.platforms(&actor, 1).await.unwrap();
    assert!(
        platforms
            .items
            .iter()
            .any(|item| item.id == platform && item.name == "platform")
    );
    let units = directory.units(&actor, platform, 1).await.unwrap();
    assert!(
        units
            .items
            .iter()
            .any(|item| item.id == unit && item.name == "单位一")
    );
    assert!(
        directory
            .units(&actor, other_platform, 1)
            .await
            .unwrap()
            .items
            .is_empty()
    );
    let mut non_admin = actor.clone();
    non_admin.platform_admin = false;
    assert!(matches!(
        directory.platforms(&non_admin, 1).await,
        Err(wekeeper_api::business_directory::domain::DirectoryError::Forbidden)
    ));
    assert!(matches!(
        directory.units(&non_admin, platform, 1).await,
        Err(wekeeper_api::business_directory::domain::DirectoryError::Forbidden)
    ));
    assert!(matches!(
        directory.platforms(&actor, 0).await,
        Err(wekeeper_api::business_directory::domain::DirectoryError::InvalidPage)
    ));
    sqlx::query("INSERT INTO business_units(id,platform_id,name,active,created_by,updated_by) SELECT gen_random_uuid(),$1,'inactive fixture',false,$2,$2 FROM generate_series(1,31)")
        .bind(platform).bind(actor_id).execute(&pool).await.unwrap();
    let first_page = directory.units(&actor, platform, 1).await.unwrap();
    let second_page = directory.units(&actor, platform, 2).await.unwrap();
    assert_eq!(first_page.items.len(), 30);
    assert!(first_page.has_more);
    assert_eq!(second_page.items.len(), 2);
    assert!(!second_page.has_more);

    use axum::{
        body::{Body, to_bytes},
        http::Request,
    };
    use tower::ServiceExt;
    let app = wekeeper_api::bootstrap::build_application(pool.clone());
    let (admin_token, _) = app
        .identity
        .0
        .session(actor_id, "browser", false)
        .await
        .unwrap();
    let normal_id = Uuid::new_v4();
    sqlx::query("INSERT INTO accounts(id,username,display_name,platform_admin) VALUES($1,$2,'operator',false)")
        .bind(normal_id).bind(normal_id.to_string()).execute(&pool).await.unwrap();
    let (normal_token, _) = app
        .identity
        .0
        .session(normal_id, "browser", false)
        .await
        .unwrap();
    let router = wekeeper_api::http::app_router(app);
    let platform_response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v2/admin/business-platforms?page=1")
                .header("cookie", format!("wekeeper_session={admin_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(platform_response.status(), 200);
    let platform_body: serde_json::Value = serde_json::from_slice(
        &to_bytes(platform_response.into_body(), 100_000)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(platform_body["data"]["type"], "business-platforms");
    assert!(
        platform_body["data"]["attributes"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["id"] == platform.to_string() && item["name"] == "platform")
    );
    let path = format!("/api/v2/admin/business-platforms/{platform}/units?page=1");
    let request = |token: &str| {
        Request::builder()
            .uri(&path)
            .header("cookie", format!("wekeeper_session={token}"))
            .body(Body::empty())
            .unwrap()
    };
    let denied = router
        .clone()
        .oneshot(request(&normal_token))
        .await
        .unwrap();
    assert_eq!(denied.status(), 403);
    let allowed = router.clone().oneshot(request(&admin_token)).await.unwrap();
    assert_eq!(allowed.status(), 200);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(allowed.into_body(), 100_000).await.unwrap()).unwrap();
    assert_eq!(body["data"]["type"], "business-units");
    assert_eq!(body["data"]["attributes"]["has_more"], true);
    assert_eq!(
        body["data"]["attributes"]["items"]
            .as_array()
            .unwrap()
            .len(),
        30
    );
    let authorized: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM source_key_routes WHERE key_id=$1 AND route_id=$2 AND status='active')")
        .bind(&key).bind(route).fetch_one(&pool).await.unwrap();
    assert!(authorized);

    // Force the final authorization write to fail, then verify that neither
    // the unit nor the route from this request escaped the transaction.
    let suffix = Uuid::new_v4().simple().to_string();
    let trigger = format!("test_reject_unit_authorization_{suffix}");
    let function = format!("test_reject_unit_authorization_fn_{suffix}");
    sqlx::query(&format!("CREATE FUNCTION {function}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected authorization failure'; END $$"))
        .execute(&pool).await.unwrap();
    sqlx::query(&format!("CREATE TRIGGER {trigger} BEFORE INSERT ON source_key_routes FOR EACH ROW WHEN (NEW.key_id = '{key}') EXECUTE FUNCTION {function}()"))
        .execute(&pool).await.unwrap();
    let failed_name = format!("rollback-{suffix}");
    let failed = service
        .manage(
            &actor,
            "source-routes",
            None,
            "POST",
            Some(
                serde_json::from_value(
                    json!({"key_id":key,"business_unit_name":failed_name,"status":"active"}),
                )
                .unwrap(),
            ),
            1,
            "",
            "",
            None,
        )
        .await;
    assert!(failed.is_err());
    let remaining: i64 =
        sqlx::query_scalar("SELECT count(*) FROM business_units WHERE platform_id=$1 AND name=$2")
            .bind(platform)
            .bind(&failed_name)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(remaining, 0);
    sqlx::query(&format!("DROP TRIGGER {trigger} ON source_key_routes"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(&format!("DROP FUNCTION {function}()"))
        .execute(&pool)
        .await
        .unwrap();
    let duplicate = sqlx::query("INSERT INTO source_routes(route_id,source_id,business_unit_id,route_name,status) VALUES($1,$2,$3,'duplicate','active')")
        .bind(Uuid::new_v4().to_string()).bind(platform).bind(unit).execute(&pool).await;
    assert_eq!(
        duplicate
            .unwrap_err()
            .as_database_error()
            .unwrap()
            .code()
            .as_deref(),
        Some("23505")
    );
    let other_unit = Uuid::new_v4();
    sqlx::query("INSERT INTO business_units(id,platform_id,name,created_by,updated_by) VALUES($1,$2,'other',$3,$3)")
        .bind(other_unit).bind(other_platform).bind(actor_id).execute(&pool).await.unwrap();
    // This unit has no route, so the foreign key alone cannot protect its identity.
    let moved = sqlx::query("UPDATE business_units SET platform_id=$1 WHERE id=$2")
        .bind(platform)
        .bind(other_unit)
        .execute(&pool)
        .await
        .unwrap_err();
    assert_eq!(
        moved.as_database_error().unwrap().message(),
        "business unit identity immutable"
    );
    let renamed = sqlx::query("UPDATE business_units SET name='renamed' WHERE id=$1")
        .bind(other_unit)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(renamed.rows_affected(), 1);
    let wrong_platform = sqlx::query("INSERT INTO source_routes(route_id,source_id,business_unit_id,route_name,status) VALUES($1,$2,$3,'wrong','active')")
        .bind(Uuid::new_v4().to_string()).bind(platform).bind(other_unit).execute(&pool).await;
    assert_eq!(
        wrong_platform
            .unwrap_err()
            .as_database_error()
            .unwrap()
            .code()
            .as_deref(),
        Some("23503")
    );
    let missing = sqlx::query("INSERT INTO source_routes(route_id,source_id,route_name,status) VALUES($1,$2,'missing','active')")
        .bind(Uuid::new_v4().to_string()).bind(platform).execute(&pool).await;
    assert_eq!(
        missing
            .unwrap_err()
            .as_database_error()
            .unwrap()
            .code()
            .as_deref(),
        Some("23502")
    );
    let same_platform_unit = Uuid::new_v4();
    sqlx::query("INSERT INTO business_units(id,platform_id,name,created_by,updated_by) VALUES($1,$2,'another',$3,$3)")
        .bind(same_platform_unit).bind(platform).bind(actor_id).execute(&pool).await.unwrap();
    let receipt = Uuid::new_v4();
    sqlx::query("INSERT INTO source_receipts(receipt_id,source_id,business_unit_id,event_id,route_id,body_sha256,schema_version) VALUES($1,$2,$3,$4,$5,'hash',2)")
        .bind(receipt).bind(platform).bind(unit).bind(Uuid::new_v4()).bind(route).execute(&pool).await.unwrap();
    let forged = sqlx::query("INSERT INTO source_receipts(receipt_id,source_id,business_unit_id,event_id,route_id,body_sha256,schema_version) VALUES($1,$2,$3,$4,$5,'hash',2)")
        .bind(Uuid::new_v4()).bind(platform).bind(same_platform_unit).bind(Uuid::new_v4()).bind(route).execute(&pool).await.unwrap_err();
    assert_eq!(
        forged.as_database_error().unwrap().code().as_deref(),
        Some("23503")
    );
    let modified =
        sqlx::query("UPDATE source_receipts SET business_unit_id=$1 WHERE receipt_id=$2")
            .bind(same_platform_unit)
            .bind(receipt)
            .execute(&pool)
            .await
            .unwrap_err();
    assert_eq!(
        modified.as_database_error().unwrap().message(),
        "source receipt business identity immutable"
    );
    service
        .manage(
            &actor,
            "source-credentials",
            Some(&key),
            "DELETE",
            Some(serde_json::from_value(json!({})).unwrap()),
            1,
            "",
            "",
            None,
        )
        .await
        .unwrap();
    let active: bool = sqlx::query_scalar("SELECT active FROM business_units WHERE id=$1")
        .bind(unit)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(!active);
    let historical_entry = directory.find(platform, unit).await.unwrap().unwrap();
    assert!(!historical_entry.active);
    assert_eq!(historical_entry.id, unit);
    let frozen: Uuid =
        sqlx::query_scalar("SELECT business_unit_id FROM source_receipts WHERE receipt_id=$1")
            .bind(receipt)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(frozen, unit);
    pool.close().await;
}
