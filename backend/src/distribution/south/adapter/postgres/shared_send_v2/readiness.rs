//! Planning remains possible while device conditions are waiting.
use super::*;
pub(super) async fn school(tx: &mut Tx<'_>, school: Uuid) -> Result<Vec<Value>> {
    let policy = policy(tx).await?;
    let rows:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(b)||jsonb_build_object('device_name',d.name,'target_name',coalesce(nullif(t.alias,''),t.search_text),'search_text',coalesce(b.search_text_override,t.search_text),'online',coalesce(cp.state='online' AND cp.last_control_heartbeat_at>clock_timestamp()-interval '30 seconds',false),'ui_ready',coalesce(rt.ui_ready AND NOT rt.paused AND rt.credential_generation=d.credential_generation AND rt.observed_at>clock_timestamp()-interval '30 seconds' AND NOT cp.desired_paused,false),'context_epoch',rd.ui_context_epoch,'runtime_observed_at',rt.observed_at,'runtime_fresh',coalesce(rt.credential_generation=d.credential_generation AND rd.credential_generation=d.credential_generation AND rt.observed_at>clock_timestamp()-interval '30 seconds' AND rd.observed_at>clock_timestamp()-interval '30 seconds',false),'runtime_conditions',rd.conditions,'paused',coalesce(rt.paused,false) OR coalesce(cp.desired_paused,false)) FROM mobile_device_target_bindings b JOIN mobile_targets t ON t.id=b.target_id JOIN execution_devices d ON d.id=b.device_id LEFT JOIN device_control_presence cp ON cp.id=d.id LEFT JOIN mobile_shared_runtime rt ON rt.device_id=d.id LEFT JOIN mobile_v9_runtime_details rd ON rd.device_id=d.id WHERE b.school_id=$1 ORDER BY d.name,b.id").bind(school).fetch_all(&mut **tx).await?;
    let mut result = Vec::new();
    for row in rows {
        let candidate = tasks::raw_candidates(tx, school, uid(&row, "target_id")?)
            .await?
            .into_iter()
            .find(|c| c["device_id"] == row["device_id"]);
        let selectable = candidate.is_some();
        let mut stages = Vec::new();
        if let Some(ref c) = candidate {
            for stage in ["production_allowlist"] {
                let mut p = c.clone();
                p["stage"] = json!(stage);
                if tasks::validate_plan(tx, school, &p).await.is_ok() {
                    stages.push(stage);
                }
            }
        }
        let inspection = if let Some(ref c) = candidate {
            sqlx::query_scalar::<_,Value>("SELECT to_jsonb(i)-'challenge' FROM mobile_v9_send_inspections i WHERE binding_id=$1 AND dependencies=$2 AND purpose='automatic' ORDER BY created_at DESC,id DESC LIMIT 1").bind(uid(&row,"id")?).bind(tasks::dependencies(c)).fetch_optional(&mut **tx).await?
        } else {
            None
        };
        let passed = inspection.as_ref().is_some_and(|i| {
            i["state"] == "passed"
                && i["result"]["ui_context_epoch"] == row["context_epoch"]
                && i["expires_at"]
                    .as_str()
                    .and_then(|s| s.parse::<chrono::DateTime<chrono::Utc>>().ok())
                    .is_some_and(|t| t > chrono::Utc::now())
        });
        let accepted = candidate.as_ref().is_some_and(|c| c["accepted"] == true);
        let mut scope_exists = false;
        if let Some(ref c) = candidate {
            for stage in ["production_allowlist"] {
                if tasks::validate_scope(tx, school, c, stage).await.is_ok() {
                    scope_exists = true;
                }
            }
        }
        let risk:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_v9_invocations WHERE device_id=$1 AND state='risk_held') OR EXISTS(SELECT 1 FROM mobile_v9_risk_guards WHERE school_id=$2 AND target_id=$3 AND state='active')").bind(uid(&row,"device_id")?).bind(school).bind(uid(&row,"target_id")?).fetch_one(&mut **tx).await?;
        let retry_seconds = policy
            .as_ref()
            .and_then(|p| p["retry_seconds"].as_i64())
            .unwrap_or(300);
        let retry_limit = policy
            .as_ref()
            .and_then(|p| p["retry_limit"].as_i64())
            .unwrap_or(3);
        let retry = if let Some(ref c) = candidate {
            sqlx::query_scalar::<_,Value>("SELECT jsonb_build_object('used',count(*),'retry_at',max(created_at)+make_interval(secs=>$3::double precision),'budget_at',min(created_at)+interval '24 hours') FROM mobile_v9_send_inspections WHERE binding_id=$1 AND dependencies=$2 AND state IN ('failed','stale','pending','leased') AND created_at>clock_timestamp()-interval '24 hours'").bind(uid(&row,"id")?).bind(tasks::dependencies(c)).bind(retry_seconds as f64).fetch_one(&mut **tx).await?
        } else {
            json!({"used":0})
        };
        let remaining = (retry_limit - retry["used"].as_i64().unwrap_or(0)).max(0);
        let active_check = inspection.as_ref().is_some_and(|i| {
            matches!(i["state"].as_str(), Some("pending" | "leased"))
                && i["expires_at"]
                    .as_str()
                    .and_then(|v| v.parse::<chrono::DateTime<chrono::Utc>>().ok())
                    .is_some_and(|t| t > chrono::Utc::now())
        });
        let exhausted = remaining == 0 && !active_check;
        let retry_at = if active_check {
            Value::Null
        } else if exhausted {
            retry["budget_at"].clone()
        } else {
            retry["retry_at"].clone()
        };
        let retry_wait = retry_at
            .as_str()
            .and_then(|v| v.parse::<chrono::DateTime<chrono::Utc>>().ok())
            .is_some_and(|t| t > chrono::Utc::now());
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
        let mut execution_conditions = execution_conditions(
            &row,
            consent_state,
            busy,
            policy.as_ref().and_then(|p| p["mode"].as_str()),
        );
        let missing_rules = candidate.as_ref().is_some_and(tasks::missing_rules);
        if !passed && !missing_rules {
            execution_conditions.push(condition(
                if exhausted {
                    "inspection_budget_exhausted"
                } else {
                    "waiting_inspection"
                },
                if exhausted {
                    "自动检查预算已耗尽，需审查失败原因"
                } else if retry_wait {
                    "自动检查等待配置的退避时间"
                } else {
                    "等待当前手机上的无发送检查"
                },
                if exhausted { "engineering" } else { "system" },
                if exhausted {
                    "查看失败诊断并处理适配支持；重新评估不会重置预算"
                } else {
                    "后台按预算自动检查；失败后可提交适配支持"
                },
                !exhausted,
            ));
        }
        let ready = selectable
            && !busy
            && !stages.is_empty()
            && accepted
            && row["online"] == true
            && row["ui_ready"] == true
            && (passed || missing_rules);
        let inspection_state = if missing_rules {
            "check_on_execution"
        } else if passed {
            "passed"
        } else if exhausted {
            "retry_exhausted"
        } else if !accepted || row["online"] != true || row["ui_ready"] != true {
            "waiting_prerequisites"
        } else {
            match inspection.as_ref().and_then(|i| i["state"].as_str()) {
                Some("pending") => "queued",
                Some("leased") => "running",
                Some("passed") => "expired",
                Some("failed" | "stale") if retry_wait => "retry_wait",
                Some("failed") => "failed",
                Some("stale") => "stale",
                _ => "not_scheduled",
            }
        };
        result.push(json!({"binding_id":row["id"],"device_id":row["device_id"],"target_id":row["target_id"],"device_name":row["device_name"],"target_name":row["target_name"],"search_text":row["search_text"],"plan_selectable":selectable,"confirmable":!stages.is_empty(),"confirmable_stages":stages,"execution_ready":ready,"ready":ready,"state":if ready{json!("ready_for_task")}else{blockers.first().or_else(||execution_conditions.first()).map(|b|b["code"].clone()).unwrap_or(json!("waiting"))},"consent_state":consent_state,"runtime_state":runtime_state(&row),"runtime_observed_at":row["runtime_observed_at"],"execution_conditions":execution_conditions,"blockers":blockers,"inspection":inspection,"inspection_state":inspection_state,"retry_remaining":remaining,"retry_at":retry_at,"automatic":true}));
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
    } else if row["runtime_conditions"]["unlocked"] == false {
        "locked"
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
        "locked" => (
            "device_locked",
            "手机当前锁屏，任务执行等待解锁",
            "device",
            "解锁手机",
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
        "waiting" => (
            "waiting_device_ready",
            "手机暂未就绪，可能正在处理其他工作；不要求重新授权",
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
        let mut row = json!({"online":true,"runtime_observed_at":"now","runtime_fresh":true,"runtime_conditions":{"unlocked":true,"accessibility":true,"notification":true},"ui_ready":true});
        assert!(execution_conditions(&row, "accepted", false, Some("enabled")).is_empty());
        row["runtime_conditions"]["unlocked"] = json!(false);
        assert_eq!(runtime_state(&row), "locked");
        row["runtime_conditions"]["unlocked"] = json!(true);
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
