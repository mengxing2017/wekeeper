use crate::{
    bootstrap::Application,
    business::{Actor, Decision, Page, Result, Revision},
    community::domain::{CommentInput, PostInput},
    http::jsonapi::{ApiResponse, Input, collection, completed, resource},
};
use axum::{
    Router,
    extract::{Path, Query, State},
    routing::{get, post, put},
};
use serde_json::Value;
use uuid::Uuid;
async fn posts(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
    Query(p): Query<Page>,
) -> Result<ApiResponse> {
    Ok(collection(
        "posts",
        a.community.posts(&actor, s, p.clone()).await?,
        &p,
    ))
}
async fn detail(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id)): Path<(Uuid, Uuid)>,
) -> Result<ApiResponse> {
    Ok(resource("posts", a.community.detail(&actor, s, id).await?))
}
async fn create(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
    input_document: Input,
) -> Result<ApiResponse> {
    let v = input_document.decode::<PostInput>("posts")?;
    Ok(resource(
        "posts",
        a.community.post(&actor, s, None, v).await?,
    ))
}
async fn update(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id)): Path<(Uuid, Uuid)>,
    input_document: Input,
) -> Result<ApiResponse> {
    let v = input_document.decode::<PostInput>("posts")?;
    Ok(resource(
        "posts",
        a.community.post(&actor, s, Some(id), v).await?,
    ))
}
async fn visibility(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id)): Path<(Uuid, Uuid)>,
    input_document: Input,
) -> Result<ApiResponse> {
    let v = input_document.decode::<Value>("posts")?;
    Ok(resource(
        "posts",
        a.community.visibility(&actor, s, id, v).await?,
    ))
}
async fn moderate(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id)): Path<(Uuid, Uuid)>,
    input_document: Input,
) -> Result<ApiResponse> {
    let v = input_document.decode::<Decision>("posts")?;
    Ok(resource(
        "posts",
        a.community.moderate(&actor, s, id, v, false).await?,
    ))
}
async fn moderate_comment(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id)): Path<(Uuid, Uuid)>,
    input_document: Input,
) -> Result<ApiResponse> {
    let v = input_document.decode::<Decision>("comments")?;
    Ok(resource(
        "comments",
        a.community.moderate(&actor, s, id, v, true).await?,
    ))
}
async fn comments(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id)): Path<(Uuid, Uuid)>,
    Query(p): Query<Page>,
) -> Result<ApiResponse> {
    Ok(collection(
        "comments",
        a.community.comments(&actor, s, Some(id), p.clone()).await?,
        &p,
    ))
}
async fn all_comments(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
    Query(p): Query<Page>,
) -> Result<ApiResponse> {
    Ok(collection(
        "comments",
        a.community.comments(&actor, s, None, p.clone()).await?,
        &p,
    ))
}
async fn comment(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id)): Path<(Uuid, Uuid)>,
    input_document: Input,
) -> Result<ApiResponse> {
    let v = input_document.decode::<CommentInput>("comments")?;
    Ok(resource(
        "comments",
        a.community.comment(&actor, s, id, v).await?,
    ))
}
async fn delete_comment(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id)): Path<(Uuid, Uuid)>,
    input_document: Input,
) -> Result<ApiResponse> {
    let v = input_document.decode::<Revision>("comments")?;
    a.community
        .delete_comment(&actor, s, id, v.revision)
        .await?;
    Ok(completed())
}
async fn reaction(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id, kind)): Path<(Uuid, Uuid, String)>,
    method: axum::http::Method,
) -> Result<ApiResponse> {
    a.community
        .react(&actor, s, id, &kind, method == axum::http::Method::PUT)
        .await?;
    Ok(completed())
}
async fn reactions(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
) -> Result<ApiResponse> {
    Ok(resource(
        "reactions",
        a.community.reactions(&actor, s).await?,
    ))
}
async fn follow(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id)): Path<(Uuid, Uuid)>,
    method: axum::http::Method,
) -> Result<ApiResponse> {
    a.community
        .follow(&actor, s, id, method == axum::http::Method::PUT)
        .await?;
    Ok(completed())
}
async fn follows(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
) -> Result<ApiResponse> {
    Ok(resource("accounts", a.community.follows(&actor, s).await?))
}
async fn notices(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
    Query(page): Query<Page>,
) -> Result<ApiResponse> {
    Ok(collection(
        "notifications",
        a.community.notifications(&actor, s, page.clone()).await?,
        &page,
    ))
}
async fn read(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id)): Path<(Uuid, Uuid)>,
) -> Result<ApiResponse> {
    a.community.read(&actor, s, id).await?;
    Ok(completed())
}
async fn cases(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
    Query(page): Query<Page>,
) -> Result<ApiResponse> {
    Ok(collection(
        "cases",
        a.community.cases(&actor, s, page.clone()).await?,
        &page,
    ))
}
async fn case(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
    input_document: Input,
) -> Result<ApiResponse> {
    let v = input_document.decode::<Value>("cases")?;
    Ok(resource("cases", a.community.case(&actor, s, v).await?))
}
async fn resolve(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id)): Path<(Uuid, Uuid)>,
    input_document: Input,
) -> Result<ApiResponse> {
    let v = input_document.decode::<Value>("cases")?;
    Ok(resource(
        "cases",
        a.community.resolve(&actor, s, id, v).await?,
    ))
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
