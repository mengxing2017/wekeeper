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
            Self::Policy("mobile_target_unresolved") => (StatusCode::CONFLICT,"mobile_target_unresolved","目标已有待执行或待核对项，请先处理原任务，不要换设备重复安排".into()),
            Self::Policy("statistics_snapshot_changed") => (StatusCode::CONFLICT,"statistics_snapshot_changed","发送结果已更新，请同步刷新统计".into()),
            Self::Policy("statistics_snapshot_expired") => (StatusCode::GONE,"statistics_snapshot_expired","统计快照已过期，请重新查询".into()),
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized", "请先登录".into()),
            Self::Forbidden => (
                StatusCode::FORBIDDEN,
                "forbidden",
                "没有执行此操作的权限".into(),
            ),
            Self::NotFound => (StatusCode::NOT_FOUND, "not_found", "资源不存在".into()),
            Self::Conflict => (
                StatusCode::CONFLICT,
                "conflict",
                "数据已变化或操作重复，请刷新后重试".into(),
            ),
            Self::Policy("physical_device_busy") => (StatusCode::CONFLICT,"physical_device_busy","该手机正在执行其他操作或存在待核对结果，请先处理原任务，不能强制解锁".into()),
            Self::Policy("plan_dependency_changed") => (StatusCode::CONFLICT,"plan_dependency_changed","该尝试的有效目标或授权已变化，已结束该未发放许可的尝试；其他合法任务继续调度".into()),
            Self::Policy("business_scheme_changed") => (StatusCode::CONFLICT,"business_scheme_changed","业务方案已变化，请保留当前内容并重新预览".into()),
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
            Self::Policy("source_entry_not_authorized") => (StatusCode::FORBIDDEN,"source_entry_not_authorized","当前入口不能使用该来源身份，请通过受信内容入口操作".into()),
            Self::Policy("source_content_unavailable") => (StatusCode::CONFLICT,"source_content_unavailable","来源内容已撤回、版本改变或当前无法核实，尚未调用部分不会发送，请重新核对来源和预览".into()),
            Self::Policy("source_version_conflict") => (StatusCode::CONFLICT,"source_version_conflict","来源内容版本已变化，请重新选取素材并预览确认".into()),
            Self::Policy("preview_expired") => (StatusCode::CONFLICT,"preview_expired","官方群发预览已过期，请重新检查全部正文和目标后确认".into()),
            Self::Policy("shared_binding_missing") => (
                StatusCode::CONFLICT,
                "shared_binding_missing",
                "目标群没有有效手机绑定，请系统管理员配置或恢复绑定".into(),
            ),
            Self::Policy(code) if code.starts_with("shared_send_") => (
                StatusCode::CONFLICT,code,match code {
                    "shared_send_policy_invalid" => "业务方案的额度、时区或允许时段无效，请系统管理员核对",
                    "shared_send_key_required" => "平台尚未配置共享发送密钥，请联系部署维护人员配置后再开放发送",
                    "shared_send_scheme_missing" => "业务方案无效，请系统管理员核对当前单位方案",
                    "shared_send_preparation_only" => "平台仅开放内容准备或预览，实际发送需系统管理员开放",
                    "shared_send_scheme_unsupported" => "现有方案不支持当前发送业务，请系统管理员重新保存业务方案",
                    "shared_send_platform_closed" => "平台已关闭新发送，请系统管理员核对业务方案",
                    "shared_send_review_required" => "原授权仍可能执行或该业务意图已有发送证据，请核对原尝试",
                    "shared_send_resource_invalid" => "所选手机、目标或资源资格不可用，请刷新资源并核对设备状态和有效绑定",
                    "shared_send_selection_invalid" => "固定执行依据已变化或过期，请核对任务状态并重新预览；已调用部分不能重发",
                    "shared_send_targets_invalid" => "目标群选择或顺序无效，请核对重复项、有效绑定和本次群数上限",
                    "shared_send_reason_required" => "请填写本次操作原因，以便保留可核对的处理记录",
                    "shared_send_review_invalid" => "核对依据或结论无效，请查看原尝试与手机回执后重新核对",
                    "shared_send_diagnostic_without_failure" => "当前没有可关联的真实执行失败，不接受提前上报应用版本诊断",
                    "shared_send_diagnostic_invalid" => "诊断内容不符合允许字段或对应失败记录，请联系维护人员核对",
                    "shared_send_secret_invalid" => "共享发送凭据无效或已变化，请联系平台维护人员核对配置",
                    _ => "共享发送依据不满足，请刷新任务并核对业务授权与资源",
                }.into(),
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
