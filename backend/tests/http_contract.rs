use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use serde_json::Value;
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;
use wekeeper_api::{bootstrap, http};

fn app() -> axum::Router {
    // Lazy connection: public metadata/liveness must not require a running database.
    let pool = PgPoolOptions::new()
        .connect_lazy("postgres://unused:unused@127.0.0.1:1/unused")
        .unwrap();
    http::app_router(bootstrap::build_application(pool))
}

#[tokio::test]
async fn public_contract_matches_shared_fixture() {
    let response = app()
        .oneshot(
            Request::builder()
                .uri("/api/v1/system")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.headers().contains_key("x-request-id"));
    let actual: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap()).unwrap();
    let fixture: Value =
        serde_json::from_str(include_str!("../../api-contract/fixtures/system.json")).unwrap();
    assert_eq!(actual, fixture);
}

#[tokio::test]
async fn unknown_business_routes_fail_closed() {
    let response = app()
        .oneshot(
            Request::builder()
                .uri("/api/v1/posts")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        response.headers()["content-type"],
        cairnkit_jsonapi::api::JSON_API_MEDIA_TYPE
    );
    assert!(response.headers().contains_key("x-request-id"));
    let actual: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap()).unwrap();
    let fixture: Value =
        serde_json::from_str(include_str!("../../api-contract/fixtures/not-found.json")).unwrap();
    assert_eq!(actual, fixture);
    let doc: cairnkit_jsonapi::api::JsonApiDocument = serde_json::from_value(actual).unwrap();
    assert!(doc.is_valid());
}

#[tokio::test]
async fn liveness_does_not_use_database() {
    let response = app()
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn readiness_fails_when_database_is_unavailable() {
    let response = app()
        .oneshot(
            Request::builder()
                .uri("/readyz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn wecom_status_is_public_and_contains_no_credentials() {
    let pool = PgPoolOptions::new()
        .connect_lazy("postgres://unused:unused@127.0.0.1:1/unused")
        .unwrap();
    let mut application = bootstrap::build_application(pool);
    application.integrations.corp.clear();
    let response = http::app_router(application)
        .oneshot(
            Request::builder()
                .uri("/api/v1/wecom/status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let actual: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap()).unwrap();
    let fixture: Value = serde_json::from_str(include_str!(
        "../../api-contract/fixtures/wecom-status.json"
    ))
    .unwrap();
    assert_eq!(actual, fixture);
}

#[tokio::test]
async fn oauth_missing_cookie_returns_to_the_frontend_error_state() {
    let response = app()
        .oneshot(
            Request::builder()
                .uri("/api/v1/wecom/oauth/callback?code=invalid&state=invalid")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers()["location"], "/wecom?oauth_error=failed");
}

#[tokio::test]
async fn delivery_history_serializes_the_shared_contract() {
    use wekeeper_api::distribution::domain::RelatedHistory;
    let fixture: Value = serde_json::from_str(include_str!(
        "../../api-contract/fixtures/delivery-history.json"
    ))
    .unwrap();
    let history = RelatedHistory {
        items: vec![
            serde_json::json!({"id":"00000000-0000-0000-0000-000000000001","school_id":"00000000-0000-0000-0000-000000000002","draft_id":"00000000-0000-0000-0000-000000000003","actor_id":"00000000-0000-0000-0000-000000000004","state":"unknown","reconciliation":null,"client_result":null,"actor_name":"运营甲","group_name":"学校客户群","content":"待核对内容","created_at":"2026-10-21T00:00:00Z"}),
        ],
        latest_attempt_id: Some(uuid::Uuid::from_u128(1)),
        blocked: true,
    };
    use axum::response::IntoResponse;
    let response = http::jsonapi::collection("delivery-attempts", history.items, &Default::default())
        .meta(serde_json::json!({"blocked":history.blocked,"latest_attempt_id":history.latest_attempt_id})).into_response();
    assert_eq!(
        response.headers()["content-type"],
        cairnkit_jsonapi::api::JSON_API_MEDIA_TYPE
    );
    let actual: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 8192).await.unwrap()).unwrap();
    assert_eq!(actual, fixture);
}

#[tokio::test]
async fn errors_use_cairnkit_documents_for_business_and_axum_rejections() {
    for (method, path, content_type, body, status, code) in [
        (
            "GET",
            "/api/v1/auth/session",
            "application/vnd.api+json",
            "",
            401,
            "unauthorized",
        ),
        (
            "POST",
            "/api/v1/auth/sessions",
            "application/vnd.api+json",
            "{secret-password",
            400,
            "invalid_request",
        ),
        (
            "POST",
            "/api/v1/auth/sessions",
            "application/vnd.api+json",
            "{}",
            422,
            "validation",
        ),
        (
            "POST",
            "/api/v1/auth/sessions",
            "text/plain",
            "{}",
            415,
            "unsupported_media_type",
        ),
        (
            "GET",
            "/api/v1/wecom/oauth/start?school=invalid",
            "application/vnd.api+json",
            "",
            400,
            "invalid_request",
        ),
        (
            "POST",
            "/api/v1/system",
            "application/vnd.api+json",
            "{}",
            405,
            "method_not_allowed",
        ),
    ] {
        let response = app()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .header("content-type", content_type)
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), status, "{method} {path}");
        assert_eq!(
            response.headers()["content-type"],
            cairnkit_jsonapi::api::JSON_API_MEDIA_TYPE
        );
        assert!(response.headers().contains_key("x-request-id"));
        if status == 405 {
            assert!(
                response.headers()["allow"]
                    .to_str()
                    .unwrap()
                    .contains("GET")
            );
        }
        let body = to_bytes(response.into_body(), 8192).await.unwrap();
        let value: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["errors"][0]["status"], status.to_string());
        assert_eq!(value["errors"][0]["code"], code);
        assert!(value.get("error").is_none());
        assert!(!String::from_utf8_lossy(&body).contains("secret-password"));
        let doc: cairnkit_jsonapi::api::JsonApiDocument = serde_json::from_value(value).unwrap();
        assert!(doc.is_valid());
    }
}

