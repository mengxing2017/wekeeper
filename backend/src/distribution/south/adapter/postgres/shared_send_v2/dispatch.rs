//! Fixed-rate database reconciliation; no task owns a waiting thread or connection.
use super::*;
use crate::distribution::domain::shared_mobile::execution::replacement_reason;
impl Repository {
    pub async fn reconcile_dispatch(&self) -> Result<()> {
        {
            let mut tx = self.0.begin().await?;
            guard(&mut tx).await?;
            execution::apply_results(&mut tx).await?;
            sqlx::query("UPDATE mobile_execution_grants SET session_closed=true WHERE NOT session_closed AND execute_before<=clock_timestamp()").execute(&mut *tx).await?;
            sqlx::query("UPDATE mobile_execution_grants SET state='superseded',session_closed=true WHERE kind='inspection' AND state='active' AND result_deadline_at<=clock_timestamp()").execute(&mut *tx).await?;
            tx.commit().await?;
        }
        let ids:Vec<Uuid>=sqlx::query_scalar("SELECT item_id FROM mobile_v9_dispatch_state WHERE next_retry_at<=clock_timestamp() AND state NOT IN ('completed','stopped','confirmation_expired') ORDER BY next_retry_at,item_id LIMIT 100").fetch_all(&self.0).await?;
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
                    tracing::warn!(%error,item=%id,"execution dispatch will retry");
                }
            }
        }
        Ok(())
    }
}
async fn status(tx: &mut Tx<'_>, item: Uuid, state: &str, reason: &str) -> Result<()> {
    sqlx::query("UPDATE mobile_v9_dispatch_state SET revision=revision+CASE WHEN (state,reason) IS DISTINCT FROM ($2,$3) THEN 1 ELSE 0 END,state=$2,reason=$3,next_retry_at=clock_timestamp()+interval '5 seconds',updated_at=clock_timestamp() WHERE item_id=$1").bind(item).bind(state).bind(reason).execute(&mut **tx).await?;
    Ok(())
}
async fn online(tx: &mut Tx<'_>, candidate: &Value) -> Result<bool> {
    Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM device_control_presence cp JOIN execution_devices d ON d.id=cp.id WHERE cp.id=$1 AND cp.state='online' AND NOT cp.desired_paused AND cp.last_control_heartbeat_at>clock_timestamp()-interval '30 seconds' AND d.enabled AND d.credential_generation=$2)").bind(uid(candidate,"device_id")?).bind(candidate["credential_generation"].as_i64().ok_or(Error::Conflict)?).fetch_one(&mut **tx).await?)
}
async fn ready(tx: &mut Tx<'_>, candidate: &Value) -> Result<bool> {
    if candidate["accepted"] != true || !online(tx, candidate).await? {
        return Ok(false);
    }
    Ok(!sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM mobile_execution_grants WHERE device_id=$1 AND NOT session_closed)").bind(uid(candidate,"device_id")?).fetch_one(&mut **tx).await?)
}
pub(super) async fn reconcile(tx: &mut Tx<'_>, item: Uuid) -> Result<()> {
    let row:Value=sqlx::query_scalar("SELECT to_jsonb(a)||jsonb_build_object('plan',p.plan,'snapshot_id',p.snapshot_id,'target_id',i.target_id,'task_id',t.id,'stopped',t.stopped,'expired',t.confirmed_until<=clock_timestamp(),'candidates',sc.candidates,'candidate_cursor',ds.candidate_cursor) FROM mobile_v9_task_items i JOIN mobile_v9_tasks t ON t.id=i.task_id JOIN mobile_v9_attempts a ON a.item_id=i.id AND a.current JOIN mobile_v9_plans p ON p.id=a.plan_id JOIN mobile_v9_dispatch_scopes sc ON sc.item_id=i.id JOIN mobile_v9_dispatch_state ds ON ds.item_id=i.id WHERE i.id=$1 FOR UPDATE OF a,ds").bind(item).fetch_one(&mut **tx).await?;
    let aid = uid(&row, "id")?;
    if row["stopped"] == true || row["expired"] == true {
        sqlx::query("UPDATE mobile_execution_grants SET state='superseded',superseded_reason=$2 WHERE attempt_id=$1 AND state='active'").bind(aid).bind("task_not_active").execute(&mut **tx).await?;
        return status(
            tx,
            item,
            if row["stopped"] == true {
                "stopped"
            } else {
                "confirmation_expired"
            },
            "task_not_active",
        )
        .await;
    }
    let sent:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_v9_attempts WHERE item_id=$1 AND state IN ('observed_sent','manually_confirmed_sent'))").bind(item).fetch_one(&mut **tx).await?;
    if sent {
        sqlx::query("UPDATE mobile_execution_grants g SET state='superseded' FROM mobile_v9_attempts a WHERE a.item_id=$1 AND g.attempt_id=a.id AND g.state='active'").bind(item).execute(&mut **tx).await?;
        return status(tx, item, "completed", "result_recorded").await;
    }
    let grant:Option<Value>=sqlx::query_scalar("SELECT (to_jsonb(g)-'envelope')||jsonb_build_object('deadline_reached',result_deadline_at<=clock_timestamp()) FROM mobile_execution_grants g WHERE attempt_id=$1").bind(aid).fetch_optional(&mut **tx).await?;
    let state = row["state"].as_str().ok_or(Error::Conflict)?;
    let outcome = grant
        .as_ref()
        .and_then(|g| g["result_body"]["outcome"].as_str())
        .or(
            if matches!(
                state,
                "failed_before_call" | "invalidated" | "verified_not_sent"
            ) {
                Some("failed")
            } else {
                None
            },
        );
    let mut reason = replacement_reason(
        online(tx, &row["plan"]).await?,
        outcome,
        grant
            .as_ref()
            .is_some_and(|g| g["deadline_reached"] == true),
    );
    if reason.is_none() && grant.is_none() && state == "queued" {
        let school = uid(&row, "school_id")?;
        let choices = tasks::raw_candidates(tx, school, uid(&row, "target_id")?).await?;
        let available = choices
            .iter()
            .find(|c| tasks::dependencies(c) == tasks::dependencies(&row["plan"]));
        if let Some(candidate) = available {
            if !ready(tx, candidate).await? {
                reason = Some("candidate_unavailable");
            }
            match tasks::validate_plan(tx, school, &row["plan"]).await {
                Ok(()) => {}
                Err(Error::Policy(_) | Error::NotFound | Error::Forbidden) => {
                    reason = Some("plan_dependency_changed")
                }
                Err(e) => return Err(e),
            }
        } else {
            reason = Some("plan_dependency_changed");
        }
    }
    if grant.is_none() && row["plan"]["execution"]["contract"] != "device_owned/1" {
        reason = Some("execution_contract_replaced");
    }
    if reason.is_none() && grant.is_some() {
        return status(tx, item, "executing", "awaiting_result").await;
    }
    if reason.is_none() && state == "queued" {
        return status(tx, item, "assigned", "candidate_ready").await;
    }
    let reason = reason.unwrap_or("execution_failed");
    sqlx::query("UPDATE mobile_execution_grants SET state='superseded',superseded_reason=$2 WHERE attempt_id=$1 AND state='active'").bind(aid).bind(reason).execute(&mut **tx).await?;
    sqlx::query("UPDATE mobile_v9_quota_reservations SET state='risk_held' WHERE attempt_id=$1 AND state='reserved'").bind(aid).execute(&mut **tx).await?;
    sqlx::query("UPDATE mobile_v9_attempts SET termination_reason=$2 WHERE id=$1")
        .bind(aid)
        .bind(reason)
        .execute(&mut **tx)
        .await?;
    let school = uid(&row, "school_id")?;
    authorize(tx, uid(&row, "confirmed_by")?, Some(school), true).await?;
    let choices = tasks::raw_candidates(tx, school, uid(&row, "target_id")?).await?;
    let approved = row["candidates"].as_array().ok_or(Error::Conflict)?;
    let current = approved
        .iter()
        .position(|c| c["device_id"] == row["plan"]["device_id"])
        .unwrap_or(0);
    let start = row["candidate_cursor"]
        .as_u64()
        .unwrap_or((current + 1) as u64) as usize;
    // A round never immediately selects the failing phone again. After exhaustion, next tick starts a new round.
    let previous: String =
        sqlx::query_scalar("SELECT state FROM mobile_v9_dispatch_state WHERE item_id=$1")
            .bind(item)
            .fetch_one(&mut **tx)
            .await?;
    let start = if start == 0 && previous != "waiting_resources" {
        current + 1
    } else {
        start
    };
    for (index, snapshot) in approved.iter().enumerate().skip(start) {
        let Some(candidate) = choices
            .iter()
            .find(|c| tasks::dependencies(c) == tasks::dependencies(snapshot))
        else {
            continue;
        };
        if !ready(tx, candidate).await? {
            continue;
        }
        match tasks::validate_plan(tx, school, snapshot).await {
            Ok(()) => {}
            Err(Error::Policy(_) | Error::NotFound | Error::Forbidden) => continue,
            Err(e) => return Err(e),
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
        let mut next = candidate.clone();
        if !snapshot["scope_id"].is_null() {
            next["scope_id"] = snapshot["scope_id"].clone();
        }
        replace(tx, &row, &next, "automatic", reason).await?;
        sqlx::query("UPDATE mobile_v9_dispatch_state SET candidate_cursor=$2 WHERE item_id=$1")
            .bind(item)
            .bind((index + 1) as i32)
            .execute(&mut **tx)
            .await?;
        return status(tx, item, "assigned", reason).await;
    }
    // Persist exhaustion, then allow a fresh candidate round on a later scan.
    sqlx::query("UPDATE mobile_v9_dispatch_state SET candidate_cursor=0 WHERE item_id=$1")
        .bind(item)
        .execute(&mut **tx)
        .await?;
    status(
        tx,
        item,
        "waiting_resources",
        "no_available_authorized_candidate",
    )
    .await
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
