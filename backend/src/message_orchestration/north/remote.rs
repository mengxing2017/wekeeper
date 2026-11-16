use crate::{
    bootstrap::Application,
    business::{Actor, Page, Result},
    http::jsonapi::{ApiResponse, singleton},
    message_orchestration::south::adapter::postgres::Repository,
};
use axum::{
    Router,
    extract::{Path, Query, State},
    routing::get,
};
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Filter {
    pub route_id: Option<String>,
    pub channel: Option<String>,
    pub stage: Option<String>,
    pub attention: Option<String>,
    pub receipt_id: Option<uuid::Uuid>,
    #[serde(rename = "page[number]")]
    pub page_number: Option<i64>,
    #[serde(rename = "page[size]")]
    pub page_size: Option<i64>,
}
impl Filter {
    pub fn pagination(&self) -> Page {
        Page {
            page: self.page_number,
            page_size: self.page_size,
            ..Default::default()
        }
    }
}
async fn list(
    State(a): State<Application>,
    actor: Actor,
    Query(f): Query<Filter>,
) -> Result<ApiResponse> {
    actor.platform()?;
    let page = f.pagination();
    page.bounds()?;
    let v = Repository(a.pool)
        .list_progress(
            &actor,
            f.route_id,
            f.receipt_id,
            page,
            super::super::south::adapter::postgres::progress::ProgressFilter {
                channel: f.channel,
                stage: f.stage,
                attention: f.attention,
            },
        )
        .await?;
    Ok(singleton("messages", "result", v))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DeliveryFilter {
    #[serde(rename = "page[number]")]
    number: Option<i64>,
    #[serde(rename = "page[size]")]
    size: Option<i64>,
}
async fn deliveries(
    State(a): State<Application>,
    actor: Actor,
    Path(id): Path<uuid::Uuid>,
    Query(f): Query<DeliveryFilter>,
) -> Result<ApiResponse> {
    let value = Repository(a.pool)
        .deliveries(
            &actor,
            id,
            Page {
                page: f.number,
                page_size: f.size,
                ..Default::default()
            },
        )
        .await?;
    Ok(singleton("message-deliveries", "result", value))
}
pub fn router() -> Router<Application> {
    Router::new()
        .route("/api/v2/admin/messages", get(list))
        .route("/api/v2/admin/messages/{id}/deliveries", get(deliveries))
}
