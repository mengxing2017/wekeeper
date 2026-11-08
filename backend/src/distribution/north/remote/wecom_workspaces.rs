use crate::{
    bootstrap::Application,
    business::{Actor, Result},
    http::jsonapi::{ApiResponse, Input, resource},
};
use axum::{
    Router,
    extract::{Path, State},
    routing::get,
};
use serde::Deserialize;
use uuid::Uuid;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkspaceInput {
    name: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GrantInput {
    username: String,
    role: String,
}
async fn list(State(a): State<Application>, actor: Actor) -> Result<ApiResponse> {
    Ok(resource(
        "wecom-workspaces",
        a.wecom_workspaces().list(&actor).await?,
    ))
}
async fn save(
    State(a): State<Application>,
    actor: Actor,
    Path(id): Path<Uuid>,
    input: Input,
) -> Result<ApiResponse> {
    actor.platform()?;
    let v: WorkspaceInput = input.decode("wecom-workspaces")?;
    Ok(resource(
        "wecom-workspaces",
        a.wecom_workspaces().save(&actor, id, &v.name).await?,
    ))
}
async fn members(
    State(a): State<Application>,
    actor: Actor,
    Path(id): Path<Uuid>,
) -> Result<ApiResponse> {
    actor.platform()?;
    Ok(resource(
        "wecom-workspace-members",
        a.wecom_workspaces().members(&actor, id).await?,
    ))
}
async fn grant(
    State(a): State<Application>,
    actor: Actor,
    Path(id): Path<Uuid>,
    input: Input,
) -> Result<ApiResponse> {
    actor.platform()?;
    let v: GrantInput = input.decode("wecom-workspace-members")?;
    Ok(resource(
        "wecom-workspace-members",
        a.wecom_workspaces()
            .grant(&actor, id, &v.username, &v.role)
            .await?,
    ))
}
pub fn router() -> Router<Application> {
    Router::new()
        .route("/api/v1/wecom-distribution/workspaces", get(list))
        .route(
            "/api/v1/wecom-distribution/workspaces/{workspace}",
            axum::routing::put(save),
        )
        .route(
            "/api/v1/wecom-distribution/workspaces/{workspace}/members",
            get(members).put(grant),
        )
}
