//! Inspections carry no message and cannot obtain a send invocation.
use super::*;
pub(super) async fn request(
    tx: &mut Tx<'_>,
    a: &Actor,
    s: Uuid,
    v: InspectionInput,
) -> Result<Value> {
    let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_device_target_bindings WHERE school_id=$1 AND target_id=$2 AND device_id=$3 AND enabled)").bind(s).bind(v.target_id).bind(v.device_id).fetch_one(&mut **tx).await?;
    require(exists, "shared_binding_missing")?;
    let _ = a;
    let status = readiness::school(tx, s)
        .await?
        .into_iter()
        .find(|r| r["device_id"] == json!(v.device_id) && r["target_id"] == json!(v.target_id))
        .ok_or(Error::NotFound)?;
    Ok(json!({"id":status["binding_id"],"state":status["state"],"readiness":status}))
}
// Enabled bindings are durable desired checks, including while offline or awaiting consent.
pub(super) async fn reconcile(tx: &mut Tx<'_>, device: Uuid) -> Result<()> {
    // Expired claims count once only while their original resources still match.
    let expired: Vec<Value> = sqlx::query_scalar("SELECT to_jsonb(i) FROM mobile_v9_send_inspections i WHERE device_id=$1 AND state='leased' AND expires_at<=clock_timestamp() ORDER BY expires_at,id LIMIT 100")
        .bind(device).fetch_all(&mut **tx).await?;
    for row in expired {
        let current = match material(tx, &row).await {
            Ok(_) => true,
            Err(Error::Policy(_) | Error::NotFound | Error::Invalid(_)) => false,
            Err(error) => return Err(error),
        };
        sqlx::query("UPDATE mobile_v9_send_inspections SET state=$2,finished_at=expires_at,termination_reason=$3 WHERE id=$1 AND state='leased'")
            .bind(uid(&row,"id")?).bind(if current && !row["started_at"].is_null() { "failed" } else { "stale" })
            .bind(if row["started_at"].is_null() { "not_started" } else if current { "execution_timeout" } else { "resources_changed" })
            .execute(&mut **tx).await?;
    }
    // Revoke queued work immediately, including bindings excluded from the active scan.
    sqlx::query("UPDATE mobile_v9_send_inspections i SET state='stale' WHERE i.automatic AND i.state='pending' AND i.device_id=$1 AND NOT EXISTS(SELECT 1 FROM mobile_device_target_bindings b JOIN mobile_targets t ON t.id=b.target_id JOIN schools s ON s.id=b.school_id JOIN execution_devices d ON d.id=b.device_id JOIN mobile_shared_delegations m ON m.id=i.delegation_id WHERE b.id=i.binding_id AND b.enabled AND t.enabled AND t.archived_at IS NULL AND s.active AND d.enabled AND m.credential_generation=d.credential_generation AND (b.source='platform' OR EXISTS(SELECT 1 FROM mobile_shared_school_access a WHERE a.device_id=b.device_id AND a.school_id=b.school_id AND a.enabled)))").bind(device).execute(&mut **tx).await?;
    // UI recognition and local checks belong to the device's preparation step.
    // Only settle legacy leases; never create platform UI inspection jobs.
    sqlx::query("UPDATE mobile_v9_send_inspections SET state='stale',termination_reason='device_owned_execution',finished_at=clock_timestamp() WHERE device_id=$1 AND purpose='automatic' AND state='pending'")
        .bind(device).execute(&mut **tx).await?;
    Ok(())
}
pub(super) async fn busy(tx: &mut Tx<'_>, device: Uuid) -> Result<bool> {
    Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_v9_send_claims WHERE device_id=$1 AND state='active' AND expires_at>clock_timestamp()) OR EXISTS(SELECT 1 FROM mobile_v9_invocations WHERE device_id=$1 AND state='risk_held') OR EXISTS(SELECT 1 FROM mobile_shared_device_leases WHERE device_id=$1 AND expires_at>clock_timestamp()) OR EXISTS(SELECT 1 FROM mobile_shared_runtime WHERE device_id=$1 AND NOT quiescent)").bind(device).fetch_one(&mut **tx).await?)
}
pub(super) async fn busy_sources(tx: &mut Tx<'_>, device: Uuid) -> Result<Vec<Value>> {
    sqlx::query_scalar("SELECT jsonb_build_object('code',code,'id',id,'observed_at',observed_at,'expires_at',expires_at) FROM (SELECT 'device_claim_active' AS code,id,created_at AS observed_at,expires_at FROM mobile_v9_send_claims WHERE device_id=$1 AND state='active' AND expires_at>clock_timestamp() UNION ALL SELECT 'device_invocation_risk',id,NULL::timestamptz,NULL::timestamptz FROM mobile_v9_invocations WHERE device_id=$1 AND state='risk_held' UNION ALL SELECT 'device_lease_active',holder_id,NULL::timestamptz,expires_at FROM mobile_shared_device_leases WHERE device_id=$1 AND expires_at>clock_timestamp() UNION ALL SELECT CASE WHEN observed_at<clock_timestamp()-interval '30 seconds' THEN 'device_idle_unconfirmed' ELSE 'device_ui_active' END,device_id,observed_at,NULL::timestamptz FROM mobile_shared_runtime WHERE device_id=$1 AND NOT quiescent) facts LIMIT 100").bind(device).fetch_all(&mut **tx).await.map_err(Into::into)
}
// A task owns its whole remaining target sequence. Background future-group checks wait too.
pub(super) async fn cursor_allows(tx: &mut Tx<'_>, binding: Uuid) -> Result<bool> {
    Ok(sqlx::query_scalar("SELECT NOT EXISTS(SELECT 1 FROM mobile_v9_plans p JOIN mobile_v9_attempts a ON a.plan_id=p.id JOIN mobile_v9_task_items i ON i.id=a.item_id JOIN mobile_v9_tasks t ON t.id=i.task_id WHERE p.plan->>'binding_id'=$1::uuid::text AND a.current AND a.state='queued' AND NOT t.stopped AND t.confirmed_until>clock_timestamp()) OR EXISTS(SELECT 1 FROM mobile_v9_plans p JOIN mobile_v9_attempts a ON a.plan_id=p.id JOIN mobile_v9_task_items i ON i.id=a.item_id JOIN mobile_v9_tasks t ON t.id=i.task_id WHERE p.plan->>'binding_id'=$1::uuid::text AND a.current AND a.state='queued' AND NOT t.stopped AND t.confirmed_until>clock_timestamp() AND i.position=t.cursor_position AND t.next_eligible_at<=clock_timestamp())").bind(binding).fetch_one(&mut **tx).await?)
}
async fn material(tx: &mut Tx<'_>, row: &Value) -> Result<Value> {
    let mut candidate = tasks::raw_candidates(tx, uid(row, "school_id")?, uid(row, "target_id")?)
        .await?
        .into_iter()
        .find(|x| x["binding_id"] == row["binding_id"])
        .ok_or(Error::Policy("plan_dependency_changed"))?;
    require(
        candidate["accepted"] == true,
        "shared_send_consent_required",
    )?;
    // Automatic checks use the immutable configuration already frozen by the task.
    candidate["flow"] = row["flow"].clone();
    candidate["flow_digest"] = row["flow_digest"].clone();
    // An archival dependency annotation is not a source of rules or an approval.
    candidate["release_id"] = row["dependencies"]["release_id"].clone();

    tasks::validate_scope(tx, uid(row, "school_id")?, &candidate).await?;
    require(
        tasks::dependencies(&candidate) == row["dependencies"],
        "plan_dependency_changed",
    )?;
    Ok(candidate)
}

