use super::*;
use crate::integrations::{domain::directory::BindingInput, south::port::DirectoryStore};
use axum::{
    body::{Body, to_bytes},
    extract::Query,
    http::Request,
    routing::get,
};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tower::ServiceExt;

fn service(pool: sqlx::PgPool) -> Service {
    let mut service = Service::new(pool);
    service.corp = "test-corp".into();
    service.secret = "test-secret".into();
    service.agent.clear();
    service
}
#[tokio::test]
async fn capabilities_and_cache_identity_are_independent_of_agent_id() {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgres://unused@127.0.0.1:1/unused")
        .unwrap();
    let mut s = service(pool);
    let key = s.cache_key("token");
    for agent in ["", " ", "0", "-1", "+1", "abc", "4294967296"] {
        s.agent = agent.into();
        assert!(s.server_configured());
        assert!(!s.client_configured());
        let status = s.capabilities("https://example.test", false);
        assert!(status.oauth_available);
        assert!(!status.share_available);
        assert_eq!(s.cache_key("token"), key);
    }
    s.agent = "1000002".into();
    assert!(
        s.capabilities("https://example.test", false)
            .share_available
    );
    assert_eq!(s.cache_key("token"), key);
    for (origin, insecure) in [
        ("http://example.test", false),
        ("https://example.test", true),
        ("invalid", false),
    ] {
        let status = s.capabilities(origin, insecure);
        assert!(status.server_configured && status.client_configured);
        assert!(!status.oauth_available && !status.share_available);
    }
    assert_ne!(
        s.cache_key("ticket:enterprise"),
        s.cache_key("ticket:application")
    );
    assert!(!key.contains("test-secret"));
    s.secret = "changed-secret".into();
    assert_ne!(s.cache_key("token"), key);
    s.corp = "ab".into();
    s.secret = "c".into();
    let unambiguous = s.credential_scope();
    s.corp = "a".into();
    s.secret = "bc".into();
    assert_ne!(s.credential_scope(), unambiguous);
    for (corp, secret) in [(" ", "secret"), ("corp", " ")] {
        s.corp = corp.into();
        s.secret = secret.into();
        assert!(!s.server_configured() && !s.client_configured());
        assert!(
            !s.capabilities("https://example.test", false)
                .oauth_available
        );
        // A missing credential must fail before touching the lazy database or network.
        assert!(s.token().await.is_err());
    }
}

