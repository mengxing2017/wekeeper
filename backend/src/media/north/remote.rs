use crate::{
    bootstrap::Application,
    business::{Actor, Result},
    http::error::data,
};
use axum::{
    Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Path, State},
    http::header,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde_json::json;
use uuid::Uuid;
async fn upload(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
    b: Bytes,
) -> Result<axum::Json<serde_json::Value>> {
    Ok(data(json!({"url":a.media.put(&actor,s,b.to_vec()).await?})))
}
async fn download(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id)): Path<(Uuid, Uuid)>,
) -> Result<Response> {
    let (mime, b) = a.media.get(&actor, s, id).await?;
    Ok((
        [
            (header::CONTENT_TYPE, mime),
            (header::CACHE_CONTROL, "private, no-store".into()),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff".into()),
        ],
        b,
    )
        .into_response())
}
pub fn router() -> Router<Application> {
    Router::new()
        .route("/api/v1/schools/{school}/media", post(upload))
        .route("/api/v1/schools/{school}/media/{id}", get(download))
        .layer(DefaultBodyLimit::max(5 * 1024 * 1024))
}
