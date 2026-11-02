use crate::{
    bootstrap::Application,
    business::{Actor, Result},
    http::jsonapi::{ApiResponse, resource, singleton},
};
use axum::{
    Router,
    extract::{Path, State},
    routing::get,
};
use uuid::Uuid;
async fn templates(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
) -> Result<ApiResponse> {
    Ok(resource(
        "templates",
        a.distribution.templates(&actor, s).await?,
    ))
}
async fn capability(State(a): State<Application>, _actor: Actor) -> ApiResponse {
    singleton(
        "capabilities",
        "current",
        a.integrations
            .capabilities(&a.options.public_origin, a.options.insecure_local_auth),
    )
}
pub fn router() -> Router<Application> {
    Router::new()
        .route("/api/v1/capabilities", get(capability))
        .route("/api/v1/schools/{school}/templates", get(templates))
}

pub mod mobile_delivery;

pub mod device_control;

pub mod mobile_execution;

pub mod shared_send_v2;

pub mod workflows;
