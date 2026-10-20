use crate::business::Error;
use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;
impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized", "请先登录".into()),
            Self::Forbidden => (
                StatusCode::FORBIDDEN,
                "forbidden",
                "没有此学校或操作的权限".into(),
            ),
            Self::NotFound => (StatusCode::NOT_FOUND, "not_found", "资源不存在".into()),
            Self::Conflict => (
                StatusCode::CONFLICT,
                "conflict",
                "数据已变化或操作重复，请刷新后重试".into(),
            ),
            Self::Invalid(s) => (StatusCode::UNPROCESSABLE_ENTITY, "validation", s),
            Self::Unavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                "unavailable",
                "平台能力暂不可用或尚未配置".into(),
            ),
            Self::Database(e) => {
                tracing::error!(error=%e,"database operation failed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal",
                    "服务处理失败".into(),
                )
            }
        };
        (
            status,
            Json(json!({"error":{"code":code,"message":message}})),
        )
            .into_response()
    }
}
pub fn data(value: impl serde::Serialize) -> Json<serde_json::Value> {
    Json(serde_json::json!({"data":value}))
}
