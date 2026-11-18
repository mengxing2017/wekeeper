use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
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
async fn receiving_configuration_routes_require_identity_and_serialize_channel_resources() {
    use axum::response::IntoResponse;
    use wekeeper_api::distribution::domain::receiving::{Channel, Configuration};
    let unit = "00000000-0000-4000-8000-000000000001";
    let id = "00000000-0000-4000-8000-000000000002";
    let fixture: Value = serde_json::from_str(include_str!(
        "../../api-contract/fixtures/receiving-configuration.json"
    ))
    .unwrap();
    for (base, channel, key) in [
        ("mobile-execution", Channel::Mobile, "configuration"),
        ("wecom-distribution", Channel::Wecom, "wecom_configuration"),
    ] {
        let path = format!("/api/v1/{base}/business-units/{unit}/receiving-configurations");
        for (method, uri) in [
            ("GET", path.clone()),
            ("GET", format!("{path}/{id}")),
            (
                "GET",
                format!("/api/v1/{base}/business-units/{unit}/receiving-resources"),
            ),
            (
                "PUT",
                format!("/api/v1/{base}/business-units/{unit}/receiving-targets/{id}"),
            ),
            ("PUT", format!("{path}/{id}")),
        ] {
            let response = app()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(uri)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        }
        let config: Configuration = serde_json::from_value(fixture[key].clone()).unwrap();
        let response = http::jsonapi::resource(channel.kind(), config).into_response();
        assert_eq!(response.status(), StatusCode::OK);
        let document: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        assert_eq!(document["data"]["type"], channel.kind());
        assert_eq!(document["data"]["id"], fixture[key]["id"]);
        let mut attributes = fixture[key].as_object().unwrap().clone();
        attributes.remove("id");
        assert_eq!(document["data"]["attributes"], Value::Object(attributes));
    }
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
                .uri("/api/v1/not-a-business-route")
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
    assert_eq!(response.headers()["location"], "/admin?oauth_error=failed");
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
            "/api/v1/wecom/oauth/start?workspace=invalid",
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
        serde_json::json!({"data":{"type":"groups","attributes":{"username":"admin","password":"secret"}}}),
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
        document(http::jsonapi::resource("groups", Vec::<Value>::new())).await,
        json!({"data":[]})
    );
    let page = Page {
        page: Some(2),
        page_size: Some(1),
        ..Default::default()
    };
    let value = document(http::jsonapi::collection(
        "groups",
        vec![json!({"id":"a","name":"A"}), json!({"id":"b"})],
        &page,
    ))
    .await;
    assert_eq!(
        value,
        json!({"data":[{"type":"groups","id":"a","attributes":{"name":"A"}}],"meta":{"page":{"number":2,"size":1,"has_more":true}}})
    );
    let value = document(http::jsonapi::collection(
        "groups",
        vec![json!({"id":"a"})],
        &page,
    ))
    .await;
    assert_eq!(value["meta"]["page"]["has_more"], false);
    let value = document(http::jsonapi::collection("groups", vec![], &page)).await;
    assert_eq!(value["meta"]["page"]["has_more"], false);
    let response = http::jsonapi::resource("groups", json!({"name":"missing id"})).into_response();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn pagination_rejects_old_names_and_invalid_bounds() {
    use axum::{extract::Query, routing::get};
    use wekeeper_api::business::{Page, Result};
    async fn page(Query(page): Query<Page>) -> Result<http::jsonapi::ApiResponse> {
        page.bounds()?;
        Ok(http::jsonapi::collection("groups", vec![], &page))
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
        source_namespace: "builtin.direct".into(),
        id,

        creator_id: id,
        content_source: wekeeper_api::distribution::domain::broadcast::ContentSource::Text,
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
        source_task_id: None,
        resend_reason: String::new(),
        draft_input: serde_json::from_value(
            serde_json::json!({"content_source":"text","text_content":"校园通知","group_ids":[id]}),
        )
        .unwrap(),
    };
    let target = Target {
        execution_mode: "official_customer_group_broadcast".into(),
        sync_failures: 0,
        next_sync_at: None,
        evidence: None,
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
    let mut text_task = task.clone();
    text_task.content_source = wekeeper_api::distribution::domain::broadcast::ContentSource::Text;
    text_task.source_namespace = "builtin.direct".into();
    text_task.draft_input = serde_json::from_value(
        serde_json::json!({"content_source":"text","text_content":"校园通知","group_ids":[id]}),
    )
    .unwrap();
    for (kind, value, fixture) in [
        (
            "broadcast-tasks",
            serde_json::to_value(text_task).unwrap(),
            include_str!("../../api-contract/fixtures/broadcast-text-task.json"),
        ),
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

#[tokio::test]
async fn revision_conflicts_explain_refresh_without_task_review_instructions() {
    use axum::response::IntoResponse;
    let response = wekeeper_api::business::Error::Policy("revision_conflict").into_response();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 8192).await.unwrap()).unwrap();
    assert_eq!(body["errors"][0]["code"], "revision_conflict");
    assert_eq!(
        body["errors"][0]["detail"],
        "记录已更新，请刷新后核对并重新提交"
    );
}

#[tokio::test]
async fn global_resource_errors_explain_current_authorization_and_binding_requirements() {
    use axum::response::IntoResponse;
    use wekeeper_api::business::Error;
    for (error, code, status, message) in [
        (
            Error::Forbidden,
            "forbidden",
            StatusCode::FORBIDDEN,
            "没有执行此操作的权限",
        ),
        (
            Error::Policy("shared_binding_missing"),
            "shared_binding_missing",
            StatusCode::CONFLICT,
            "目标群没有有效手机绑定，请系统管理员配置或恢复绑定",
        ),
    ] {
        let response = error.into_response();
        assert_eq!(response.status(), status);
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["errors"][0]["code"], code);
        assert_eq!(body["errors"][0]["detail"], message);
    }
}

