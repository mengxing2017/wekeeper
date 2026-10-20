use crate::{bootstrap::Application, business::Error};
use axum::{
    Form, Json, Router,
    extract::{Path, State},
    http::{HeaderMap, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde_json::json;
use std::collections::HashMap;
async fn dispatch(
    State(a): State<Application>,
    Path((controller, method)): Path<(String, String)>,
    headers: HeaderMap,
    Form(v): Form<HashMap<String, String>>,
) -> Response {
    let result = super::local::dispatch(&a, &controller, &method, &headers, v).await;
    let body = match result {
        Ok(v) => json!({"code":1,"msg":"操作成功","time":chrono::Utc::now().timestamp(),"data":v}),
        Err(e) => {
            let msg = match e {
                Error::Unauthorized => "请先登录".into(),
                Error::Forbidden => "无权访问该学校或内容".into(),
                Error::NotFound => "内容不存在".into(),
                Error::Invalid(s) => s,
                Error::Conflict => "内容已变化，请刷新后重试".into(),
                _ => "服务暂不可用".into(),
            };
            json!({"code":0,"msg":msg,"data":null})
        }
    };
    ([(header::CACHE_CONTROL, "no-store")], Json(body)).into_response()
}
async fn upload(
    State(a): State<Application>,
    headers: HeaderMap,
    mut form: axum::extract::Multipart,
) -> crate::business::Result<Response> {
    let token = headers
        .get("token")
        .and_then(|h| h.to_str().ok())
        .ok_or(Error::Unauthorized)?;
    let u = a.identity.0.authenticate(token, "miniapp").await?;
    let school = u.bound_school_id.ok_or(Error::Forbidden)?;
    while let Some(field) = form
        .next_field()
        .await
        .map_err(|_| Error::Invalid("上传格式错误".into()))?
    {
        if field.name() == Some("file") {
            let bytes = field
                .bytes()
                .await
                .map_err(|_| Error::Invalid("图片读取失败".into()))?;
            let url = a.media.put(&u, school, bytes.to_vec()).await?;
            return Ok(
                Json(json!({"code":1,"msg":"上传成功","data":{"url":url,"fullurl":url}}))
                    .into_response(),
            );
        }
    }
    Err(Error::Invalid("缺少图片".into()))
}
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
    Router::new()
        .route(
            "/api/discover/Ajax/upload",
            post(upload).layer(axum::extract::DefaultBodyLimit::max(6 * 1024 * 1024)),
        )
        .route("/api/discover/{controller}/{method}", post(dispatch))
        .route("/api/legacy/media/{token}", get(media))
}
