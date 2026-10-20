use crate::{
    bootstrap::Application,
    business::{Actor, Decision, Page, Result, Revision},
    community::domain::{CommentInput, PostInput},
    http::error::data,
};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::{get, post, put},
};
use serde_json::{Value, json};
use uuid::Uuid;
async fn posts(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
    Query(p): Query<Page>,
) -> Result<Json<Value>> {
    Ok(data(a.community.posts(&actor, s, p).await?))
}
async fn detail(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>> {
    Ok(data(a.community.detail(&actor, s, id).await?))
}
async fn create(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
    Json(v): Json<PostInput>,
) -> Result<Json<Value>> {
    Ok(data(a.community.post(&actor, s, None, v).await?))
}
async fn update(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id)): Path<(Uuid, Uuid)>,
    Json(v): Json<PostInput>,
) -> Result<Json<Value>> {
    Ok(data(a.community.post(&actor, s, Some(id), v).await?))
}
async fn visibility(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id)): Path<(Uuid, Uuid)>,
    Json(v): Json<Value>,
) -> Result<Json<Value>> {
    Ok(data(a.community.visibility(&actor, s, id, v).await?))
}
async fn moderate(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id)): Path<(Uuid, Uuid)>,
    Json(v): Json<Decision>,
) -> Result<Json<Value>> {
    Ok(data(a.community.moderate(&actor, s, id, v, false).await?))
}
async fn moderate_comment(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id)): Path<(Uuid, Uuid)>,
    Json(v): Json<Decision>,
) -> Result<Json<Value>> {
    Ok(data(a.community.moderate(&actor, s, id, v, true).await?))
}
async fn comments(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id)): Path<(Uuid, Uuid)>,
    Query(p): Query<Page>,
) -> Result<Json<Value>> {
    Ok(data(a.community.comments(&actor, s, Some(id), p).await?))
}
async fn all_comments(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
    Query(p): Query<Page>,
) -> Result<Json<Value>> {
    Ok(data(a.community.comments(&actor, s, None, p).await?))
}
async fn comment(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id)): Path<(Uuid, Uuid)>,
    Json(v): Json<CommentInput>,
) -> Result<Json<Value>> {
    Ok(data(a.community.comment(&actor, s, id, v).await?))
}
async fn delete_comment(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id)): Path<(Uuid, Uuid)>,
    Json(v): Json<Revision>,
) -> Result<Json<Value>> {
    a.community
        .delete_comment(&actor, s, id, v.revision)
        .await?;
    Ok(data(json!({"ok":true})))
}
async fn reaction(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id, kind)): Path<(Uuid, Uuid, String)>,
    method: axum::http::Method,
) -> Result<Json<Value>> {
    a.community
        .react(&actor, s, id, &kind, method == axum::http::Method::PUT)
        .await?;
    Ok(data(json!({"ok":true})))
}
async fn reactions(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
) -> Result<Json<Value>> {
    Ok(data(a.community.reactions(&actor, s).await?))
}
async fn follow(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id)): Path<(Uuid, Uuid)>,
    method: axum::http::Method,
) -> Result<Json<Value>> {
    a.community
        .follow(&actor, s, id, method == axum::http::Method::PUT)
        .await?;
    Ok(data(json!({"ok":true})))
}
async fn follows(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
) -> Result<Json<Value>> {
    Ok(data(a.community.follows(&actor, s).await?))
}
async fn notices(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
) -> Result<Json<Value>> {
    Ok(data(a.community.notifications(&actor, s).await?))
}
async fn read(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>> {
    a.community.read(&actor, s, id).await?;
    Ok(data(json!({"ok":true})))
}
async fn cases(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
) -> Result<Json<Value>> {
    Ok(data(a.community.cases(&actor, s).await?))
}
async fn case(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
    Json(v): Json<Value>,
) -> Result<Json<Value>> {
    Ok(data(a.community.case(&actor, s, v).await?))
}
async fn resolve(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id)): Path<(Uuid, Uuid)>,
    Json(v): Json<Value>,
) -> Result<Json<Value>> {
    Ok(data(a.community.resolve(&actor, s, id, v).await?))
}
pub fn router() -> Router<Application> {
    Router::new()
        .route("/api/v1/schools/{school}/posts", get(posts).post(create))
        .route(
            "/api/v1/schools/{school}/posts/{id}",
            get(detail).put(update),
        )
        .route(
            "/api/v1/schools/{school}/posts/{id}/visibility",
            put(visibility),
        )
        .route(
            "/api/v1/schools/{school}/posts/{id}/moderation-decisions",
            post(moderate),
        )
        .route(
            "/api/v1/schools/{school}/posts/{id}/comments",
            get(comments).post(comment),
        )
        .route("/api/v1/schools/{school}/comments", get(all_comments))
        .route(
            "/api/v1/schools/{school}/comments/{id}",
            axum::routing::delete(delete_comment),
        )
        .route(
            "/api/v1/schools/{school}/comments/{id}/moderation-decisions",
            post(moderate_comment),
        )
        .route(
            "/api/v1/schools/{school}/posts/{id}/reactions/{kind}",
            put(reaction).delete(reaction),
        )
        .route("/api/v1/schools/{school}/reactions", get(reactions))
        .route("/api/v1/schools/{school}/follows", get(follows))
        .route(
            "/api/v1/schools/{school}/follows/{id}",
            put(follow).delete(follow),
        )
        .route("/api/v1/schools/{school}/notifications", get(notices))
        .route(
            "/api/v1/schools/{school}/notifications/{id}/read",
            put(read),
        )
        .route("/api/v1/schools/{school}/cases", get(cases).post(case))
        .route("/api/v1/schools/{school}/cases/{id}", put(resolve))
}
