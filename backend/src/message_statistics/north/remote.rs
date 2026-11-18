use crate::{
    bootstrap::Application,
    business::{Actor, Result, require},
    http::jsonapi::{ApiResponse, Input, singleton},
    message_statistics::{
        domain::{DetailFilter, Filter},
        north::local::Statistics,
        south::adapter::postgres::Repository,
    },
};
use axum::{
    Router,
    extract::{Path, Query, State},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;
async fn create(State(app): State<Application>, actor: Actor, input: Input) -> Result<ApiResponse> {
    let filter: Filter = input.decode("message-statistics-queries")?;
    let result = Statistics(Repository(app.pool))
        .create(&actor, filter)
        .await?;
    Ok(singleton("message-statistics-queries", result.id, &result))
}
async fn overview(
    State(app): State<Application>,
    actor: Actor,
    Path(id): Path<Uuid>,
) -> Result<ApiResponse> {
    let result = Statistics(Repository(app.pool))
        .overview(&actor, id)
        .await?;
    Ok(singleton("message-statistics-queries", id, result))
}
async fn details(
    State(app): State<Application>,
    actor: Actor,
    Path(id): Path<Uuid>,
    Query(filter): Query<DetailFilter>,
) -> Result<ApiResponse> {
    Ok(singleton(
        "message-statistics-details",
        id,
        Statistics(Repository(app.pool))
            .details(&actor, id, filter)
            .await?,
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BreakdownFilter {
    dimension: String,
    page: Option<usize>,
}
async fn breakdowns(
    State(app): State<Application>,
    actor: Actor,
    Path(id): Path<Uuid>,
    Query(filter): Query<BreakdownFilter>,
) -> Result<ApiResponse> {
    require(
        matches!(filter.dimension.as_str(), "unit" | "channel"),
        "分组不合法",
    )?;
    let page = filter.page.unwrap_or(1);
    require((1..=100000).contains(&page), "页码不合法")?;
    let data = Statistics(Repository(app.pool))
        .overview(&actor, id)
        .await?;
    let groups = if filter.dimension == "unit" {
        data.units
    } else {
        data.channels
    };
    let has_more = groups.len() > page * 30;
    Ok(singleton(
        "message-statistics-breakdowns",
        id,
        json!({"items":groups.into_iter().skip((page-1)*30).take(30).collect::<Vec<_>>(),"has_more":has_more}),
    ))
}
async fn export(
    State(app): State<Application>,
    actor: Actor,
    Path(id): Path<Uuid>,
    input: Input,
) -> Result<ApiResponse> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Empty {}
    let _: Empty = input.decode("message-statistics-exports")?;
    Ok(singleton(
        "message-statistics-exports",
        id,
        Statistics(Repository(app.pool)).export(&actor, id).await?,
    ))
}
pub fn router() -> Router<Application> {
    Router::new()
        .route("/api/v2/admin/message-statistics/queries", post(create))
        .route(
            "/api/v2/admin/message-statistics/queries/{id}",
            get(overview),
        )
        .route(
            "/api/v2/admin/message-statistics/queries/{id}/breakdowns",
            get(breakdowns),
        )
        .route(
            "/api/v2/admin/message-statistics/queries/{id}/details",
            get(details),
        )
        .route(
            "/api/v2/admin/message-statistics/queries/{id}/exports",
            post(export),
        )
}
