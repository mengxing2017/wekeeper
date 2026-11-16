use crate::{
    bootstrap::Application,
    business::{Actor, Page, Result},
    http::jsonapi::{ApiResponse, singleton},
    message_orchestration::south::adapter::postgres::Repository,
};
use axum::{
    Router,
    extract::{Query, State},
    routing::get,
};
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Filter {
    pub route_id: Option<String>,
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
        .list(&actor, f.route_id, f.receipt_id, page)
        .await?;
    Ok(singleton("messages", "result", v))
}
pub fn router() -> Router<Application> {
    Router::new().route("/api/v2/admin/messages", get(list))
}
