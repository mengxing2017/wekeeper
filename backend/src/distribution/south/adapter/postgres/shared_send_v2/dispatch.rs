//! Fixed-rate database reconciliation; no task owns a waiting thread or connection.
use super::*;
use crate::distribution::domain::shared_mobile::execution::{
    dispatch_terminal, replacement_reason, round_exhausted,
};
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
        let ids:Vec<Uuid>=sqlx::query_scalar("SELECT item_id FROM mobile_v9_dispatch_state WHERE next_retry_at<=clock_timestamp() AND state NOT IN ('completed','stopped','confirmation_expired','dispatch_failed') ORDER BY next_retry_at,item_id LIMIT 100").fetch_all(&self.0).await?;
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
                    let mut retry = self.0.begin().await?;
                    guard(&mut retry).await?;
                    sqlx::query("UPDATE mobile_v9_dispatch_state SET infrastructure_failures=least(infrastructure_failures+1,4),next_retry_at=clock_timestamp()+make_interval(secs=>least(60,5*power(2,infrastructure_failures+1))::double precision) WHERE item_id=$1 AND state NOT IN ('completed','stopped','confirmation_expired','dispatch_failed')").bind(id).execute(&mut *retry).await?;
                    retry.commit().await?;
                    tracing::warn!(%error,item=%id,"execution dispatch infrastructure error; retry backed off");
                }
            }
        }
        Ok(())
    }
}
async fn status(tx: &mut Tx<'_>, item: Uuid, state: &str, reason: &str) -> Result<()> {
    sqlx::query("UPDATE mobile_v9_dispatch_state SET revision=revision+CASE WHEN (state,reason) IS DISTINCT FROM ($2,$3) THEN 1 ELSE 0 END,state=$2,reason=$3,infrastructure_failures=0,next_retry_at=CASE WHEN $2 IN ('completed','stopped','confirmation_expired','dispatch_failed') THEN NULL ELSE clock_timestamp()+interval '5 seconds' END,finished_at=CASE WHEN $2 IN ('completed','stopped','confirmation_expired','dispatch_failed') THEN coalesce(finished_at,clock_timestamp()) ELSE finished_at END,updated_at=clock_timestamp() WHERE item_id=$1").bind(item).bind(state).bind(reason).execute(&mut **tx).await?;
    Ok(())
}
async fn candidate_available(tx: &mut Tx<'_>, candidate: &Value) -> Result<bool> {
    Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM device_control_presence cp JOIN execution_devices d ON d.id=cp.id WHERE cp.id=$1 AND cp.state IN ('online','suspect') AND d.enabled AND d.credential_generation=$2)").bind(uid(candidate,"device_id")?).bind(candidate["credential_generation"].as_i64().ok_or(Error::Conflict)?).fetch_one(&mut **tx).await?)
}
async fn presence_offline(tx: &mut Tx<'_>, device: Uuid) -> Result<bool> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM device_control_presence WHERE id=$1 AND state='offline')",
    )
    .bind(device)
    .fetch_one(&mut **tx)
    .await?)
}
async fn ready(tx: &mut Tx<'_>, candidate: &Value) -> Result<bool> {
    if candidate["accepted"] != true || !candidate_available(tx, candidate).await? {
        return Ok(false);
    }
    Ok(!sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM mobile_execution_grants WHERE device_id=$1 AND NOT session_closed)").bind(uid(candidate,"device_id")?).fetch_one(&mut **tx).await?)
}
pub(super) async fn reconcile(tx: &mut Tx<'_>, item: Uuid) -> Result<()> {
    let source:Option<(Uuid,Uuid)>=sqlx::query_as("SELECT l.receipt_id,i.target_id FROM mobile_v9_task_items i JOIN source_execution_generations l ON l.task_id=i.task_id WHERE i.id=$1 AND NOT EXISTS(SELECT 1 FROM mobile_v9_attempts a JOIN mobile_execution_grants g ON g.attempt_id=a.id WHERE a.item_id=i.id AND a.current)").bind(item).fetch_optional(&mut **tx).await?;
    if let Some((receipt, _)) = source {
        let current:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM source_execution_tasks l JOIN mobile_v9_task_items i ON i.task_id=l.task_id WHERE l.receipt_id=$1 AND i.id=$2)").bind(receipt).bind(item).fetch_one(&mut **tx).await?;
        if !current {
            return status(tx, item, "stopped", "composition_superseded").await;
        }
    }
    if let Some((receipt, target)) = source
        && let Err(error) =
            crate::source_access::south::adapter::postgres::routing::guard_submission(
                tx, receipt, target,
            )
            .await
    {
        if error.0 == "temporarily_unavailable" {
            return Err(Error::Unavailable);
        }
        return status(
            tx,
            item,
            if matches!(
                error.0,
                "content_version_superseded"
                    | "route_retired"
                    | "target_removed"
                    | "target_identity_changed"
                    | "not_found"
            ) {
                "stopped"
            } else {
                "scheduling"
            },
            error.0,
        )
        .await;
    }

    let row:Value=sqlx::query_scalar("SELECT to_jsonb(a)||jsonb_build_object('plan',p.plan,'snapshot_id',p.snapshot_id,'target_id',i.target_id,'task_id',t.id,'stopped',t.stopped,'expired',t.confirmed_until<=clock_timestamp(),'candidates',sc.candidates,'candidate_cursor',ds.candidate_cursor,'dispatch_state',ds.state,'completed_rounds',ds.completed_rounds,'round_origin',ds.round_origin,'due',ds.next_retry_at<=clock_timestamp(),'eligible',a.not_before<=clock_timestamp() AND t.next_eligible_at<=clock_timestamp() AND (t.dispatch_mode<>'legacy_sequence' OR NOT EXISTS(SELECT 1 FROM mobile_v9_task_items prev WHERE prev.task_id=t.id AND prev.position<i.position AND NOT EXISTS(SELECT 1 FROM mobile_v9_attempts sent WHERE sent.item_id=prev.id AND sent.state IN ('observed_sent','manually_confirmed_sent')))),'delivery_expired',ds.assigned_at+interval '5 minutes'<=clock_timestamp()) FROM mobile_v9_task_items i JOIN mobile_v9_tasks t ON t.id=i.task_id JOIN mobile_v9_attempts a ON a.item_id=i.id AND a.current JOIN mobile_v9_plans p ON p.id=a.plan_id JOIN mobile_v9_dispatch_scopes sc ON sc.item_id=i.id JOIN mobile_v9_dispatch_state ds ON ds.item_id=i.id WHERE i.id=$1 FOR UPDATE OF a,ds").bind(item).fetch_one(&mut **tx).await?;
    if dispatch_terminal(row["dispatch_state"].as_str().unwrap_or("")) || row["due"] != true {
        return Ok(());
    }
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
    let issued: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM mobile_execution_grants WHERE attempt_id=$1)",
    )
    .bind(aid)
    .fetch_one(&mut **tx)
    .await?;
    if row["eligible"] != true && !issued {
        sqlx::query("UPDATE mobile_v9_dispatch_state SET assigned_at=NULL,round_started_at=NULL WHERE item_id=$1")
            .bind(item)
            .execute(&mut **tx)
            .await?;
        return status(tx, item, "scheduling", "business_interval").await;
    }
    sqlx::query("UPDATE mobile_v9_dispatch_state SET assigned_at=coalesce(assigned_at,clock_timestamp()),round_started_at=coalesce(round_started_at,clock_timestamp()) WHERE item_id=$1").bind(item).execute(&mut **tx).await?;
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
    // Delivery is a durable execution fact. New-assignment readiness (pause,
    // heartbeat freshness, etc.) cannot cancel an already delivered packet.
    // A disconnected phone can still hand off to an unvisited authorized
    // candidate, but a singleton must retain its full result-wait deadline.
    let available = if grant.is_some() {
        let has_successor = row["candidate_cursor"].as_u64().ok_or(Error::Conflict)?
            < row["candidates"].as_array().ok_or(Error::Conflict)?.len() as u64;
        !has_successor || !presence_offline(tx, uid(&row["plan"], "device_id")?).await?
    } else {
        candidate_available(tx, &row["plan"]).await?
    };
    let mut reason = replacement_reason(
        available,
        outcome,
        grant
            .as_ref()
            .is_some_and(|g| g["deadline_reached"] == true),
    );
    // Pause, stale authorization and other candidate constraints do not mean
    // that authenticated device presence has timed out.
    if grant.is_none()
        && reason == Some("device_offline")
        && !presence_offline(tx, uid(&row["plan"], "device_id")?).await?
    {
        reason = Some("candidate_unavailable");
    }
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
    if row["dispatch_state"] == "waiting_resources" {
        reason = Some("next_candidate_round");
    } else if grant.is_none() && row["delivery_expired"] == true {
        reason = Some("delivery_not_received_timeout");
    }
    if reason.is_none() && grant.is_some() {
        return status(tx, item, "executing", "awaiting_result").await;
    }
    if reason.is_none() && state == "queued" {
        return status(tx, item, "assigned", "candidate_ready").await;
    }
    let reason = reason.unwrap_or("execution_failed");
    sqlx::query("UPDATE mobile_execution_grants SET state='superseded',superseded_reason=$2 WHERE attempt_id=$1 AND state='active'").bind(aid).bind(reason).execute(&mut **tx).await?;
    sqlx::query("UPDATE mobile_v9_quota_reservations SET state=CASE WHEN EXISTS(SELECT 1 FROM mobile_execution_grants WHERE attempt_id=$1) THEN 'risk_held' ELSE 'released' END WHERE attempt_id=$1 AND state='reserved'").bind(aid).execute(&mut **tx).await?;
    sqlx::query("UPDATE mobile_v9_attempts SET termination_reason=$2 WHERE id=$1")
        .bind(aid)
        .bind(reason)
        .execute(&mut **tx)
        .await?;
    if row["dispatch_state"] != "waiting_resources" {
        event(
            tx,
            item,
            &row,
            "assignment_failed",
            Some(uid(&row["plan"], "device_id")?),
            reason,
        )
        .await?;
    }
    let school = uid(&row, "school_id")?;
    match authorize(tx, uid(&row, "confirmed_by")?, Some(school), true).await {
        Ok(()) => {}
        Err(Error::Forbidden | Error::Unauthorized | Error::NotFound) => {
            return status(tx, item, "stopped", "authorization_revoked").await;
        }
        Err(error) => return Err(error),
    }
    let choices = tasks::raw_candidates(tx, school, uid(&row, "target_id")?).await?;
    let approved = row["candidates"].as_array().ok_or(Error::Conflict)?;
    let start = row["candidate_cursor"].as_u64().ok_or(Error::Conflict)? as usize;
    let origin = row["round_origin"].as_u64().ok_or(Error::Conflict)? as usize;
    for offset in start..approved.len() {
        let index = (origin + offset) % approved.len();
        let snapshot = &approved[index];
        event(
            tx,
            item,
            &row,
            "candidate_checked",
            Some(uid(snapshot, "device_id")?),
            "evaluating",
        )
        .await?;
        let Some(candidate) = choices
            .iter()
            .find(|c| tasks::dependencies(c) == tasks::dependencies(snapshot))
        else {
            event(
                tx,
                item,
                &row,
                "candidate_skipped",
                Some(uid(snapshot, "device_id")?),
                "plan_dependency_changed",
            )
            .await?;
            continue;
        };
        if !ready(tx, candidate).await? {
            event(
                tx,
                item,
                &row,
                "candidate_skipped",
                Some(uid(snapshot, "device_id")?),
                "candidate_unavailable",
            )
            .await?;
            continue;
        }
        match tasks::validate_plan(tx, school, snapshot).await {
            Ok(()) => {}
            Err(Error::Policy(_) | Error::NotFound | Error::Forbidden) => {
                event(
                    tx,
                    item,
                    &row,
                    "candidate_skipped",
                    Some(uid(snapshot, "device_id")?),
                    "plan_dependency_changed",
                )
                .await?;
                continue;
            }
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
            event(
                tx,
                item,
                &row,
                "candidate_skipped",
                Some(uid(snapshot, "device_id")?),
                "quota_wait",
            )
            .await?;
            continue;
        }
        let mut next = candidate.clone();
        if !snapshot["scope_id"].is_null() {
            next["scope_id"] = snapshot["scope_id"].clone();
        }
        replace(tx, &row, &next, "automatic", reason).await?;
        sqlx::query("UPDATE mobile_v9_dispatch_state SET candidate_cursor=$2,assigned_at=clock_timestamp() WHERE item_id=$1")
            .bind(item)
            .bind((offset + 1) as i32)
            .execute(&mut **tx)
            .await?;
        return status(tx, item, "assigned", reason).await;
    }
    // An obsolete frozen scope cannot regain authority by waiting for a phone.
    // Keep its original plans and results, and never silently replace its target.
    if !approved.iter().any(|snapshot| {
        choices.iter().any(|candidate| {
            tasks::dependencies(candidate) == tasks::dependencies(snapshot)
                && snapshot["target"]["search_query"] == snapshot["target_name"]
        })
    }) {
        sqlx::query("UPDATE mobile_v9_attempts SET state='invalidated',revision=revision+1,invalidation_reason='plan_dependency_changed',invalidated_at=clock_timestamp() WHERE id=$1 AND state='queued' AND NOT EXISTS(SELECT 1 FROM mobile_execution_grants WHERE attempt_id=$1)")
            .bind(aid).execute(&mut **tx).await?;
        return status(tx, item, "stopped", "plan_dependency_changed").await;
    }
    // Exhausting candidates must not turn the five-second scanner into the
    // retry clock. Retire unissued work, but persist this round until its full
    // five-minute window has elapsed (including offline/busy singleton scopes).
    let waiting: bool = sqlx::query_scalar("SELECT round_started_at+interval '5 minutes'>clock_timestamp() FROM mobile_v9_dispatch_state WHERE item_id=$1")
        .bind(item).fetch_one(&mut **tx).await?;
    if waiting {
        sqlx::query("UPDATE mobile_v9_attempts SET state='invalidated',revision=revision+1,invalidation_reason=$2,invalidated_at=clock_timestamp() WHERE id=$1 AND state='queued' AND NOT EXISTS(SELECT 1 FROM mobile_execution_grants WHERE attempt_id=$1)")
            .bind(aid).bind(reason).execute(&mut **tx).await?;
        status(tx, item, "waiting_resources", "awaiting_round_deadline").await?;
        sqlx::query("UPDATE mobile_v9_dispatch_state SET candidate_cursor=$2,next_retry_at=round_started_at+interval '5 minutes' WHERE item_id=$1")
            .bind(item).bind(approved.len() as i32).execute(&mut **tx).await?;
        return Ok(());
    }
    event(tx, item, &row, "round_exhausted", None, reason).await?;
    let exhausted =
        round_exhausted(row["completed_rounds"].as_i64().ok_or(Error::Conflict)? as i32);
    sqlx::query("UPDATE mobile_v9_dispatch_state SET completed_rounds=completed_rounds+1,last_failed_attempt_id=$2,assigned_at=NULL,round_started_at=NULL,revision=revision+1 WHERE item_id=$1").bind(item).bind(aid).execute(&mut **tx).await?;
    // The old queued assignment must never become executable between rounds.
    sqlx::query("UPDATE mobile_v9_attempts SET state='invalidated',revision=revision+1,invalidation_reason=$2,termination_reason=$2,invalidated_at=clock_timestamp() WHERE id=$1 AND state='queued' AND NOT EXISTS(SELECT 1 FROM mobile_execution_grants WHERE attempt_id=$1)").bind(aid).bind(if exhausted {"dispatch_round_limit_exceeded"} else {reason}).execute(&mut **tx).await?;
    if exhausted {
        event(
            tx,
            item,
            &row,
            "dispatch_failed",
            None,
            "dispatch_round_limit_exceeded",
        )
        .await?;
        tracing::warn!(%item, completed_rounds=3, "task dispatch failed: candidate rounds exhausted");
        return status(tx, item, "dispatch_failed", "dispatch_round_limit_exceeded").await;
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
async fn event(
    tx: &mut Tx<'_>,
    item: Uuid,
    row: &Value,
    kind: &str,
    device: Option<Uuid>,
    reason: &str,
) -> Result<()> {
    sqlx::query("INSERT INTO mobile_v9_dispatch_events(item_id,round_no,kind,device_id,attempt_id,reason) VALUES($1,$2,$3,$4,$5,$6)")
        .bind(item).bind(row["completed_rounds"].as_i64().ok_or(Error::Conflict)? as i32+1).bind(kind).bind(device).bind(uid(row,"id")?).bind(reason).execute(&mut **tx).await?;
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
