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
    input: Input,
) -> Result<ApiResponse> {
    Ok(resource(
        "broadcast-tasks",
        a.broadcast()
            .save(
                &actor,
                s,
                None,
                input.decode::<DraftInput>("broadcast-tasks")?,
            )
            .await?,
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
    use crate::distribution::south::port::broadcast::Store;
    let b = a.broadcast();
    b.store.authorize(actor.id, s, false).await?;
    Ok(singleton(
        "broadcast-capabilities",
        "current",
        json!({"broadcast_available":b.enabled,"management_available":b.management_available,"reason":if b.enabled{None}else{Some("需配置自建应用权限、HTTPS 部署并显式启用群发（成员终端须至少 4.1.10）")}}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Window {
    start_time: i64,
    end_time: i64,
}
async fn candidates(
    State(a): State<Application>,
    actor: Actor,
    Query(w): Query<Window>,
) -> Result<ApiResponse> {
    let values = a
        .broadcast()
        .candidates(&actor, w.start_time, w.end_time)
        .await?
        .into_iter()
        .filter_map(|mut v| {
            let id = v["msgid"].as_str()?.to_owned();
            v["id"] = id.into();
            Some(v)
        })
        .collect::<Vec<_>>();
    Ok(resource("broadcast-candidates", values))
}
pub fn router() -> Router<Application> {
    Router::new()
        .route(
            "/api/v1/schools/{school}/broadcast-tasks",
            get(list).post(create),
        )
        .route(
            "/api/v1/schools/{school}/broadcast-capabilities",
            get(capability),
        )
        .route(
            "/api/v1/schools/{school}/broadcast-tasks/{id}",
            get(task).patch(edit),
        )
        .route(
            "/api/v1/schools/{school}/broadcast-tasks/{id}/{operation}",
            get(rows).post(action),
        )
        .route("/api/v1/platform/broadcast-candidates", get(candidates))
}
