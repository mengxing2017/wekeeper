pub mod south;
use crate::{
    bootstrap::Application,
    business::{Actor, Page, Result},
    http::jsonapi::{ApiResponse, collection},
};
use axum::extract::{Query, State};
pub use south::adapter::postgres::record;
pub async fn list(
    axum::extract::State(a): State<Application>,
    actor: Actor,
    Query(page): Query<Page>,
) -> Result<ApiResponse> {
    actor.platform()?;
    let (limit, offset) = page.fetch_bounds()?;
    Ok(collection(
        "audit-events",
        south::adapter::postgres::list(&a.pool, page.search.as_deref(), limit, offset).await?,
        &page,
    ))
}
