//! Diagnostic decisions contain facts, never an execution permission.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordEvent {
    pub id: i64,
    pub intake_id: Option<Uuid>,
    pub operation_id: Option<Uuid>,
    pub delivery_request_id: Option<Uuid>,
    pub item_id: Option<Uuid>,
    pub round_no: Option<i32>,
    pub kind: String,
    pub reason: String,
    pub result: String,
    pub device_id: Option<Uuid>,
    pub attempt_id: Option<Uuid>,
    pub execution_id: Option<Uuid>,
    pub evidence: Value,
    pub created_at: DateTime<Utc>,
}
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordQuery {
    pub cursor: Option<String>,
    pub page_size: Option<i64>,
    pub source_kind: Option<String>,
    pub message_id: Option<Uuid>,
    pub receipt_id: Option<Uuid>,
    pub task_id: Option<Uuid>,
    pub target: Option<String>,
    pub device_id: Option<Uuid>,
    pub stage: Option<String>,
    pub reason: Option<String>,
    pub reason_scope: Option<String>,
    pub outcome: Option<String>,
    pub from: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
    pub kind: Option<String>,
    pub result: Option<String>,
    pub round_no: Option<i32>,
    pub configuration_id: Option<Uuid>,
    pub intake_id: Option<Uuid>,
    pub execution_id: Option<Uuid>,
}
impl RecordQuery {
    pub fn limit(&self) -> crate::business::Result<i64> {
        crate::business::require(
            self.page_size.is_none_or(|n| (1..=100).contains(&n)),
            "每页记录数必须在 1–100 之间",
        )?;
        crate::business::require(
            self.source_kind
                .as_deref()
                .is_none_or(|s| matches!(s, "source_route" | "manual" | "all")),
            "不支持的记录来源",
        )?;
        crate::business::require(
            self.reason_scope
                .as_deref()
                .is_none_or(|s| matches!(s, "current" | "history")),
            "不支持的原因范围",
        )?;
        crate::business::require(
            self.from.zip(self.until).is_none_or(|(a, b)| a <= b),
            "开始时间不能晚于结束时间",
        )?;
        Ok(self.page_size.unwrap_or(30))
    }
}
pub fn reason_label(code: &str) -> &str {
    match code {
        "device_offline" => "手机离线",
        "device_presence_unknown" => "尚无有效在线状态",
        "device_disabled" => "手机已停用",
        "delegation_not_accepted" => "手机尚未同意托管",
        "delegation_revoked" => "手机已撤销托管",
        "credential_generation_changed" => "手机凭据代次已变化",
        "device_busy" => "手机仍有未关闭的执行占用",
        "awaiting_round_deadline" => "本轮候选已耗尽，等待轮次截止后重评",
        "candidate_ready" => "候选已选中，等待手机拉取",
        "business_interval" => "业务发送间隔未到",
        "mobile_targets_missing" => "未提供手机目标群",
        "mobile_candidates_missing" => "冻结授权候选为空",
        "channel_resources_missing" => "冻结目标没有合法发送资源",
        "channel_content_invalid" => "正文不符合手机通道限制",
        "plan_dependency_changed" => "冻结的目标或手机绑定依据已变化",
        "dispatch_round_limit_exceeded" => "已耗尽三轮自动调度预算",
        "quota_exhausted" => "发送额度已用完",
        "quota_cooldown" => "尚在发送冷却期",
        "outside_allowed_window" => "不在允许发送的时间段",
        "execution_window_too_short" => "剩余执行时间窗不足",
        "execution_credential_renewal_required" => "手机凭据需要续期",
        "shared_send_usage_paused" => "生效的使用范围已暂停",
        "shared_send_usage_revoked" => "生效的使用范围已撤销",
        "shared_send_usage_expired" => "生效的使用范围已到期",
        "shared_send_consent_required" => "手机未同意托管",
        "processing_interrupted" => "处理未完成，等待系统恢复",
        "internal_processing_unavailable" => "系统处理异常，等待退避重试",
        "no_matching_assignment" => "没有符合交付条件的任务",
        "awaiting_result" => "等待手机最终执行结果",
        "device_received" => "手机已确认落盘接收",
        "execution_generated" => "执行包已生成，尚不代表手机已接收",
        "intake_payload_conflict" => "同一业务意图提交了不同冻结内容",
        "delivery_not_received_timeout" => "期限内未观察到设备接收",
        "result_recorded" => "最终结果已入账",
        "signature_failed" => "执行包签名失败",
        "encryption_failed" => "执行包加密失败",
        "recording_failed" => "记录持久化失败",
        "server_write_completed" => "服务端已完成响应流写出，尚不代表手机接收",
        "server_write_interrupted" => "服务端响应流中断",
        "response_observation_unknown" => "服务端响应观测不完整",
        "task_created" => "手机任务已创建",
        "intake_registered" => "请求已进入手机分发领域",
        "observed_sent" => "手机报告已发送",
        "unknown" => "结果不确定",
        _ => "查看原因码与当次检查证据",
    }
}
/// Only facts already checked participate; missing observations are not a pass.
pub fn candidate_reason(f: &Value) -> &'static str {
    if f["enabled"] != true {
        return "device_disabled";
    }
    if f["generation"] != f["expected_generation"] {
        return "credential_generation_changed";
    }
    if f["accepted"] != true {
        return if f["consent_state"] == "revoked" {
            "delegation_revoked"
        } else {
            "delegation_not_accepted"
        };
    }
    if f["presence"] == "offline" {
        return "device_offline";
    }
    if !matches!(f["presence"].as_str(), Some("online" | "suspect")) {
        return "device_presence_unknown";
    }
    if f["busy"].is_object() {
        return "device_busy";
    }
    "passed"
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn event_contract() {
        let v: Value = serde_json::from_str(include_str!(
            "../../../../../api-contract/fixtures/mobile-dispatch-event.json"
        ))
        .unwrap();
        let e: RecordEvent = serde_json::from_value(v.clone()).unwrap();
        assert_eq!(serde_json::to_value(e).unwrap(), v);
    }
}
