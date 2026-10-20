use super::local::{AttemptInput, DraftInput};
use crate::{
    bootstrap::Application,
    business::{Actor, Named, Result, Revision},
    http::error::data,
};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::HeaderMap,
    routing::{get, post, put},
};
use serde_json::{Value, json};
use uuid::Uuid;
async fn templates(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
) -> Result<Json<Value>> {
    Ok(data(a.distribution.templates(&actor, s).await?))
}
async fn template(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
    Json(v): Json<Named>,
) -> Result<Json<Value>> {
    Ok(data(a.distribution.template(&actor, s, &v.name).await?))
}
async fn version(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id)): Path<(Uuid, Uuid)>,
    Json(v): Json<Value>,
) -> Result<Json<Value>> {
    Ok(data(
        a.distribution
            .version(&actor, s, id, v["body"].as_str().unwrap_or(""))
            .await?,
    ))
}
async fn publish(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id)): Path<(Uuid, Uuid)>,
    Json(v): Json<Revision>,
) -> Result<Json<Value>> {
    Ok(data(
        a.distribution.publish(&actor, s, id, v.revision).await?,
    ))
}
async fn drafts(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
) -> Result<Json<Value>> {
    Ok(data(a.distribution.drafts(&actor, s).await?))
}
async fn draft(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
    Json(v): Json<DraftInput>,
) -> Result<Json<Value>> {
    Ok(data(
        a.distribution
            .draft(&actor, s, v, &a.options.public_origin)
            .await?,
    ))
}
async fn attempt(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
    Json(v): Json<AttemptInput>,
) -> Result<Json<Value>> {
    let key = headers
        .get("idempotency-key")
        .and_then(|s| s.to_str().ok())
        .unwrap_or("");
    Ok(data(
        a.distribution
            .prepare(&actor, s, id, key, v, &a.integrations)
            .await?,
    ))
}
async fn attempts(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
) -> Result<Json<Value>> {
    Ok(data(a.distribution.attempts(&actor, s).await?))
}
async fn claim(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>> {
    a.distribution.claim(&actor, s, id).await?;
    Ok(data(json!({"ok":true})))
}
async fn result(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id)): Path<(Uuid, Uuid)>,
    Json(v): Json<Value>,
) -> Result<Json<Value>> {
    Ok(data(a.distribution.result(&actor, s, id, v).await?))
}
async fn reconcile(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id)): Path<(Uuid, Uuid)>,
    Json(v): Json<Value>,
) -> Result<Json<Value>> {
    Ok(data(a.distribution.reconcile(&actor, s, id, v).await?))
}
async fn capability(State(a): State<Application>, _actor: Actor) -> Json<Value> {
    data(json!({"wecom_configured":a.integrations.configured()}))
}
pub fn router() -> Router<Application> {
    Router::new()
        .route(
            "/api/v1/schools/{school}/delivery-attempts/{id}/invocation",
            post(claim),
        )
        .route("/api/v1/capabilities", get(capability))
        .route(
            "/api/v1/schools/{school}/templates",
            get(templates).post(template),
        )
        .route(
            "/api/v1/schools/{school}/templates/{id}/versions",
            post(version),
        )
        .route(
            "/api/v1/schools/{school}/template-versions/{id}/publish",
            post(publish),
        )
        .route(
            "/api/v1/schools/{school}/delivery-drafts",
            get(drafts).post(draft),
        )
        .route(
            "/api/v1/schools/{school}/delivery-drafts/{id}/attempts",
            post(attempt),
        )
        .route("/api/v1/schools/{school}/delivery-attempts", get(attempts))
        .route(
            "/api/v1/schools/{school}/delivery-attempts/{id}/client-result",
            put(result),
        )
        .route(
            "/api/v1/schools/{school}/delivery-attempts/{id}/reconciliations",
            post(reconcile),
        )
}
