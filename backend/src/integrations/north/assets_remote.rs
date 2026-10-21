use crate::{
    bootstrap::Application,
    business::{Actor, Error, Result},
    http::jsonapi::{ApiResponse, Input, collection, resource, singleton},
    integrations::domain::assets::*,
};
use axum::{
    Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use uuid::Uuid;
async fn status(State(a): State<Application>, actor: Actor) -> Result<ApiResponse> {
    Ok(singleton(
        "wecom-asset-status",
        "current",
        a.integrations.assets().status(&actor).await?,
    ))
}
async fn sync(State(a): State<Application>, actor: Actor, input: Input) -> Result<Response> {
    Ok((
        StatusCode::ACCEPTED,
        resource(
            "wecom-asset-sync-runs",
            a.integrations
                .assets()
                .enqueue(&actor, input.decode::<SyncInput>("wecom-asset-sync-runs")?)
                .await?,
        ),
    )
        .into_response())
}
async fn list(
    State(a): State<Application>,
    actor: Actor,
    Path(kind): Path<String>,
    Query(q): Query<AssetQuery>,
) -> Result<ApiResponse> {
    if !["groups", "member-statistics", "sync-runs"].contains(&kind.as_str()) {
        return Err(Error::NotFound);
    }
    Ok(collection(
        &format!("wecom-asset-{kind}"),
        a.integrations
            .assets()
            .list(&actor, &kind, &q, None, None)
            .await?,
        &q.pagination(),
    ))
}
async fn detail(
    State(a): State<Application>,
    actor: Actor,
    Path((kind, id)): Path<(String, Uuid)>,
) -> Result<ApiResponse> {
    if !["groups", "sync-runs"].contains(&kind.as_str()) {
        return Err(Error::NotFound);
    }
    let rows = a
        .integrations
        .assets()
        .list(&actor, &kind, &AssetQuery::default(), None, Some(id))
        .await?;
    Ok(resource(
        &format!("wecom-asset-{kind}"),
        rows.into_iter().next().ok_or(Error::NotFound)?,
    ))
}
async fn relations(
    State(a): State<Application>,
    actor: Actor,
    Path((kind, id, relation)): Path<(String, Uuid, String)>,
    Query(q): Query<AssetQuery>,
) -> Result<ApiResponse> {
    if !matches!((kind.as_str(), relation.as_str()), ("groups", "members")) {
        return Err(Error::NotFound);
    }
    Ok(collection(
        &format!("wecom-asset-{relation}"),
        a.integrations
            .assets()
            .list(&actor, &relation, &q, Some(id), None)
            .await?,
        &q.pagination(),
    ))
}
async fn runs(
    State(a): State<Application>,
    actor: Actor,
    Query(q): Query<AssetQuery>,
) -> Result<ApiResponse> {
    Ok(collection(
        "wecom-asset-sync-runs",
        a.integrations
            .assets()
            .list(&actor, "sync-runs", &q, None, None)
            .await?,
        &q.pagination(),
    ))
}
async fn candidates(
    State(a): State<Application>,
    actor: Actor,
    Path(school): Path<Uuid>,
    Query(q): Query<AssetQuery>,
) -> Result<ApiResponse> {
    Ok(collection(
        "wecom-group-candidates",
        a.integrations
            .assets()
            .candidates(&actor, school, &q)
            .await?,
        &q.pagination(),
    ))
}
async fn bind(
    State(a): State<Application>,
    actor: Actor,
    Path(school): Path<Uuid>,
    input: Input,
) -> Result<ApiResponse> {
    Ok(resource(
        "groups",
        a.integrations
            .assets()
            .bind(
                &actor,
                school,
                input.decode::<GroupBinding>("wecom-group-bindings")?,
            )
            .await?,
    ))
}
async fn confirm_scope(
    State(a): State<Application>,
    actor: Actor,
    input: Input,
) -> Result<ApiResponse> {
    a.integrations
        .assets()
        .confirm_scope(
            &actor,
            input.decode::<OwnerScopeDecision>("wecom-owner-scope-decisions")?,
        )
        .await?;
    Ok(crate::http::jsonapi::completed())
}
pub fn router() -> Router<Application> {
    Router::new()
        .route(
            "/api/v1/platform/wecom-assets/owner-scope-decisions",
            axum::routing::post(confirm_scope),
        )
        .route("/api/v1/platform/wecom-assets/status", get(status))
        .route(
            "/api/v1/platform/wecom-assets/sync-runs",
            get(runs).post(sync),
        )
        .route("/api/v1/platform/wecom-assets/{kind}", get(list))
        .route("/api/v1/platform/wecom-assets/{kind}/{id}", get(detail))
        .route(
            "/api/v1/platform/wecom-assets/{kind}/{id}/{relation}",
            get(relations),
        )
        .route(
            "/api/v1/schools/{school}/wecom-group-candidates",
            get(candidates),
        )
        .route(
            "/api/v1/schools/{school}/groups/from-directory",
            axum::routing::post(bind),
        )
}
