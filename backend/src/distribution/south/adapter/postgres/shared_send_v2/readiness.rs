//! Planning remains possible while device conditions are waiting.
use super::*;
pub(super) async fn school(tx: &mut Tx<'_>, school: Uuid) -> Result<Vec<Value>> {
    let policy = policy(tx).await?;
    let rows:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(b)||jsonb_build_object('device_name',d.name,'target_name',coalesce(nullif(t.alias,''),t.search_text),'search_text',coalesce(b.search_text_override,t.search_text),'online',coalesce(cp.state='online' AND cp.last_control_heartbeat_at>clock_timestamp()-interval '30 seconds',false),'ui_ready',coalesce(rt.ui_ready AND NOT rt.paused AND rt.credential_generation=d.credential_generation AND rt.observed_at>clock_timestamp()-interval '30 seconds' AND NOT cp.desired_paused,false),'context_epoch',rd.ui_context_epoch) FROM mobile_device_target_bindings b JOIN mobile_targets t ON t.id=b.target_id JOIN execution_devices d ON d.id=b.device_id LEFT JOIN device_control_presence cp ON cp.id=d.id LEFT JOIN mobile_shared_runtime rt ON rt.device_id=d.id LEFT JOIN mobile_v9_runtime_details rd ON rd.device_id=d.id WHERE b.school_id=$1 ORDER BY d.name,b.id").bind(school).fetch_all(&mut **tx).await?;
    let mut result = Vec::new();
    for row in rows {
        let candidate = tasks::raw_candidates(tx, school, uid(&row, "target_id")?)
            .await?
            .into_iter()
            .find(|c| c["device_id"] == row["device_id"]);
        let selectable = candidate.is_some();
        let mut stages = Vec::new();
        if let Some(ref c) = candidate {
            for stage in ["test_send", "production_allowlist"] {
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
            for stage in ["test_send", "production_allowlist"] {
                if tasks::grant(tx, school, c, stage).await?.is_some() {
                    scope_exists = true;
                }
            }
        }
        let risk:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_v9_invocations WHERE device_id=$1 AND state='risk_held') OR EXISTS(SELECT 1 FROM mobile_shared_invocations WHERE device_id=$1 AND state='risk_held') OR EXISTS(SELECT 1 FROM mobile_v9_risk_guards WHERE school_id=$2 AND target_id=$3 AND state='active') OR EXISTS(SELECT 1 FROM mobile_shared_risk_guards WHERE school_id=$2 AND target_id=$3 AND state='active')").bind(uid(&row,"device_id")?).bind(school).bind(uid(&row,"target_id")?).fetch_one(&mut **tx).await?;
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
        let helper_update:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_capability_deliveries d JOIN mobile_v9_runtime_details r ON r.device_id=$1 WHERE d.release_id=$2 AND NOT (r.apk_sha256=ANY(d.supported_apks)))").bind(uid(&row,"device_id")?).bind(candidate.as_ref().and_then(|c|c["release_id"].as_str()).and_then(|v|v.parse::<Uuid>().ok())).fetch_one(&mut **tx).await?;
        let mut blockers = Vec::new();
        let mut add = |code: &str, message: &str, owner: &str, action: &str, automatic: bool| {
            blockers.push(json!({"code":code,"message":message,"owner":owner,"action":action,"automatic":automatic,"observed_at":chrono::Utc::now()}))
        };
        if helper_update {
            add(
                "shared_send_helper_update_required",
                "手机助手需要更新至产品支持版本",
                "device",
                "更新手机助手后等待自动检查",
                false,
            );
        }
        if !selectable {
            add(
                "resource_unavailable",
                "绑定、手机、群或共享资格已停用",
                "platform",
                "检查共享资格和群绑定",
                false,
            );
        }
        if selectable
            && candidate
                .as_ref()
                .is_some_and(|c| c["release_id"].is_null())
        {
            add(
                "release_missing",
                "产品尚未交付",
                "engineering",
                "打开能力办理，完成候选验证与限定测试放行",
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
        if busy && !risk {
            add(
                "waiting_physical_owner",
                "手机正在处理原有界面工作",
                "system",
                "等待原会话结束并确认手机静止",
                true,
            );
        }
        if policy.as_ref().is_none_or(|p| p["mode"] != "enabled") {
            add(
                "policy_not_enabled",
                "使用方案尚未启用",
                "platform",
                "等待产品提供可用的使用方案",
                false,
            );
        }
        if !scope_exists {
            add(
                "business_scope_missing",
                "本校手机和目标群尚无有效使用授权",
                "platform",
                "打开手机群消息使用管理开通或核对授权",
                false,
            );
        }
        if stages.is_empty()
            && scope_exists
            && !risk
            && policy.as_ref().is_some_and(|p| p["mode"] == "enabled")
        {
            add(
                "stage_not_allowed",
                "政策允许阶段与有效业务范围不一致",
                "platform",
                "核对限定测试或生产阶段设置",
                false,
            );
        }
        if !accepted {
            add(
                "waiting_initial_consent",
                "等待手机本人首次授权或恢复托管",
                "device",
                "在手机打开平台连接与长期托管",
                false,
            );
        }
        if row["online"] != true {
            add(
                "waiting_connection",
                "手机离线，连接后自动继续",
                "system",
                "等待连接恢复",
                true,
            );
        }
        if row["ui_ready"] != true {
            add(
                "waiting_device_ready",
                "等待解锁、无障碍和通知权限",
                "device",
                "查看手机执行条件",
                true,
            );
        }
        if !passed {
            add(
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
            );
        }
        let ready = !helper_update
            && selectable
            && !busy
            && !stages.is_empty()
            && accepted
            && row["online"] == true
            && row["ui_ready"] == true
            && passed;
        let inspection_state = if passed {
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
        result.push(json!({"binding_id":row["id"],"device_id":row["device_id"],"target_id":row["target_id"],"device_name":row["device_name"],"target_name":row["target_name"],"search_text":row["search_text"],"plan_selectable":selectable,"confirmable":!stages.is_empty(),"confirmable_stages":stages,"execution_ready":ready,"ready":ready,"state":if ready{json!("ready_for_task")}else{blockers.first().map(|b|b["code"].clone()).unwrap_or(json!("waiting"))},"blockers":blockers,"inspection":inspection,"inspection_state":inspection_state,"retry_remaining":remaining,"retry_at":retry_at,"automatic":true}));
    }
    Ok(result)
}
pub(super) async fn pool(tx: &mut Tx<'_>, s: Uuid) -> Result<Vec<Value>> {
    sqlx::query_scalar("SELECT jsonb_build_object('id',d.id,'name',d.name,'access_revision',a.revision) FROM mobile_shared_school_access a JOIN execution_devices d ON d.id=a.device_id JOIN schools s ON s.id=a.school_id WHERE a.school_id=$1 AND a.enabled AND d.enabled AND s.active ORDER BY d.name,d.id").bind(s).fetch_all(&mut **tx).await.map_err(Into::into)
}
