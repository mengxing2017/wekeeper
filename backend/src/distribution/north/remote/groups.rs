use crate::{
    bootstrap::Application,
    business::{Actor, Result},
    http::jsonapi::{ApiResponse, Input, resource},
};
use axum::{Router, extract::State, routing::get};
use serde_json::Value;
async fn groups(State(a): State<Application>, actor: Actor) -> Result<ApiResponse> {
    Ok(resource(
        "groups",
        a.distribution_groups().groups(&actor).await?,
    ))
}
async fn group(
    State(a): State<Application>,
    actor: Actor,
    input_document: Input,
) -> Result<ApiResponse> {
    let v = input_document.decode::<Value>("groups")?;
    Ok(resource(
        "groups",
        a.distribution_groups()
            .group(&actor, v, &a.integrations)
            .await?,
    ))
}
pub fn router() -> Router<Application> {
    Router::new().route("/api/v1/wecom-distribution/groups", get(groups).post(group))
}
