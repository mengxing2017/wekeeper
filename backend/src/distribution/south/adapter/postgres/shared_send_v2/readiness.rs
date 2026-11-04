//! Planning remains possible while device conditions are waiting.
use super::*;
pub(super) async fn school(tx: &mut Tx<'_>, school: Uuid) -> Result<Vec<Value>> {
    let policy = policy(tx).await?;
    let rows:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(b)||jsonb_build_object('device_name',d.name,'target_name',coalesce(nullif(t.alias,''),t.search_text),'search_text',coalesce(b.search_text_override,t.search_text),'online',coalesce(cp.state='online' AND cp.last_control_heartbeat_at>clock_timestamp()-interval '30 seconds',false),'ui_ready',coalesce(rt.ui_ready AND NOT rt.paused AND rt.credential_generation=d.credential_generation AND rt.observed_at>clock_timestamp()-interval '30 seconds' AND NOT cp.desired_paused,false),'context_epoch',rd.ui_context_epoch,'runtime_observed_at',rt.observed_at,'runtime_fresh',coalesce(rt.credential_generation=d.credential_generation AND rd.credential_generation=d.credential_generation AND rt.observed_at>clock_timestamp()-interval '30 seconds' AND rd.observed_at>clock_timestamp()-interval '30 seconds',false),'runtime_conditions',rd.conditions,'runtime_reason',rd.ready_reason,'preparation',rd.preparation,'paused',coalesce(rt.paused,false) OR coalesce(cp.desired_paused,false)) FROM mobile_device_target_bindings b JOIN mobile_targets t ON t.id=b.target_id JOIN execution_devices d ON d.id=b.device_id LEFT JOIN device_control_presence cp ON cp.id=d.id LEFT JOIN mobile_shared_runtime rt ON rt.device_id=d.id LEFT JOIN mobile_v9_runtime_details rd ON rd.device_id=d.id WHERE b.school_id=$1 ORDER BY d.name,b.id").bind(school).fetch_all(&mut **tx).await?;
    let mut result = Vec::new();
    for row in rows {
        let candidate = tasks::raw_candidates(tx, school, uid(&row, "target_id")?)
            .await?
            .into_iter()
            .find(|c| c["device_id"] == row["device_id"]);
        let selectable = candidate.is_some();
        let confirmable = if let Some(ref c) = candidate {
            tasks::validate_plan(tx, school, c).await.is_ok()
        } else {
            false
        };
        let inspection: Option<Value> = None;
        let accepted = candidate.as_ref().is_some_and(|c| c["accepted"] == true);
        let scope_exists = if let Some(ref c) = candidate {
            tasks::validate_scope(tx, school, c).await.is_ok()
        } else {
            false
        };
        let risk:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_v9_invocations WHERE device_id=$1 AND state='risk_held') OR EXISTS(SELECT 1 FROM mobile_v9_risk_guards WHERE school_id=$2 AND target_id=$3 AND state='active')").bind(uid(&row,"device_id")?).bind(school).bind(uid(&row,"target_id")?).fetch_one(&mut **tx).await?;
        let remaining = 0;
        let retry_at = Value::Null;
        let busy = inspections::busy(tx, uid(&row, "device_id")?).await?;
        let mut blockers = Vec::new();
        let mut add = |code: &str, message: &str, owner: &str, action: &str, automatic: bool| {
            blockers.push(json!({"code":code,"message":message,"owner":owner,"action":action,"automatic":automatic,"observed_at":chrono::Utc::now()}))
        };
        if !selectable {
            add(
                "resource_unavailable",
                "绑定、手机、群或共享资格已停用",
                "platform",
                "检查共享资格和群绑定",
                false,
            );
        }
        if risk {
            add(
                "risk_held",
                "存在未核对发送风险，禁止新界面操作",
                "school",
                "打开当前或历史任务核对原结果",
                false,
            );
        }
        if !scope_exists && selectable {
            blockers.push(
                scope_issue(tx, school, uid(&row, "device_id")?, uid(&row, "target_id")?).await?,
            );
        }
        let consent_state = candidate
            .as_ref()
            .and_then(|c| c["consent_state"].as_str())
            .unwrap_or("unknown");
        let execution_conditions = execution_conditions(
            &row,
            consent_state,
            busy,
            policy.as_ref().and_then(|p| p["mode"].as_str()),
        );
        let ready = selectable
            && !busy
            && confirmable
            && accepted
            && row["online"] == true
            && runtime_state(&row) == "ready";
        let inspection_state = "not_scheduled";
        result.push(json!({"binding_id":row["id"],"device_id":row["device_id"],"target_id":row["target_id"],"device_name":row["device_name"],"target_name":row["target_name"],"search_text":row["search_text"],"plan_selectable":selectable,"confirmable":confirmable,"execution_ready":ready,"ready":ready,"state":if ready{json!("ready_for_task")}else{blockers.first().or_else(||execution_conditions.first()).map(|b|b["code"].clone()).unwrap_or(json!("waiting"))},"consent_state":consent_state,"preparation":row["preparation"],"runtime_state":runtime_state(&row),"runtime_reason":row["runtime_reason"],"runtime_observed_at":row["runtime_observed_at"],"execution_conditions":execution_conditions,"blockers":blockers,"inspection":inspection,"inspection_state":inspection_state,"retry_remaining":remaining,"retry_at":retry_at,"automatic":true}));
    }
    Ok(result)
}
pub(super) async fn pool(tx: &mut Tx<'_>, s: Uuid) -> Result<Vec<Value>> {
    sqlx::query_scalar("SELECT jsonb_build_object('id',d.id,'name',d.name,'access_revision',a.revision) FROM mobile_shared_school_access a JOIN execution_devices d ON d.id=a.device_id JOIN schools s ON s.id=a.school_id WHERE a.school_id=$1 AND a.enabled AND d.enabled AND s.active ORDER BY d.name,d.id").bind(s).fetch_all(&mut **tx).await.map_err(Into::into)
}