#[test]
fn device_control_frame_matches_shared_contract() {
    use wekeeper_api::distribution::domain::device_control::{Frame, Pulse};
    let fixture: Value = serde_json::from_str(include_str!(
        "../../api-contract/fixtures/device-control-frame.json"
    ))
    .unwrap();
    let frame: Frame = serde_json::from_value(fixture.clone()).unwrap();
    assert_eq!(serde_json::to_value(frame).unwrap(), fixture);
    assert!(serde_json::from_value::<Pulse>(json!({"connection_id":"00000000-0000-4000-8000-000000000002","epoch":2,"readiness":"ready","permit_token":"must-not-authorize"})).is_err());
}

#[tokio::test]
async fn device_control_does_not_accept_browser_identity_as_a_device() {
    let response = app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/device-control/credential-activations")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[test]
fn control_presence_matches_shared_contract_and_policy() {
    use wekeeper_api::distribution::domain::device_control::Presence;
    let fixture: Value = serde_json::from_str(include_str!(
        "../../api-contract/fixtures/device-control-presence.json"
    ))
    .unwrap();
    let presence: Presence = serde_json::from_value(fixture.clone()).unwrap();
    assert!(presence.policy.valid());
    assert_eq!(serde_json::to_value(presence).unwrap(), fixture);
}

#[test]
fn preview_without_phone_keeps_target_identity_and_binding_names() {
    use wekeeper_api::distribution::domain::shared_mobile::shared_send_v2::PreviewItem;
    let fixture: Value = serde_json::from_str(include_str!(
        "../../api-contract/fixtures/shared-mobile-preview-item.json"
    ))
    .unwrap();
    let item: PreviewItem = serde_json::from_value(fixture.clone()).unwrap();
    assert!(item.selected.is_none());
    assert_eq!(item.target.name, "家长通知群");
    assert_eq!(serde_json::to_value(item).unwrap(), fixture);
}
#[test]
fn inspection_projection_distinguishes_expiry_and_real_queue() {
    use wekeeper_api::distribution::domain::shared_mobile::shared_send_v2::inspection_state as state;
    assert_eq!(state(None, false, false, 0, false), "waiting_prerequisites");
    assert_eq!(state(None, false, true, 0, false), "not_scheduled");
    assert_eq!(state(Some("leased"), false, true, 0, false), "expired");
    assert_eq!(state(Some("passed"), false, true, 0, false), "expired");
    assert_eq!(state(Some("pending"), true, true, 1, true), "queued");
    assert_eq!(state(Some("leased"), true, true, 1, true), "running");
    assert_eq!(state(Some("passed"), true, true, 3, true), "passed");
    assert_eq!(state(Some("failed"), true, true, 1, true), "retry_wait");
    assert_eq!(
        state(Some("failed"), true, true, 3, true),
        "retry_exhausted"
    );
}

#[test]
fn inspected_phone_requires_current_readiness_before_confirmation() {
    use wekeeper_api::distribution::domain::shared_mobile::shared_send_v2::preview_blocker;
    let device = uuid::Uuid::from_u128(1);
    assert_eq!(
        preview_blocker(None, &[]).as_deref(),
        Some("shared_binding_missing")
    );
    let blocked =
        json!({"device_id":device,"ready":false,"blockers":[{"code":"acceptance_missing"}]});
    assert_eq!(
        preview_blocker(Some(device), &[blocked]).as_deref(),
        Some("acceptance_missing")
    );
    assert_eq!(
        preview_blocker(Some(device), &[]).as_deref(),
        Some("device_not_eligible")
    );
    assert!(
        preview_blocker(
            Some(device),
            &[json!({"device_id":device,"ready":true,"blockers":[]})]
        )
        .is_none()
    );
}

#[tokio::test]
async fn mobile_research_management_routes_are_gone() {
    for (method, path) in [
        (
            "POST",
            "/api/v1/platform/mobile-execution/shared-v2-management",
        ),
        (
            "POST",
            "/api/v1/platform/mobile-execution/shared-v2-artifacts",
        ),
        (
            "GET",
            "/api/v1/platform/mobile-execution/shared-v2-readiness",
        ),
        (
            "POST",
            "/api/v1/platform/mobile-execution/shared-v2-send-policies",
        ),
    ] {
        let response = app()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
    }
}

#[tokio::test]
async fn product_workflow_routes_require_identity_before_any_database_access() {
    for (method, path) in [
        ("POST", "/api/v1/wecom-distribution/broadcast-task-previews"),
        ("POST", "/api/v1/platform/member-policy-previews"),
        ("GET", "/api/v1/platform/business-requests"),
        (
            "POST",
            "/api/v1/platform/business-requests/00000000-0000-4000-8000-000000000001/decisions",
        ),
    ] {
        let response = app()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "{method} {path}"
        );
    }
}

