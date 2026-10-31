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
async fn responsible_actor_contract_matches_shared_fixture_and_requires_identity() {
    use axum::response::IntoResponse;
    use wekeeper_api::{
        business::Page,
        distribution::domain::mobile_delivery::{ResponsibleActor, ResponsibleSchool},
    };
    let actor = ResponsibleActor {
        id: "00000000-0000-4000-8000-000000000001".parse().unwrap(),
        username: "zhangsan".into(),
        display_name: "张三".into(),
        active: true,
        eligible: true,
        schools: vec![ResponsibleSchool {
            id: "00000000-0000-4000-8000-000000000002".parse().unwrap(),
            name: "A 学校".into(),
        }],
    };
    let response = wekeeper_api::http::jsonapi::collection(
        "sending-account-responsible-actors",
        vec![serde_json::to_value(actor).unwrap()],
        &Page::default(),
    )
    .into_response();
    let actual: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap()).unwrap();
    let expected: Value = serde_json::from_str(include_str!(
        "../../api-contract/fixtures/sending-account-responsible-actors.json"
    ))
    .unwrap();
    assert_eq!(actual, expected);
    let response = app()
        .oneshot(
            Request::builder()
                .uri("/api/v1/platform/sending-account-responsible-actors")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[test]
fn account_profile_review_projection_has_a_shared_serialization_contract() {
    use wekeeper_api::distribution::domain::mobile_delivery::AccountProfileReview;
    let fixture: Value = serde_json::from_str(include_str!(
        "../../api-contract/fixtures/sending-account-profile-review.json"
    ))
    .unwrap();
    let review = AccountProfileReview {
        actor_id: "00000000-0000-4000-8000-000000000001".parse().unwrap(),
        actor_name: "张三".into(),
        actor_username: "zhangsan".into(),
        reviewed_at: "2026-10-31T02:00:00Z".parse().unwrap(),
        profile_revision: Some(2),
        evidence_ref: "本次现场核对企业和账号页面".into(),
    };
    assert_eq!(serde_json::to_value(review).unwrap(), fixture);
    let mut historical = fixture;
    historical["profile_revision"] = Value::Null;
    let review: AccountProfileReview = serde_json::from_value(historical.clone()).unwrap();
    assert_eq!(serde_json::to_value(review).unwrap(), historical);
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
        template_version_id: Some(id),
        content_source: wekeeper_api::distribution::domain::broadcast::ContentSource::Posts,
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
    let mut text_task = task.clone();
    text_task.content_source = wekeeper_api::distribution::domain::broadcast::ContentSource::Text;
    text_task.template_version_id = None;
    text_task.post_ids.clear();
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
async fn mobile_permission_serialization_matches_shared_fixture() {
    use axum::response::IntoResponse;
    use wekeeper_api::distribution::domain::mobile_delivery::SendPermit;
    let permit = SendPermit {
        id: "11111111-1111-4111-8111-111111111111".parse().unwrap(),
        claim_id: "22222222-2222-4222-8222-222222222222".parse().unwrap(),
        invocation_id: "33333333-3333-4333-8333-333333333333".parse().unwrap(),
        fence: 4,
        snapshot_digest: "snapshot-sha256".into(),
        permit_token: Some("fixture-only-secret".into()),
        not_before: "2026-10-24T04:00:00Z".parse().unwrap(),
        execute_before: "2026-10-24T04:01:00Z".parse().unwrap(),
        server_time: "2026-10-24T04:00:00Z".parse().unwrap(),
    };
    let response =
        wekeeper_api::http::jsonapi::resource("mobile-send-permits", permit).into_response();
    let actual: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap()).unwrap();
    let fixture: Value = serde_json::from_str(include_str!(
        "../../api-contract/fixtures/mobile-send-permit.json"
    ))
    .unwrap();
    assert_eq!(actual, fixture);
    let wait: Value = serde_json::from_str(include_str!(
        "../../api-contract/fixtures/mobile-execution-wait.json"
    ))
    .unwrap();
    let response = wekeeper_api::http::jsonapi::completed()
        .meta(wait["meta"].clone())
        .into_response();
    assert_eq!(
        serde_json::from_slice::<Value>(&to_bytes(response.into_body(), 4096).await.unwrap())
            .unwrap(),
        wait
    );
}

#[tokio::test]
async fn mobile_device_routes_require_dedicated_credentials() {
    for auth in [None, Some("Bearer miniapp-token")] {
        let mut request = Request::builder()
            .uri("/api/v1/device-execution/session")
            .header("Cookie", "wekeeper_session=browser-token");
        if let Some(auth) = auth {
            request = request.header("Authorization", auth);
        }
        let response = app()
            .oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            response.headers()["content-type"],
            "application/vnd.api+json"
        );
        assert!(response.headers().contains_key("x-request-id"));
        let body: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 8192).await.unwrap()).unwrap();
        assert_eq!(body["errors"][0]["code"], "unauthorized");
        assert_eq!(
            body["errors"][0]["detail"],
            "设备身份认证未通过，请管理员核对设备启用状态和凭据"
        );
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

