use crate::{
    bootstrap::Application,
    business::{Actor, Page, Result},
    distribution::domain::broadcast::{Action, DraftInput},
    http::jsonapi::{ApiResponse, Input, collection, resource, singleton},
};
use axum::{
    Router,
    extract::{Path, Query, State},
    http::HeaderMap,
    routing::get,
};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;
async fn list(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
    Query(page): Query<Page>,
) -> Result<ApiResponse> {
    let rows = a.broadcast().list(&actor, s, page.clone()).await?;
    Ok(collection(
        "broadcast-tasks",
        rows.into_iter().map(|v| json!(v)).collect(),
        &page,
    ))
}
async fn create(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
    headers: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    let mut body = input.decode::<DraftInput>("broadcast-tasks")?;
    body.request_key = Some(crate::business::text(
        headers
            .get("idempotency-key")
            .and_then(|v| v.to_str().ok())
            .unwrap_or(""),
        1,
        128,
    )?);
    Ok(resource(
        "broadcast-tasks",
        a.broadcast().save(&actor, s, None, body).await?,
    ))
}
async fn task(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id)): Path<(Uuid, Uuid)>,
) -> Result<ApiResponse> {
    Ok(resource(
        "broadcast-tasks",
        a.broadcast().task(&actor, s, id).await?,
    ))
}
async fn edit(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id)): Path<(Uuid, Uuid)>,
    input: Input,
) -> Result<ApiResponse> {
    Ok(resource(
        "broadcast-tasks",
        a.broadcast()
            .save(
                &actor,
                s,
                Some(id),
                input.decode::<DraftInput>("broadcast-tasks")?,
            )
            .await?,
    ))
}
async fn rows(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id, kind)): Path<(Uuid, Uuid, String)>,
    Query(page): Query<Page>,
) -> Result<ApiResponse> {
    Ok(collection(
        &format!("broadcast-{kind}"),
        a.broadcast()
            .rows(&actor, s, id, &kind, page.clone())
            .await?,
        &page,
    ))
}
async fn action(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id, operation)): Path<(Uuid, Uuid, String)>,
    headers: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    let operation = match operation.as_str() {
        "reminders" => "remind",
        "reconciliations" => "reconcile",
        other => other,
    };
    Ok(resource(
        "broadcast-tasks",
        a.broadcast()
            .action(
                &actor,
                s,
                id,
                operation,
                headers
                    .get("idempotency-key")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or(""),
                input.decode::<Action>("broadcast-tasks")?,
            )
            .await?,
    ))
}
async fn capability(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
) -> Result<ApiResponse> {
    let value = crate::distribution::north::local::workflows::Service(
        crate::distribution::south::adapter::postgres::workflows::Repository(a.pool.clone()),
    )
    .channel(
        &actor,
        s,
        a.integrations.server_configured(),
        &a.integrations.credential_scope(),
    )
    .await?;
    Ok(resource("broadcast-capabilities", value))
}
async fn preview(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id)): Path<(Uuid, Uuid)>,
) -> Result<ApiResponse> {
    Ok(resource(
        "broadcast-previews",
        a.broadcast().preview(&actor, s, id).await?,
    ))
}
async fn school_senders(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
    Query(page): Query<Page>,
) -> Result<ApiResponse> {
    let rows = crate::distribution::north::local::governance::Service(a.pool.clone())
        .senders(&actor, Some(s), page.clone())
        .await?;
    Ok(collection(
        "wecom-senders",
        rows.into_iter().map(|x| json!(x)).collect(),
        &page,
    ))
}
async fn platform_senders(
    State(a): State<Application>,
    actor: Actor,
    Query(page): Query<Page>,
) -> Result<ApiResponse> {
    let rows = crate::distribution::north::local::governance::Service(a.pool.clone())
        .senders(&actor, None, page.clone())
        .await?;
    Ok(collection(
        "wecom-senders",
        rows.into_iter().map(|x| json!(x)).collect(),
        &page,
    ))
}
async fn qualify(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id)): Path<(Uuid, Uuid)>,
    input: Input,
) -> Result<ApiResponse> {
    let body = input.decode::<crate::distribution::domain::content::SenderQualificationInput>(
        "wecom-senders",
    )?;
    crate::distribution::north::local::governance::Service(a.pool.clone())
        .qualify(&actor, s, id, body)
        .await?;
    Ok(singleton(
        "wecom-sender-qualifications",
        id,
        json!({"updated":true}),
    ))
}

