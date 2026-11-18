//! Management observations only; these values never authorize execution.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Diagnostic {
    pub channel: Option<String>,
    pub stage: String,
    pub label: String,
    pub reason_code: String,
    pub explanation: String,
    pub severity: String,
    pub affected_targets: i64,
    pub since: Option<DateTime<Utc>>,
    pub retry_at: Option<DateTime<Utc>>,
    pub deadline_at: Option<DateTime<Utc>>,
    pub owner: String,
    pub next_action: String,
    pub historical_risk: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DeliveryView {
    pub id: String,
    pub channel: String,
    pub target_id: Uuid,
    pub target_name: String,
    pub task_id: Option<Uuid>,
    pub attempt_id: Option<Uuid>,
    pub attempt_no: Option<i32>,
    pub device_id: Option<Uuid>,
    pub device_name: Option<String>,
    pub sender: Option<String>,
    pub outcome: String,
    pub diagnostic: Diagnostic,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Progress {
    pub version: String,
    pub stage: String,
    pub total: Option<i64>,
    pub counts_complete: bool,
    pub diagnostics: Vec<Diagnostic>,
    pub observed_at: DateTime<Utc>,
    pub results_as_of: Option<DateTime<Utc>>,
    pub projection_lag: bool,
}
#[derive(Clone, Debug, Deserialize)]
pub struct Observation {
    pub channel: Option<String>,
    pub stage: String,
    pub severity: String,
    pub outcome: String,
    pub count: i64,
    pub reason: Option<String>,
    pub since: Option<DateTime<Utc>>,
    pub retry_at: Option<DateTime<Utc>>,
    pub deadline_at: Option<DateTime<Utc>>,
    pub historical_risk: bool,
}
impl Observation {
    pub fn diagnose(&self) -> Diagnostic {
        let (label, explanation, owner, next) = match self.stage.as_str() {
            "handoff" => (
                "内部交接",
                "消息已接收，等待交给内部编排",
                "system",
                "系统自动处理交接",
            ),
            "checking" => (
                "内部检查",
                "正在检查并冻结正文和目标",
                "system",
                "系统自动完成内部检查",
            ),
            "creating_tasks" => (
                "通道任务创建",
                "正在为冻结目标创建通道任务",
                "system",
                "系统自动创建通道任务",
            ),
            "scheduling" => (
                "手机调度",
                "正在授权候选范围内选择或切换手机",
                "system",
                "系统继续调度下一合法候选",
            ),
            "device_receipt" => (
                "设备接收",
                "执行包已分配，尚无设备落盘接收记录",
                "device",
                "设备接收任务；平台按现行规则自动换机",
            ),
            "device_execution" => (
                "设备执行",
                "设备已接收，等待最终执行结果；尚未证明具体本地步骤",
                "device",
                "等待设备回报；接收后五分钟无最终结果自动换机",
            ),
            "official_queue" => (
                "企业微信排队",
                "目标尚未完成官方调用",
                "system",
                "系统按调度时间提交官方接口",
            ),
            "official_call" => (
                "企业微信调用",
                "已开始官方调用，尚未取得确定结果",
                "system",
                "系统核对调用结果，避免盲目重发",
            ),
            "official_result" => (
                "官方结果",
                "官方任务已受理，等待目标发送结果",
                "official",
                "系统继续查询官方目标结果",
            ),
            "sent" => (
                "已发送",
                "目标已有成功事实，不代表对方已阅读",
                "system",
                "本目标无需重投",
            ),
            "failed" => (
                "已结束：失败",
                "本目标已确定失败或停止",
                "admin",
                "查看目标任务和失败原因；不自动补发旧消息",
            ),
            "unknown" => (
                "结果待核对",
                "本目标尚无确定最终结果，保留原尝试证据",
                "admin",
                "查看原尝试结果与风险；其他合法目标或候选继续处理",
            ),
            "not_dispatched" => (
                "未下发成功",
                "消息在创建通道任务前结束",
                "admin",
                "核对原因；配置修改只影响之后的新消息",
            ),
            _ => (
                "进度暂不可确认",
                "关联的任务或执行证据不完整",
                "admin",
                "系统管理员核对目标与任务关联，不重发来源消息",
            ),
        };
        let reason = self.reason.as_deref().unwrap_or(&self.stage);
        let explanation = match reason {
            "keyword_blocked" => "正文命中敏感词，内部分发已结束",
            "route_execution_disabled" => "路由内部执行已停用，本条消息未下发",
            "route_no_targets" => "没有目标群，本条消息未下发",
            "channel_resources_missing" => "缺少合法发送资源，受影响目标未下发",
            "channel_content_invalid" => "正文不满足通道发送限制",
            "device_disabled" => "当前手机已停用，系统将在授权范围内选择其他合法候选",
            "device_busy" => "当前手机仍持有执行物理锁，系统可选择其他授权候选",
            "candidate_unavailable" => "当前手机不可用，系统正在授权范围内选择下一候选",
            "device_offline" => "当前手机离线，系统正在切换下一授权候选",
            "candidate_ready" => "已选择手机，等待平台交付完整执行包",
            "awaiting_dispatch" => "目标已冻结，等待平台首次调度",
            "awaiting_round_deadline" => "本轮暂无可用候选，等待当前轮次截止后继续自动遍历",
            "next_candidate_round" => "当前候选轮次已结束，正在进入下一轮",
            "delivery_not_received_timeout" => "手机未在期限内接收，系统正在自动换机",
            "plan_dependency_changed" => "当前手机执行依据已变化，系统正在授权范围内改派",
            "dispatch_round_limit_exceeded" => "授权候选已遍历三轮，自动下发已结束",
            "task_stopped" => "任务已停止，本目标不再自动执行",
            "confirmation_expired" => "任务业务授权已到期",
            _ => explanation,
        };
        let next = if reason == "awaiting_round_deadline" {
            "系统将在下次重评时间重新评估候选；查看手机分发记录中的逐候选原因"
        } else {
            next
        };
        let (explanation, owner, next) =
            if self.severity == "attention" && self.outcome == "pending" {
                if matches!(
                    self.stage.as_str(),
                    "device_receipt" | "device_execution" | "scheduling"
                ) {
                    (
                        "调度期限已过，尚未观察到后继推进",
                        "admin",
                        "系统管理员检查自动调度；无需等待原手机恢复",
                    )
                } else {
                    (
                        "超过观察阈值仍未确认推进",
                        "admin",
                        "系统管理员核对通道 worker 与调用进度，不盲目重发",
                    )
                }
            } else if self.severity == "retrying" {
                (
                    "本阶段暂时处理失败，系统正在退避重试",
                    "system",
                    "系统按下次重试时间继续处理",
                )
            } else {
                (explanation, owner, next)
            };
        Diagnostic {
            channel: self.channel.clone(),
            stage: self.stage.clone(),
            label: label.into(),
            reason_code: reason.into(),
            explanation: explanation.into(),
            severity: self.severity.clone(),
            affected_targets: self.count,
            since: self.since,
            retry_at: self.retry_at,
            deadline_at: self.deadline_at,
            owner: owner.into(),
            next_action: next.into(),
            historical_risk: self.historical_risk,
        }
    }
}
/// Delegation is an immutable milestone, not the current message outcome.
/// A terminal channel without targets must not be hidden by another channel's success.
pub fn message_state_with_channel_failures(
    dispatch: &str,
    total: i64,
    counts: &super::MessageCounts,
    complete: bool,
    channel_failure: bool,
) -> &'static str {
    let state = message_state(dispatch, total, counts, complete);
    if channel_failure && state == "completed" {
        "finished_with_failures"
    } else {
        state
    }
}

