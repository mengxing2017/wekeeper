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
            Self::Policy("legacy_mobile_maintenance_retired") => (StatusCode::CONFLICT,"legacy_mobile_maintenance_retired","旧学校/账号维护与执行流程已停用，请使用共享手机业务能力开通；历史记录可查看，取消、撤销、结果补报及未决核对继续保留".into()),
            Self::Policy("mobile_device_reserved") => (StatusCode::CONFLICT,"mobile_device_reserved","该手机已有任务占用，请处理原任务；跨学校占用请联系平台管理员协调".into()),
            Self::Policy("mobile_target_unresolved") => (StatusCode::CONFLICT,"mobile_target_unresolved","目标已有待执行或待核对项，请先处理原任务，不要换设备重复安排".into()),
            Self::Policy("mobile_execution_configuration_required") => (StatusCode::CONFLICT,"mobile_execution_configuration_required","手机执行配置尚不可用，请检查学校授权、手机同意、流程发布及现场验证".into()),
            Self::Policy("mobile_target_binding_required") => (StatusCode::CONFLICT,"mobile_target_binding_required","目标已停用、解绑或不属于所选手机，请刷新并检查手机目标群绑定".into()),
            Self::Policy("mobile_preview_expired") => (StatusCode::CONFLICT,"mobile_preview_expired","任务预览已过期，请重新预览全部目标和消息后确认".into()),
            Self::Policy("mobile_task_not_cancellable") => (StatusCode::CONFLICT,"mobile_task_not_cancellable","任务版本或状态已变化，不能按未领取任务取消，请刷新任务并核对结果".into()),
            Self::Policy("device_consent_required") => (StatusCode::CONFLICT,"device_consent_required","请在手机上接受该学校当前用途的设备授权后重试".into()),
            Self::Policy("device_validation_required") => (StatusCode::CONFLICT,"device_validation_required","请先由已授权设备完成当前流程验证，验证通过后 24 小时内发布".into()),
            Self::Policy("flow_release_required") => (StatusCode::CONFLICT,"flow_release_required","请选择本校尚未撤销的已发布手机流程".into()),
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
            Self::Policy("binding_platform_permission_required") => (
                StatusCode::FORBIDDEN,
                "binding_platform_permission_required",
                "绑定手机发送账号需要平台管理员权限，请联系平台管理员操作".into(),
            ),
            Self::Policy("binding_account_disabled") => (
                StatusCode::CONFLICT,
                "binding_account_disabled",
                "发送账号已停用，请先启用账号再绑定手机".into(),
            ),
            Self::Policy("binding_school_authorization_required") => (
                StatusCode::CONFLICT,
                "binding_school_authorization_required",
                "该发送账号缺少有效学校群授权：请先配置目标群与账号授权，并确认责任人已启用且具有对应启用学校的管理员权限；修改核对备注不能解决此问题".into(),
            ),
            Self::Policy("school_device_grant_required") => (
                StatusCode::CONFLICT,
                "school_device_grant_required",
                "设备尚未获得本校使用授权，或设备/目标已停用；请联系平台管理员核对学校设备授权，并刷新可用设备后重试".into(),
            ),
            Self::Policy("field_check_dependencies_changed") => (StatusCode::CONFLICT,"field_check_dependencies_changed","检查依据已变化或过期，请刷新手机环境与授权，由学校重新邀请检查".into()),
            Self::Policy("field_invitation_invalid") => (StatusCode::CONFLICT,"field_invitation_invalid","学校检查邀请已过期、撤销或绑定已变化，请联系该校管理员重新邀请".into()),
            Self::Policy("field_invitation_consumed") => (StatusCode::CONFLICT,"field_invitation_consumed","该邀请已用于一次检查，请由学校管理员重新发出邀请".into()),
            Self::Policy("field_check_already_pending") => (StatusCode::CONFLICT,"field_check_already_pending","该手机已有待处理检查，请先核对已有检查或等待其过期".into()),
            Self::Policy("physical_device_busy") => (StatusCode::CONFLICT,"physical_device_busy","该手机正在执行其他操作或存在待核对结果，请先处理原任务，不能强制解锁".into()),
            Self::Policy("field_check_permit_mismatch") => (StatusCode::CONFLICT,"field_check_permit_mismatch","检查结果与原领取许可不一致，请保留本机记录并联系维护人员核对".into()),
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
            Self::Policy("source_entry_not_authorized") => (StatusCode::FORBIDDEN,"source_entry_not_authorized","当前入口不能使用该来源身份，请通过受信内容入口操作".into()),
            Self::Policy("source_content_unavailable") => (StatusCode::CONFLICT,"source_content_unavailable","来源内容已撤回、版本改变或当前无法核实，尚未调用部分不会发送，请重新核对来源和预览".into()),
            Self::Policy("source_version_conflict") => (StatusCode::CONFLICT,"source_version_conflict","来源内容版本已变化，请重新选取素材并预览确认".into()),
            Self::Policy("preview_expired") => (StatusCode::CONFLICT,"preview_expired","官方群发预览已过期，请重新检查全部正文和目标后确认".into()),
            Self::Policy(code) if code.starts_with("shared_send_") => (
                StatusCode::CONFLICT,code,match code {
                    "shared_send_key_required" => "平台尚未配置共享发送密钥，请联系部署维护人员配置后再开放发送",
                    "shared_send_not_released" => "当前政策尚未放行共享发送，请联系平台管理员核对发送阶段",
                    "shared_send_scope_not_accepted" => "该手机、学校、目标群与实际应用环境尚无有效真机验收记录，请联系产品验收人员完成测试",
                    "shared_send_runtime_required" => "等待手机上线、解锁并报告可执行状态；请检查长期托管、通知和无障碍权限",
                    "shared_send_review_required" => "原尝试必须先完成未发送核对并确认设备已停止，不能直接改派或重发",
                    "shared_send_consent_or_binding_required" => "缺少有效目标绑定或独立发送同意，请平台配置绑定并由手机使用者确认发送范围",
                    "shared_send_environment_required" | "shared_send_environment_unsupported" => "等待手机刷新实际环境或由产品研发完成该环境适配",
                    "shared_send_flow_invalid" => "发送适配发行不符合 v8 契约，请产品研发重新核对发行",
                    "shared_send_policy_invalid" => "发送政策的额度、时区或允许时段不完整，请平台管理员核对后保存",
                    _ => "共享发送依据不满足，请刷新逐群预览并核对绑定、发送同意及检查结果",
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
