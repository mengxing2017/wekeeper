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
    let mut app = crate::bootstrap::build_application(pool.clone());
    app.integrations = service(pool.clone());
    for (origin, local_login) in [
        ("http://localhost:8080", true),
        ("https://example.test", false),
    ] {
        app.options.public_origin = origin.into();
        app.options.insecure_local_auth = local_login;
        let broadcast = app.broadcast();
        assert!(broadcast.management_available);
        assert_eq!(
            broadcast.enabled,
            std::env::var("WEKEEPER_WECOM_BROADCAST_ENABLED").is_ok_and(|v| v == "true")
        );
    }
    app.integrations.secret.clear();
    assert!(!app.broadcast().management_available);
    assert!(!app.broadcast().enabled);
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
        !s.capabilities("https://example.test", false)
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
        assert!(body.contains("逐群分享已停用"), "{path}: {body}");
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

#[tokio::test]
#[ignore = "requires isolated WEKEEPER_TEST_DATABASE_URL"]
async fn asset_http_reads_preserve_errors_and_do_not_need_agent_id() {
    use crate::integrations::{
        domain::directory::ReadError,
        south::{adapter::directory_http::Client, port::assets::AssetClient},
    };
    use axum::{Json, extract::State, routing::post};
    use serde_json::json;
    #[derive(Default)]
    struct Mock {
        token_error: i64,
        code: i64,
        calls: usize,
        missing_members: bool,
        unknown_type: bool,
    }
    async fn token(State(s): State<Arc<Mutex<Mock>>>) -> Json<Value> {
        let s = s.lock().unwrap();
        if s.token_error != 0 {
            Json(json!({"errcode":s.token_error,"errmsg":"must not be copied"}))
        } else {
            Json(json!({"access_token":"test-token","expires_in":7200}))
        }
    }
    async fn groups(State(s): State<Arc<Mutex<Mock>>>, Json(v): Json<Value>) -> Json<Value> {
        assert_eq!(v["status_filter"], 0);
        assert_eq!(v["limit"], 100);
        assert_eq!(v["owner_filter"]["userid_list"][0], "Alice");
        let mut s = s.lock().unwrap();
        s.calls += 1;
        if s.code != 0 {
            Json(json!({"errcode":s.code}))
        } else {
            Json(json!({"errcode":0,"group_chat_list":[],"next_cursor":""}))
        }
    }
    async fn group(State(s): State<Arc<Mutex<Mock>>>, Json(v): Json<Value>) -> Json<Value> {
        assert_eq!(v["need_name"], 0);
        let s = s.lock().unwrap();
        let mut g = json!({"chat_id":v["chat_id"],"owner":"Alice","member_list":[{"userid":"External","type":if s.unknown_type{3}else{2}}]});
        if s.missing_members {
            g.as_object_mut().unwrap().remove("member_list");
        }
        Json(json!({"errcode":0,"group_chat":g}))
    }
    let url = std::env::var("WEKEEPER_TEST_DATABASE_URL").expect("isolated database required");
    let root = sqlx::PgPool::connect(&url).await.unwrap();
    let schema = format!("assets_http_{}", uuid::Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE SCHEMA {schema}"))
        .execute(&root)
        .await
        .unwrap();
    let search = schema.clone();
    let pool = sqlx::postgres::PgPoolOptions::new()
        .after_connect(move |c, _| {
            let sql = format!("SET search_path TO {search}");
            Box::pin(async move {
                sqlx::query(&sql).execute(c).await?;
                Ok(())
            })
        })
        .connect(&url)
        .await
        .unwrap();
    crate::database::migrate(&pool).await.unwrap();
    let mock = Arc::new(Mutex::new(Mock::default()));
    let router = axum::Router::new()
        .route("/cgi-bin/gettoken", get(token))
        .route("/cgi-bin/user/get",get(|Query(q):Query<HashMap<String,String>>|async move {Json(json!({"errcode":0,"userid":q["userid"],"name":"企业成员","status":1,"department":[]}))}))
        .route("/cgi-bin/externalcontact/groupchat/list", post(groups))
        .route("/cgi-bin/externalcontact/groupchat/get", post(group))
        .with_state(mock.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut integration = service(pool.clone());
    integration.api_origin = Some(format!("http://{}", listener.local_addr().unwrap()));
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    integration.directory().ensure_config().await.unwrap();
    let client = Client(integration.clone());
    assert!(integration.agent.is_empty());
    assert!(
        client
            .groups(&["Alice".into()], "")
            .await
            .unwrap()
            .group_chat_list
            .is_empty()
    );
    assert_eq!(
        client.group("CHAT").await.unwrap().member_list[0].userid,
        "External"
    );
    mock.lock().unwrap().missing_members = true;
    assert!(matches!(
        client.group("CHAT").await,
        Err(ReadError::Invalid)
    ));
    mock.lock().unwrap().missing_members = false;
    mock.lock().unwrap().unknown_type = true;
    assert!(matches!(
        client.group("CHAT").await,
        Err(ReadError::Invalid)
    ));
    mock.lock().unwrap().code = 60020;
    assert!(matches!(
        client.groups(&["Alice".into()], "").await,
        Err(ReadError::Rejected(60020))
    ));
    mock.lock().unwrap().code = 45009;
    mock.lock().unwrap().calls = 0;
    assert!(matches!(
        client.groups(&["Alice".into()], "").await,
        Err(ReadError::Transient)
    ));
    assert_eq!(mock.lock().unwrap().calls, 3);
    integration
        .repository
        .expire_token(&integration.cache_key("token"), "stale-token")
        .await
        .unwrap();
    assert_eq!(integration.token().await.unwrap(), "test-token");
    integration
        .repository
        .expire_token(&integration.cache_key("token"), "test-token")
        .await
        .unwrap();
    mock.lock().unwrap().token_error = 40001;
    assert!(matches!(
        client.groups(&["Alice".into()], "").await,
        Err(ReadError::Rejected(40001))
    ));
    // Actual API authorization stays outside all enterprise-wide asset reads.
    let mut app = crate::bootstrap::build_application(pool.clone());
    app.integrations = integration;
    let aid = app
        .identity
        .bootstrap("admin", "admin", "test-password-123", true)
        .await
        .unwrap();
    let (admin_token, _) = app.identity.0.session(aid, "browser", false).await.unwrap();
    let uid = app
        .identity
        .bootstrap("user", "user", "test-password-123", false)
        .await
        .unwrap();
    let (_user_token, _) = app.identity.0.session(uid, "browser", false).await.unwrap();
    // Bind an account to a verified member; local nickname never grants ownership.
    let member_id = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO wecom_members(id,userid,name,status,verification,checked_at) VALUES($1,'Alice','企业成员',1,'verified',now())").bind(member_id).execute(&pool).await.unwrap();
    sqlx::query("UPDATE accounts SET wecom_member_id=$2,wecom_binding_revision=1 WHERE id=$1")
        .bind(uid)
        .bind(member_id)
        .execute(&pool)
        .await
        .unwrap();
    let (_, admin_actor) = app.identity.0.session(aid, "browser", false).await.unwrap();
    let school = app.schools.create(&admin_actor, "School").await.unwrap();
    let school: uuid::Uuid = school["id"].as_str().unwrap().parse().unwrap();
    app.schools
        .grant(&admin_actor, school, uid, "admin")
        .await
        .unwrap();
    let (bound_token, bound_actor) = app.identity.0.session(uid, "browser", false).await.unwrap();
    mock.lock().unwrap().token_error = 0;
    mock.lock().unwrap().code = 0;
    mock.lock().unwrap().unknown_type = false;
    let official = client.group("BOUND").await.unwrap();
    app.integrations
        .assets()
        .store
        .observe(1, &official, chrono::Utc::now())
        .await
        .unwrap();
    let router = crate::http::app_router(app.clone());
    let path = format!("/api/v1/schools/{school}/wecom-group-candidates");
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri(&path)
                .header("cookie", format!("wekeeper_session={bound_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let candidates: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 16384).await.unwrap()).unwrap();
    assert_eq!(candidates["data"].as_array().unwrap().len(), 1);
    assert!(
        candidates["data"][0]["attributes"]
            .get("member_list")
            .is_none()
    );
    let selected = &candidates["data"][0];
    let input = crate::integrations::domain::assets::GroupBinding {
        directory_group_id: selected["id"].as_str().unwrap().parse().unwrap(),
        expected_generation: 1,
        expected_checked_at: serde_json::from_value(selected["attributes"]["checked_at"].clone())
            .unwrap(),
        name: None,
    };
    app.integrations
        .assets()
        .bind(&bound_actor, school, input)
        .await
        .unwrap();
    for role in ["operator", "member"] {
        app.schools
            .grant(&admin_actor, school, uid, role)
            .await
            .unwrap();
        let (role_token, _) = app.identity.0.session(uid, "browser", false).await.unwrap();
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri(&path)
                    .header("cookie", format!("wekeeper_session={role_token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 403);
    }
    let (user_token, _) = app.identity.0.session(uid, "browser", false).await.unwrap();
    for (path, token, expected) in [
        ("/api/v1/platform/wecom-assets/status", &admin_token, 200),
        ("/api/v1/platform/wecom-assets/customers", &admin_token, 404),
        (
            "/api/v1/platform/wecom-assets/contact-members",
            &admin_token,
            404,
        ),
        ("/api/v1/platform/wecom-assets/groups", &user_token, 403),
        (
            "/api/v1/platform/wecom-assets/groups?corp_id=forged",
            &admin_token,
            400,
        ),
        (
            "/api/v1/platform/wecom-assets/member-statistics?associated=true",
            &admin_token,
            422,
        ),
    ] {
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri(path)
                    .header("cookie", format!("wekeeper_session={token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), expected, "{path}");
    }
    server.abort();
    pool.close().await;
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&root)
        .await
        .unwrap();
    root.close().await;
}

#[tokio::test]
#[ignore = "requires isolated WEKEEPER_TEST_DATABASE_URL"]
async fn broadcast_http_preserves_explicit_targets_and_never_retries_writes() {
    use crate::distribution::{
        domain::broadcast::{Outcome, Work},
        south::port::broadcast::Client,
    };
    use axum::routing::post;
    let url = std::env::var("WEKEEPER_TEST_DATABASE_URL").unwrap();
    let base = sqlx::PgPool::connect(&url).await.unwrap();
    let schema = format!("broadcast_http_{}", uuid::Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE SCHEMA {schema}"))
        .execute(&base)
        .await
        .unwrap();
    let pool = sqlx::postgres::PgPoolOptions::new()
        .after_connect(move |c, _| {
            let sql = format!("SET search_path TO {schema}");
            Box::pin(async move {
                sqlx::query(&sql).execute(c).await?;
                Ok(())
            })
        })
        .connect(&url)
        .await
        .unwrap();
    crate::database::migrate(&pool).await.unwrap();
    let writes = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
    let seen = writes.clone();
    let router=axum::Router::new()
      .route("/cgi-bin/gettoken",get(||async{axum::Json(json!({"errcode":0,"access_token":"mock-token","expires_in":7200}))}))
      .route("/cgi-bin/externalcontact/add_msg_template",post(move|axum::Json(v):axum::Json<serde_json::Value>|{seen.lock().unwrap().push(v);async{(axum::http::StatusCode::BAD_GATEWAY,"lost response")}}))
      .route("/cgi-bin/externalcontact/get_groupmsg_task",post(||async{axum::Json(json!({"errcode":0,"next_cursor":"","task_list":[{"userid":"owner","status":2}]}))}))
      .route("/cgi-bin/externalcontact/get_groupmsg_send_result",post(|axum::Json(v):axum::Json<serde_json::Value>|async move{
          axum::Json(if v["cursor"]=="second" {json!({"errcode":0,"next_cursor":"","send_list":[{"userid":"owner","chat_id":"chat-b","status":3}]})} else {json!({"errcode":0,"next_cursor":"second","send_list":[{"userid":"owner","chat_id":"chat-a","status":1,"send_time":1700000000}]})})
      }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let mut integration = service(pool);
    integration.api_origin = Some(origin);
    integration.directory().ensure_config().await.unwrap();
    let corp = integration.corp.clone();
    let client = crate::integrations::south::adapter::broadcast_http::Client(integration);
    let id = uuid::Uuid::new_v4();
    let mut w = Work {
        id,
        school_id: id,
        task_id: id,
        sender: "owner".into(),
        sender_account: id,
        content: "Frozen text".into(),
        corp_id: corp,
        generation: 1,
        chats: vec!["chat-a".into(), "chat-b".into()],
        msgid: None,
        operation: "create".into(),
        operation_id: None,
    };
    assert!(matches!(client.invoke(&w).await, Outcome::Unknown));
    assert_eq!(writes.lock().unwrap().len(), 1);
    assert_eq!(
        writes.lock().unwrap()[0],
        json!({"chat_type":"group","sender":"owner","chat_id_list":["chat-a","chat-b"],"text":{"content":"Frozen text"}})
    );
    w.msgid = Some("msgid".into());
    w.operation = "read".into();
    let result = client.results(&w).await.unwrap();
    assert_eq!(result.member_status, Some(2));
    assert_eq!(result.groups.len(), 2);
    assert_eq!(result.groups[0].status, 1);
    assert_eq!(result.groups[1].status, 3);
    server.abort();
}
