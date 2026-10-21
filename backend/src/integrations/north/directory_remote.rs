use crate::{
    bootstrap::Application,
    business::{Actor, Error, Result},
    http::jsonapi::{ApiResponse, Input, collection, completed, resource, singleton},
    integrations::domain::directory::{BindingInput, CredentialRotation, DirectoryQuery},
};
use axum::{
    Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, put},
};
use uuid::Uuid;
async fn status(State(a): State<Application>, actor: Actor) -> Result<ApiResponse> {
    Ok(singleton(
        "wecom-directory-status",
        "current",
        a.integrations.directory().status(&actor).await?,
    ))
}
async fn rotate(State(a): State<Application>, actor: Actor, input: Input) -> Result<ApiResponse> {
    a.integrations
        .directory()
        .rotate(
            &actor,
            input.decode::<CredentialRotation>("wecom-credential-rotations")?,
        )
        .await?;
    Ok(completed())
}
async fn members(
    State(a): State<Application>,
    actor: Actor,
    Query(q): Query<DirectoryQuery>,
) -> Result<ApiResponse> {
    Ok(collection(
        "wecom-directory-members",
        a.integrations.directory().list(&actor, &q, false).await?,
        &q.pagination(),
    ))
}
async fn departments(
    State(a): State<Application>,
    actor: Actor,
    Query(q): Query<DirectoryQuery>,
) -> Result<ApiResponse> {
    Ok(collection(
        "wecom-directory-departments",
        a.integrations.directory().list(&actor, &q, true).await?,
        &q.pagination(),
    ))
}
async fn sync(State(a): State<Application>, actor: Actor) -> Result<Response> {
    Ok((
        StatusCode::ACCEPTED,
        resource(
            "wecom-directory-sync-runs",
            a.integrations.directory().enqueue(&actor).await?,
        ),
    )
        .into_response())
}
async fn job(
    State(a): State<Application>,
    actor: Actor,
    Path(id): Path<Uuid>,
) -> Result<ApiResponse> {
    Ok(resource(
        "wecom-directory-sync-runs",
        a.integrations.directory().job(&actor, id).await?,
    ))
}
async fn bind(
    State(a): State<Application>,
    actor: Actor,
    Path(id): Path<Uuid>,
    input: Input,
) -> Result<ApiResponse> {
    a.integrations
        .directory()
        .bind(&actor, id, input.decode::<BindingInput>("wecom-bindings")?)
        .await?;
    Ok(completed())
}
async fn unbind(
    State(a): State<Application>,
    actor: Actor,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<ApiResponse> {
    let revision = headers
        .get("if-match")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix('"'))
        .and_then(|v| v.strip_suffix('"'))
        .and_then(|v| v.parse::<i64>().ok())
        .ok_or_else(|| Error::Invalid("解绑需要 If-Match 绑定版本".into()))?;
    a.integrations
        .directory()
        .unbind(&actor, id, revision)
        .await?;
    Ok(completed())
}
pub fn router() -> Router<Application> {
    Router::new()
        .route("/api/v1/platform/wecom-directory/status", get(status))
        .route(
            "/api/v1/platform/wecom-directory/credential-rotations",
            axum::routing::post(rotate),
        )
        .route("/api/v1/platform/wecom-directory/members", get(members))
        .route(
            "/api/v1/platform/wecom-directory/departments",
            get(departments),
        )
        .route(
            "/api/v1/platform/wecom-directory/sync-runs",
            axum::routing::post(sync),
        )
        .route("/api/v1/platform/wecom-directory/sync-runs/{id}", get(job))
        .route(
            "/api/v1/accounts/{id}/wecom-binding",
            put(bind).delete(unbind),
        )
}