#[tokio::test]
async fn body_limit_rejection_keeps_413_and_uses_cairnkit() {
    let response = app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/sessions")
                .header("content-type", "application/vnd.api+json")
                .body(Body::from("x".repeat(3 * 1024 * 1024)))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(
        response.headers()["content-type"],
        cairnkit_jsonapi::api::JSON_API_MEDIA_TYPE
    );
    let actual: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 8192).await.unwrap()).unwrap();
    assert_eq!(actual["errors"][0]["code"], "payload_too_large");
}

#[tokio::test]
async fn private_database_details_are_not_serialized() {
    use axum::response::IntoResponse;
    let response =
        wekeeper_api::business::Error::Database("secret-db-credentials".into()).into_response();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let body = to_bytes(response.into_body(), 8192).await.unwrap();
    assert!(!String::from_utf8_lossy(&body).contains("secret-db-credentials"));
    let actual: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(actual["errors"][0]["code"], "internal");
}

#[tokio::test]
async fn writes_reject_legacy_payloads_wrong_types_and_reserved_attributes() {
    for body in [
        serde_json::json!({"username":"admin","password":"secret"}),
        serde_json::json!({"data":{"type":"schools","attributes":{"username":"admin","password":"secret"}}}),
        serde_json::json!({"data":{"type":"sessions","attributes":{"id":"forbidden","username":"admin","password":"secret"}}}),
        serde_json::json!({"data":[]}),
        serde_json::json!({"data":null}),
    ] {
        let response = app()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/auth/sessions")
                    .header("content-type", "application/vnd.api+json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let document: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 8192).await.unwrap()).unwrap();
        assert_eq!(document["errors"][0]["code"], "validation");
    }
    let response = app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/sessions")
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
}

#[tokio::test]
async fn collections_commands_and_pagination_have_one_wire_shape() {
    use axum::response::IntoResponse;
    use serde_json::json;
    use wekeeper_api::business::Page;
    async fn document(response: impl IntoResponse) -> Value {
        let response = response.into_response();
        assert_eq!(
            response.headers()["content-type"],
            cairnkit_jsonapi::api::JSON_API_MEDIA_TYPE
        );
        serde_json::from_slice(&to_bytes(response.into_body(), 8192).await.unwrap()).unwrap()
    }
    assert_eq!(
        document(http::jsonapi::completed()).await,
        json!({"data":null})
    );
    assert_eq!(
        document(http::jsonapi::resource("posts", Vec::<Value>::new())).await,
        json!({"data":[]})
    );
    let page = Page {
        page: Some(2),
        page_size: Some(1),
        ..Default::default()
    };
    let value = document(http::jsonapi::collection(
        "posts",
        vec![json!({"id":"a","title":"A"}), json!({"id":"b"})],
        &page,
    ))
    .await;
    assert_eq!(
        value,
        json!({"data":[{"type":"posts","id":"a","attributes":{"title":"A"}}],"meta":{"page":{"number":2,"size":1,"has_more":true}}})
    );
    let value = document(http::jsonapi::collection(
        "posts",
        vec![json!({"id":"a"})],
        &page,
    ))
    .await;
    assert_eq!(value["meta"]["page"]["has_more"], false);
    let value = document(http::jsonapi::collection("posts", vec![], &page)).await;
    assert_eq!(value["meta"]["page"]["has_more"], false);
    let response = http::jsonapi::resource("posts", json!({"title":"missing id"})).into_response();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn pagination_rejects_old_names_and_invalid_bounds() {
    use axum::{extract::Query, routing::get};
    use wekeeper_api::business::{Page, Result};
    async fn page(Query(page): Query<Page>) -> Result<http::jsonapi::ApiResponse> {
        page.bounds()?;
        Ok(http::jsonapi::collection("posts", vec![], &page))
    }
    let router = axum::Router::new()
        .route("/api/page", get(page))
        .layer(axum::middleware::from_fn(http::error::normalize_errors));
    for (query, status) in [
        ("page_size=1", 400),
        ("page=2", 400),
        ("page[number]=0", 422),
        ("page[size]=101", 422),
        ("page[size]=-1", 422),
        ("page[number]=abc", 400),
        ("page%5Bnumber%5D=2&page%5Bsize%5D=1", 200),
    ] {
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/page?{query}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), status, "{query}");
        assert_eq!(
            response.headers()["content-type"],
            cairnkit_jsonapi::api::JSON_API_MEDIA_TYPE
        );
    }
}

