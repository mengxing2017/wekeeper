pub mod south;
use crate::{
    bootstrap::Application,
    business::{Actor, Page, Result},
    http::error::data,
};
use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde_json::Value;
pub use south::adapter::postgres::record;
use uuid::Uuid;
pub async fn list(
    axum::extract::State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
    Query(page): Query<Page>,
) -> Result<Json<Value>> {
    actor.authorize(s, "operator")?;
    let (limit, offset) = page.bounds()?;
    Ok(data(
        south::adapter::postgres::list(&a.pool, s, limit, offset).await?,
    ))
}
