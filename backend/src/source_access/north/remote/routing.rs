use super::*;
use crate::source_access::{
    domain::routing::*, north::local::routing::Service,
    south::adapter::postgres::routing::RoutingRepository,
};
async fn run(
    a: Application,
    actor: Actor,
    c: Command,
    kind: &str,
) -> std::result::Result<Response, Response> {
    let v = Service(RoutingRepository(a.pool))
        .execute(&actor, c)
        .await
        .map_err(|e| e.response(&actor.request_id))?;
    Ok(crate::http::jsonapi::singleton(kind, "result", v).into_response())
}
async fn assigned(
    State(a): State<Application>,
    actor: Actor,
) -> std::result::Result<Response, Response> {
    run(a, actor, Command::Assigned, "source-route-assignments").await
}
async fn configuration(
    State(a): State<Application>,
    actor: Actor,
    Path(route): Path<String>,
) -> std::result::Result<Response, Response> {
    run(
        a,
        actor,
        Command::Configuration { route },
        "source-route-configurations",
    )
    .await
}
pub fn router() -> Router<Application> {
    Router::new()
        .route("/api/v2/admin/source-route-assignments", get(assigned))
        .route(
            "/api/v2/admin/source-routes/{route}/configuration",
            get(configuration),
        )
}