#[tokio::test]
async fn directory_member_contract_matches_shared_fixture() {
    use axum::response::IntoResponse;
    use wekeeper_api::integrations::domain::directory::{Department, DirectoryMember};
    let member = DirectoryMember {
        id: uuid::Uuid::from_u128(1),
        userid: "Zhang.San".into(),
        name: "张三".into(),
        status: Some(1),
        visibility: "visible".into(),
        verification: "verified".into(),
        departments: vec![Department {
            id: 2,
            parentid: 1,
            name: "运营部".into(),
            order: 0,
        }],
        checked_at: Some("2026-10-21T00:00:00+00:00".into()),
        bound_account_id: None,
        bound_account_name: None,
    };
    let response = wekeeper_api::http::jsonapi::collection(
        "wecom-directory-members",
        vec![serde_json::to_value(member).unwrap()],
        &Default::default(),
    )
    .into_response();
    let actual: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 8192).await.unwrap()).unwrap();
    let fixture: Value = serde_json::from_str(include_str!(
        "../../api-contract/fixtures/wecom-directory-member.json"
    ))
    .unwrap();
    assert_eq!(actual, fixture);
}

#[tokio::test]
async fn broadcast_rust_serialization_matches_shared_fixtures() {
    use axum::response::IntoResponse;
    use uuid::Uuid;
    use wekeeper_api::distribution::domain::broadcast::{Target, Task};
    let id = Uuid::from_u128(1);
    let task = Task {
        id,
        school_id: id,
        creator_id: id,
        template_version_id: id,
        content: "校园通知".into(),
        revision: 1,
        state: "confirmed".into(),
        stopped: false,
        created_at: "2026-10-22T00:00:00Z".into(),
        total: 2,
        sent: 1,
        rejected: 0,
        uncertain: 0,
        pending: 1,
        batch_count: 1,
        post_ids: vec![id],
        source_task_id: None,
        source_draft_id: None,
        resend_reason: String::new(),
    };
    let target = Target {
        id,
        group_id: id,
        chat_id: "CHAT".into(),
        group_name: "校园群".into(),
        batch_id: id,
        sender: "owner".into(),
        creation_state: "created".into(),
        msgid: Some("official-task".into()),
        rejected: false,
        official_status: None,
        send_time: None,
        member_status: Some(2),
        last_synced_at: None,
        sync_error: None,
        error_code: None,
        stop_state: "none".into(),
        reconciliation: None,
        operation_unknown: false,
    };
    for (kind, value, fixture) in [
        (
            "broadcast-tasks",
            serde_json::to_value(task).unwrap(),
            include_str!("../../api-contract/fixtures/broadcast-task.json"),
        ),
        (
            "broadcast-targets",
            serde_json::to_value(target).unwrap(),
            include_str!("../../api-contract/fixtures/broadcast-target.json"),
        ),
    ] {
        let response = http::jsonapi::resource(kind, value).into_response();
        let actual: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 8192).await.unwrap()).unwrap();
        assert_eq!(actual, serde_json::from_str::<Value>(fixture).unwrap());
    }
}
