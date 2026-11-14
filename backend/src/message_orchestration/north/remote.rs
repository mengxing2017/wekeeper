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
pub struct Filter {
    pub route_id: Option<String>,
    pub receipt_id: Option<uuid::Uuid>,
    #[serde(flatten)]
    pub page: Page,
}
async fn list(
    State(a): State<Application>,
    actor: Actor,
    Query(f): Query<Filter>,
) -> Result<ApiResponse> {
    let v = Repository(a.pool)
        .list(&actor, f.route_id, f.receipt_id, f.page)
        .await?;
    Ok(singleton("messages", "result", v))
}
pub fn router() -> Router<Application> {
    Router::new().route("/api/v2/admin/messages", get(list))
}
