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
fn pending_time(v: &Value) -> bool {
    v.as_str()
        .and_then(|s| s.parse::<chrono::DateTime<chrono::Utc>>().ok())
        .is_some_and(|t| t > chrono::Utc::now())
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
pub(super) async fn assess(
    tx: &mut Tx<'_>,
    school: Uuid,
    task: &Value,
    position: i64,
    attempt: &Value,
) -> Result<Value> {
    let mut conditions = Vec::new();
    let plan = &attempt["plan"];
    let device = uid(plan, "device_id")?;
    let state = attempt["state"].as_str().unwrap_or("unknown");
    let runtime:Value=sqlx::query_scalar("SELECT jsonb_build_object('observed_at',r.observed_at,'fresh',r.credential_generation=$2 AND r.observed_at>clock_timestamp()-interval '30 seconds','ready',r.ui_ready,'paused',r.paused,'conditions',d.conditions,'online',coalesce(cp.state='online' AND NOT cp.desired_paused AND cp.last_control_heartbeat_at>clock_timestamp()-interval '30 seconds',false)) FROM mobile_shared_runtime r LEFT JOIN mobile_v9_runtime_details d ON d.device_id=r.device_id AND d.credential_generation=r.credential_generation LEFT JOIN device_control_presence cp ON cp.id=r.device_id WHERE r.device_id=$1").bind(device).bind(plan["credential_generation"].as_i64().unwrap_or(0)).fetch_optional(&mut **tx).await?.unwrap_or(Value::Null);
    let consent_observed_at:Option<chrono::DateTime<chrono::Utc>>=sqlx::query_scalar("SELECT c.created_at FROM mobile_shared_delegations d JOIN LATERAL(SELECT created_at FROM mobile_shared_delegation_decisions WHERE delegation_id=d.id ORDER BY revision DESC LIMIT 1)c ON true WHERE d.device_id=$1 AND d.credential_generation=$2").bind(device).bind(plan["credential_generation"].as_i64().unwrap_or(0)).fetch_optional(&mut **tx).await?;
    if state == "queued" && attempt["current"] == true {
        if task["stopped"] == true {
            add(
                &mut conditions,
                "task_stopped",
                "任务已停止新领取",
                "school",
                false,
                device,
            );
        }
        if !pending_time(&task["confirmed_until"]) {
            add(
                &mut conditions,
                "confirmation_expired",
                "任务确认已到期，请复制为新草稿并重新确认",
                "school",
                false,
                device,
            );
        }
        if task["cursor_position"].as_i64() != Some(position) {
            add(
                &mut conditions,
                "previous_target_pending",
                "等待前一群的确定结果与界面静止",
                "system",
                true,
                device,
            );
        }
        if pending_time(&task["next_eligible_at"]) || pending_time(&attempt["not_before"]) {
            add(
                &mut conditions,
                "waiting_interval",
                "等待已确认的群间隔或最早执行时间",
                "system",
                true,
                device,
            );
        }
        match tasks::validate_plan(tx, school, plan).await {
            Ok(()) => {}
            Err(Error::Policy(code)) => add(
                &mut conditions,
                code,
                "本任务使用范围或计划依赖已变化，请重新预览核对",
                "school",
                false,
                device,
            ),
            Err(e) => return Err(e),
        }
        match authorize(tx, uid(attempt, "confirmed_by")?, Some(school), true).await {
            Ok(()) => {}
            Err(Error::Forbidden | Error::Unauthorized) => add(
                &mut conditions,
                "confirmation_permission_changed",
                "原确认人的学校发送权限已失效",
                "school",
                false,
                device,
            ),
            Err(e) => return Err(e),
        }
        let consent:Option<bool>=sqlx::query_scalar("SELECT c.accepted FROM mobile_shared_delegations d JOIN LATERAL(SELECT accepted FROM mobile_shared_delegation_decisions WHERE delegation_id=d.id ORDER BY revision DESC LIMIT 1)c ON true WHERE d.device_id=$1 AND d.credential_generation=$2").bind(device).bind(plan["credential_generation"].as_i64().unwrap_or(0)).fetch_optional(&mut **tx).await?;
        match consent {
            Some(true) => {}
            Some(false) => add(
                &mut conditions,
                "consent_revoked",
                "本机已撤回共享托管同意",
                "device",
                false,
                device,
            ),
            None => add(
                &mut conditions,
                "consent_unknown",
                "托管状态待同步，不代表用户未授权",
                "system",
                true,
                device,
            ),
        }
        if runtime["fresh"] != true {
            add(
                &mut conditions,
                "runtime_stale",
                "等待手机上报当前执行条件，不要求重新授权",
                "system",
                true,
                device,
            );
        }
        if runtime["online"] != true {
            add(
                &mut conditions,
                "device_offline",
                "等待手机后台连接恢复",
                "device",
                true,
                device,
            );
        }
        if runtime["fresh"] == true && runtime["paused"] == true {
            add(
                &mut conditions,
                "execution_paused",
                "手机执行已暂停",
                "device",
                false,
                device,
            );
        }
        if runtime["fresh"] == true && runtime["ready"] != true {
            let (code, message) = if runtime["conditions"]["unlocked"] == false {
                ("device_locked", "等待手机解锁")
            } else if runtime["conditions"]["accessibility"] == false {
                ("accessibility_required", "等待手机无障碍服务恢复")
            } else if runtime["conditions"]["notification"] == false {
                ("notification_required", "等待手机通知权限恢复")
            } else {
                ("runtime_not_ready", "等待手机界面执行条件恢复")
            };
            add(&mut conditions, code, message, "device", true, device);
        }
        let risk = match tasks::validate_risk(tx, school, plan).await {
            Ok(()) => false,
            Err(Error::Policy(_)) => true,
            Err(e) => return Err(e),
        };
        if risk {
            add(
                &mut conditions,
                "shared_send_review_required",
                "存在不确定调用，请先人工核对相关手机或目标群的原结果",
                "school",
                false,
                device,
            );
        } else {
            for source in inspections::busy_sources(tx, device).await? {
                let code = source["code"].as_str().ok_or(Error::Conflict)?;
                let message = match code {
                    "device_claim_active" => "手机仍持有原任务领取，等待安全恢复",
                    "device_lease_active" => "等待原界面租约安全释放",
                    "device_idle_unconfirmed" => "手机静止状态已过期，等待重新核验",
                    "device_invocation_risk" => "原调用结果待核对，禁止重新发送",
                    _ => "等待手机当前界面操作停止并确认静止",
                };
                add(&mut conditions, code, message, "system", true, device);
                if let Some(condition) = conditions.last_mut() {
                    condition.valid_until = serde_json::from_value(source["expires_at"].clone())
                        .map_err(|e| Error::Invalid(e.to_string()))?;
                    condition.retry_at = condition.valid_until;
                    if let Ok(at) = serde_json::from_value(source["observed_at"].clone()) {
                        condition.observed_at = at;
                    }
                    condition.scope_type = if code == "device_claim_active" {
                        "claim"
                    } else {
                        "device"
                    }
                    .into();
                    if code == "device_claim_active" {
                        condition.scope_id = uid(&source, "id")?;
                    }
                }
            }
        }
        let closed = match business_scheme::ensure_open(tx).await {
            Ok(()) => false,
            Err(Error::Policy(_)) => true,
            Err(e) => return Err(e),
        };
        if closed {
            add(
                &mut conditions,
                "shared_send_platform_closed",
                "平台已暂停实际执行",
                "platform",
                true,
                device,
            );
        }
        let rules_valid = match tasks::validate_execution_rules(tx, plan).await {
            Ok(()) => true,
            Err(Error::Policy(_)) => false,
            Err(e) => return Err(e),
        };
        if !rules_valid {
            add(
                &mut conditions,
                "shared_send_execution_configuration_invalid",
                "执行配置缺失或损坏，本次未执行",
                "system",
                false,
                device,
            );
        } else {
            let inspection:Option<Value>=sqlx::query_scalar("SELECT to_jsonb(i)-'challenge' FROM mobile_v9_send_inspections i JOIN mobile_v9_runtime_details r ON r.device_id=i.device_id WHERE i.device_id=$1 AND i.binding_id=$2 AND i.dependencies=$3 AND i.purpose='automatic' AND i.state='passed' AND i.expires_at>clock_timestamp() AND i.result->>'ui_context_epoch'=r.ui_context_epoch LIMIT 1").bind(device).bind(uid(plan,"binding_id")?).bind(tasks::dependencies(plan)).fetch_optional(&mut **tx).await?;
            if inspection.is_none() {
                add(
                    &mut conditions,
                    "waiting_inspection",
                    "等待当前群自动无发送检查通过",
                    "system",
                    true,
                    device,
                );
                let p = policy(tx).await?;
                let retry_seconds = p
                    .as_ref()
                    .and_then(|v| v["retry_seconds"].as_i64())
                    .unwrap_or(300);
                let retry_limit = p
                    .as_ref()
                    .and_then(|v| v["retry_limit"].as_i64())
                    .unwrap_or(3);
                let pending:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_v9_send_inspections WHERE binding_id=$1 AND dependencies=$2 AND state IN ('pending','leased') AND expires_at>clock_timestamp())").bind(uid(plan,"binding_id")?).bind(tasks::dependencies(plan)).fetch_one(&mut **tx).await?;
                if !pending {
                    let budget = inspection_budget::load(
                        tx,
                        uid(plan, "binding_id")?,
                        tasks::dependencies(plan),
                        retry_seconds,
                    )
                    .await?;
                    if budget.exhausted(retry_limit) {
                        let condition = conditions.last_mut().expect("inspection condition");
                        condition.code = "inspection_retry_exhausted".into();
                        condition.message =
                            "自动检查暂未完成，预算恢复后自动继续；可查看失败原因".into();
                        condition.retry_at = budget.next_retry(retry_limit);
                    } else if budget.waiting(retry_limit) {
                        let condition = conditions.last_mut().expect("inspection condition");
                        condition.code = "inspection_retry_wait".into();
                        condition.message = "当前群检查失败，等待自动重试时间".into();
                        condition.retry_at = budget.next_retry(retry_limit);
                    }
                }
            }
        }
        let quota_available = match device::quota(
            tx,
            school,
            device,
            uid(plan, "target_id")?,
            uid(attempt, "id")?,
            false,
        )
        .await
        {
            Ok(available) => available,
            Err(Error::Policy(code)) => {
                add(
                    &mut conditions,
                    code,
                    "发送政策暂不可执行，请管理员查看政策配置；任务已保留",
                    "school",
                    false,
                    device,
                );
                true
            }
            Err(error) => return Err(error),
        };
        if !quota_available {
            add(
                &mut conditions,
                "quota_wait",
                "等待发送时段、冷却或可用额度",
                "system",
                true,
                device,
            );
        }
    }
    if state == "invalidated" {
        add(
            &mut conditions,
            attempt["invalidation_reason"]
                .as_str()
                .unwrap_or("plan_dependency_changed"),
            if attempt["invalidation_reason"] == "shared_send_execution_configuration_invalid" {
                "执行配置缺失或损坏，本次未执行，请重新创建任务"
            } else {
                "执行依据已失效，请核对原计划后重新安排"
            },
            "school",
            false,
            device,
        );
    }
    for c in &mut conditions {
        let scope = match c.code.as_str() {
            "task_stopped"
            | "confirmation_expired"
            | "previous_target_pending"
            | "waiting_interval"
            | "confirmation_permission_changed" => Some(("task", uid(task, "id")?)),
            "shared_send_review_required" => Some(("target", uid(plan, "target_id")?)),
            "waiting_inspection" | "inspection_retry_wait" | "inspection_retry_exhausted" => {
                Some(("binding", uid(plan, "binding_id")?))
            }
            _ => None,
        };
        if let Some((kind, id)) = scope {
            c.scope_type = kind.into();
            c.scope_id = id;
        }
    }
    let observed = wait_states::observations(tx, uid(attempt, "id")?).await?;
    if let Some(rows) = observed.as_ref().and_then(Value::as_array) {
        for condition in &mut conditions {
            if let Some(old) = rows
                .iter()
                .find(|v| v["code"] == condition.code && v["scope_id"] == json!(condition.scope_id))
            {
                condition.waiting_since = serde_json::from_value(old["waiting_since"].clone())
                    .ok()
                    .flatten();
                if old["message"] == condition.message && old["owner"] == condition.owner {
                    condition.reason_changed_at =
                        serde_json::from_value(old["reason_changed_at"].clone())
                            .ok()
                            .flatten();
                }
            }
        }
    }
    serde_json::to_value(ExecutionStatus {
        phase: ExecutionPhase::project(state, attempt["current"] == true, !conditions.is_empty()),
        observed_at: chrono::Utc::now(),
        device_state_observed_at: serde_json::from_value(runtime["observed_at"].clone())
            .map_err(|e| Error::Invalid(e.to_string()))?,
        consent_observed_at,
        conditions,
    })
    .map_err(|e| Error::Invalid(e.to_string()))
}
