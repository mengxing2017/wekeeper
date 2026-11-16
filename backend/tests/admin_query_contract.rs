use axum::{
    body::{Body, to_bytes},
    extract::Query,
    http::Request,
    routing::get,
};
use serde_json::Value;
use tower::ServiceExt;
use wekeeper_api::{
    business::{Page, Pagination, Result},
    http,
    message_orchestration::north::remote::Filter,
    source_access::domain::ManagementFilter,
};

#[tokio::test]
async fn actual_admin_queries_parse_client_urls_and_reject_invalid_parameters() {
    async fn messages(Query(f): Query<Filter>) -> Result<http::jsonapi::ApiResponse> {
        let page = f.pagination();
        page.bounds()?;
        Ok(http::jsonapi::singleton(
            "messages",
            "result",
            serde_json::json!({
                "number":page.page.unwrap_or(1),"size":page.page_size.unwrap_or(30),
                "route_id":f.route_id,"receipt_id":f.receipt_id
            }),
        ))
    }
    async fn pagination(Query(q): Query<Pagination>) -> Result<http::jsonapi::ApiResponse> {
        result(q.page())
    }
    async fn management(Query(q): Query<ManagementFilter>) -> Result<http::jsonapi::ApiResponse> {
        result(q.pagination())
    }
    fn result(page: Page) -> Result<http::jsonapi::ApiResponse> {
        let (size, offset) = page.bounds()?;
        Ok(http::jsonapi::singleton(
            "pages",
            "result",
            serde_json::json!({"size":size,"offset":offset}),
        ))
    }
    let app = axum::Router::new()
        .route("/api/messages", get(messages))
        .route("/api/pagination", get(pagination))
        .route("/api/management", get(management))
        .layer(axum::middleware::from_fn(http::error::normalize_errors));
    for path in ["messages", "pagination", "management"] {
        for (query, status) in [
            ("", 200),
            ("page%5Bnumber%5D=1&page%5Bsize%5D=30", 200),
            ("page[number]=2&page[size]=1", 200),
            ("page[size]=100", 200),
            ("page[number]=100000", 200),
            ("page[number]=abc", 400),
            ("page[number]=", 400),
            ("page[number]=1&page[number]=2", 400),
            ("page[size]=1&page[size]=2", 400),
            ("page=1", 400),
            ("page_size=30", 400),
            ("unknown=x", 400),
            ("page[number]=0", 422),
            ("page[number]=100001", 422),
            ("page[size]=0", 422),
            ("page[size]=-1", 422),
            ("page[size]=101", 422),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(format!("/api/{path}?{query}"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status().as_u16(), status, "{path}?{query}");
            assert_eq!(
                response.headers()["content-type"],
                "application/vnd.api+json"
            );
            let body: Value =
                serde_json::from_slice(&to_bytes(response.into_body(), 8192).await.unwrap())
                    .unwrap();
            if status == 400 {
                assert_eq!(body["errors"][0]["code"], "invalid_request");
                assert_eq!(body["errors"][0]["detail"], "请求参数或 JSON 格式不合法");
            } else if status == 422 {
                assert_eq!(body["errors"][0]["code"], "validation");
            } else if path == "messages" {
                assert_eq!(body["data"]["type"], "messages");
                assert_eq!(body["data"]["id"], "result");
            } else if query == "page[number]=2&page[size]=1" {
                assert_eq!(
                    body["data"]["attributes"],
                    serde_json::json!({"size":1,"offset":1})
                );
            }
        }
    }
    let uri = "/api/messages?route_id=route%2Fone&receipt_id=00000000-0000-0000-0000-000000000001&page%5Bnumber%5D=2&page%5Bsize%5D=10".parse().unwrap();
    let Query(filter) = Query::<Filter>::try_from_uri(&uri).unwrap();
    assert_eq!(filter.route_id.as_deref(), Some("route/one"));
    assert_eq!(filter.receipt_id, Some(uuid::Uuid::from_u128(1)));
    assert_eq!(filter.pagination().bounds().unwrap(), (10, 10));
    for query in ["receipt_id=invalid", "mine=true", "search=test"] {
        let uri = format!("/api/messages?{query}").parse().unwrap();
        assert!(Query::<Filter>::try_from_uri(&uri).is_err());
    }
}
