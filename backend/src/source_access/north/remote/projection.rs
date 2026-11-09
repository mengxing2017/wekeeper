use super::*;
use crate::source_access::{
    domain::projection::{Command, RecheckInput},
    north::local::projection::Service as ProjectionService,
    south::adapter::postgres::projection::ReceiptsRepository,
};
#[derive(Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Page {
    page: Option<i64>,
}
async fn execute(
    a: Application,
    actor: Actor,
    command: Command,
    kind: &str,
) -> std::result::Result<Response, Response> {
    let value = ProjectionService(ReceiptsRepository(a.pool))
        .execute(&actor, command)
        .await
        .map_err(|e| e.response(&actor.request_id))?;
    Ok(crate::http::jsonapi::singleton(kind, "result", value).into_response())
}
async fn list(
    State(a): State<Application>,
    actor: Actor,
    Path(route): Path<String>,
    Query(page): Query<Page>,
) -> std::result::Result<Response, Response> {
    execute(
        a,
        actor,
        Command::List {
            route,
            page: page.page.unwrap_or(1),
        },
        "source-receipt-lists",
    )
    .await
}
async fn detail(
    State(a): State<Application>,
    actor: Actor,
    Path((route, receipt)): Path<(String, uuid::Uuid)>,
) -> std::result::Result<Response, Response> {
    execute(
        a,
        actor,
        Command::Detail { route, receipt },
        "source-receipt-details",
    )
    .await
}
async fn recheck(
    State(a): State<Application>,
    actor: Actor,
    Path((route, receipt)): Path<(String, uuid::Uuid)>,
    input: crate::http::jsonapi::Input,
) -> std::result::Result<Response, Response> {
    let input = input
        .decode::<RecheckInput>("source-receipt-rechecks")
        .map_err(|_| Fault("invalid_request").response(&actor.request_id))?;
    execute(
        a,
        actor,
        Command::Recheck {
            route,
            receipt,
            input,
        },
        "source-receipt-rechecks",
    )
    .await
}
pub fn router() -> Router<Application> {
    Router::new()
        .route("/api/v2/admin/source-routes/{route}/receipts", get(list))
        .route(
            "/api/v2/admin/source-routes/{route}/receipts/{receipt}",
            get(detail),
        )
        .route(
            "/api/v2/admin/source-routes/{route}/receipts/{receipt}/rechecks",
            post(recheck),
        )
}
