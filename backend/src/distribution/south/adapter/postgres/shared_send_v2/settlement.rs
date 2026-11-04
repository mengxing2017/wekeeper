//! A result ACK and an idle physical owner are both required before moving the global cursor.
use super::*;
pub(super) async fn settle(tx: &mut Tx<'_>, c: &Credential, v: Settlement) -> Result<Value> {
    require(v.quiescent, "shared_send_runtime_required")?;
    let e:Value=sqlx::query_scalar("SELECT to_jsonb(e)||jsonb_build_object('claim_id',cl.id,'runtime_epoch',cl.runtime_epoch,'lease_fence',cl.lease_fence,'position',it.position,'task_id',it.task_id,'plan',p.plan,'attempt_state',a.state) FROM mobile_v9_send_events e JOIN mobile_v9_attempts a ON a.id=e.attempt_id JOIN mobile_v9_plans p ON p.id=a.plan_id JOIN mobile_v9_task_items it ON it.id=a.item_id JOIN mobile_v9_send_claims cl ON cl.attempt_id=a.id WHERE e.id=$1 AND e.device_id=$2 AND cl.credential_generation=$3").bind(v.event_id).bind(c.device_id).bind(c.generation).fetch_optional(&mut **tx).await?.ok_or(Error::NotFound)?;
    require(
        e["receipt"]["id"] == json!(v.receipt_id) && e["lease_fence"] == v.lease_fence,
        "result_identity_mismatch",
    )?;
    let idle:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_shared_runtime WHERE device_id=$1 AND credential_generation=$2 AND runtime_epoch=$3 AND quiescent AND observed_at>clock_timestamp()-interval '30 seconds')").bind(c.device_id).bind(c.generation).bind(v.runtime_epoch).fetch_one(&mut **tx).await?;
    require(idle, "shared_send_runtime_required")?;
    sqlx::query("INSERT INTO mobile_v9_settlements(attempt_id,receipt_id,event_id,device_id,credential_generation,runtime_epoch,lease_fence) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT(attempt_id) DO NOTHING").bind(uid(&e,"attempt_id")?).bind(v.receipt_id).bind(v.event_id).bind(c.device_id).bind(c.generation).bind(v.runtime_epoch).bind(v.lease_fence).execute(&mut **tx).await?;
    sqlx::query("UPDATE mobile_shared_device_leases SET expires_at=clock_timestamp() WHERE device_id=$1 AND holder_id=$2 AND fence=$3").bind(c.device_id).bind(uid(&e,"claim_id")?).bind(v.lease_fence).execute(&mut **tx).await?;
    if matches!(
        e["attempt_state"].as_str(),
        Some("observed_sent" | "manually_confirmed_sent")
    ) {
        let p = policy(tx)
            .await?
            .ok_or(Error::Policy("shared_send_not_released"))?;
        let gap = e["plan"]["between_targets_seconds"]
            .as_i64()
            .unwrap_or(1)
            .max(
                (e["plan"]["flow"]["pacing"]["between_targets_min_ms"]
                    .as_i64()
                    .unwrap_or(1000)
                    + 999)
                    / 1000,
            );
        let gap = ["global", "school", "device", "target"]
            .iter()
            .fold(gap, |g, k| {
                g.max(p["limits"][k]["interval_seconds"].as_i64().unwrap_or(1))
            });
        sqlx::query("UPDATE mobile_v9_tasks SET cursor_position=cursor_position+1,next_eligible_at=greatest(clock_timestamp(),$4)+make_interval(secs=>$3::double precision),revision=revision+1 WHERE id=$1 AND cursor_position=$2 AND dispatch_mode='legacy_sequence'").bind(uid(&e,"task_id")?).bind(e["position"].as_i64().ok_or(Error::Conflict)? as i32).bind(gap as f64).bind(e["created_at"].as_str().ok_or(Error::Conflict)?.parse::<chrono::DateTime<chrono::Utc>>().map_err(|_|Error::Conflict)?).execute(&mut **tx).await?;
    }
    Ok(json!({"id":v.receipt_id,"event_id":v.event_id,"settled":true}))
}
pub(super) async fn diagnostic(tx: &mut Tx<'_>, c: &Credential, v: Diagnostic) -> Result<Value> {
    require(
        !v.code.is_empty()
            && v.code.len() <= 128
            && v.stage.len() <= 128
            && v.details.len() <= 12
            && v.details.iter().all(|(k, s)| {
                [
                    "app_version",
                    "launcher_version",
                    "android_version",
                    "launcher_package",
                    "failure_page",
                    "engine_version",
                ]
                .contains(&k.as_str())
                    && s.len() <= 512
            }),
        "shared_send_diagnostic_invalid",
    )?;
    let failed:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_v9_send_inspections WHERE id=$1 AND device_id=$3 AND state IN ('failed','stale')) OR EXISTS(SELECT 1 FROM mobile_v9_send_claims cl JOIN mobile_v9_attempts a ON a.id=cl.attempt_id WHERE cl.id=$2 AND cl.device_id=$3 AND a.state IN ('failed_before_call','unknown'))").bind(v.inspection_id).bind(v.claim_id).bind(c.device_id).fetch_one(&mut **tx).await?;
    require(failed, "shared_send_diagnostic_without_failure")?;
    sqlx::query("INSERT INTO mobile_v9_diagnostics(id,device_id,credential_generation,inspection_id,claim_id,code,stage,details) VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(id) DO NOTHING").bind(v.event_id).bind(c.device_id).bind(c.generation).bind(v.inspection_id).bind(v.claim_id).bind(v.code).bind(v.stage).bind(json!(v.details)).execute(&mut **tx).await?;
    Ok(json!({"id":v.event_id,"recorded":true}))
}

pub(super) async fn advance_reviewed(tx: &mut Tx<'_>, device: Uuid) -> Result<()> {
    let Some(p) = policy(tx).await? else {
        return Ok(());
    };
    let minimum = ["global", "school", "device", "target"]
        .iter()
        .filter_map(|k| p["limits"][k]["interval_seconds"].as_i64())
        .max()
        .unwrap_or(1);
    sqlx::query("UPDATE mobile_v9_tasks task SET cursor_position=cursor_position+1,next_eligible_at=clock_timestamp()+make_interval(secs=>greatest($2::bigint,(p.plan->>'between_targets_seconds')::bigint,ceil((p.plan->'flow'->'pacing'->>'between_targets_min_ms')::numeric/1000)::bigint)::double precision),revision=task.revision+1 FROM mobile_v9_task_items i JOIN mobile_v9_attempts a ON a.item_id=i.id AND a.current JOIN mobile_v9_plans p ON p.id=a.plan_id JOIN mobile_v9_settlements s ON s.attempt_id=a.id JOIN mobile_shared_runtime rt ON rt.device_id=p.device_id WHERE task.dispatch_mode='legacy_sequence' AND i.task_id=task.id AND i.position=task.cursor_position AND a.state='manually_confirmed_sent' AND p.device_id=$1 AND rt.quiescent AND rt.observed_at>clock_timestamp()-interval '30 seconds' AND NOT EXISTS(SELECT 1 FROM mobile_shared_device_leases l WHERE l.device_id=$1 AND l.expires_at>clock_timestamp())").bind(device).bind(minimum).execute(&mut **tx).await?;
    Ok(())
}
