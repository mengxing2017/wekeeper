use crate::{
    bootstrap::Application,
    business::{Actor, Named, Result},
    http::jsonapi::{ApiResponse, Input, completed, resource},
};
use axum::{
    Router,
    extract::{Path, State},
    routing::{get, post, put},
};
use serde_json::Value;
use uuid::Uuid;
async fn directory(State(a): State<Application>, _actor: Actor) -> Result<ApiResponse> {
    Ok(resource("schools", a.schools.directory().await?))
}
async fn list(State(a): State<Application>, actor: Actor) -> Result<ApiResponse> {
    Ok(resource("schools", a.schools.list(&actor).await?))
}
async fn create(
    State(a): State<Application>,
    actor: Actor,
    input_document: Input,
) -> Result<ApiResponse> {
    let v = input_document.decode::<Named>("schools")?;
    Ok(resource(
        "schools",
        a.schools.create(&actor, &v.name).await?,
    ))
}
async fn update(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
    input_document: Input,
) -> Result<ApiResponse> {
    let v = input_document.decode::<Value>("schools")?;
    Ok(resource("schools", a.schools.update(&actor, s, v).await?))
}
async fn bind(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
) -> Result<ApiResponse> {
    a.schools.bind(&actor, s).await?;
    Ok(completed())
}
async fn members(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
) -> Result<ApiResponse> {
    Ok(resource("accounts", a.schools.members(&actor, s).await?))
}
async fn grant(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id)): Path<(Uuid, Uuid)>,
    input_document: Input,
) -> Result<ApiResponse> {
    let v = input_document.decode::<Value>("accounts")?;
    a.schools
        .grant(&actor, s, id, v["role"].as_str().unwrap_or(""))
        .await?;
    Ok(completed())
}
async fn resources(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
) -> Result<ApiResponse> {
    Ok(resource("resources", a.schools.resources(&actor, s).await?))
}
async fn save_resource(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
    input_document: Input,
) -> Result<ApiResponse> {
    let v = input_document.decode::<Value>("resources")?;
    Ok(resource(
        "resources",
        a.schools.resource(&actor, s, v).await?,
    ))
}
async fn click(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id)): Path<(Uuid, Uuid)>,
) -> Result<ApiResponse> {
    a.schools.click(&actor, s, id).await?;
    Ok(completed())
}
async fn groups(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
) -> Result<ApiResponse> {
    Ok(resource("groups", a.schools.groups(&actor, s).await?))
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
        a.schools.group(&actor, s, v, &a.integrations).await?,
    ))
}
async fn global_resources(State(a): State<Application>, actor: Actor) -> Result<ApiResponse> {
    Ok(resource(
        "resources",
        a.schools.global_resources(&actor).await?,
    ))
}
async fn global_resource(
    State(a): State<Application>,
    actor: Actor,
    input_document: Input,
) -> Result<ApiResponse> {
    let v = input_document.decode::<Value>("resources")?;
    Ok(resource(
        "resources",
        a.schools.resource(&actor, Uuid::nil(), v).await?,
    ))
}
pub fn router() -> Router<Application> {
    Router::new()
        .route(
            "/api/v1/platform/resources",
            get(global_resources).post(global_resource),
        )
        .route("/api/v1/schools", get(list).post(create))
        .route("/api/v1/school-directory", get(directory))
        .route("/api/v1/schools/{school}", put(update))
        .route("/api/v1/schools/{school}/bind", post(bind))
        .route("/api/v1/schools/{school}/members", get(members))
        .route("/api/v1/schools/{school}/members/{id}", put(grant))
        .route(
            "/api/v1/schools/{school}/resources",
            get(resources).post(save_resource),
        )
        .route("/api/v1/schools/{school}/resources/{id}/click", post(click))
        .route("/api/v1/schools/{school}/groups", get(groups).post(group))
}
