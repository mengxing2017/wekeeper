//! Diagnose each resource independently; a missing candidate is not a missing binding.
use super::*;
pub(super) async fn school(tx: &mut Tx<'_>, school: Uuid) -> Result<Vec<Value>> {
    let rows:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(b)||jsonb_build_object('device_name',d.name,'target_name',coalesce(nullif(t.alias,''),t.search_text),'search_text',coalesce(b.search_text_override,t.search_text),'pool_allowed',b.source='platform' OR EXISTS(SELECT 1 FROM mobile_shared_school_access a WHERE a.school_id=b.school_id AND a.device_id=b.device_id AND a.enabled),'device_enabled',d.enabled,'target_enabled',t.enabled AND t.archived_at IS NULL AND s.active,'online',cp.state='online' AND cp.last_control_heartbeat_at>clock_timestamp()-interval '30 seconds','environment_fresh',obs.observed_at>clock_timestamp()-interval '5 minutes' AND e.credential_generation=d.credential_generation,'accepted',coalesce((SELECT v.accepted FROM mobile_shared_delegations m JOIN mobile_shared_delegation_decisions v ON v.delegation_id=m.id WHERE m.device_id=d.id AND m.credential_generation=d.credential_generation ORDER BY v.revision DESC LIMIT 1),false),'release_count',(SELECT count(*) FROM mobile_send_adapter_releases r WHERE r.environment=e.snapshot-'instance_ref' AND NOT EXISTS(SELECT 1 FROM mobile_send_adapter_withdrawals w WHERE w.release_id=r.id)),'risk_count',(SELECT count(*) FROM mobile_shared_invocations i WHERE i.device_id=d.id AND i.state='risk_held'),'paused',coalesce(rt.paused AND rt.credential_generation=d.credential_generation,false) OR coalesce(cp.desired_paused,false),'ui_ready',coalesce(rt.ui_ready,false) AND rt.credential_generation=d.credential_generation AND rt.observed_at>clock_timestamp()-interval '30 seconds') FROM mobile_device_target_bindings b JOIN mobile_targets t ON t.id=b.target_id JOIN schools s ON s.id=t.school_id JOIN execution_devices d ON d.id=b.device_id LEFT JOIN device_control_presence cp ON cp.id=d.id LEFT JOIN mobile_device_environment_observations obs ON obs.device_id=d.id LEFT JOIN mobile_device_environments e ON e.id=obs.snapshot_id LEFT JOIN mobile_shared_runtime rt ON rt.device_id=d.id WHERE b.school_id=$1 ORDER BY b.id").bind(school).fetch_all(&mut **tx).await?;
    let current_policy = policy(tx).await?;
    let policy_released = current_policy
        .as_ref()
        .and_then(|p| p["mode"].as_str())
        .is_some_and(|m| matches!(m, "test_send" | "production_allowlist"));
    let mut out = Vec::new();
    for row in rows {
        let mut blockers = Vec::new();
        let mut add = |code: &str, message: &str, owner: &str, automatic: bool| {
            blockers
                .push(json!({"code":code,"message":message,"owner":owner,"automatic":automatic}))
        };
        if row["pool_allowed"] != true {
            add(
                "shared_pool_access_required",
                "平台已撤销本校使用该手机的资格",
                "platform",
                false,
            )
        }
        if row["enabled"] != true {
            add("binding_disabled", "绑定已停用", "platform", false)
        }
        if row["device_enabled"] != true {
            add("device_disabled", "手机已停用", "platform", false)
        }
        if row["target_enabled"] != true {
            add("target_disabled", "目标群或学校已停用", "school", false)
        }
        if row["accepted"] != true {
            add(
                "waiting_initial_consent",
                "等待手机首次启用托管，若已撤回则需本人重新同意",
                "device",
                false,
            )
        }
        if row["online"] != true {
            add(
                "waiting_connection",
                "手机离线，连接恢复后自动检查",
                "system",
                true,
            )
        }
        if row["environment_fresh"] != true {
            add(
                "waiting_environment",
                "等待后台更新实际应用环境",
                "system",
                true,
            )
        }
        if row["release_count"] != 1 {
            add(
                "waiting_adapter",
                "当前应用没有唯一可用适配，请维护人员核对发行",
                "engineering",
                false,
            )
        }
        if row["paused"] == true {
            add("paused", "托管已暂停，请明确恢复后继续", "device", false)
        }
        if row["risk_count"].as_i64().unwrap_or(0) > 0 {
            add(
                "risk_blocked",
                "存在发送结果待核对，禁止新界面操作",
                "school",
                false,
            )
        }
        if row["ui_ready"] != true {
            add(
                "waiting_device_ready",
                "等待手机解锁并恢复无障碍、通知等执行条件",
                "device",
                true,
            )
        }
        let candidate = tasks::raw_candidates(tx, school, uid(&row, "target_id")?)
            .await?
            .into_iter()
            .find(|x| x["device_id"] == row["device_id"]);
        if candidate.is_none()
            && row["accepted"] == true
            && row["enabled"] == true
            && row["pool_allowed"] == true
            && row["device_enabled"] == true
            && row["target_enabled"] == true
            && row["environment_fresh"] == true
            && row["release_count"] == 1
        {
            add(
                "adapter_invalid",
                "适配规则校验未通过，请维护人员核对发行内容",
                "engineering",
                false,
            );
        }
        let eligible = candidate.is_some();
        let deps = candidate.as_ref().map(tasks::dependencies);
        let current = if let Some(c) = candidate {
            let deps = tasks::dependencies(&c);
            sqlx::query_scalar::<_,Value>("SELECT to_jsonb(i)-'challenge' FROM mobile_shared_send_inspections i WHERE binding_id=$1 AND dependencies=$2 ORDER BY created_at DESC,id DESC LIMIT 1").bind(uid(&row,"id")?).bind(deps).fetch_optional(&mut **tx).await?
        } else {
            None
        };
        let mut failures = 0;
        let mut retry_at: Option<chrono::DateTime<chrono::Utc>> = None;
        if let Some(deps) = deps {
            let (count,first,last):(i64,Option<chrono::DateTime<chrono::Utc>>,Option<chrono::DateTime<chrono::Utc>>)=sqlx::query_as("SELECT count(*),(array_agg(created_at ORDER BY created_at DESC))[3],max(created_at) FROM mobile_shared_send_inspections WHERE binding_id=$1 AND dependencies=$2 AND state IN ('failed','stale','pending','leased') AND created_at>clock_timestamp()-interval '24 hours'").bind(uid(&row,"id")?).bind(deps).fetch_one(&mut **tx).await?;
            failures = count;
            retry_at = if count >= 3 {
                first.map(|t| t + chrono::Duration::hours(24))
            } else {
                last.map(|t| t + chrono::Duration::minutes(5))
            }
            .filter(|t| *t > chrono::Utc::now());
        }
        let live = current
            .as_ref()
            .and_then(|i| i["expires_at"].as_str())
            .and_then(|s| s.parse::<chrono::DateTime<chrono::Utc>>().ok())
            .is_some_and(|t| t > chrono::Utc::now());
        let inspection_state = inspection_state(
            current.as_ref().and_then(|i| i["state"].as_str()),
            live,
            eligible,
            failures,
            retry_at.is_some(),
        );
        if matches!(inspection_state, "passed" | "queued" | "running") {
            retry_at = None;
        }
        if inspection_state != "passed" {
            add_inspection(&mut blockers, inspection_state);
        }
        let acceptance:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_shared_release_scopes sc JOIN mobile_device_environment_observations o ON o.device_id=sc.device_id AND o.snapshot_id=sc.environment_snapshot_id JOIN mobile_device_environments e ON e.id=o.snapshot_id JOIN mobile_send_adapter_releases r ON r.id=sc.release_id WHERE sc.device_id=$1 AND sc.school_id=$2 AND sc.target_id=$3 AND sc.expires_at>clock_timestamp() AND sc.stage=(SELECT mode FROM mobile_shared_send_policy_versions ORDER BY revision DESC LIMIT 1) AND r.environment=e.snapshot-'instance_ref' AND NOT EXISTS(SELECT 1 FROM mobile_send_adapter_withdrawals w WHERE w.release_id=r.id))").bind(uid(&row,"device_id")?).bind(school).bind(uid(&row,"target_id")?).fetch_one(&mut **tx).await?;
        if !policy_released {
            blockers.push(json!({"code":"policy_not_released","message":"共享发送政策尚未进入限定测试或已验收生产阶段","owner":"platform","automatic":false}));
        } else if !acceptance {
            blockers.push(json!({"code":"acceptance_missing","message":"该范围尚未完成真机验收放行","owner":"engineering","automatic":false}));
        }
        out.push(json!({"binding_id":row["id"],"device_id":row["device_id"],"target_id":row["target_id"],"state":blockers.first().map(|b|b["code"].clone()).unwrap_or(json!("ready_for_task")),"ready":blockers.is_empty(),"blockers":blockers,"inspection":current,"inspection_state":inspection_state,"retry_at":retry_at,"retry_remaining":(3-failures).max(0),"device_name":row["device_name"],"target_name":row["target_name"],"search_text":row["search_text"],"automatic":true}));
    }
    Ok(out)
}
fn add_inspection(blockers: &mut Vec<Value>, state: &str) {
    let (code, message, automatic) = match state {
        "running" => ("inspection_running", "手机正在自动检查", true),
        "failed" => (
            "inspection_failed",
            "检查未通过，请核对实际页面或搜索文字；系统仅有界重试",
            false,
        ),
        "stale" => ("inspection_stale", "检查依据已更新，等待自动重新检查", true),
        "queued" => ("inspection_queued", "检查任务已排队，等待手机领取", true),
        "waiting_prerequisites" => (
            "inspection_prerequisites",
            "前置条件未满足，尚不能安排自动检查",
            true,
        ),
        "retry_wait" => (
            "inspection_retry_wait",
            "正在退避，等待下一次自动检查",
            true,
        ),
        "retry_exhausted" => (
            "inspection_retry_exhausted",
            "自动重试预算暂已用尽，请核对失败原因后重新评估",
            false,
        ),
        "expired" => (
            "inspection_expired",
            "检查记录或领取租约已过期，等待重新评估",
            true,
        ),
        _ => (
            "inspection_not_scheduled",
            "尚未建立可领取的检查任务，等待系统评估",
            true,
        ),
    };
    blockers.push(json!({"code":code,"message":message,"owner":if automatic{"system"}else{"administrator"},"automatic":automatic}));
}

pub(super) async fn pool(tx: &mut Tx<'_>, s: Uuid) -> Result<Vec<Value>> {
    sqlx::query_scalar("SELECT jsonb_build_object('id',d.id,'name',d.name,'access_revision',a.revision) FROM mobile_shared_school_access a JOIN execution_devices d ON d.id=a.device_id JOIN schools s ON s.id=a.school_id WHERE a.school_id=$1 AND a.enabled AND d.enabled AND s.active ORDER BY d.name,d.id").bind(s).fetch_all(&mut **tx).await.map_err(Into::into)
}
