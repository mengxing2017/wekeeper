use super::local::{AttemptInput, DraftInput};
use crate::{
    bootstrap::Application,
    business::{Actor, Named, Result, Revision},
    http::jsonapi::{ApiResponse, Input, collection, completed, resource, singleton},
};
use axum::{
    Router,
    extract::{Path, Query, State},
    http::HeaderMap,
    routing::{get, post, put},
};
use serde_json::{Value, json};
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
async fn template(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
    input_document: Input,
) -> Result<ApiResponse> {
    let v = input_document.decode::<Named>("templates")?;
    Ok(resource(
        "templates",
        a.distribution.template(&actor, s, &v.name).await?,
    ))
}
async fn version(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id)): Path<(Uuid, Uuid)>,
    input_document: Input,
) -> Result<ApiResponse> {
    let v = input_document.decode::<Value>("template-versions")?;
    Ok(resource(
        "template-versions",
        a.distribution
            .version(&actor, s, id, v["body"].as_str().unwrap_or(""))
            .await?,
    ))
}
async fn publish(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id)): Path<(Uuid, Uuid)>,
    input_document: Input,
) -> Result<ApiResponse> {
    let v = input_document.decode::<Revision>("template-versions")?;
    Ok(resource(
        "template-versions",
        a.distribution.publish(&actor, s, id, v.revision).await?,
    ))
}
async fn drafts(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
    Query(page): Query<crate::business::Page>,
) -> Result<ApiResponse> {
    Ok(collection(
        "delivery-drafts",
        a.distribution.drafts(&actor, s, page.clone()).await?,
        &page,
    ))
}
async fn get_draft(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id)): Path<(Uuid, Uuid)>,
) -> Result<ApiResponse> {
    Ok(resource(
        "delivery-drafts",
        a.distribution.get_draft(&actor, s, id).await?,
    ))
}
async fn related(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id)): Path<(Uuid, Uuid)>,
    Query(page): Query<crate::business::Page>,
) -> Result<ApiResponse> {
    let history = a.distribution.related(&actor, s, id, page.clone()).await?;
    Ok(collection("delivery-attempts", history.items, &page)
        .meta(json!({"latest_attempt_id":history.latest_attempt_id,"blocked":history.blocked})))
}
async fn draft(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
    input_document: Input,
) -> Result<ApiResponse> {
    let v = input_document.decode::<DraftInput>("delivery-drafts")?;
    Ok(resource(
        "delivery-drafts",
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
    input_document: Input,
) -> Result<ApiResponse> {
    crate::integrations::north::remote::require_share(&a, &actor)?;
    let v = input_document.decode::<AttemptInput>("delivery-attempts")?;
    let key = headers
        .get("idempotency-key")
        .and_then(|s| s.to_str().ok())
        .unwrap_or("");
    Ok(resource(
        "delivery-attempts",
        a.distribution
            .prepare(&actor, s, id, key, v, &a.integrations)
            .await?,
    ))
}
async fn attempts(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
    Query(page): Query<crate::business::Page>,
) -> Result<ApiResponse> {
    Ok(collection(
        "delivery-attempts",
        a.distribution.attempts(&actor, s, page.clone()).await?,
        &page,
    ))
}
async fn claim(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id)): Path<(Uuid, Uuid)>,
) -> Result<ApiResponse> {
    crate::integrations::north::remote::require_share(&a, &actor)?;
    a.distribution.claim(&actor, s, id).await?;
    Ok(completed())
}
async fn result(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id)): Path<(Uuid, Uuid)>,
    input_document: Input,
) -> Result<ApiResponse> {
    let v = input_document.decode::<Value>("delivery-attempts")?;
    Ok(resource(
        "delivery-attempts",
        a.distribution.result(&actor, s, id, v).await?,
    ))
}
async fn reconcile(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id)): Path<(Uuid, Uuid)>,
    input_document: Input,
) -> Result<ApiResponse> {
    let v = input_document.decode::<Value>("delivery-attempts")?;
    Ok(resource(
        "delivery-attempts",
        a.distribution.reconcile(&actor, s, id, v).await?,
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
        .route(
            "/api/v1/schools/{school}/delivery-drafts/{id}",
            get(get_draft),
        )
        .route(
            "/api/v1/schools/{school}/delivery-drafts/{id}/history",
            get(related),
        )
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