#[test]
fn platform_business_scheme_fixture_roundtrips_without_technical_inputs() {
    use wekeeper_api::distribution::domain::shared_mobile::business_scheme::BusinessSchemeInput;
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../api-contract/fixtures/mobile-business-scheme.json"
    ))
    .unwrap();
    let input: BusinessSchemeInput = serde_json::from_value(fixture.clone()).unwrap();
    assert_eq!(serde_json::to_value(&input).unwrap(), fixture);
    input.policy().unwrap();
}
#[tokio::test]
async fn business_scheme_route_requires_identity() {
    let response = app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/mobile-execution/business-schemes/default/versions")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[test]
fn shared_submission_status_separates_defaults_and_execution_pause() {
    use wekeeper_api::distribution::domain::shared_mobile::business_scheme::SendingStatus;
    let defaults = serde_json::to_value(SendingStatus::project(None)).unwrap();
    assert_eq!(defaults["platform"], "default");
    assert_eq!(defaults["can_confirm"], true);
    let paused = serde_json::to_value(SendingStatus::project(Some("disabled"))).unwrap();
    assert_eq!(paused["platform"], "disabled");
    assert_eq!(paused["can_confirm"], true);
    assert!(paused["message"].as_str().unwrap().contains("任务仍可提交"));
}

#[tokio::test]
async fn registry_rotation_requires_device_identity_and_admin_registry_requires_account() {
    for (method, path) in [
        ("POST", "/api/v10/device-execution/credential-rotations"),
        ("GET", "/api/v1/platform/execution-devices"),
    ] {
        let response = app()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .header("content-type", "application/vnd.api+json")
                    .header("cookie", "session=browser-only")
                    .body(Body::from(
                        r#"{"data":{"type":"device-credentials","attributes":{}}}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{path}");
    }
}

#[test]
fn definitive_dependency_rejection_uses_conflict_status() {
    use axum::response::IntoResponse;
    let response = wekeeper_api::business::Error::Policy("plan_dependency_changed").into_response();
    assert_eq!(response.status(), axum::http::StatusCode::CONFLICT);
}

#[tokio::test]
async fn inspection_is_a_device_authenticated_jsonapi_endpoint() {
    let response = app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v10/device-execution/inspection-executions")
                .header("content-type", "application/vnd.api+json")
                .body(Body::from(
                    json!({"data":{"type":"inspection-executions","attributes":{"runtime_epoch":"00000000-0000-4000-8000-000000000001","inspection_id":"00000000-0000-4000-8000-000000000002"}}}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn session_requires_authenticated_credentials() {
    let response = app()
        .oneshot(
            Request::builder()
                .uri("/api/v1/auth/session")
                .header("authorization", "Bearer invalid-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn business_management_collections_require_identity() {
    for path in [
        "/api/v1/wecom-distribution/groups",
        "/api/v1/mobile-execution/targets",
        "/api/v1/platform/administrators",
        "/api/v1/audit-events",
    ] {
        let response = app()
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{path}");
    }
}

#[tokio::test]
async fn system_audit_rejects_non_administrators_before_database_access() {
    let pool = PgPoolOptions::new()
        .connect_lazy("postgres://unused:unused@127.0.0.1:1/unused")
        .unwrap();
    let actor = serde_json::from_value(json!({"id":"00000000-0000-4000-8000-000000000001", "username":"operator", "display_name":"operator", "platform_admin":false, "wecom_authenticated":false})).unwrap();
    let result = wekeeper_api::audit::list(
        axum::extract::State(bootstrap::build_application(pool)),
        actor,
        axum::extract::Query(Default::default()),
    )
    .await;
    assert!(matches!(
        result,
        Err(wekeeper_api::business::Error::Forbidden)
    ));
}

#[test]
fn platform_heartbeat_probe_and_response_match_shared_contract() {
    use wekeeper_api::distribution::domain::device_control::{Frame, Pulse};
    let fixture: Value = serde_json::from_str(include_str!(
        "../../api-contract/fixtures/device-control-probe.json"
    ))
    .unwrap();
    let frame: Frame = serde_json::from_value(fixture["frame"].clone()).unwrap();
    let response: Pulse = serde_json::from_value(fixture["response"].clone()).unwrap();
    assert_eq!(frame.probe_id, response.probe_id);
    assert!(response.heartbeat_id.is_some());
    assert_eq!(response.heartbeat_sequence, 1);
    assert_eq!(serde_json::to_value(frame).unwrap(), fixture["frame"]);
    assert_eq!(serde_json::to_value(response).unwrap(), fixture["response"]);
}

#[test]
fn remote_hosting_pause_commands_are_rejected_by_the_wire_contract() {
    use wekeeper_api::distribution::domain::device_control::CommandInput;
    for kind in ["pause_execution", "resume_execution"] {
        assert!(serde_json::from_value::<CommandInput>(json!({"kind": kind})).is_err());
    }
    let status: CommandInput = serde_json::from_value(json!({"kind": "request_status"})).unwrap();
    assert_eq!(
        serde_json::to_value(status).unwrap(),
        json!({"kind": "request_status"})
    );
}

#[tokio::test]
async fn removed_pull_and_http_control_routes_are_absent() {
    for path in [
        "/api/v10/device-execution/task-deliveries",
        "/api/v10/device-execution/execution-grants",
        "/api/v10/device-execution/execution-receipts",
        "/api/v1/device-control/session",
        "/api/v1/device-control/pulses",
    ] {
        let response = app()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(path)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
    }
}

#[tokio::test]
async fn native_hosting_routes_are_removed_and_environment_requires_device_identity() {
    for (method, path) in [
        ("GET", "authority-context"),
        ("POST", "authorization-decisions"),
        ("POST", "idle-telemetry"),
    ] {
        let response = app()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(format!("/api/v10/device-execution/{path}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
    }
    let response = app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v10/device-execution/environment-observations")
                .header("Content-Type", "application/vnd.api+json")
                .body(Body::from(
                    json!({"data":{"type":"mobile-device-environments","attributes":{}}})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[test]
fn attempt_execution_observation_preserves_missing_receipt_and_device_times() {
    use wekeeper_api::distribution::domain::shared_mobile::execution::AttemptExecutionObservation;
    let fixture: Value = serde_json::from_str(include_str!(
        "../../api-contract/fixtures/attempt-execution-observation.json"
    ))
    .unwrap();
    let observation: AttemptExecutionObservation = serde_json::from_value(fixture.clone()).unwrap();
    assert_eq!(serde_json::to_value(observation).unwrap(), fixture);
    let empty = serde_json::to_value(AttemptExecutionObservation::default()).unwrap();
    assert!(empty.as_object().unwrap().values().all(Value::is_null));
    assert!(empty.get("device_received_at").is_none());
}

#[test]
fn unit_task_preview_has_a_server_side_context_and_rejects_unknown_fields() {
    use wekeeper_api::distribution::domain::shared_mobile::business_scheme::TaskPreviewInput;
    let body = serde_json::json!({"draft_id":null,"expected_revision":null,"business_unit_id":"00000000-0000-0000-0000-000000000001","client_kind":"wecom","target_ids":["00000000-0000-0000-0000-000000000002"],"message":" 原样正文\n","between_targets_seconds":30,"selections":[]});
    let preview: TaskPreviewInput = serde_json::from_value(body.clone()).unwrap();
    assert_eq!(serde_json::to_value(preview).unwrap(), body);
    let mut invalid = body.clone();
    invalid["scheme_snapshot"] = serde_json::json!({});
    assert!(serde_json::from_value::<TaskPreviewInput>(invalid).is_err());
    let mut no_unit = body;
    no_unit["business_unit_id"] = serde_json::Value::Null;
    assert!(
        serde_json::from_value::<TaskPreviewInput>(no_unit)
            .unwrap()
            .business_unit_id
            .is_none()
    );
}

#[tokio::test]
async fn statistics_wire_counts_and_all_routes_require_identity() {
    use axum::response::IntoResponse;
    use wekeeper_api::message_statistics::domain::Overview;
    let fixture: Value = serde_json::from_str(include_str!(
        "../../api-contract/fixtures/message-statistics.json"
    ))
    .unwrap();
    let model: Overview = serde_json::from_value(fixture.clone()).unwrap();
    let response =
        http::jsonapi::singleton("message-statistics-queries", model.id, model).into_response();
    assert_eq!(response.status(), StatusCode::OK);
    let actual: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(
        actual["data"]["attributes"]["summary"]["targets"]["total"],
        "9007199254740993"
    );
    let mut expected = fixture;
    expected.as_object_mut().unwrap().remove("id");
    assert_eq!(actual["data"]["attributes"], expected);
    let base = "/api/v2/admin/message-statistics/queries";
    for (method, suffix) in [
        ("POST", ""),
        ("GET", "/00000000-0000-4000-8000-000000000001"),
        ("GET", "/00000000-0000-4000-8000-000000000001/details"),
        (
            "GET",
            "/00000000-0000-4000-8000-000000000001/breakdowns?dimension=unit",
        ),
        ("POST", "/00000000-0000-4000-8000-000000000001/exports"),
    ] {
        let r = app()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(format!("{base}{suffix}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
    }
}
