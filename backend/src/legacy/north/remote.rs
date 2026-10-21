use crate::bootstrap::Application;
use axum::{
    Router,
    extract::{Path, State},
    http::header,
    response::{IntoResponse, Response},
    routing::get,
};
async fn media(
    State(a): State<Application>,
    Path(token): Path<String>,
) -> crate::business::Result<Response> {
    let (mime, b) = a.legacy.media(&token).await?;
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
    Router::new().route("/api/legacy/media/{token}", get(media))
}