#[tokio::test]
#[ignore = "requires isolated WEKEEPER_TEST_DATABASE_URL"]
async fn server_only_oauth_directory_cache_and_share_http_gates() {
    let url = std::env::var("WEKEEPER_TEST_DATABASE_URL").expect("isolated test DB required");
    let admin_pool = sqlx::PgPool::connect(&url).await.unwrap();
    let schema = format!("credentials_{}", uuid::Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE SCHEMA {schema}"))
        .execute(&admin_pool)
        .await
        .unwrap();
    let search = schema.clone();
    let pool = sqlx::postgres::PgPoolOptions::new()
        .after_connect(move |conn, _| {
            let sql = format!("SET search_path TO {search}");
            Box::pin(async move {
                sqlx::query(&sql).execute(conn).await?;
                Ok(())
            })
        })
        .connect(&url)
        .await
        .unwrap();
    crate::database::migrate(&pool).await.unwrap();
    let requests = Arc::new(Mutex::new(Vec::<HashMap<String, String>>::new()));
    let seen = requests.clone();
    let upstream=axum::Router::new()
        .route("/cgi-bin/gettoken",get(move |Query(q):Query<HashMap<String,String>>| {
            seen.lock().unwrap().push(q);
            async { axum::Json(json!({"access_token":"fake-token","expires_in":7200})) }
        }))
        .route("/cgi-bin/department/simplelist",get(||async{axum::Json(json!({"department_id":[{"id":1}]}))}))
        .route("/cgi-bin/department/get",get(||async{axum::Json(json!({"department":{"id":1,"parentid":0,"name":"部门"}}))}))
        .route("/cgi-bin/user/list",get(||async{axum::Json(json!({"userlist":[{"userid":"user.one","name":"成员","status":1,"department":[1]}]}))}))
        .route("/cgi-bin/user/get",get(||async{axum::Json(json!({"userid":"user.one","name":"成员","status":1,"department":[1]}))}))
        .route("/cgi-bin/auth/getuserinfo",get(||async{axum::Json(json!({"userid":"user.one"}))}));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let upstream = tokio::spawn(async move { axum::serve(listener, upstream).await.unwrap() });
    let mut app = crate::bootstrap::build_application(pool.clone());
    app.options.public_origin = "https://example.test".into();
    app.options.insecure_local_auth = false;
    app.integrations = service(pool.clone());
    app.integrations.api_origin = Some(origin);
    let admin_id = app
        .identity
        .bootstrap("admin", "Admin", "test-password-123", true)
        .await
        .unwrap();
    let (_, admin) = app
        .identity
        .0
        .session(admin_id, "browser", false)
        .await
        .unwrap();
    let user = app
        .identity
        .bootstrap("user", "User", "test-password-123", false)
        .await
        .unwrap();
    let directory = app.integrations.directory();
    directory.enqueue(&admin).await.unwrap();
    directory.run_once(0).await.unwrap();
    let member: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM wecom_members WHERE userid='user.one'")
            .fetch_one(&pool)
            .await
            .unwrap();
    directory
        .bind(
            &admin,
            user,
            BindingInput {
                member_id: member,
                revision: 0,
            },
        )
        .await
        .unwrap();
    // AgentID changes neither the cache nor directory generation.
    let mut with_agent = app.integrations.clone();
    with_agent.agent = "1000002".into();
    assert_eq!(with_agent.directory().ensure_config().await.unwrap(), 1);
    assert_eq!(with_agent.token().await.unwrap(), "fake-token");
    assert_eq!(requests.lock().unwrap().len(), 1);
    assert_eq!(
        requests.lock().unwrap()[0],
        HashMap::from([
            ("corpid".into(), "test-corp".into()),
            ("corpsecret".into(), "test-secret".into())
        ])
    );
    let repo = &app.integrations.repository;
    let key = app.integrations.cache_key("token");
    repo.expire_token(&key, "obsolete-token").await.unwrap();
    assert_eq!(app.integrations.token().await.unwrap(), "fake-token");
    assert_eq!(requests.lock().unwrap().len(), 1);
    repo.expire_token(&key, "fake-token").await.unwrap();
    app.integrations.token().await.unwrap();
    assert_eq!(requests.lock().unwrap().len(), 2);
    let mut rotated = app.integrations.clone();
    rotated.secret = "new-secret".into();
    rotated.token().await.unwrap();
    assert_eq!(requests.lock().unwrap().len(), 3);
    assert!(rotated.directory().ensure_config().await.is_err());
    // Real callback route establishes a bound enterprise session without AgentID.
    let state = repo.oauth_state("/wecom").await.unwrap();
    let router = crate::http::app_router(app.clone());
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/v1/wecom/oauth/callback?code=fake&state={state}"
                ))
                .header("cookie", format!("wekeeper_oauth={state}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 303);
    assert_eq!(response.headers()["location"], "/wecom");
    let cookie = response
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find(|v| v.starts_with("wekeeper_session="))
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let token = cookie.strip_prefix("wekeeper_session=").unwrap();
    let actor = app.identity.authenticate(token, "browser").await.unwrap();
    assert!(actor.wecom_authenticated);
    for (method, path) in [
        ("GET", "/api/v1/wecom/config".to_owned()),
        (
            "GET",
            "/api/v1/wecom/js-sdk-config?url=https%3A%2F%2Fexample.test%2Fwecom&agent=true"
                .to_owned(),
        ),
        (
            "POST",
            format!(
                "/api/v1/schools/{}/delivery-drafts/{}/attempts",
                uuid::Uuid::nil(),
                uuid::Uuid::nil()
            ),
        ),
        (
            "POST",
            format!(
                "/api/v1/schools/{}/delivery-attempts/{}/invocation",
                uuid::Uuid::nil(),
                uuid::Uuid::nil()
            ),
        ),
    ] {
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(&path)
                    .header("cookie", &cookie)
                    .header("origin", "https://example.test")
                    .header("x-csrf-token", &actor.csrf)
                    .header("content-type", "application/vnd.api+json")
                    .body(Body::from(
                        json!({"data":{"type":"delivery-attempts","attributes":{}}}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 422, "{path}");
        let body = String::from_utf8(to_bytes(response.into_body(), 8192).await.unwrap().to_vec())
            .unwrap();
        assert!(body.contains("AgentID"), "{path}: {body}");
    }
    // The new scope never silently adopts the old directory.
    assert_eq!(
        directory
            .store
            .configure("test-corp", &app.integrations.credential_scope())
            .await
            .unwrap(),
        1
    );
    upstream.abort();
    pool.close().await;
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&admin_pool)
        .await
        .unwrap();
    admin_pool.close().await;
}
