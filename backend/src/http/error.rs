use crate::business::Error;
use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use cairnkit_jsonapi::api::{JSON_API_MEDIA_TYPE, error_document};

/// Map application policy to Cairnkit's shared JSON:API document model.
pub fn response(status: StatusCode, code: &str, detail: &str) -> Response {
    (
        status,
        [(axum::http::header::CONTENT_TYPE, JSON_API_MEDIA_TYPE)],
        Json(error_document(
            status.as_u16(),
            code,
            status.canonical_reason().unwrap_or("HTTP Error").to_owned(),
            detail.to_owned(),
            None,
        )),
    )
        .into_response()
}

/// Normalize Axum's extractor, body-limit and method rejections through the same
/// document builder. Never reflect raw rejection bodies or credentials to users.
pub async fn normalize_errors(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let api = request.uri().path().starts_with("/api/");
    let result = next.run(request).await;
    let status = result.status();
    if !api
        || !(status.is_client_error() || status.is_server_error())
        || result
            .headers()
            .get(axum::http::header::CONTENT_TYPE)
            .is_some_and(|v| v == JSON_API_MEDIA_TYPE)
    {
        return result;
    }
    let (code, detail) = match status {
        StatusCode::BAD_REQUEST => ("invalid_request", "请求参数或 JSON 格式不合法"),
        StatusCode::UNPROCESSABLE_ENTITY => ("validation", "请求字段类型错误或缺少必填字段"),
        StatusCode::METHOD_NOT_ALLOWED => ("method_not_allowed", "此接口不支持该请求方式"),
        StatusCode::PAYLOAD_TOO_LARGE => ("payload_too_large", "请求内容超过允许大小"),
        StatusCode::UNSUPPORTED_MEDIA_TYPE => {
            ("unsupported_media_type", "请使用接口要求的 Content-Type")
        }
        StatusCode::NOT_FOUND => ("not_found", "接口不存在"),
        _ => ("http_error", "服务暂时不可用"),
    };
    let (mut parts, _) = result.into_parts();
    let normalized = response(status, code, detail);
    parts.headers.remove(axum::http::header::CONTENT_LENGTH);
    parts.headers.insert(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static(JSON_API_MEDIA_TYPE),
    );
    Response::from_parts(parts, normalized.into_body())
}
impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let (status, code, message): (StatusCode, &str, String) = match self {
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
            Self::Policy("revision_conflict") => (
                StatusCode::CONFLICT,
                "revision_conflict",
                "记录已更新，请刷新后核对并重新提交".into(),
            ),
            Self::Policy("pairing_code_invalid") => (
                StatusCode::UNAUTHORIZED,
                "pairing_code_invalid",
                "配对码无效或已过期，请在管理端重新生成".into(),
            ),
            Self::Policy("pairing_rate_limited") => (
                StatusCode::TOO_MANY_REQUESTS,
                "pairing_rate_limited",
                "配对尝试过多，请等待 10 分钟后重试".into(),
            ),
            Self::Policy("device_request_limit") => (
                StatusCode::TOO_MANY_REQUESTS,
                "device_request_limit",
                "设备请求过于频繁，请稍后重试".into(),
            ),
            Self::Policy("automation_environment_required") => (
                StatusCode::CONFLICT,
                "automation_environment_required",
                "等待手机上报当前环境；请检查手机端版本、连接与权限".into(),
            ),
            Self::Policy("automation_instance_ambiguous") => (
                StatusCode::CONFLICT,
                "automation_instance_ambiguous",
                "无法唯一确认应用安装实例，请在手机端重新检查".into(),
            ),
            Self::Policy("automation_timing_required") => (
                StatusCode::CONFLICT,
                "automation_timing_required",
                "缺少同环境的真实人工计时，请由适配人员完成采样".into(),
            ),
            Self::Policy("automation_readiness_required") => (
                StatusCode::CONFLICT,
                "automation_readiness_required",
                "等待手机完成当前应用准备和核验".into(),
            ),
            Self::Policy(code) => (
                StatusCode::CONFLICT,
                code,
                "手机执行约束未满足，请检查任务和核对状态".into(),
            ),
            Self::Invalid(s) => (StatusCode::UNPROCESSABLE_ENTITY, "validation", s),
            Self::Unavailable | Self::Upstream(_) => (
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
        let mut response = response(status, code, &message);
        if code == "pairing_rate_limited" {
            response.headers_mut().insert(
                axum::http::header::RETRY_AFTER,
                axum::http::HeaderValue::from_static("600"),
            );
        }
        response
    }
}
