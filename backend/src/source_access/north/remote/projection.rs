use super::*;
use crate::source_access::{
    domain::projection::Command, north::local::projection::Service as ProjectionService,
    south::adapter::postgres::projection::ReceiptsRepository,
};
type Page = crate::business::Pagination;
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
            page: page.page(),
        },
        "source-receipt-lists",
    )
    .await
}
pub fn router() -> Router<Application> {
    Router::new().route("/api/v2/admin/source-routes/{route}/receipts", get(list))
}
