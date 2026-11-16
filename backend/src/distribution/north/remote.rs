use crate::{
    bootstrap::Application,
    business::Actor,
    http::jsonapi::{ApiResponse, singleton},
};
use axum::{Router, extract::State, routing::get};
async fn capability(State(a): State<Application>, _actor: Actor) -> ApiResponse {
    singleton(
        "capabilities",
        "current",
        a.integrations
            .capabilities(&a.options.public_origin, a.options.insecure_local_auth),
    )
}
pub fn router() -> Router<Application> {
    Router::new().route("/api/v1/capabilities", get(capability))
}

pub mod device_registry;

pub mod device_control;

pub mod shared_mobile_resources;

pub mod shared_send_v2;

pub mod workflows;

pub mod groups;
pub mod receiving;
