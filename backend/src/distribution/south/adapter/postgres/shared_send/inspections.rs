//! Inspections carry no message and cannot obtain a send invocation.
use super::*;
pub(super) async fn request(
    tx: &mut Tx<'_>,
    a: &Actor,
    s: Uuid,
    v: InspectionInput,
) -> Result<Value> {
    let candidate = tasks::raw_candidates(tx, s, v.target_id)
        .await?
        .into_iter()
        .find(|x| x["device_id"] == json!(v.device_id))
        .ok_or(Error::Policy("shared_send_consent_or_binding_required"))?;
    let id = Uuid::now_v7();
    let challenge = Uuid::new_v4().simple().to_string();
    let deps = tasks::dependencies(&candidate);
    sqlx::query("INSERT INTO mobile_shared_send_inspections(id,school_id,device_id,target_id,binding_id,opening_id,consent_id,dependencies,requested_by,challenge,expires_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,clock_timestamp()+interval '10 minutes')").bind(id).bind(s).bind(v.device_id).bind(v.target_id).bind(uid(&candidate,"binding_id")?).bind(uid(&candidate,"opening_id")?).bind(uid(&candidate,"consent_id")?).bind(deps).bind(a.id).bind(challenge).execute(&mut **tx).await?;
    Ok(json!({"id":id,"state":"pending","device_id":v.device_id,"target_id":v.target_id}))
}
pub(super) async fn busy(tx: &mut Tx<'_>, device: Uuid) -> Result<bool> {
    Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_shared_send_claims WHERE device_id=$1 AND state='active') OR EXISTS(SELECT 1 FROM mobile_shared_invocations WHERE device_id=$1 AND state='risk_held') OR EXISTS(SELECT 1 FROM mobile_shared_device_leases WHERE device_id=$1 AND expires_at>clock_timestamp()) OR EXISTS(SELECT 1 FROM mobile_shared_runtime WHERE device_id=$1 AND NOT quiescent) OR EXISTS(SELECT 1 FROM mobile_execution_claims c JOIN mobile_delivery_items i ON i.id=c.item_id WHERE c.device_id=$1 AND (c.state IN ('claimed','prepared','permit_issued') OR i.state='unknown')) OR EXISTS(SELECT 1 FROM automation_flow_validations WHERE device_id=$1 AND state='accepted' AND expires_at>clock_timestamp()) OR EXISTS(SELECT 1 FROM device_validation_jobs WHERE device_id=$1 AND state IN ('accepted','running') AND expires_at>clock_timestamp())").bind(device).fetch_one(&mut **tx).await?)
}
pub(super) async fn claim(tx: &mut Tx<'_>, c: &Credential, v: InspectionClaim) -> Result<Value> {
    let row:Value=sqlx::query_scalar("SELECT to_jsonb(i) FROM mobile_shared_send_inspections i WHERE id=$1 AND device_id=$2 AND state='pending' AND expires_at>clock_timestamp()").bind(v.inspection_id).bind(c.device_id).fetch_optional(&mut **tx).await?.ok_or(Error::Policy("inspection_expired"))?;
    let s = uid(&row, "school_id")?;
    authorize(tx, uid(&row, "requested_by")?, Some(s), true).await?;
    let candidate = tasks::raw_candidates(tx, s, uid(&row, "target_id")?)
        .await?
        .into_iter()
        .find(|x| tasks::dependencies(x) == row["dependencies"])
        .ok_or(Error::Policy("plan_dependency_changed"))?;
    let ready:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_shared_runtime WHERE device_id=$1 AND credential_generation=$2 AND runtime_epoch=$3 AND ui_ready AND quiescent AND NOT paused AND observed_at>clock_timestamp()-interval '30 seconds') AND EXISTS(SELECT 1 FROM device_control_presence WHERE id=$1 AND state='online' AND NOT desired_paused AND last_control_heartbeat_at>clock_timestamp()-interval '30 seconds')").bind(c.device_id).bind(c.generation).bind(v.runtime_epoch).fetch_one(&mut **tx).await?;
    require(ready, "shared_send_runtime_required")?;
    require(!busy(tx, c.device_id).await?, "device_busy")?;
    let fence:i64=sqlx::query_scalar("INSERT INTO mobile_shared_device_leases(device_id,fence,holder_id,purpose,expires_at) VALUES($1,1,$2,'field_check',clock_timestamp()+interval '120 seconds') ON CONFLICT(device_id) DO UPDATE SET fence=mobile_shared_device_leases.fence+1,holder_id=$2,purpose='field_check',expires_at=clock_timestamp()+interval '120 seconds' RETURNING fence").bind(c.device_id).bind(v.inspection_id).fetch_one(&mut **tx).await?;
    sqlx::query("UPDATE mobile_shared_send_inspections SET state='leased',lease_fence=$2,runtime_epoch=$3,expires_at=clock_timestamp()+interval '120 seconds' WHERE id=$1").bind(v.inspection_id).bind(fence).bind(v.runtime_epoch).execute(&mut **tx).await?;
    Ok(
        json!({"id":v.inspection_id,"schema":SCHEMA,"state":"leased","challenge":row["challenge"],"lease_fence":fence,"runtime_epoch":v.runtime_epoch,"dependencies":row["dependencies"],"dependencies_digest":digest(&row["dependencies"])? ,"flow":candidate["flow"],"flow_digest":candidate["flow_digest"],"search_text":candidate["search_text"],"environment_snapshot_id":candidate["environment_snapshot_id"]}),
    )
}
pub(super) async fn finish(tx: &mut Tx<'_>, c: &Credential, v: InspectionResult) -> Result<Value> {
    require(
        !v.evidence.trim().is_empty() && v.evidence.len() <= 2000,
        "inspection_evidence_required",
    )?;
    let row:Value=sqlx::query_scalar("SELECT to_jsonb(i) FROM mobile_shared_send_inspections i WHERE i.id=$1 AND i.device_id=$2 AND i.dependencies->>'credential_generation'=$3 AND i.state='leased'").bind(v.inspection_id).bind(c.device_id).bind(c.generation.to_string()).fetch_optional(&mut **tx).await?.ok_or(Error::Policy("lease_superseded"))?;
    require(
        row["runtime_epoch"] == json!(v.runtime_epoch)
            && row["lease_fence"] == v.lease_fence
            && row["challenge"] == v.challenge
            && digest(&row["dependencies"])? == v.dependencies_digest
            && row["dependencies"]["environment_snapshot_id"] == json!(v.environment_snapshot_id),
        "result_identity_mismatch",
    )?;
    let s = uid(&row, "school_id")?;
    authorize(tx, uid(&row, "requested_by")?, Some(s), true).await?;
    let current = tasks::raw_candidates(tx, s, uid(&row, "target_id")?)
        .await?
        .iter()
        .any(|x| tasks::dependencies(x) == row["dependencies"]);
    let live:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_shared_device_leases l JOIN mobile_shared_runtime r ON r.device_id=l.device_id WHERE l.device_id=$1 AND l.holder_id=$2 AND l.fence=$3 AND l.expires_at>clock_timestamp() AND r.runtime_epoch=$4 AND r.credential_generation=$5 AND r.ui_ready AND NOT r.paused AND r.observed_at>clock_timestamp()-interval '30 seconds') AND EXISTS(SELECT 1 FROM mobile_shared_send_inspections WHERE id=$2 AND expires_at>clock_timestamp())").bind(c.device_id).bind(v.inspection_id).bind(v.lease_fence).bind(v.runtime_epoch).bind(c.generation).fetch_one(&mut **tx).await?;
    let state = if !current || !live {
        "stale"
    } else if v.passed {
        "passed"
    } else {
        "failed"
    };
    sqlx::query("UPDATE mobile_shared_send_inspections SET state=$2,result=$3,expires_at=clock_timestamp()+interval '24 hours' WHERE id=$1").bind(v.inspection_id).bind(state).bind(json!(v)).execute(&mut **tx).await?;
    // Physical lease remains until expiry or an explicit quiescent release.
    Ok(json!({"id":v.inspection_id,"state":state,"dependencies_digest":v.dependencies_digest}))
}
