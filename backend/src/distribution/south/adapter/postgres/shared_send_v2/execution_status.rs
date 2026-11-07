//! Observations never grant UI ownership or reserve quota.
use super::*;
fn add(
    rows: &mut Vec<ExecutionCondition>,
    code: &str,
    message: &str,
    owner: &str,
    automatic: bool,
    device: Uuid,
) {
    rows.push(ExecutionCondition {
        code: code.into(),
        category: "execution".into(),
        affected_action: "claim".into(),
        scope_type: "device".into(),
        scope_id: device,
        owner: match owner {
            "platform" | "school" => "admin",
            other => other,
        }
        .into(),
        message: message.into(),
        automatic,
        next_action: if automatic {
            "等待系统自动处理"
        } else {
            "查看并处理此条件"
        }
        .into(),
        observed_at: chrono::Utc::now(),
        waiting_since: None,
        reason_changed_at: None,
        valid_until: None,
        retry_at: None,
    });
}
pub(super) async fn task(tx: &mut Tx<'_>, school: Uuid, value: &mut Value) -> Result<()> {
    let snapshot = value.clone();
    if let Some(items) = value["items"].as_array_mut() {
        for item in items {
            let position = item["position"].as_i64().unwrap_or(-1);
            if let Some(attempts) = item["attempts"].as_array_mut() {
                for attempt in attempts {
                    attempt["execution_status"] =
                        assess(tx, school, &snapshot, position, attempt).await?;
                }
            }
        }
    }
    Ok(())
}
/// Display the durable dispatcher and grant facts. Runtime telemetry is informational only.
pub(super) async fn assess(
    tx: &mut Tx<'_>,
    _school: Uuid,
    task: &Value,
    _position: i64,
    attempt: &Value,
) -> Result<Value> {
    let device = uid(&attempt["plan"], "device_id")?;
    let state = attempt["state"].as_str().unwrap_or("unknown");
    let current = attempt["current"] == true;
    let dispatch: Option<Value> =
        sqlx::query_scalar("SELECT to_jsonb(ds) FROM mobile_v9_dispatch_state ds WHERE item_id=$1")
            .bind(uid(attempt, "item_id")?)
            .fetch_optional(&mut **tx)
            .await?;
    let grant:Option<Value>=sqlx::query_scalar("SELECT jsonb_build_object('state',state,'accepted_at',accepted_at,'result_deadline_at',result_deadline_at,'outcome',result_body->>'outcome') FROM mobile_execution_grants WHERE attempt_id=$1").bind(uid(attempt,"id")?).fetch_optional(&mut **tx).await?;
    let mut conditions = Vec::new();
    if current && !matches!(state, "observed_sent" | "manually_confirmed_sent") {
        let expired = task["confirmed_until"]
            .as_str()
            .and_then(|s| s.parse::<chrono::DateTime<chrono::Utc>>().ok())
            .is_some_and(|t| t <= chrono::Utc::now());
        if task["stopped"] == true {
            add(
                &mut conditions,
                "task_stopped",
                "任务已停止，撤销消息已排队",
                "admin",
                false,
                device,
            );
        } else if dispatch
            .as_ref()
            .is_some_and(|d| d["state"] == "dispatch_failed")
        {
            add(
                &mut conditions,
                "dispatch_round_limit_exceeded",
                "任务下发失败：候选手机已遍历3轮，已停止自动下发；历史未知结果仍可补报",
                "admin",
                false,
                device,
            );
        } else if expired {
            add(
                &mut conditions,
                "confirmation_expired",
                "任务确认已到期，请重新确认业务意图",
                "admin",
                false,
                device,
            );
        } else if let Some(g) = &grant
            && g["state"] == "active"
        {
            add(
                &mut conditions,
                "awaiting_execution_result",
                if state == "unknown" {
                    "原尝试结果不确定，平台将在截止时间自动切机"
                } else {
                    "等待手机最终结果；离线或明确失败直接切机"
                },
                "system",
                true,
                device,
            );
            conditions.last_mut().ok_or(Error::Conflict)?.retry_at =
                serde_json::from_value(g["result_deadline_at"].clone()).ok();
        }
        if conditions.is_empty()
            && let Some(d) = &dispatch
            && d["state"] != "assigned"
            && d["state"] != "completed"
        {
            add(
                &mut conditions,
                "waiting_resources",
                "平台正在已授权候选范围内自动选择可用手机",
                "system",
                true,
                device,
            );
            conditions.last_mut().ok_or(Error::Conflict)?.retry_at =
                serde_json::from_value(d["next_retry_at"].clone()).ok();
        }
    }
    let phase = if grant.as_ref().is_some_and(|g| g["state"] == "superseded")
        && !matches!(
            state,
            "observed_sent" | "manually_confirmed_sent" | "unknown"
        ) {
        ExecutionPhase::Blocked
    } else {
        ExecutionPhase::project(state, current, !conditions.is_empty())
    };
    serde_json::to_value(ExecutionStatus {
        phase,
        observed_at: chrono::Utc::now(),
        device_state_observed_at: None,
        consent_observed_at: None,
        conditions,
    })
    .map_err(|_| Error::Conflict)
}
