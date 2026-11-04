//! Business deliveries outlive individual devices. All mutations share the invocation guard.
use super::*;

impl Repository {
    pub async fn reconcile_dispatch(&self) -> Result<()> {
        // Independent of usage projection, device recovery and diagnostic workers.
        let ids: Vec<Uuid> = sqlx::query_scalar("SELECT ds.item_id FROM mobile_v9_dispatch_state ds WHERE ds.next_retry_at<=clock_timestamp() AND ds.state NOT IN ('completed','stopped','confirmation_expired') ORDER BY ds.next_retry_at,ds.item_id LIMIT 100")
            .fetch_all(&self.0).await?;
        for id in ids {
            let mut tx = self.0.begin().await?;
            if !sqlx::query_scalar::<_, bool>("SELECT pg_try_advisory_xact_lock(78234610243)")
                .fetch_one(&mut *tx)
                .await?
            {
                continue;
            }
            sqlx::query("SET LOCAL statement_timeout='2s'")
                .execute(&mut *tx)
                .await?;
            match reconcile(&mut tx, id).await {
                Ok(()) => tx.commit().await?,
                Err(error) => {
                    tx.rollback().await?;
                    tracing::warn!(%error,item=%id,"mobile delivery dispatch will retry");
                }
            }
        }
        Ok(())
    }
}
async fn status(tx: &mut Tx<'_>, item: Uuid, state: &str, reason: &str) -> Result<()> {
    sqlx::query("UPDATE mobile_v9_dispatch_state SET revision=revision+CASE WHEN (state,reason) IS DISTINCT FROM ($2,$3) THEN 1 ELSE 0 END,state=$2,reason=$3,next_retry_at=clock_timestamp()+interval '5 seconds',updated_at=clock_timestamp() WHERE item_id=$1")
        .bind(item).bind(state).bind(reason).execute(&mut **tx).await?;
    Ok(())
}
async fn ready(tx: &mut Tx<'_>, candidate: &Value) -> Result<bool> {
    if candidate["accepted"] != true {
        return Ok(false);
    }
    let device = uid(candidate, "device_id")?;
    let fresh:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM device_control_presence cp JOIN mobile_shared_runtime r ON r.device_id=cp.id JOIN mobile_v9_runtime_details rd ON rd.device_id=r.device_id AND rd.credential_generation=r.credential_generation WHERE cp.id=$1 AND cp.state='online' AND NOT cp.desired_paused AND cp.last_control_heartbeat_at>clock_timestamp()-interval '30 seconds' AND r.credential_generation=$2 AND r.ui_ready AND NOT r.paused AND r.quiescent AND r.observed_at>clock_timestamp()-interval '30 seconds' AND rd.observed_at>clock_timestamp()-interval '30 seconds')")
        .bind(device).bind(candidate["credential_generation"].as_i64().ok_or(Error::Conflict)?).fetch_one(&mut **tx).await?;
    Ok(fresh && !inspections::busy(tx, device).await?)
}
pub(super) async fn reconcile(tx: &mut Tx<'_>, item: Uuid) -> Result<()> {
    let row:Value=sqlx::query_scalar("SELECT to_jsonb(a)||jsonb_build_object('plan',p.plan,'snapshot_id',p.snapshot_id,'target_id',i.target_id,'task_id',t.id,'stopped',t.stopped,'expired',t.confirmed_until<=clock_timestamp(),'candidates',sc.candidates,'last_failed_attempt_id',ds.last_failed_attempt_id) FROM mobile_v9_task_items i JOIN mobile_v9_tasks t ON t.id=i.task_id JOIN mobile_v9_attempts a ON a.item_id=i.id AND a.current JOIN mobile_v9_plans p ON p.id=a.plan_id JOIN mobile_v9_dispatch_scopes sc ON sc.item_id=i.id JOIN mobile_v9_dispatch_state ds ON ds.item_id=i.id WHERE i.id=$1 FOR UPDATE OF a,ds")
        .bind(item).fetch_one(&mut **tx).await?;
    if row["stopped"] == true {
        return status(tx, item, "stopped", "task_stopped").await;
    }
    if row["expired"] == true {
        return status(tx, item, "confirmation_expired", "confirmation_expired").await;
    }
    let state = row["state"].as_str().ok_or(Error::Conflict)?;
    let aid = uid(&row, "id")?;
    let issued: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM mobile_v9_invocations WHERE attempt_id=$1)",
    )
    .bind(aid)
    .fetch_one(&mut **tx)
    .await?;
    let previous_pending:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_v9_attempts a JOIN mobile_v9_invocations v ON v.attempt_id=a.id WHERE a.item_id=$1 AND a.id<>$2 AND v.state='risk_held')")
        .bind(item).bind(aid).fetch_one(&mut **tx).await?;
    if previous_pending {
        return status(tx, item, "needs_review", "target_result_pending").await;
    }
    if matches!(state, "observed_sent" | "manually_confirmed_sent") {
        return status(tx, item, "completed", "result_recorded").await;
    }
    if state == "unknown" || (issued && !matches!(state, "verified_not_sent")) {
        // Never infer no side effect from a timeout, even when the phone is offline.
        let expired:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_v9_invocations WHERE attempt_id=$1 AND execute_before<clock_timestamp())").bind(aid).fetch_one(&mut **tx).await?;
        if expired && state == "invocation_issued" {
            sqlx::query("UPDATE mobile_v9_attempts SET state='unknown',revision=revision+1 WHERE id=$1 AND state='invocation_issued'").bind(aid).execute(&mut **tx).await?;
        }
        return status(
            tx,
            item,
            if state == "unknown" || expired {
                "needs_review"
            } else {
                "executing"
            },
            if state == "unknown" || expired {
                "result_unknown"
            } else {
                "invocation_issued"
            },
        )
        .await;
    }
    if state == "cancelled" {
        return status(tx, item, "stopped", "attempt_cancelled").await;
    }
    let previous_sent:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_v9_attempts WHERE item_id=$1 AND id<>$2 AND state IN ('observed_sent','manually_confirmed_sent'))")
        .bind(item).bind(aid).fetch_one(&mut **tx).await?;
    if previous_sent {
        if !issued {
            close_unissued(tx, aid).await?;
        }
        return status(tx, item, "completed", "result_recorded").await;
    }
    let school = uid(&row, "school_id")?;
    match authorize(tx, uid(&row, "confirmed_by")?, Some(school), true).await {
        Ok(()) => {}
        Err(Error::Forbidden | Error::Unauthorized) => {
            return status(
                tx,
                item,
                "waiting_resources",
                "confirmation_permission_changed",
            )
            .await;
        }
        Err(error) => return Err(error),
    }
    if state == "verified_not_sent" {
        let unsafe_permit:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_v9_invocations WHERE attempt_id=$1 AND (state<>'released' OR execute_before>clock_timestamp()))").bind(aid).fetch_one(&mut **tx).await?;
        if unsafe_permit {
            return status(tx, item, "needs_review", "result_unknown").await;
        }
    }
    if matches!(state, "leased" | "prepared") {
        let alive:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_v9_send_claims c JOIN device_control_presence cp ON cp.id=c.device_id JOIN execution_devices d ON d.id=c.device_id WHERE c.attempt_id=$1 AND c.state='active' AND c.expires_at>clock_timestamp() AND d.enabled AND c.credential_generation=d.credential_generation AND cp.state='online' AND NOT cp.desired_paused AND cp.last_control_heartbeat_at>clock_timestamp()-interval '30 seconds')")
            .bind(aid).fetch_one(&mut **tx).await?;
        if alive {
            return status(tx, item, "executing", "preparing").await;
        }
        close_unissued(tx, aid).await?;
    }
    let failed = matches!(
        state,
        "failed_before_call" | "invalidated" | "verified_not_sent" | "leased" | "prepared"
    );
    if failed && row["last_failed_attempt_id"] != row["id"] {
        sqlx::query("INSERT INTO mobile_v9_dispatch_failures(item_id,device_id,retry_at) VALUES($1,$2,clock_timestamp()+interval '30 seconds') ON CONFLICT(item_id,device_id) DO UPDATE SET failures=least(mobile_v9_dispatch_failures.failures+1,10),retry_at=clock_timestamp()+make_interval(secs=>least(300,30*power(2,least(mobile_v9_dispatch_failures.failures,4)))::double precision)")
            .bind(item).bind(uid(&row["plan"],"device_id")?).execute(&mut **tx).await?;
        sqlx::query(
            "UPDATE mobile_v9_dispatch_state SET last_failed_attempt_id=$2 WHERE item_id=$1",
        )
        .bind(item)
        .bind(aid)
        .execute(&mut **tx)
        .await?;
    }
    // A delayed result may contradict a review after a replacement was queued.
    // Preserve the replacement identity, but never let it receive a permit while
    // the original target is unresolved. A later positive review completes the
    // delivery without another send.
    match tasks::validate_risk(tx, school, &row["plan"]).await {
        Ok(()) => {}
        Err(Error::Policy(_)) => {
            return status(tx, item, "needs_review", "target_result_pending").await;
        }
        Err(error) => return Err(error),
    }
    let choices = tasks::raw_candidates(tx, school, uid(&row, "target_id")?).await?;
    let mut approved = row["candidates"].as_array().ok_or(Error::Conflict)?.clone();
    // Keep a healthy current assignment; do not oscillate back to a recovered preference.
    approved.sort_by_key(|c| c["device_id"] != row["plan"]["device_id"]);
    for snapshot in approved {
        let Some(candidate) = choices
            .iter()
            .find(|c| tasks::dependencies(c) == tasks::dependencies(&snapshot))
        else {
            continue;
        };
        match tasks::validate_plan(tx, school, &snapshot).await {
            Ok(()) => {}
            Err(Error::Policy(_) | Error::NotFound | Error::Forbidden | Error::Unauthorized) => {
                continue;
            }
            Err(error) => return Err(error),
        }
        let cooldown:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_v9_dispatch_failures WHERE item_id=$1 AND device_id=$2 AND retry_at>clock_timestamp())")
            .bind(item).bind(uid(candidate,"device_id")?).fetch_one(&mut **tx).await?;
        if cooldown || !ready(tx, candidate).await? {
            continue;
        }
        if !device::quota(
            tx,
            school,
            uid(candidate, "device_id")?,
            uid(candidate, "target_id")?,
            aid,
            false,
        )
        .await?
        {
            continue;
        }
        if !failed && candidate["device_id"] == row["plan"]["device_id"] {
            return status(tx, item, "assigned", "candidate_ready").await;
        }
        // No old-device quiescent proof is necessary to dispatch a different phone.
        if !issued {
            close_unissued(tx, aid).await?;
        }
        let reason = if failed {
            "attempt_failed_before_send"
        } else {
            "candidate_unavailable"
        };
        replace(tx, &row, &snapshot, "automatic", reason).await?;
        return status(tx, item, "assigned", reason).await;
    }
    status(
        tx,
        item,
        "waiting_resources",
        "no_available_authorized_candidate",
    )
    .await
}
async fn close_unissued(tx: &mut Tx<'_>, attempt: Uuid) -> Result<()> {
    let issued: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM mobile_v9_invocations WHERE attempt_id=$1)",
    )
    .bind(attempt)
    .fetch_one(&mut **tx)
    .await?;
    require(!issued, "shared_send_review_required")?;
    sqlx::query(
        "UPDATE mobile_v9_send_claims SET state='closed' WHERE attempt_id=$1 AND state='active'",
    )
    .bind(attempt)
    .execute(&mut **tx)
    .await?;
    sqlx::query("UPDATE mobile_v9_attempts SET state='failed_before_call',revision=revision+1 WHERE id=$1 AND state IN ('queued','leased','prepared')").bind(attempt).execute(&mut **tx).await?;
    sqlx::query("UPDATE mobile_v9_quota_reservations SET state='released' WHERE attempt_id=$1 AND state='reserved'").bind(attempt).execute(&mut **tx).await?;
    // The old physical lease/runtime are intentionally untouched, isolating that device.
    Ok(())
}
pub(super) async fn replace(
    tx: &mut Tx<'_>,
    old: &Value,
    candidate: &Value,
    source: &str,
    reason: &str,
) -> Result<Uuid> {
    let aid = Uuid::now_v7();
    let pid = Uuid::now_v7();
    let mut plan = candidate.clone();
    for k in [
        "schema",
        "school_id",
        "intent_id",
        "item_id",
        "message",
        "content_digest",
        "selection_policy",
        "between_targets_seconds",
        "policy_id",
    ] {
        plan[k] = old["plan"][k].clone();
    }
    plan["attempt_id"] = json!(aid);
    sqlx::query(
        "UPDATE mobile_v9_attempts SET current=false,revision=revision+1 WHERE id=$1 AND current",
    )
    .bind(uid(old, "id")?)
    .execute(&mut **tx)
    .await?;
    sqlx::query("INSERT INTO mobile_v9_plans(id,school_id,item_id,target_id,device_id,snapshot_id,plan,digest,content_digest) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)")
        .bind(pid).bind(uid(old,"school_id")?).bind(uid(old,"item_id")?).bind(uid(old,"target_id")?).bind(uid(&plan,"device_id")?).bind(uid(old,"snapshot_id")?).bind(&plan).bind(digest(&plan)?).bind(plan["content_digest"].as_str()).execute(&mut **tx).await?;
    sqlx::query("INSERT INTO mobile_v9_attempts(id,school_id,item_id,plan_id,attempt_no,confirmed_by) VALUES($1,$2,$3,$4,$5,$6)")
        .bind(aid).bind(uid(old,"school_id")?).bind(uid(old,"item_id")?).bind(pid).bind(old["attempt_no"].as_i64().ok_or(Error::Conflict)? as i32+1).bind(uid(old,"confirmed_by")?).execute(&mut **tx).await?;
    sqlx::query("INSERT INTO mobile_v9_reassignments(id,school_id,old_attempt_id,new_attempt_id,actor_id,reason,source) VALUES($1,$2,$3,$4,$5,$6,$7)")
        .bind(Uuid::now_v7()).bind(uid(old,"school_id")?).bind(uid(old,"id")?).bind(aid).bind(uid(old,"confirmed_by")?).bind(reason).bind(source).execute(&mut **tx).await?;
    Ok(aid)
}
