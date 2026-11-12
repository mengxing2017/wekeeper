use crate::{
    bootstrap::Application,
    business::{Actor, Error},
    business_directory::{
        domain::DirectoryError, north::local::UnitDirectory, south::adapter::postgres::Repository,
    },
};
use axum::{
    Router,
    extract::{Path, Query, State},
    response::{IntoResponse, Response},
    routing::get,
};
use uuid::Uuid;

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Filter {
    page: Option<i64>,
}

fn error(error: DirectoryError) -> Response {
    match error {
        DirectoryError::Forbidden => Error::Forbidden,
        DirectoryError::InvalidPage => Error::Invalid("页码不合法".into()),
        DirectoryError::Unavailable => Error::Unavailable,
    }
    .into_response()
}

async fn platforms(
    State(app): State<Application>,
    actor: Actor,
    Query(filter): Query<Filter>,
) -> Result<Response, Response> {
    let page = UnitDirectory(Repository(app.pool))
        .platforms(&actor, filter.page.unwrap_or(1))
        .await
        .map_err(error)?;
    Ok(crate::http::jsonapi::singleton("business-platforms", "list", page).into_response())
}

async fn units(
    State(app): State<Application>,
    actor: Actor,
    Path(platform_id): Path<Uuid>,
    Query(filter): Query<Filter>,
) -> Result<Response, Response> {
    let page = UnitDirectory(Repository(app.pool))
        .units(&actor, platform_id, filter.page.unwrap_or(1))
        .await
        .map_err(error)?;
    Ok(crate::http::jsonapi::singleton("business-units", "list", page).into_response())
}

pub fn router() -> Router<Application> {
    Router::new()
        .route("/api/v2/admin/business-platforms", get(platforms))
        .route(
            "/api/v2/admin/business-platforms/{platform_id}/units",
            get(units),
        )
}
