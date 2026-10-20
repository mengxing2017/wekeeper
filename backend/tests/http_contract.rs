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
    let actual: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap()).unwrap();
    assert_eq!(actual["error"]["code"], "not_found");
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