pub fn message_state(
    dispatch: &str,
    total: i64,
    counts: &super::MessageCounts,
    complete: bool,
) -> &'static str {
    if !complete {
        return "unobserved";
    }
    if dispatch == "failed" {
        return "not_dispatched";
    }
    if matches!(dispatch, "handoff" | "pending" | "preparing" | "ready") {
        return match dispatch {
            "handoff" => "handoff",
            "pending" => "checking",
            _ => "creating_tasks",
        };
    }
    if total == 0 {
        return match dispatch {
            "handoff" => "handoff",
            "pending" => "checking",
            "preparing" | "ready" => "creating_tasks",
            _ => "unobserved",
        };
    }
    if counts.pending > 0 {
        if counts.sent + counts.failed > 0 {
            "partial"
        } else {
            "sending"
        }
    } else if counts.unknown > 0 {
        "needs_review"
    } else if counts.sent == total {
        "completed"
    } else if counts.failed == total {
        "failed"
    } else {
        "finished_with_failures"
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn delegation_is_not_the_message_outcome() {
        let mut counts = crate::message_orchestration::domain::MessageCounts {
            pending: 2,
            sent: 0,
            failed: 0,
            unknown: 0,
        };
        assert_eq!(message_state("delegated", 2, &counts, true), "sending");
        counts.pending = 1;
        counts.sent = 1;
        assert_eq!(message_state("delegated", 2, &counts, true), "partial");
        counts.pending = 0;
        counts.sent = 2;
        assert_eq!(message_state("delegated", 2, &counts, true), "completed");
        counts.sent = 0;
        counts.failed = 2;
        assert_eq!(message_state("delegated", 2, &counts, true), "failed");
        counts.failed = 1;
        counts.unknown = 1;
        assert_eq!(message_state("delegated", 2, &counts, true), "needs_review");
        assert_eq!(message_state("delegated", 2, &counts, false), "unobserved");
        assert_eq!(message_state("delegated", 0, &counts, true), "unobserved");
        assert_eq!(message_state("failed", 0, &counts, true), "not_dispatched");
    }
}

#[cfg(test)]
mod channel_failure_tests {
    use super::*;
    #[test]
    fn a_failed_channel_without_targets_is_not_all_success() {
        let counts = crate::message_orchestration::domain::MessageCounts {
            sent: 1,
            pending: 0,
            failed: 0,
            unknown: 0,
        };
        assert_eq!(
            message_state_with_channel_failures("delegated", 1, &counts, true, true),
            "finished_with_failures"
        );
        assert_eq!(
            message_state_with_channel_failures("delegated", 1, &counts, true, false),
            "completed"
        );
    }
}