#[test]
fn mobile_not_clicked_result_preserves_null_timestamp() {
    use wekeeper_api::distribution::domain::mobile_delivery::Command;
    let fixture: Value = serde_json::from_str(include_str!(
        "../../api-contract/fixtures/mobile-execution-not-clicked.json"
    ))
    .unwrap();
    let mut input = fixture["data"]["attributes"].clone();
    input["operation"] = serde_json::json!("result");
    let command: Command = serde_json::from_value(input.clone()).unwrap();
    match &command {
        Command::Result {
            reported_clicked_at,
            result,
            ..
        } => {
            assert!(reported_clicked_at.is_none());
            assert_eq!(result, "not_observed");
        }
        _ => panic!("expected result command"),
    }
    assert_eq!(serde_json::to_value(command).unwrap(), input);
}

#[tokio::test]
async fn v2_device_routes_never_accept_browser_identity() {
    let response = app()
        .oneshot(
            Request::builder()
                .uri("/api/v2/device-execution/session")
                .header("Cookie", "wekeeper_session=browser-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
#[test]
fn strategy_commands_are_strict_and_default_to_disabled() {
    use wekeeper_api::distribution::domain::mobile_delivery::Command;
    let fixture: Value = serde_json::from_str(include_str!(
        "../../api-contract/fixtures/mobile-visible-claim-v2.json"
    ))
    .unwrap();
    let a = &fixture["data"]["attributes"];
    let mut v = serde_json::json!({"operation":"create_strategy","name":"测试策略","sender_account_id":a["sender_binding"]["id"],"group_id":a["group_snapshot"]["id"],"device_id":a["execution_target"]["device_id"],"text_content":"测试"});
    let c: Command = serde_json::from_value(v.clone()).unwrap();
    if let Command::CreateStrategy { input } = c {
        assert!(!input.enabled);
        input.validate().unwrap();
    } else {
        panic!("strategy expected");
    }
    v["resource_id"] = serde_json::json!("unavailable");
    assert!(serde_json::from_value::<Command>(v).is_err());
}

#[test]
fn automation_v4_shared_wire_is_typed_and_does_not_relax_v3() {
    use wekeeper_api::distribution::domain::mobile_delivery::{
        AutomationCommand, Command, EnvironmentReport, ValidationEvent, ValidationKind,
    };
    let environment: Value = serde_json::from_str(include_str!(
        "../../api-contract/fixtures/automation-environment-v4.json"
    ))
    .unwrap();
    let report: EnvironmentReport =
        serde_json::from_value(environment["data"]["attributes"]["report"].clone()).unwrap();
    report.validate().unwrap();
    assert!(report.supports_execution());
    assert_eq!(
        serde_json::to_value(&report).unwrap(),
        environment["data"]["attributes"]["report"]
    );
    let parsed: AutomationCommand =
        serde_json::from_value(serde_json::json!({"action":"report_environment","report":report}))
            .unwrap();
    assert!(matches!(
        parsed,
        AutomationCommand::ReportEnvironment { .. }
    ));
    let mut old_claim =
        serde_json::json!({"operation":"claim","binding_revision":1,"environment":report});
    assert!(serde_json::from_value::<Command>(old_claim.clone()).is_err());
    old_claim.as_object_mut().unwrap().remove("environment");
    assert_eq!(
        serde_json::to_value(serde_json::from_value::<Command>(old_claim.clone()).unwrap())
            .unwrap(),
        old_claim
    );
    let event: Value = serde_json::from_str(include_str!(
        "../../api-contract/fixtures/automation-validation-v4.json"
    ))
    .unwrap();
    let parsed: ValidationEvent =
        serde_json::from_value(event["data"]["attributes"]["event"].clone()).unwrap();
    parsed.validate(ValidationKind::OpenApplication).unwrap();
    assert_eq!(
        serde_json::to_value(parsed).unwrap(),
        event["data"]["attributes"]["event"]
    );
}
#[tokio::test]
async fn automation_and_v4_routes_are_registered_and_require_authorization() {
    for (path, method) in [
        (
            "/api/v1/schools/00000000-0000-0000-0000-000000000001/automation-apps",
            "GET",
        ),
        ("/api/v1/platform/automation-adaptations/components", "GET"),
        (
            "/api/v4/device-execution/execution-authorization-context",
            "GET",
        ),
        ("/api/v4/device-execution/execution-authorizations", "POST"),
        (
            "/api/v4/device-execution/validation-jobs/00000000-0000-0000-0000-000000000001/standing-acceptances",
            "POST",
        ),
        (
            "/api/v4/device-execution/validation-jobs/00000000-0000-0000-0000-000000000001/rejections",
            "POST",
        ),
        ("/api/v4/device-execution/validation-jobs", "GET"),
        ("/api/v4/device-execution/environment-snapshots", "POST"),
        ("/api/v4/device-execution/claims", "POST"),
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
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{path}");
    }
}

#[test]
fn automation_manifest_canonicalization_matches_shared_utf8_bytes() {
    use wekeeper_api::distribution::domain::mobile_delivery::automation_digest;
    let fixture: Value = serde_json::from_str(include_str!(
        "../../api-contract/fixtures/automation-canonical-v4.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["value"].to_string(),
        fixture["canonical"].as_str().unwrap()
    );
    assert_eq!(
        automation_digest(&fixture["value"]),
        fixture["sha256"].as_str().unwrap()
    );
}

#[test]
fn standing_authorization_shared_wire_preserves_receipt_and_decline_semantics() {
    use wekeeper_api::distribution::domain::mobile_delivery::{
        AutomationCommand, ExecutionAuthorizationReceipt,
    };
    let fixture: Value = serde_json::from_str(include_str!(
        "../../api-contract/fixtures/execution-authorization-receipt-v4.json"
    ))
    .unwrap();
    let wire = &fixture["data"]["attributes"]["receipt"];
    let r: ExecutionAuthorizationReceipt = serde_json::from_value(wire.clone()).unwrap();
    r.validate().unwrap();
    assert_eq!(serde_json::to_value(r).unwrap(), *wire);
    let mut decline: Value = serde_json::from_str(include_str!(
        "../../api-contract/fixtures/execution-validation-decline-v4.json"
    ))
    .unwrap();
    decline["data"]["attributes"]["action"] = serde_json::json!("reject_validation");
    assert!(matches!(
        serde_json::from_value::<AutomationCommand>(decline["data"]["attributes"].clone()).unwrap(),
        AutomationCommand::RejectValidation { .. }
    ));
}

#[tokio::test]
async fn dispatch_v5_routes_are_registered_and_require_dedicated_authorization() {
    for (path, method) in [
        ("/api/v1/platform/automation-flows", "GET"),
        ("/api/v1/platform/automation-pacing-profiles", "GET"),
        (
            "/api/v1/schools/00000000-0000-0000-0000-000000000001/automation-dispatch-batches",
            "GET",
        ),
        ("/api/v5/device-execution/session", "GET"),
        ("/api/v5/device-execution/task-inbox", "GET"),
        (
            "/api/v5/device-execution/execution-authorization-context",
            "GET",
        ),
        ("/api/v5/device-execution/execution-authorizations", "POST"),
        ("/api/v5/device-execution/flow-validations", "GET"),
        (
            "/api/v5/device-execution/task-deliveries/00000000-0000-0000-0000-000000000001/preparation-leases",
            "POST",
        ),
        (
            "/api/v5/device-execution/task-deliveries/00000000-0000-0000-0000-000000000001/failures",
            "POST",
        ),
        ("/api/v5/device-execution/claims", "POST"),
        (
            "/api/v5/device-execution/claims/00000000-0000-0000-0000-000000000001/preparations",
            "POST",
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
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{path}");
    }
}
