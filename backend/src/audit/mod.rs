pub mod south;
use crate::{
    bootstrap::Application,
    business::{Actor, Page, Result},
    http::jsonapi::{ApiResponse, collection},
};
use axum::extract::{Path, Query, State};
pub use south::adapter::postgres::record;
use uuid::Uuid;
pub async fn list(
    axum::extract::State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
    Query(page): Query<Page>,
) -> Result<ApiResponse> {
    actor.authorize(s, "operator")?;
    let (limit, offset) = page.fetch_bounds()?;
    Ok(collection(
        "audit-events",
        south::adapter::postgres::list(&a.pool, s, limit, offset).await?,
        &page,
    ))
}