async fn policies(State(a): State<Application>, actor: Actor) -> Result<ApiResponse> {
    Ok(singleton(
        "distribution-policies",
        "current",
        crate::distribution::north::local::governance::Service(a.pool.clone())
            .policies(&actor)
            .await?,
    ))
}
async fn source_policy(
    State(a): State<Application>,
    actor: Actor,
    input: Input,
) -> Result<ApiResponse> {
    let policy = input.decode::<crate::distribution::domain::resource_policy::SourcePolicy>(
        "distribution-source-policies",
    )?;
    crate::distribution::north::local::governance::Service(a.pool.clone())
        .source_policy(&actor, policy)
        .await?;
    Ok(singleton(
        "distribution-policy-updates",
        "current",
        json!({"updated":true}),
    ))
}
async fn save_preview(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
    headers: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    let mut v = input.decode::<serde_json::Value>("broadcast-task-previews")?;
    let id = match v.as_object_mut().and_then(|v| v.remove("id")) {
        None | Some(serde_json::Value::Null) => None,
        Some(v) => Some(
            v.as_str()
                .and_then(|s| s.parse::<Uuid>().ok())
                .ok_or_else(|| crate::business::Error::Invalid("草稿引用无效".into()))?,
        ),
    };
    let input: DraftInput = serde_json::from_value(v)
        .map_err(|_| crate::business::Error::Invalid("草稿字段无效".into()))?;
    Ok(resource(
        "broadcast-task-previews",
        a.broadcast()
            .save_preview(
                &actor,
                s,
                id,
                input,
                headers
                    .get("idempotency-key")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or(""),
            )
            .await?,
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EvidenceQuery {
    batch_id: Uuid,
}
async fn evidence_candidates(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id)): Path<(Uuid, Uuid)>,
    Query(q): Query<EvidenceQuery>,
) -> Result<ApiResponse> {
    Ok(resource(
        "broadcast-evidence-candidates",
        a.broadcast()
            .evidence_candidates(&actor, s, id, q.batch_id)
            .await?,
    ))
}
pub fn router() -> Router<Application> {
    Router::new()
        .route(
            "/api/v1/wecom-distribution/workspaces/{workspace}/broadcast-task-previews",
            axum::routing::post(save_preview),
        )
        .route(
            "/api/v1/wecom-distribution/workspaces/{workspace}/broadcast-tasks/{id}/evidence-candidates",
            get(evidence_candidates),
        )
        .route("/api/v1/platform/distribution-policies", get(policies))
        .route(
            "/api/v1/platform/distribution-source-policies",
            axum::routing::post(source_policy),
        )
        .route(
            "/api/v1/wecom-distribution/workspaces/{workspace}/wecom-senders",
            get(school_senders),
        )
        .route("/api/v1/platform/wecom-senders", get(platform_senders))
        .route(
            "/api/v1/platform/wecom-distribution/workspaces/{workspace}/wecom-senders/{id}/qualification",
            axum::routing::post(qualify),
        )
        .route(
            "/api/v1/wecom-distribution/workspaces/{workspace}/broadcast-tasks/{id}/preview",
            axum::routing::post(preview),
        )
        .route(
            "/api/v1/wecom-distribution/workspaces/{workspace}/broadcast-tasks",
            get(list).post(create),
        )
        .route(
            "/api/v1/wecom-distribution/workspaces/{workspace}/broadcast-capabilities",
            get(capability),
        )
        .route(
            "/api/v1/wecom-distribution/workspaces/{workspace}/broadcast-tasks/{id}",
            get(task).patch(edit),
        )
        .route(
            "/api/v1/wecom-distribution/workspaces/{workspace}/broadcast-tasks/{id}/{operation}",
            get(rows).post(action),
        )
}