pub(super) async fn claim(tx: &mut Tx<'_>, c: &Credential, v: InspectionClaim) -> Result<Value> {
    let row:Value=sqlx::query_scalar("SELECT to_jsonb(i) FROM mobile_v9_send_inspections i WHERE id=$1 AND device_id=$2 AND state='pending' AND purpose='automatic' AND expires_at>clock_timestamp()").bind(v.inspection_id).bind(c.device_id).fetch_optional(&mut **tx).await?.ok_or(Error::Policy("inspection_expired"))?;
    let candidate = material(tx, &row).await?;
    require(
        cursor_allows(tx, uid(&row, "binding_id")?).await?,
        "task_cursor_wait",
    )?;
    let ready:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_shared_runtime WHERE device_id=$1 AND credential_generation=$2 AND runtime_epoch=$3 AND ui_ready AND quiescent AND NOT paused AND observed_at>clock_timestamp()-interval '30 seconds') AND EXISTS(SELECT 1 FROM device_control_presence WHERE id=$1 AND state='online' AND NOT desired_paused AND last_control_heartbeat_at>clock_timestamp()-interval '30 seconds')").bind(c.device_id).bind(c.generation).bind(v.runtime_epoch).fetch_one(&mut **tx).await?;
    require(ready, "shared_send_runtime_required")?;
    require(!busy(tx, c.device_id).await?, "device_busy")?;
    let fence:i64=sqlx::query_scalar("INSERT INTO mobile_shared_device_leases(device_id,fence,holder_id,purpose,expires_at) VALUES($1,1,$2,'field_check',clock_timestamp()+interval '120 seconds') ON CONFLICT(device_id) DO UPDATE SET fence=mobile_shared_device_leases.fence+1,holder_id=$2,purpose='field_check',expires_at=clock_timestamp()+interval '120 seconds' RETURNING fence").bind(c.device_id).bind(v.inspection_id).fetch_one(&mut **tx).await?;
    sqlx::query("UPDATE mobile_v9_send_inspections SET state='leased',lease_fence=$2,runtime_epoch=$3,expires_at=clock_timestamp()+interval '120 seconds' WHERE id=$1").bind(v.inspection_id).bind(fence).bind(v.runtime_epoch).execute(&mut **tx).await?;
    let epoch:String=sqlx::query_scalar("SELECT ui_context_epoch FROM mobile_v9_runtime_details WHERE device_id=$1 AND credential_generation=$2").bind(c.device_id).bind(c.generation).fetch_one(&mut **tx).await?;
    Ok(
        json!({"id":v.inspection_id,"schema":SCHEMA,"state":"leased","challenge":row["challenge"],"lease_fence":fence,"runtime_epoch":v.runtime_epoch,"ui_context_epoch":epoch,"dependencies":row["dependencies"],"dependencies_digest":digest(&row["dependencies"])? ,"flow":candidate["flow"],"flow_digest":candidate["flow_digest"],"search_text":candidate["search_text"],"launch_path":row["launch_path"]}),
    )
}
pub(super) async fn start(tx: &mut Tx<'_>, c: &Credential, v: InspectionStart) -> Result<Value> {
    let row:Value=sqlx::query_scalar("SELECT to_jsonb(i) FROM mobile_v9_send_inspections i WHERE id=$1 AND device_id=$2 AND dependencies->>'credential_generation'=$3 AND state='leased' AND purpose='automatic' AND expires_at>clock_timestamp()")
        .bind(v.inspection_id).bind(c.device_id).bind(c.generation.to_string()).fetch_optional(&mut **tx).await?.ok_or(Error::Policy("inspection_expired"))?;
    require(
        row["runtime_epoch"] == json!(v.runtime_epoch)
            && row["lease_fence"] == v.lease_fence
            && row["challenge"] == v.challenge
            && digest(&row["dependencies"])? == v.dependencies_digest,
        "result_identity_mismatch",
    )?;
    let live:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_shared_device_leases WHERE device_id=$1 AND holder_id=$2 AND fence=$3 AND expires_at>clock_timestamp())").bind(c.device_id).bind(v.inspection_id).bind(v.lease_fence).fetch_one(&mut **tx).await?;
    require(live, "lease_superseded")?;
    sqlx::query("UPDATE mobile_v9_send_inspections SET started_at=coalesce(started_at,clock_timestamp()) WHERE id=$1").bind(v.inspection_id).execute(&mut **tx).await?;
    Ok(json!({"id":v.inspection_id,"state":"started","dependencies_digest":v.dependencies_digest}))
}
pub(super) async fn finish(tx: &mut Tx<'_>, c: &Credential, v: InspectionResult) -> Result<Value> {
    require(
        !v.evidence.trim().is_empty() && v.evidence.len() <= 2000,
        "inspection_evidence_required",
    )?;
    let row:Value=sqlx::query_scalar("SELECT to_jsonb(i) FROM mobile_v9_send_inspections i WHERE id=$1 AND device_id=$2 AND dependencies->>'credential_generation'=$3 AND state='leased' AND purpose='automatic'").bind(v.inspection_id).bind(c.device_id).bind(c.generation.to_string()).fetch_optional(&mut **tx).await?.ok_or(Error::Policy("lease_superseded"))?;
    require(
        row["runtime_epoch"] == json!(v.runtime_epoch)
            && row["lease_fence"] == v.lease_fence
            && row["challenge"] == v.challenge
            && digest(&row["dependencies"])? == v.dependencies_digest,
        "result_identity_mismatch",
    )?;
    let current = material(tx, &row).await.is_ok();
    let live:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_shared_device_leases l JOIN mobile_shared_runtime r ON r.device_id=l.device_id JOIN mobile_v9_runtime_details d ON d.device_id=l.device_id WHERE l.device_id=$1 AND l.holder_id=$2 AND l.fence=$3 AND l.expires_at>clock_timestamp() AND r.runtime_epoch=$4 AND r.credential_generation=$5 AND r.ui_ready AND NOT r.paused AND d.ui_context_epoch=$6 AND d.apk_sha256=$7 AND r.observed_at>clock_timestamp()-interval '30 seconds')").bind(c.device_id).bind(v.inspection_id).bind(v.lease_fence).bind(v.runtime_epoch).bind(c.generation).bind(&v.ui_context_epoch).bind(&v.apk_sha256).fetch_one(&mut **tx).await?;
    let checks = crate::distribution::domain::shared_mobile::FLOW_CHECKS
        .iter()
        .all(|k| v.checks.get(*k) == Some(&true));
    let started = !row["started_at"].is_null()
        || v.checks.get("execution_started") == Some(&true)
        || v.checks.get("application_opened") == Some(&true);
    let state = if !current || !live || !started {
        "stale"
    } else if v.passed && checks {
        "passed"
    } else {
        "failed"
    };
    let ttl = policy(tx)
        .await?
        .and_then(|p| p["inspection_seconds"].as_i64())
        .unwrap_or(86400);
    sqlx::query("UPDATE mobile_v9_send_inspections SET state=$2,result=$3,started_at=CASE WHEN $3::jsonb->'checks'->>'execution_started'='true' OR $3::jsonb->'checks'->>'application_opened'='true' THEN coalesce(started_at,clock_timestamp()) ELSE started_at END,finished_at=clock_timestamp(),termination_reason=CASE WHEN $2='failed' THEN 'check_failed' WHEN $2='stale' AND started_at IS NULL AND coalesce($3::jsonb->'checks'->>'execution_started','false')<>'true' AND coalesce($3::jsonb->'checks'->>'application_opened','false')<>'true' THEN 'not_started' WHEN $2='stale' THEN 'resources_or_runtime_changed' ELSE NULL END,expires_at=clock_timestamp()+make_interval(secs=>$4::double precision) WHERE id=$1").bind(v.inspection_id).bind(state).bind(json!(v)).bind(ttl as f64).execute(&mut **tx).await?;
    Ok(json!({"id":v.inspection_id,"state":state,"dependencies_digest":v.dependencies_digest}))
}
