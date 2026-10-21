use crate::{
    bootstrap::Application,
    business::{Actor, Named, Result},
    http::error::data,
};
use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post, put},
};
use serde_json::{Value, json};
use uuid::Uuid;
async fn directory(State(a): State<Application>, _actor: Actor) -> Result<Json<Value>> {
    Ok(data(a.schools.directory().await?))
}
async fn list(State(a): State<Application>, actor: Actor) -> Result<Json<Value>> {
    Ok(data(a.schools.list(&actor).await?))
}
async fn create(
    State(a): State<Application>,
    actor: Actor,
    Json(v): Json<Named>,
) -> Result<Json<Value>> {
    Ok(data(a.schools.create(&actor, &v.name).await?))
}
async fn update(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
    Json(v): Json<Value>,
) -> Result<Json<Value>> {
    Ok(data(a.schools.update(&actor, s, v).await?))
}
async fn bind(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
) -> Result<Json<Value>> {
    a.schools.bind(&actor, s).await?;
    Ok(data(json!({"ok":true})))
}
async fn members(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
) -> Result<Json<Value>> {
    Ok(data(a.schools.members(&actor, s).await?))
}
async fn grant(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id)): Path<(Uuid, Uuid)>,
    Json(v): Json<Value>,
) -> Result<Json<Value>> {
    a.schools
        .grant(&actor, s, id, v["role"].as_str().unwrap_or(""))
        .await?;
    Ok(data(json!({"ok":true})))
}
async fn resources(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
) -> Result<Json<Value>> {
    Ok(data(a.schools.resources(&actor, s).await?))
}
async fn resource(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
    Json(v): Json<Value>,
) -> Result<Json<Value>> {
    Ok(data(a.schools.resource(&actor, s, v).await?))
}
async fn click(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>> {
    a.schools.click(&actor, s, id).await?;
    Ok(data(json!({"ok":true})))
}
async fn groups(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
) -> Result<Json<Value>> {
    Ok(data(a.schools.groups(&actor, s).await?))
}
async fn group(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
    Json(v): Json<Value>,
) -> Result<Json<Value>> {
    Ok(data(a.schools.group(&actor, s, v, &a.integrations).await?))
}
async fn global_resources(State(a): State<Application>, actor: Actor) -> Result<Json<Value>> {
    Ok(data(a.schools.global_resources(&actor).await?))
}
async fn global_resource(
    State(a): State<Application>,
    actor: Actor,
    Json(v): Json<Value>,
) -> Result<Json<Value>> {
    Ok(data(a.schools.resource(&actor, Uuid::nil(), v).await?))
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
            get(resources).post(resource),
        )
        .route("/api/v1/schools/{school}/resources/{id}/click", post(click))
        .route("/api/v1/schools/{school}/groups", get(groups).post(group))
}
