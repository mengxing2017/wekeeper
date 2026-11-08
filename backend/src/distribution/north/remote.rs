use crate::{
    bootstrap::Application,
    business::{Actor, Page, Result},
    http::jsonapi::{ApiResponse, collection, resource, singleton},
};
use axum::{
    Router,
    extract::{Path, Query, State},
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
async fn materials(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
    Query(page): Query<Page>,
) -> Result<ApiResponse> {
    Ok(collection(
        "distribution-materials",
        a.distribution.materials(&actor, s, page.clone()).await?,
        &page,
    ))
}
pub fn router() -> Router<Application> {
    Router::new()
        .route("/api/v1/capabilities", get(capability))
        .route(
            "/api/v1/wecom-distribution/workspaces/{workspace}/distribution-materials",
            get(materials),
        )
        .route(
            "/api/v1/wecom-distribution/workspaces/{workspace}/templates",
            get(templates),
        )
}

pub mod device_registry;

pub mod device_control;

pub mod shared_mobile_resources;

pub mod shared_send_v2;

pub mod workflows;

pub mod mobile_workspaces;

pub mod wecom_workspaces;

pub mod groups;