fn condition(code: &str, message: &str, owner: &str, action: &str, automatic: bool) -> Value {
    json!({"code":code,"message":message,"owner":owner,"action":action,"automatic":automatic,"observed_at":chrono::Utc::now()})
}
fn runtime_state(row: &Value) -> &'static str {
    if row["runtime_observed_at"].is_null() {
        "unknown"
    } else if row["runtime_fresh"] != true {
        "stale"
    } else if row["paused"] == true {
        "paused"
    } else if !super::screen::facts_complete(&row["runtime_conditions"]) {
        "unknown"
    } else if let Some(state) = super::screen::state(&row["runtime_conditions"]) {
        state
    } else if row["runtime_conditions"]["accessibility"] == false {
        "accessibility_required"
    } else if row["runtime_conditions"]["notification"] == false {
        "notification_required"
    } else if row["ui_ready"] == true {
        "ready"
    } else {
        "waiting"
    }
}
fn execution_conditions(row: &Value, consent: &str, busy: bool, mode: Option<&str>) -> Vec<Value> {
    let mut conditions = Vec::new();
    if mode.is_some_and(|m| m != "enabled") {
        conditions.push(condition(
            "execution_paused",
            "平台暂停实际执行，任务仍可提交排队",
            "platform",
            "等待平台恢复执行",
            true,
        ));
    }
    match consent {
        "accepted" => {}
        "revoked" => conditions.push(condition(
            "consent_revoked",
            "本机已撤回共享托管同意；任务可提交，执行需本人恢复同意",
            "device",
            "在手机查看共享托管状态",
            false,
        )),
        _ => conditions.push(condition(
            "consent_unknown",
            "当前设备的托管状态尚未同步，不能据此判定未授权；不影响提交任务",
            "system",
            "后台同步当前设备与凭据代次的托管状态",
            true,
        )),
    }
    if row["online"] != true {
        conditions.push(condition(
            "waiting_connection",
            "手机当前离线，任务可提交，连接后继续",
            "system",
            "等待连接恢复",
            true,
        ));
    }
    let screen_issue = if row["runtime_fresh"] == true
        && row["paused"] != true
        && (row["ui_ready"] != true || super::screen::state(&row["runtime_conditions"]).is_some())
    {
        super::screen::issue(&row["runtime_conditions"], &row["preparation"])
    } else {
        None
    };
    if let Some((code, message, action, automatic)) = screen_issue {
        conditions.push(condition(code, message, "device", action, automatic));
    }
    let (code, message, owner, action) = match runtime_state(row) {
        "unknown" => (
            "runtime_unknown",
            "尚未收到手机执行状态，不代表系统权限未开启",
            "system",
            "等待手机上报执行状态",
        ),
        "stale" => (
            "runtime_stale",
            "手机执行状态已过期，不要求重新授权",
            "system",
            "等待手机更新执行状态",
        ),
        "paused" => (
            "runtime_paused",
            "手机执行已暂停，已有托管同意保留",
            "device",
            "在手机恢复执行",
        ),

        "accessibility_required" => (
            "accessibility_required",
            "手机最新状态报告无障碍服务不可用",
            "device",
            "在手机检查无障碍服务",
        ),
        "notification_required" => (
            "notification_required",
            "手机最新状态报告通知不可用",
            "device",
            "在手机检查通知权限",
        ),
        "secure_locked" | "screen_off" | "keyguard_locked" => ("", "", "", ""),
        "waiting" if super::screen::runtime_issue(&row["runtime_reason"]).is_some() => {
            let (code, message) = super::screen::runtime_issue(&row["runtime_reason"]).unwrap();
            (code, message, "device", "等待手机完成当前处理")
        }
        "waiting" => (
            "waiting_device_ready",
            "尚未收到明确就绪原因，系统正在同步；不要求重新授权",
            "system",
            "等待手机更新状态",
        ),
        _ => ("", "", "", ""),
    };
    if !code.is_empty() {
        conditions.push(condition(code, message, owner, action, true));
    }
    if busy {
        conditions.push(condition(
            "waiting_physical_owner",
            "手机正在处理原工作，当前任务可以提交排队",
            "system",
            "等待原工作结束",
            true,
        ));
    }
    conditions
}
async fn scope_issue(tx: &mut Tx<'_>, school: Uuid, device: Uuid, target: Uuid) -> Result<Value> {
    // Reuse the single business judgment that also gates preview, confirm and claim,
    // so the readiness page never guesses a different reason than the real rejection.
    // Only an explicit pause, revocation or expiry blocks; pending, declined and
    // superseded history, or a projection still being reconciled, are not a disable.
    let (code, message) = match tasks::scope_control(tx, school, device, target).await? {
        tasks::ScopeControl::Revoked => (
            "business_scope_revoked",
            "此学校使用范围已撤销，手机托管同意不受此状态说明影响",
        ),
        tasks::ScopeControl::Paused => (
            "business_scope_paused",
            "此学校使用范围已暂停，请在使用管理恢复",
        ),
        tasks::ScopeControl::Expired => {
            ("business_scope_expired", "此学校使用范围已到期，请办理续期")
        }
        // Authorized never reaches here because validate_scope already passed; kept as
        // a non-blocking note should the execution projection still be reconciling.
        tasks::ScopeControl::Authorized => (
            "business_scope_sync_pending",
            "使用范围已批准，后台正在关联手机与目标群",
        ),
    };
    Ok(condition(
        code,
        message,
        "school",
        "查看本校手机与目标群使用管理",
        code == "business_scope_sync_pending",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn false_readiness_is_not_busy_and_missing_screen_facts_are_unknown() {
        let mut row = serde_json::json!({"online":true,"runtime_observed_at":"now","runtime_fresh":true,"ui_ready":false,"runtime_conditions":{"interactive":true,"keyguard_locked":false,"device_locked":false,"keyguard_secure":false}});
        let conditions = execution_conditions(&row, "accepted", false, Some("enabled"));
        assert!(
            conditions
                .iter()
                .any(|v| v["code"] == "waiting_device_ready")
        );
        assert!(
            !conditions
                .iter()
                .any(|v| v["code"] == "waiting_physical_owner")
        );
        assert!(
            execution_conditions(&row, "accepted", true, Some("enabled"))
                .iter()
                .any(|v| v["code"] == "waiting_physical_owner")
        );
        row["runtime_conditions"] = serde_json::json!({"interactive":false});
        assert_eq!(runtime_state(&row), "unknown");
        row["preparation"] = json!({"automatic_wake_enabled":true,"last_error":"wake_timeout"});
        let partial = execution_conditions(&row, "accepted", false, Some("enabled"));
        assert_eq!(partial.len(), 1);
        assert_eq!(partial[0]["code"], "runtime_unknown");
    }
    #[test]
    fn unknown_and_stale_runtime_never_imply_missing_permissions_or_consent() {
        let row = json!({"online":true,"ui_ready":false});
        assert_eq!(runtime_state(&row), "unknown");
        let c = execution_conditions(&row, "accepted", false, Some("enabled"));
        assert_eq!(c[0]["code"], "runtime_unknown");
        assert_eq!(c.len(), 1);
        let row = json!({"runtime_observed_at":"2026-11-03","runtime_fresh":false,"runtime_conditions":{"accessibility":false}});
        assert_eq!(runtime_state(&row), "stale");
    }
    #[test]
    fn fresh_runtime_reports_the_actual_condition_and_preserves_consent() {
        let mut row = json!({"online":true,"runtime_observed_at":"now","runtime_fresh":true,"runtime_conditions":{"interactive":true,"keyguard_locked":false,"device_locked":false,"keyguard_secure":false,"unlocked":true,"accessibility":true,"notification":true},"ui_ready":true});
        assert!(execution_conditions(&row, "accepted", false, Some("enabled")).is_empty());
        row["runtime_conditions"]["interactive"] = json!(false);
        assert_eq!(runtime_state(&row), "screen_off");
        row["preparation"] = json!({"automatic_wake_enabled":true});
        assert_eq!(
            execution_conditions(&row, "accepted", false, Some("enabled"))[0]["code"],
            "device_screen_off"
        );
        row["runtime_conditions"]["device_locked"] = json!(true);
        row["runtime_conditions"]["keyguard_secure"] = json!(true);
        assert_eq!(runtime_state(&row), "secure_locked");
        assert_eq!(
            execution_conditions(&row, "accepted", false, Some("enabled"))[0]["automatic"],
            false
        );
        row["runtime_conditions"]["interactive"] = json!(true);
        row["runtime_conditions"]["device_locked"] = json!(false);
        row["runtime_conditions"]["accessibility"] = json!(false);
        assert_eq!(runtime_state(&row), "accessibility_required");
        assert_eq!(
            execution_conditions(&row, "unknown", false, None)[0]["code"],
            "consent_unknown"
        );
        assert_eq!(
            execution_conditions(&row, "revoked", false, None)[0]["code"],
            "consent_revoked"
        );
    }
}
