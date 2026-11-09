use crate::{
    advertising::{domain::*, north::local::Service, south::adapter::postgres::Repository},
    bootstrap::Application,
    business::Actor,
    source_access::domain::Fault,
};
use axum::{
    Router,
    extract::{Path, Query, State},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use uuid::Uuid;
#[derive(Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Filter {
    page: Option<i64>,
    filter: Option<String>,
}
async fn run(
    a: Application,
    actor: Actor,
    c: Command,
    kind: &str,
) -> std::result::Result<Response, Response> {
    let v = Service(Repository(a.pool))
        .execute(&actor, c)
        .await
        .map_err(|e| e.response(&actor.request_id))?;
    Ok(management_response(kind, v))
}
fn management_response(kind: &str, v: serde_json::Value) -> Response {
    let id = v
        .get("id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("result")
        .to_owned();
    crate::http::jsonapi::singleton(kind, id, v).into_response()
}
async fn directory(
    State(a): State<Application>,
    actor: Actor,
) -> std::result::Result<Response, Response> {
    run(a, actor, Command::Directory, "advertisement-directory").await
}
async fn list(
    State(a): State<Application>,
    actor: Actor,
    Query(f): Query<Filter>,
) -> std::result::Result<Response, Response> {
    run(
        a,
        actor,
        Command::List {
            page: f.page.unwrap_or(1),
            filter: f.filter.unwrap_or("all".into()),
        },
        "advertisements",
    )
    .await
}
async fn detail(
    State(a): State<Application>,
    actor: Actor,
    Path(id): Path<Uuid>,
) -> std::result::Result<Response, Response> {
    run(a, actor, Command::Detail(id), "advertisements").await
}
async fn versions(
    State(a): State<Application>,
    actor: Actor,
    Path(id): Path<Uuid>,
    Query(f): Query<Filter>,
) -> std::result::Result<Response, Response> {
    run(
        a,
        actor,
        Command::Versions(id, f.page.unwrap_or(1)),
        "advertisement-versions",
    )
    .await
}
async fn managers(
    State(a): State<Application>,
    actor: Actor,
    Path(id): Path<Uuid>,
) -> std::result::Result<Response, Response> {
    run(a, actor, Command::Managers(id), "advertisement-managers").await
}
async fn candidates(
    State(a): State<Application>,
    actor: Actor,
    Path(id): Path<String>,
    Query(f): Query<Filter>,
) -> std::result::Result<Response, Response> {
    run(
        a,
        actor,
        Command::Candidates(id, f.page.unwrap_or(1)),
        "advertisement-candidates",
    )
    .await
}
async fn configuration(
    State(a): State<Application>,
    actor: Actor,
    Path(id): Path<String>,
) -> std::result::Result<Response, Response> {
    run(
        a,
        actor,
        Command::Config(id),
        "advertisement-configurations",
    )
    .await
}
async fn create(
    State(a): State<Application>,
    actor: Actor,
    input: crate::http::jsonapi::Input,
) -> std::result::Result<Response, Response> {
    let v = input
        .decode::<Create>("advertisements")
        .map_err(|_| Fault("invalid_request").response(&actor.request_id))?;
    run(a, actor, Command::Create(v), "advertisements").await
}
async fn content(
    State(a): State<Application>,
    actor: Actor,
    Path(id): Path<Uuid>,
    input: crate::http::jsonapi::Input,
) -> std::result::Result<Response, Response> {
    let v = input
        .decode::<ContentPublication>("advertisement-content-publications")
        .map_err(|_| Fault("invalid_request").response(&actor.request_id))?;
    run(
        a,
        actor,
        Command::Content(id, v),
        "advertisement-content-publications",
    )
    .await
}
async fn policy(
    State(a): State<Application>,
    actor: Actor,
    Path(id): Path<Uuid>,
    input: crate::http::jsonapi::Input,
) -> std::result::Result<Response, Response> {
    let v = input
        .decode::<PolicyPublication>("advertisement-policy-publications")
        .map_err(|_| Fault("invalid_request").response(&actor.request_id))?;
    run(
        a,
        actor,
        Command::Policy(id, v),
        "advertisement-policy-publications",
    )
    .await
}
async fn archive(
    State(a): State<Application>,
    actor: Actor,
    Path(id): Path<Uuid>,
    input: crate::http::jsonapi::Input,
) -> std::result::Result<Response, Response> {
    let v = input
        .decode::<Revision>("advertisement-archives")
        .map_err(|_| Fault("invalid_request").response(&actor.request_id))?;
    run(a, actor, Command::Archive(id, v), "advertisement-archives").await
}
async fn manager(
    State(a): State<Application>,
    actor: Actor,
    Path(id): Path<Uuid>,
    input: crate::http::jsonapi::Input,
) -> std::result::Result<Response, Response> {
    let v = input
        .decode::<Manager>("advertisement-managers")
        .map_err(|_| Fault("invalid_request").response(&actor.request_id))?;
    run(a, actor, Command::Manager(id, v), "advertisement-managers").await
}
async fn configure(
    State(a): State<Application>,
    actor: Actor,
    Path(id): Path<String>,
    input: crate::http::jsonapi::Input,
) -> std::result::Result<Response, Response> {
    let v = input
        .decode::<Configuration>("advertisement-configurations")
        .map_err(|_| Fault("invalid_request").response(&actor.request_id))?;
    run(
        a,
        actor,
        Command::Configure(id, v),
        "advertisement-configurations",
    )
    .await
}
async fn preview(
    State(a): State<Application>,
    actor: Actor,
    Path(id): Path<String>,
    input: crate::http::jsonapi::Input,
) -> std::result::Result<Response, Response> {
    let v = input
        .decode::<Preview>("composition-previews")
        .map_err(|_| Fault("invalid_request").response(&actor.request_id))?;
    run(a, actor, Command::Preview(id, v), "composition-previews").await
}
async fn compositions(
    State(a): State<Application>,
    actor: Actor,
    Path((route, receipt)): Path<(String, Uuid)>,
    Query(f): Query<Filter>,
) -> std::result::Result<Response, Response> {
    run(
        a,
        actor,
        Command::Compositions(route, receipt, f.page.unwrap_or(1)),
        "message-compositions",
    )
    .await
}
async fn pool_detail(
    State(a): State<Application>,
    actor: Actor,
    Path(id): Path<String>,
) -> std::result::Result<Response, Response> {
    run(a, actor, Command::Pool(id), "advertisement-pool").await
}
async fn pool_consumptions(
    State(a): State<Application>,
    actor: Actor,
    Path(id): Path<String>,
    Query(f): Query<Filter>,
) -> std::result::Result<Response, Response> {
    run(
        a,
        actor,
        Command::Consumptions(id, f.page.unwrap_or(1)),
        "advertisement-pool-consumptions",
    )
    .await
}
async fn publish_pool(
    State(a): State<Application>,
    actor: Actor,
    Path(id): Path<String>,
    input: crate::http::jsonapi::Input,
) -> std::result::Result<Response, Response> {
    let v = input
        .decode::<pool::Publication>("advertisement-pool-publications")
        .map_err(|_| Fault("invalid_request").response(&actor.request_id))?;
    run(
        a,
        actor,
        Command::PublishPool(id, v),
        "advertisement-pool-publications",
    )
    .await
}
async fn reset_pool(
    State(a): State<Application>,
    actor: Actor,
    Path(id): Path<String>,
    input: crate::http::jsonapi::Input,
) -> std::result::Result<Response, Response> {
    let v = input
        .decode::<pool::Reset>("advertisement-pool-resets")
        .map_err(|_| Fault("invalid_request").response(&actor.request_id))?;
    run(
        a,
        actor,
        Command::ResetPool(id, v),
        "advertisement-pool-resets",
    )
    .await
}
async fn copy_pool(
    State(a): State<Application>,
    actor: Actor,
    Path(id): Path<String>,
    input: crate::http::jsonapi::Input,
) -> std::result::Result<Response, Response> {
    let v = input
        .decode::<pool::Copy>("advertisement-pool-copies")
        .map_err(|_| Fault("invalid_request").response(&actor.request_id))?;
    run(
        a,
        actor,
        Command::CopyPool(id, v),
        "advertisement-pool-copies",
    )
    .await
}
pub fn router() -> Router<Application> {
    Router::new()
        .route(
            "/api/v2/admin/source-routes/{id}/advertisement-pool",
            get(pool_detail),
        )
        .route(
            "/api/v2/admin/source-routes/{id}/advertisement-pool/publications",
            post(publish_pool),
        )
        .route(
            "/api/v2/admin/source-routes/{id}/advertisement-pool/resets",
            post(reset_pool),
        )
        .route(
            "/api/v2/admin/source-routes/{id}/advertisement-pool/copies",
            post(copy_pool),
        )
        .route(
            "/api/v2/admin/source-routes/{id}/advertisement-pool/consumptions",
            get(pool_consumptions),
        )
        .route("/api/v2/admin/advertisement-directory", get(directory))
        .route("/api/v2/admin/advertisements", get(list).post(create))
        .route("/api/v2/admin/advertisements/{id}", get(detail))
        .route("/api/v2/admin/advertisements/{id}/versions", get(versions))
        .route(
            "/api/v2/admin/advertisements/{id}/content-publications",
            post(content),
        )
        .route(
            "/api/v2/admin/advertisements/{id}/policy-publications",
            post(policy),
        )
        .route("/api/v2/admin/advertisements/{id}/archives", post(archive))
        .route(
            "/api/v2/admin/routing-units/{id}/advertisement-managers",
            get(managers).put(manager),
        )
        .route(
            "/api/v2/admin/source-routes/{id}/advertisement-candidates",
            get(candidates),
        )
        .route(
            "/api/v2/admin/source-routes/{id}/advertisement-configuration",
            get(configuration).put(configure),
        )
        .route(
            "/api/v2/admin/source-routes/{id}/composition-previews",
            post(preview),
        )
        .route(
            "/api/v2/admin/source-routes/{route}/receipts/{receipt}/compositions",
            get(compositions),
        )
        .route(
            "/api/v2/admin/source-routes/{route}/receipts/{receipt}/advertisement-decisions",
            get(compositions),
        )
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn management_wire_identity_is_not_replaced_by_a_singleton_id() {
        let id = uuid::Uuid::new_v4().to_string();
        let response =
            super::management_response("advertisements", serde_json::json!({"id":id,"revision":2}));
        let bytes = axum::body::to_bytes(response.into_body(), 10000)
            .await
            .unwrap();
        let doc: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(doc["data"]["id"], id);
        assert_eq!(doc["data"]["type"], "advertisements");
        assert_eq!(doc["data"]["attributes"]["revision"], 2);
        assert!(doc["data"]["attributes"].get("id").is_none());
    }
}
