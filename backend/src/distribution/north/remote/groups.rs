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
use serde_json::Value;
use uuid::Uuid;
async fn groups(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
) -> Result<ApiResponse> {
    Ok(resource(
        "groups",
        a.distribution_groups().groups(&actor, s).await?,
    ))
}
async fn group(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
    input_document: Input,
) -> Result<ApiResponse> {
    let v = input_document.decode::<Value>("groups")?;
    Ok(resource(
        "groups",
        a.distribution_groups()
            .group(&actor, s, v, &a.integrations)
            .await?,
    ))
}
pub fn router() -> Router<Application> {
    Router::new().route(
        "/api/v1/wecom-distribution/workspaces/{workspace}/groups",
        get(groups).post(group),
    )
}
