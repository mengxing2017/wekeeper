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
            // The due set may have changed before this writer acquired the lock.
            let due: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_v9_dispatch_state WHERE item_id=$1 AND next_retry_at<=clock_timestamp() AND state NOT IN ('completed','stopped','confirmation_expired','dispatch_failed'))")
                .bind(id).fetch_one(&mut *tx).await?;
            if !due {
                continue;
            }
            let operation = records::begin_evaluation(&self.0, id).await?;
            if let Some(op) = operation {
                records::context(&mut tx, "wekeeper.mobile_operation", op).await?;
            }
            match reconcile(&mut tx, id).await {
                Ok(()) => tx.commit().await?,
                Err(error) => {
                    tx.rollback().await?;
                    let mut retry = self.0.begin().await?;
                    guard(&mut retry).await?;
                    sqlx::query("UPDATE mobile_v9_dispatch_state SET infrastructure_failures=least(infrastructure_failures+1,4),next_retry_at=clock_timestamp()+make_interval(secs=>least(60,5*power(2,infrastructure_failures+1))::double precision) WHERE item_id=$1 AND state NOT IN ('completed','stopped','confirmation_expired','dispatch_failed')").bind(id).execute(&mut *retry).await?;
                    if let Some(op) = operation {
                        sqlx::query("UPDATE mobile_dispatch_operations SET result='retrying',reason_code=$2,finished_at=clock_timestamp(),retry_at=(SELECT next_retry_at FROM mobile_v9_dispatch_state WHERE item_id=$3) WHERE id=$1 AND result='started'").bind(op).bind(records::failure(&error)).bind(id).execute(&mut *retry).await?;
                        sqlx::query("INSERT INTO mobile_v9_dispatch_events(intake_id,item_id,operation_id,kind,reason,result,evidence) SELECT intake_id,$2,id,'evaluation_failed',$3,'retrying',jsonb_build_object('step','dispatch_reconcile') FROM mobile_dispatch_operations WHERE id=$1").bind(op).bind(id).bind(records::failure(&error)).execute(&mut *retry).await?;
                    }
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
    let facts:Value=sqlx::query_scalar("SELECT jsonb_build_object('presence',(SELECT state FROM device_control_presence WHERE id=$1),'enabled',d.enabled,'generation',d.credential_generation,'expected_generation',$2::bigint,'connection_available',EXISTS(SELECT 1 FROM device_control_sessions s JOIN device_credentials c ON c.device_id=s.device_id AND c.generation=s.credential_generation WHERE s.device_id=d.id AND s.connected AND s.transport='wss' AND s.runtime_epoch IS NOT NULL AND c.revoked_at IS NULL AND c.expires_at>clock_timestamp()+interval '210 seconds'),'busy',(SELECT jsonb_build_object('execution_id',id,'attempt_id',attempt_id,'execute_before',execute_before) FROM mobile_execution_grants WHERE device_id=$1 AND NOT session_closed ORDER BY assigned_at LIMIT 1)) FROM execution_devices d WHERE id=$1")
        .bind(uid(candidate,"device_id")?).bind(candidate["credential_generation"].as_i64()).fetch_optional(&mut **tx).await?.unwrap_or(json!({}));
    let reason =
        crate::distribution::domain::shared_mobile::dispatch_records::candidate_reason(&facts);
    let reason = if reason == "passed"
        && !resource_budgets::check(tx, uid(candidate, "binding_id")?).await?
    {
        "resource_budget_unavailable"
    } else if reason == "passed" && facts["connection_available"] != true {
        "delivery_connection_unavailable"
    } else {
        reason
    };
    records::check(
        tx,
        uid(candidate, "device_id")?,
        "candidate_readiness",
        reason,
        facts,
    )
    .await?;
    Ok(reason == "passed")
}
pub(super) async fn reconcile(tx: &mut Tx<'_>, item: Uuid) -> Result<()> {
    let previous: Option<String> =
        sqlx::query_scalar("SELECT nullif(current_setting('wekeeper.mobile_operation',true),'')")
            .fetch_one(&mut **tx)
            .await?;
    let evaluation: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM mobile_dispatch_operations WHERE id=$1 AND kind='evaluate'",
    )
    .bind(previous.as_deref().and_then(|s| s.parse::<Uuid>().ok()))
    .fetch_optional(&mut **tx)
    .await?;
    let op = if let Some(op) = evaluation {
        Some(op)
    } else {
        let op = Uuid::now_v7();
        let inserted=sqlx::query("INSERT INTO mobile_dispatch_operations(id,intake_id,kind,round_no,attempt_id) SELECT $1,i.id,'evaluate',least(d.completed_rounds+1,3),a.id FROM mobile_dispatch_intakes i JOIN mobile_v9_dispatch_state d ON d.item_id=i.item_id JOIN mobile_v9_attempts a ON a.item_id=i.item_id AND a.current WHERE i.item_id=$2").bind(op).bind(item).execute(&mut **tx).await?.rows_affected();
        if inserted > 0 { Some(op) } else { None }
    };
    if let Some(op) = op {
        records::context(tx, "wekeeper.mobile_operation", op).await?;
    }
    reconcile_inner(tx, item).await?;
    if let Some(op) = op {
        sqlx::query("UPDATE mobile_dispatch_operations o SET result=d.state,reason_code=d.reason,finished_at=clock_timestamp(),retry_at=d.next_retry_at FROM mobile_v9_dispatch_state d WHERE o.id=$1 AND d.item_id=$2").bind(op).bind(item).execute(&mut **tx).await?;
    }
    sqlx::query("SELECT set_config('wekeeper.mobile_operation',$1,true)")
        .bind(previous.unwrap_or_default())
        .execute(&mut **tx)
        .await?;
    Ok(())
}
async fn reconcile_inner(tx: &mut Tx<'_>, item: Uuid) -> Result<()> {
    let row:Value=sqlx::query_scalar("SELECT to_jsonb(a)||jsonb_build_object('plan',p.plan,'snapshot_id',p.snapshot_id,'target_id',i.target_id,'task_id',t.id,'stopped',t.stopped,'authorization_kind',t.authorization_kind,'expired',t.confirmed_until<=clock_timestamp(),'candidates',sc.candidates,'candidate_cursor',ds.candidate_cursor,'dispatch_state',ds.state,'completed_rounds',ds.completed_rounds,'round_origin',ds.round_origin,'due',ds.next_retry_at<=clock_timestamp(),'eligible',a.not_before<=clock_timestamp() AND t.next_eligible_at<=clock_timestamp(),'delivery_expired',ds.assigned_at+interval '5 minutes'<=clock_timestamp()) FROM mobile_v9_task_items i JOIN mobile_v9_tasks t ON t.id=i.task_id JOIN mobile_v9_attempts a ON a.item_id=i.id AND a.current JOIN mobile_v9_plans p ON p.id=a.plan_id JOIN mobile_v9_dispatch_scopes sc ON sc.item_id=i.id JOIN mobile_v9_dispatch_state ds ON ds.item_id=i.id WHERE i.id=$1 FOR UPDATE OF a,ds").bind(item).fetch_one(&mut **tx).await?;
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
    let grant:Option<Value>=sqlx::query_scalar("SELECT (to_jsonb(g)-'envelope')||jsonb_build_object('deadline_reached',CASE WHEN accepted_at IS NULL AND result_id IS NULL THEN receipt_deadline_at ELSE result_deadline_at END<=clock_timestamp()) FROM mobile_execution_grants g WHERE attempt_id=$1").bind(aid).fetch_optional(&mut **tx).await?;
    if grant.is_none()
        && device::business_wait(
            tx,
            uid(&row["plan"], "device_id")?,
            uid(&row, "target_id")?,
            aid,
        )
        .await?
    {
        sqlx::query("UPDATE mobile_v9_dispatch_state SET round_started_at=clock_timestamp(),next_retry_at=clock_timestamp()+interval '5 seconds',reason='business_scheme_wait' WHERE item_id=$1").bind(item).execute(&mut **tx).await?;
        return Ok(());
    }
    // Record the current candidate too: an offline singleton never enters the successor loop.
    if grant.is_none() {
        let current_choices = tasks::raw_candidates(tx, uid(&row, "target_id")?).await?;
        let current = current_choices
            .iter()
            .find(|c| tasks::dependencies(c) == tasks::dependencies(&row["plan"]));
        ready(tx, current.unwrap_or(&row["plan"])).await?;
    }
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
    // Delivery is a durable execution fact. New-assignment checks (business policy,
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
    if reason == Some("result_timeout")
        && grant
            .as_ref()
            .is_some_and(|g| g["accepted_at"].is_null() && g["result_id"].is_null())
    {
        reason = Some("receipt_timeout");
    }
    if let Some(issued) = &grant
        && issued["state"] == "superseded"
        && matches!(
            issued["superseded_reason"].as_str(),
            Some("receipt_timeout" | "delivery_write_failed")
        )
    {
        reason = Some(if issued["superseded_reason"] == "receipt_timeout" {
            "receipt_timeout"
        } else {
            "delivery_write_failed"
        });
    }
    // Pause, stale authorization and other candidate constraints do not mean
    // that authenticated device presence has timed out.
    if grant.is_none()
        && reason == Some("device_offline")
        && !presence_offline(tx, uid(&row["plan"], "device_id")?).await?
    {
        reason = Some("candidate_unavailable");
    }
    if reason.is_none() && grant.is_none() && state == "queued" {
        let choices = tasks::raw_candidates(tx, uid(&row, "target_id")?).await?;
        let available = choices
            .iter()
            .find(|c| tasks::dependencies(c) == tasks::dependencies(&row["plan"]));
        if let Some(candidate) = available {
            if !ready(tx, candidate).await? {
                reason = Some("candidate_unavailable");
            }
            match tasks::validate_plan(tx, &row["plan"]).await {
                Ok(()) => {}
                Err(ref error @ (Error::Policy(_) | Error::NotFound | Error::Forbidden)) => {
                    records::check(
                        tx,
                        uid(&row["plan"], "device_id")?,
                        "plan_authorization",
                        &records::failure(error),
                        json!({}),
                    )
                    .await?;
                    reason = Some("plan_dependency_changed")
                }
                Err(e) => return Err(e),
            }
        } else {
            reason = Some("plan_dependency_changed");
        }
    }
    let current_quota_wait = reason.is_none()
        && grant.is_none()
        && state == "queued"
        && !device::quota(
            tx,
            uid(&row["plan"], "device_id")?,
            uid(&row, "target_id")?,
            aid,
            false,
        )
        .await?;
    if current_quota_wait {
        reason = Some("quota_wait");
    }
    if grant.is_none() && row["plan"]["execution"]["contract"] != "device_owned/2" {
        reason = Some("execution_contract_replaced");
    }
    if row["dispatch_state"] == "waiting_resources" {
        reason = Some("next_candidate_round");
    } else if grant.is_none() && row["delivery_expired"] == true {
        reason = Some("delivery_not_received_timeout");
    }
    if reason.is_none() && grant.is_some() {
        return status(
            tx,
            item,
            "executing",
            if grant
                .as_ref()
                .is_some_and(|g| g["accepted_at"].is_null() && g["result_id"].is_null())
            {
                "awaiting_receipt"
            } else {
                "awaiting_result"
            },
        )
        .await;
    }
    if reason.is_none() && state == "queued" {
        match push::prepare(tx, aid).await? {
            None => return status(tx, item, "executing", "awaiting_receipt").await,
            Some(why) => reason = Some(why),
        }
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

    if row["authorization_kind"] != "source_route" {
        match authorize(tx, uid(&row, "confirmed_by")?, true).await {
            Ok(()) => {}
            Err(Error::Forbidden | Error::Unauthorized | Error::NotFound) => {
                return status(tx, item, "stopped", "authorization_revoked").await;
            }
            Err(error) => return Err(error),
        }
    }
    let choices = tasks::raw_candidates(tx, uid(&row, "target_id")?).await?;
    let approved = row["candidates"].as_array().ok_or(Error::Conflict)?;
    let start = row["candidate_cursor"].as_u64().ok_or(Error::Conflict)? as usize;
    let origin = row["round_origin"].as_u64().ok_or(Error::Conflict)? as usize;
    let designated_unknown:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_dispatch_intakes i JOIN receiving_dispatch_deliveries d ON d.id=i.delivery_id WHERE i.item_id=$1 AND d.device_id IS NOT NULL) AND EXISTS(SELECT 1 FROM mobile_v9_attempts a JOIN mobile_execution_grants g ON g.attempt_id=a.id WHERE a.item_id=$1 AND a.state NOT IN ('verified_not_sent','failed_before_call','observed_sent','manually_confirmed_sent') AND (g.result_id IS NULL OR g.result_body->>'outcome'='unknown'))")
        .bind(item).fetch_one(&mut **tx).await?;
    let mut quota_wait = current_quota_wait;
    for offset in start..approved.len() {
        if designated_unknown {
            event(
                tx,
                item,
                &row,
                "candidate_skipped",
                None,
                "designated_phone_result_unknown",
            )
            .await?;
            continue;
        }
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
            records::check(tx,uid(snapshot,"device_id")?,"frozen_dependencies","plan_dependency_changed",json!({"frozen_generation":snapshot["credential_generation"],"binding_id":snapshot["binding_id"]})).await?;
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
        match tasks::validate_plan(tx, snapshot).await {
            Ok(()) => {}
            Err(ref error @ (Error::Policy(_) | Error::NotFound | Error::Forbidden)) => {
                records::check(
                    tx,
                    uid(snapshot, "device_id")?,
                    "plan_authorization",
                    &records::failure(error),
                    json!({}),
                )
                .await?;
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
            uid(candidate, "device_id")?,
            uid(candidate, "target_id")?,
            aid,
            false,
        )
        .await?
        {
            quota_wait = true;
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
        let next = candidate.clone();
        replace(tx, &row, &next, "automatic", reason).await?;
        sqlx::query("UPDATE mobile_v9_dispatch_state SET candidate_cursor=$2,assigned_at=clock_timestamp() WHERE item_id=$1")
            .bind(item)
            .bind((offset + 1) as i32)
            .execute(&mut **tx)
            .await?;
        status(tx, item, "assigned", reason).await?;
        let next_attempt: Uuid =
            sqlx::query_scalar("SELECT id FROM mobile_v9_attempts WHERE item_id=$1 AND current")
                .bind(item)
                .fetch_one(&mut **tx)
                .await?;
        return match push::prepare(tx, next_attempt).await? {
            None => status(tx, item, "executing", "awaiting_receipt").await,
            Some(why) => status(tx, item, "assigned", why).await,
        };
    }
    if quota_wait {
        sqlx::query("UPDATE mobile_v9_dispatch_state SET round_started_at=clock_timestamp(),next_retry_at=clock_timestamp()+interval '5 seconds',reason='quota_wait' WHERE item_id=$1").bind(item).execute(&mut **tx).await?;
        return Ok(());
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
    sqlx::query("INSERT INTO mobile_v9_plans(id,item_id,target_id,device_id,snapshot_id,plan,digest,content_digest) VALUES($1,$2,$3,$4,$5,$6,$7,$8)")
        .bind(pid).bind(uid(old,"item_id")?).bind(uid(old,"target_id")?).bind(uid(&plan,"device_id")?).bind(uid(old,"snapshot_id")?).bind(&plan).bind(digest(&plan)?).bind(plan["content_digest"].as_str()).execute(&mut **tx).await?;
    sqlx::query("INSERT INTO mobile_v9_attempts(id,item_id,plan_id,attempt_no,confirmed_by) VALUES($1,$2,$3,$4,$5)")
        .bind(aid).bind(uid(old,"item_id")?).bind(pid).bind(old["attempt_no"].as_i64().ok_or(Error::Conflict)? as i32+1).bind(uid(old,"confirmed_by")?).execute(&mut **tx).await?;
    sqlx::query("INSERT INTO mobile_v9_reassignments(id,old_attempt_id,new_attempt_id,actor_id,reason,source) VALUES($1,$2,$3,$4,$5,$6)")
        .bind(Uuid::now_v7()).bind(uid(old,"id")?).bind(aid).bind(uid(old,"confirmed_by")?).bind(reason).bind(source).execute(&mut **tx).await?;
    Ok(aid)
}
