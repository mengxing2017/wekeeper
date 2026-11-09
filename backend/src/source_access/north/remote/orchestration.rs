use super::*;
use crate::source_access::{
    domain::orchestration::{Command, Control, Filter, PolicyInput},
    north::local::orchestration::Service,
    south::adapter::postgres::orchestration::Repository,
};
async fn execute(
    a: Application,
    actor: Actor,
    c: Command,
) -> std::result::Result<Response, Response> {
    let value = Service(Repository(a.pool))
        .execute(&actor, c)
        .await
        .map_err(|e| e.response(&actor.request_id))?;
    Ok(crate::http::jsonapi::singleton("source-automation-tasks", "result", value).into_response())
}
async fn list(
    State(a): State<Application>,
    actor: Actor,
    Query(f): Query<Filter>,
) -> std::result::Result<Response, Response> {
    execute(a, actor, Command::List(f)).await
}
async fn detail(
    State(a): State<Application>,
    actor: Actor,
    Path(id): Path<uuid::Uuid>,
) -> std::result::Result<Response, Response> {
    execute(a, actor, Command::Detail(id)).await
}
#[derive(Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    after: Option<i64>,
}
async fn events(
    State(a): State<Application>,
    actor: Actor,
    Path(id): Path<uuid::Uuid>,
    Query(p): Query<Cursor>,
) -> std::result::Result<Response, Response> {
    execute(a, actor, Command::Events(id, p.after.unwrap_or(0))).await
}
async fn targets(
    State(a): State<Application>,
    actor: Actor,
    Path(id): Path<uuid::Uuid>,
) -> std::result::Result<Response, Response> {
    execute(a, actor, Command::Targets(id)).await
}
async fn attempts(
    State(a): State<Application>,
    actor: Actor,
    Path((id, target)): Path<(uuid::Uuid, uuid::Uuid)>,
) -> std::result::Result<Response, Response> {
    execute(a, actor, Command::Attempts(id, target)).await
}
async fn stop(
    State(a): State<Application>,
    actor: Actor,
    Path(id): Path<uuid::Uuid>,
    input: crate::http::jsonapi::Input,
) -> std::result::Result<Response, Response> {
    let c = input
        .decode::<Control>("source-automation-task-stops")
        .map_err(|_| Fault("invalid_request").response(&actor.request_id))?;
    execute(a, actor, Command::Stop(id, c)).await
}
async fn retry(
    State(a): State<Application>,
    actor: Actor,
    Path(id): Path<uuid::Uuid>,
    input: crate::http::jsonapi::Input,
) -> std::result::Result<Response, Response> {
    let c = input
        .decode::<Control>("source-automation-task-retry-requests")
        .map_err(|_| Fault("invalid_request").response(&actor.request_id))?;
    execute(a, actor, Command::Retry(id, c)).await
}
async fn policy(
    State(a): State<Application>,
    actor: Actor,
    Path(route): Path<String>,
) -> std::result::Result<Response, Response> {
    execute(a, actor, Command::Policy(route, None)).await
}
async fn save_policy(
    State(a): State<Application>,
    actor: Actor,
    Path(route): Path<String>,
    input: crate::http::jsonapi::Input,
) -> std::result::Result<Response, Response> {
    let p = input
        .decode::<PolicyInput>("source-route-version-policies")
        .map_err(|_| Fault("invalid_request").response(&actor.request_id))?;
    execute(a, actor, Command::Policy(route, Some(p))).await
}
pub fn router() -> Router<Application> {
    Router::new()
        .route("/api/v2/admin/source-automation-tasks", get(list))
        .route("/api/v2/admin/source-automation-tasks/{id}", get(detail))
        .route(
            "/api/v2/admin/source-automation-tasks/{id}/events",
            get(events),
        )
        .route(
            "/api/v2/admin/source-automation-tasks/{id}/targets",
            get(targets),
        )
        .route(
            "/api/v2/admin/source-automation-tasks/{id}/targets/{target}/attempts",
            get(attempts),
        )
        .route(
            "/api/v2/admin/source-automation-tasks/{id}/stop",
            post(stop),
        )
        .route(
            "/api/v2/admin/source-automation-tasks/{id}/retry-requests",
            post(retry),
        )
        .route(
            "/api/v2/admin/source-routes/{route}/version-policy",
            get(policy).put(save_policy),
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;
    #[tokio::test]
    async fn task_reads_and_control_require_an_authenticated_session() {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://test:test@127.0.0.1:1/test")
            .unwrap();
        let app = router().with_state(crate::bootstrap::build_application(pool));
        for (method, path) in [
            ("GET", "/api/v2/admin/source-automation-tasks"),
            (
                "GET",
                "/api/v2/admin/source-automation-tasks/00000000-0000-0000-0000-000000000001",
            ),
            (
                "POST",
                "/api/v2/admin/source-automation-tasks/00000000-0000-0000-0000-000000000001/stop",
            ),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(path)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), axum::http::StatusCode::UNAUTHORIZED);
        }
    }
}
