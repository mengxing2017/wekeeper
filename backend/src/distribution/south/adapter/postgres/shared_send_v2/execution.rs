//! Durable grants and completion facts; the fixed-rate dispatcher applies results.
use super::*;
use crate::distribution::domain::shared_mobile::execution::{
    Completion, ExecutionReceipt, GrantRequest,
};
pub(super) async fn context(repo: &Repository, c: &Credential) -> Result<Value> {
    let mut tx = repo.0.begin().await?;
    authenticate(&mut tx, c, false).await?;
    let delegation = delegation::context(&mut tx, c).await?;
    let turns:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('id',a.id,'interval_ms',0) FROM mobile_v9_attempts a JOIN mobile_v9_plans p ON p.id=a.plan_id JOIN mobile_v9_task_items i ON i.id=a.item_id JOIN mobile_v9_tasks t ON t.id=i.task_id WHERE NOT EXISTS(SELECT 1 FROM mobile_v9_dispatch_state ds WHERE ds.item_id=a.item_id AND ds.state IN ('waiting_resources','completed','stopped','confirmation_expired','dispatch_failed')) AND p.device_id=$1 AND a.current AND a.state='queued' AND (p.plan->>'credential_generation')::bigint=$2 AND NOT t.stopped AND t.confirmed_until>clock_timestamp() ORDER BY t.created_at,i.position LIMIT 100").bind(c.device_id).bind(c.generation).fetch_all(&mut *tx).await?;
    let inspections:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('id',b.id,'target',jsonb_build_object('kind','group','search_query',t.search_text,'match_policy','exact_unique','identity_constraints','[]'::jsonb)) FROM mobile_device_target_bindings b JOIN mobile_targets t ON t.id=b.target_id WHERE b.device_id=$1 AND b.enabled AND t.enabled AND t.archived_at IS NULL LIMIT 100").bind(c.device_id).fetch_all(&mut *tx).await?;
    let (public_key, _) = secrets::sign(&json!({}))?;
    tx.commit().await?;
    Ok(
        json!({"id":c.device_id,"protocol_version":10,"delegation":delegation,"turns":turns,"inspections":inspections,"verification_key":public_key}),
    )
}
/// Select work upstream, before delivery. No device candidate list or follow-up claim.
pub(super) async fn deliver(
    tx: &mut Tx<'_>,
    c: &Credential,
    v: crate::distribution::domain::shared_mobile::execution::DeliveryRequest,
) -> Result<Value> {
    require(!v.runtime_epoch.is_nil(), "execution_request_invalid")?;
    // A lost delivery (including a legacy grant response) is redelivered as the same task.
    // Never make the device request another permission or wait for its own issued session.
    let issued: Option<Vec<u8>> = sqlx::query_scalar("SELECT g.envelope FROM mobile_execution_grants g LEFT JOIN mobile_v9_attempts a ON a.id=g.attempt_id LEFT JOIN mobile_v9_task_items i ON i.id=a.item_id LEFT JOIN mobile_v9_tasks t ON t.id=i.task_id WHERE g.device_id=$1 AND g.credential_generation=$2 AND g.state='active' AND NOT g.session_closed AND g.result_id IS NULL AND g.execute_before>clock_timestamp() AND (($3::uuid IS NULL AND g.kind='send' AND a.current AND NOT t.stopped AND NOT EXISTS(SELECT 1 FROM mobile_v9_dispatch_state ds WHERE ds.item_id=a.item_id AND ds.state IN ('waiting_resources','completed','stopped','confirmation_expired','dispatch_failed')) AND NOT EXISTS(SELECT 1 FROM mobile_v9_attempts sent LEFT JOIN mobile_execution_grants result ON result.attempt_id=sent.id WHERE sent.item_id=a.item_id AND (sent.state IN ('observed_sent','manually_confirmed_sent') OR result.result_body->>'outcome'='observed_sent'))) OR (g.kind='inspection' AND g.inspection_id=$3))")
        .bind(c.device_id).bind(c.generation).bind(v.inspection_id).fetch_optional(&mut **tx).await?;
    if let Some(envelope) = issued {
        return secrets::open(&envelope);
    }
    let eligible_attempt_ids: Vec<Uuid> = sqlx::query_scalar("SELECT a.id FROM mobile_v9_attempts a JOIN mobile_v9_plans p ON p.id=a.plan_id JOIN mobile_v9_task_items i ON i.id=a.item_id JOIN mobile_v9_tasks t ON t.id=i.task_id WHERE NOT EXISTS(SELECT 1 FROM mobile_v9_dispatch_state ds WHERE ds.item_id=a.item_id AND ds.state IN ('waiting_resources','completed','stopped','confirmation_expired','dispatch_failed')) AND p.device_id=$1 AND a.current AND a.state='queued' AND a.not_before<=clock_timestamp() AND NOT t.stopped AND t.confirmed_until>clock_timestamp() AND t.next_eligible_at<=clock_timestamp() ORDER BY t.created_at,i.position LIMIT 100")
        .bind(c.device_id).fetch_all(&mut **tx).await?;
    grant(
        tx,
        c,
        GrantRequest {
            runtime_epoch: v.runtime_epoch,
            eligible_attempt_ids,
            inspection_id: v.inspection_id,
        },
    )
    .await
}
/// Closed results and success on any attempt fence replay before projection reconciliation.
pub(super) async fn can_redeliver(tx: &mut Tx<'_>, id: Uuid) -> Result<bool> {
    Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_execution_grants g LEFT JOIN mobile_v9_attempts a ON a.id=g.attempt_id LEFT JOIN mobile_v9_task_items i ON i.id=a.item_id LEFT JOIN mobile_v9_tasks t ON t.id=i.task_id WHERE g.id=$1 AND g.state='active' AND NOT g.session_closed AND g.result_id IS NULL AND g.execute_before>clock_timestamp() AND (g.kind='inspection' OR (a.current AND NOT t.stopped AND NOT EXISTS(SELECT 1 FROM mobile_v9_dispatch_state ds WHERE ds.item_id=a.item_id AND ds.state IN ('waiting_resources','completed','stopped','confirmation_expired','dispatch_failed')) AND NOT EXISTS(SELECT 1 FROM mobile_v9_attempts sent LEFT JOIN mobile_execution_grants result ON result.attempt_id=sent.id WHERE sent.item_id=a.item_id AND (sent.state IN ('observed_sent','manually_confirmed_sent') OR result.result_body->>'outcome'='observed_sent')))))")
        .bind(id).fetch_one(&mut **tx).await?)
}
/// Delivery metadata is outside the signed executable envelope and contains no new permission.
pub(super) fn delivery_response(value: Value) -> Result<Value> {
    let (public_key, _) = secrets::sign(&json!({}))?;
    let rejected = value["state"] == "rejected";
    let response = crate::distribution::domain::shared_mobile::execution::TaskDelivery {
        id: uid(&value, "id")?,
        schema: "task_delivery/1".into(),
        state: if rejected {
            "idle".into()
        } else {
            value["state"].as_str().ok_or(Error::Conflict)?.into()
        },
        reason: if rejected {
            Some("queue_refreshed".into())
        } else {
            value["reason"].as_str().map(str::to_owned)
        },
        verification_key: public_key,
        execution: if value["state"] == "granted" {
            Some(value)
        } else {
            None
        },
    };
    serde_json::to_value(response).map_err(|_| Error::Conflict)
}
pub(super) async fn grant(tx: &mut Tx<'_>, c: &Credential, v: GrantRequest) -> Result<Value> {
    require(
        v.eligible_attempt_ids.len() <= 100 && !v.runtime_epoch.is_nil(),
        "execution_request_invalid",
    )?;
    business_scheme::ensure_open(tx).await?;
    require(sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM device_credentials WHERE token_hash=$1 AND expires_at>clock_timestamp()+interval '210 seconds')").bind(&c.token_hash).fetch_one(&mut **tx).await?,"execution_credential_renewal_required")?;
    require(
        delegation::context(tx, c).await?["decision"]["accepted"] == true,
        "shared_send_consent_required",
    )?;
    let busy:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_execution_grants WHERE device_id=$1 AND NOT session_closed)").bind(c.device_id).fetch_one(&mut **tx).await?;
    if busy {
        return Ok(json!({"id":c.device_id,"state":"waiting","reason":"device_busy"}));
    }
    if let Some(binding) = v.inspection_id {
        let row:Option<Value>=sqlx::query_scalar("SELECT to_jsonb(b) FROM mobile_device_target_bindings b JOIN mobile_targets t ON t.id=b.target_id WHERE b.id=$1 AND b.device_id=$2 AND b.enabled AND t.enabled AND t.archived_at IS NULL").bind(binding).bind(c.device_id).fetch_optional(&mut **tx).await?;
        let Some(row) = row else {
            return Ok(dependency_rejection(c.device_id));
        };
        let school = uid(&row, "school_id")?;
        let target = uid(&row, "target_id")?;
        let plan = tasks::raw_candidates(tx, school, target)
            .await?
            .into_iter()
            .find(|p| p["binding_id"] == json!(binding));
        let Some(plan) = plan else {
            return Ok(dependency_rejection(c.device_id));
        };
        match tasks::validate_plan(tx, school, &plan).await {
            Ok(()) => {}
            Err(Error::Policy(_) | Error::NotFound | Error::Forbidden | Error::Invalid(_)) => {
                return Ok(dependency_rejection(c.device_id));
            }
            Err(error) => return Err(error),
        }
        let id = Uuid::now_v7();
        let secret = secrets::token();
        let timing:Value=sqlx::query_scalar("SELECT jsonb_build_object('server_time',statement_timestamp(),'not_before',statement_timestamp(),'execute_before',statement_timestamp()+interval '210 seconds')").fetch_one(&mut **tx).await?;
        let mut envelope = json!({"id":id,"execution_id":id,"capability_id":id,"schema":"durable_execution/1","protocol_version":10,"kind":"inspection","state":"granted","max_calls":0,"local_budget_ms":210000,"inspection_id":binding,"device_id":c.device_id,"credential_generation":c.generation,"plan":plan,"plan_digest":digest(&plan)?,"timing":timing,"result_credential":secret});
        let (public_key, signature) = secrets::sign(&envelope)?;
        envelope["signature"] = json!(signature);
        envelope["key_id"] = json!(hash(&public_key));
        sqlx::query("INSERT INTO mobile_execution_grants(id,inspection_id,device_id,credential_generation,kind,result_credential_hash,envelope) VALUES($1,$2,$3,$4,'inspection',$5,$6)").bind(id).bind(binding).bind(c.device_id).bind(c.generation).bind(hash(&secret)).bind(secrets::seal(&envelope)?).execute(&mut **tx).await?;
        return Ok(envelope);
    }

    let mut allowed_attempts = Vec::new();
    for attempt in &v.eligible_attempt_ids {
        let source:Option<(Uuid,Uuid)>=sqlx::query_as("SELECT l.receipt_id,i.target_id FROM mobile_v9_attempts a JOIN mobile_v9_task_items i ON i.id=a.item_id JOIN source_execution_generations l ON l.task_id=i.task_id WHERE a.id=$1").bind(attempt).fetch_optional(&mut **tx).await?;
        if let Some((receipt, target)) = source {
            let current:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM source_execution_tasks l JOIN mobile_v9_task_items i ON i.task_id=l.task_id JOIN mobile_v9_attempts a ON a.item_id=i.id WHERE l.receipt_id=$1 AND a.id=$2)").bind(receipt).bind(attempt).fetch_one(&mut **tx).await?;
            if !current {
                continue;
            }
            match crate::source_access::south::adapter::postgres::routing::guard_submission(
                tx, receipt, target,
            )
            .await
            {
                Ok(_) => {}
                Err(error) if error.0 == "temporarily_unavailable" => {
                    return Err(Error::Unavailable);
                }
                Err(_) => continue,
            }
        }
        allowed_attempts.push(*attempt);
    }
    let rows:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(a)||jsonb_build_object('plan',p.plan,'plan_digest',p.digest,'content_digest',p.content_digest,'task_id',t.id,'target_id',i.target_id,'confirmed_until',t.confirmed_until) FROM mobile_v9_attempts a JOIN mobile_v9_plans p ON p.id=a.plan_id JOIN mobile_v9_task_items i ON i.id=a.item_id JOIN mobile_v9_tasks t ON t.id=i.task_id WHERE NOT EXISTS(SELECT 1 FROM mobile_v9_dispatch_state ds WHERE ds.item_id=a.item_id AND ds.state IN ('waiting_resources','completed','stopped','confirmation_expired','dispatch_failed')) AND p.device_id=$1 AND a.id=ANY($2) AND a.current AND a.state='queued' AND a.not_before<=clock_timestamp() AND NOT t.stopped AND t.confirmed_until>clock_timestamp() AND t.next_eligible_at<=clock_timestamp() ORDER BY t.created_at,i.position FOR UPDATE OF a,t").bind(c.device_id).bind(&allowed_attempts).fetch_all(&mut **tx).await?;
    let mut rejected = false;
    for row in rows {
        // Completion facts are authoritative even before the next projection scan.
        let already_sent:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_v9_attempts a LEFT JOIN mobile_execution_grants g ON g.attempt_id=a.id WHERE a.item_id=$1 AND (a.state IN ('observed_sent','manually_confirmed_sent') OR g.result_body->>'outcome'='observed_sent'))").bind(uid(&row,"item_id")?).fetch_one(&mut **tx).await?;
        if already_sent {
            continue;
        }
        let aid = uid(&row, "id")?;
        let school = uid(&row, "school_id")?;
        let validation = match authorize(tx, uid(&row, "confirmed_by")?, Some(school), true).await {
            Ok(()) => tasks::validate_plan(tx, school, &row["plan"]).await,
            Err(error) => Err(error),
        };
        match validation {
            Ok(()) => {}
            Err(Error::Policy(reason)) => {
                reject_attempt(tx, aid, reason).await?;
                rejected = true;
                continue;
            }
            Err(Error::Forbidden | Error::Unauthorized | Error::NotFound | Error::Invalid(_)) => {
                reject_attempt(tx, aid, "plan_dependency_changed").await?;
                rejected = true;
                continue;
            }
            Err(error) => return Err(error),
        }
        // A frozen historical sequence is a business dependency, not a v9 executable protocol.
        let blocked:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_v9_task_items i JOIN mobile_v9_tasks t ON t.id=i.task_id JOIN mobile_v9_task_items previous ON previous.task_id=t.id AND previous.position<i.position WHERE i.id=$1 AND t.dispatch_mode='legacy_sequence' AND NOT EXISTS(SELECT 1 FROM mobile_v9_attempts a WHERE a.item_id=previous.id AND a.state IN ('observed_sent','manually_confirmed_sent')))").bind(uid(&row,"item_id")?).fetch_one(&mut **tx).await?;
        if blocked
            || !device::quota(tx, school, c.device_id, uid(&row, "target_id")?, aid, false).await?
        {
            continue;
        }
        let usages =
            match usage_budgets::check(tx, school, c.device_id, uid(&row, "target_id")?).await {
                Ok(usages) => usages,
                Err(Error::Policy("quota_wait")) => continue,
                Err(error) => return Err(error),
            };
        let id = Uuid::now_v7();
        let secret = secrets::token();
        let timing = match timing(tx, &row, school, c.device_id).await {
            Ok(timing) => timing,
            Err(Error::Policy("execution_window_too_short")) => continue,
            Err(error) => return Err(error),
        };
        if !device::quota(tx, school, c.device_id, uid(&row, "target_id")?, aid, true).await? {
            continue;
        }
        let mut envelope = json!({"id":id,"execution_id":id,"capability_id":id,"schema":"durable_execution/1","protocol_version":10,"kind":"send","state":"granted","max_calls":1,"local_budget_ms":210000,"attempt_id":aid,"task_id":row["task_id"],"target_id":row["target_id"],"school_id":school,"device_id":c.device_id,"credential_generation":c.generation,"assignment_revision":row["revision"],"runtime_epoch":v.runtime_epoch,"plan":row["plan"],"plan_digest":row["plan_digest"],"content_digest":row["content_digest"],"timing":timing,"result_credential":secret});
        let (public_key, signature) = secrets::sign(&envelope)?;
        envelope["signature"] = json!(signature);
        envelope["key_id"] = json!(hash(&public_key));
        sqlx::query("INSERT INTO mobile_execution_grants(id,attempt_id,device_id,credential_generation,kind,execute_before,result_credential_hash,envelope) VALUES($1,$2,$3,$4,'send',$5::text::timestamptz,$6,$7)").bind(id).bind(aid).bind(c.device_id).bind(c.generation).bind(timing["execute_before"].as_str().ok_or(Error::Conflict)?).bind(hash(&secret)).bind(secrets::seal(&envelope)?).execute(&mut **tx).await?;
        usage_budgets::record(tx, id, &usages).await?;
        sqlx::query("UPDATE mobile_v9_attempts SET state='leased',revision=revision+1 WHERE id=$1")
            .bind(aid)
            .execute(&mut **tx)
            .await?;
        sqlx::query("UPDATE mobile_v9_tasks SET next_eligible_at=clock_timestamp()+make_interval(secs=>greatest(1,($2::jsonb->>'between_targets_seconds')::int)) WHERE id=$1").bind(uid(&row,"task_id")?).bind(&row["plan"]).execute(&mut **tx).await?;
        return Ok(envelope);
    }
    Ok(
        json!({"id":c.device_id,"state":if rejected {"rejected"} else {"idle"},"reason":if rejected {json!("plan_dependency_changed")} else {Value::Null}}),
    )
}
fn dependency_rejection(device: Uuid) -> Value {
    json!({"id":device,"state":"rejected","reason":"plan_dependency_changed"})
}
/// Only unissued queued attempts can be ended here. Preserve every issued envelope.
async fn reject_attempt(tx: &mut Tx<'_>, attempt: Uuid, reason: &str) -> Result<()> {
    sqlx::query("UPDATE mobile_v9_attempts a SET state='invalidated',revision=revision+1,invalidation_reason=$2,termination_reason=$2,invalidated_at=clock_timestamp() WHERE a.id=$1 AND a.state='queued' AND NOT EXISTS(SELECT 1 FROM mobile_execution_grants g WHERE g.attempt_id=a.id)")
        .bind(attempt).bind(reason).execute(&mut **tx).await?;
    sqlx::query("UPDATE mobile_v9_dispatch_state ds SET next_retry_at=clock_timestamp() FROM mobile_v9_attempts a WHERE a.id=$1 AND ds.item_id=a.item_id AND ds.state NOT IN ('completed','stopped','confirmation_expired','dispatch_failed')")
        .bind(attempt).execute(&mut **tx).await?;
    tracing::warn!(%attempt, reason, stage="before_grant", comparison_policy="group_target/1",
        "unissued execution attempt invalidated; continuing eligible tasks");
    Ok(())
}
pub(super) async fn receipt(tx: &mut Tx<'_>, c: &Credential, v: ExecutionReceipt) -> Result<Value> {
    let count=sqlx::query("UPDATE mobile_execution_grants SET accepted_at=clock_timestamp(),result_deadline_at=clock_timestamp()+interval '5 minutes' WHERE id=$1 AND device_id=$2 AND credential_generation=$3 AND accepted_at IS NULL AND state='active' AND result_deadline_at>clock_timestamp()").bind(v.execution_id).bind(c.device_id).bind(c.generation).execute(&mut **tx).await?.rows_affected();
    let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_execution_grants WHERE id=$1 AND device_id=$2 AND credential_generation=$3)").bind(v.execution_id).bind(c.device_id).bind(c.generation).fetch_one(&mut **tx).await?;
    require(exists, "execution_identity_mismatch")?;
    Ok(json!({"id":v.execution_id,"accepted":count==1,"recorded":true}))
}
pub(super) async fn complete(tx: &mut Tx<'_>, v: Completion) -> Result<Value> {
    v.validate()?;
    let mut body = json!(v);
    body.as_object_mut()
        .ok_or(Error::Conflict)?
        .remove("result_credential");
    let h = digest(&body)?;
    let row:Value=sqlx::query_scalar("SELECT to_jsonb(g)-'envelope' FROM mobile_execution_grants g WHERE id=$1 AND result_credential_hash=$2 FOR UPDATE").bind(v.execution_id).bind(hash(&v.result_credential)).fetch_optional(&mut **tx).await?.ok_or(Error::Unauthorized)?;
    if !row["result_id"].is_null() {
        require(
            row["result_id"] == json!(v.result_id) && row["result_hash"] == h,
            "execution_result_conflict",
        )?;
        return Ok(row["receipt"].clone());
    }
    require(
        (row["kind"] == "send" && !v.outcome.starts_with("inspection_"))
            || (row["kind"] == "inspection"
                && v.outcome.starts_with("inspection_")
                && !v.consumed
                && !v.calling_marked),
        "execution_kind_mismatch",
    )?;
    let receipt = json!({"id":Uuid::now_v7(),"result_id":v.result_id,"execution_id":v.execution_id,"body_digest":h,"recorded":true});
    sqlx::query("UPDATE mobile_execution_grants SET result_id=$2,result_body=$3,result_hash=$4,receipt=$5,result_received_at=clock_timestamp(),session_closed=true WHERE id=$1").bind(v.execution_id).bind(v.result_id).bind(body).bind(h).bind(&receipt).execute(&mut **tx).await?;
    sqlx::query("UPDATE mobile_v9_dispatch_state ds SET next_retry_at=clock_timestamp() FROM mobile_v9_attempts a WHERE a.id=$1 AND ds.item_id=a.item_id AND ds.state NOT IN ('completed','stopped','confirmation_expired','dispatch_failed')").bind(row["attempt_id"].as_str().and_then(|v|v.parse::<Uuid>().ok())).execute(&mut **tx).await?;
    Ok(receipt)
}
/// Apply immutable result facts once. Unknown frees the physical session but retains its deadline.
pub(super) async fn apply_results(tx: &mut Tx<'_>) -> Result<()> {
    let rows:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(g)-'envelope' FROM mobile_execution_grants g WHERE result_id IS NOT NULL AND NOT applied ORDER BY result_received_at LIMIT 100 FOR UPDATE SKIP LOCKED").fetch_all(&mut **tx).await?;
    for row in rows {
        if row["kind"] == "inspection" {
            sqlx::query(
                "UPDATE mobile_execution_grants SET applied=true,state='completed' WHERE id=$1",
            )
            .bind(uid(&row, "id")?)
            .execute(&mut **tx)
            .await?;
            continue;
        }
        let aid = uid(&row, "attempt_id")?;
        let outcome = row["result_body"]["outcome"]
            .as_str()
            .ok_or(Error::Conflict)?;
        let state = match outcome {
            "observed_sent" => "observed_sent",
            "unknown" => "unknown",
            _ => "failed_before_call",
        };
        sqlx::query("UPDATE mobile_v9_attempts SET state=$2,revision=revision+1 WHERE id=$1")
            .bind(aid)
            .bind(state)
            .execute(&mut **tx)
            .await?;
        sqlx::query("UPDATE mobile_v9_quota_reservations SET state=$2 WHERE attempt_id=$1")
            .bind(aid)
            .bind(match outcome {
                "observed_sent" => "consumed",
                "unknown" => "risk_held",
                _ => "released",
            })
            .execute(&mut **tx)
            .await?;
        sqlx::query("UPDATE mobile_execution_grants SET applied=true,state=CASE WHEN $2='unknown' AND state='active' THEN 'active' ELSE 'completed' END WHERE id=$1").bind(uid(&row,"id")?).bind(outcome).execute(&mut **tx).await?;
        if outcome == "observed_sent" {
            // A retired target scope may already be terminal when its original
            // phone reports success. Record that fact without resuming dispatch.
            sqlx::query("UPDATE mobile_execution_grants g SET state='superseded' FROM mobile_v9_attempts a WHERE g.attempt_id=a.id AND a.item_id=(SELECT item_id FROM mobile_v9_attempts WHERE id=$1) AND g.id<>$2 AND g.state='active'")
                .bind(aid).bind(uid(&row,"id")?).execute(&mut **tx).await?;
            sqlx::query("UPDATE mobile_v9_dispatch_state SET state='completed',reason='result_recorded',next_retry_at=NULL,finished_at=coalesce(finished_at,clock_timestamp()),revision=revision+1,updated_at=clock_timestamp() WHERE item_id=(SELECT item_id FROM mobile_v9_attempts WHERE id=$1)")
                .bind(aid).execute(&mut **tx).await?;
        }
    }
    Ok(())
}

/// Shorten the fixed local budget at confirmation, usage and policy time boundaries.
async fn timing(tx: &mut Tx<'_>, row: &Value, school: Uuid, device: Uuid) -> Result<Value> {
    let now: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&mut **tx)
        .await?;
    let mut until = now + chrono::Duration::seconds(210);
    let confirmed: chrono::DateTime<chrono::Utc> =
        serde_json::from_value(row["confirmed_until"].clone()).map_err(|_| Error::Conflict)?;
    until = until.min(confirmed);
    for usage in tasks::operative_usages(tx, school, device, uid(row, "target_id")?).await? {
        let expiry: chrono::DateTime<chrono::Utc> =
            serde_json::from_value(usage["expires_at"].clone()).map_err(|_| Error::Conflict)?;
        until = until.min(expiry);
    }
    let policies = [
        device::frozen_policy(tx, uid(row, "id")?).await?,
        policy(tx)
            .await?
            .ok_or(Error::Policy("shared_send_policy_invalid"))?,
    ];
    for policy in policies {
        let boundary:Option<chrono::DateTime<chrono::Utc>>=sqlx::query_scalar("WITH times AS (SELECT t,extract(isodow FROM t AT TIME ZONE $1)::int AS day,(extract(hour FROM t AT TIME ZONE $1)*60+extract(minute FROM t AT TIME ZONE $1))::int AS minute FROM generate_series($3::timestamptz,$4::timestamptz,interval '1 second') t) SELECT min(t) FROM times n WHERE NOT EXISTS(SELECT 1 FROM jsonb_array_elements($2::jsonb) w WHERE CASE WHEN (w->>'start_minute')::int<(w->>'end_minute')::int THEN (w->'weekdays') @> to_jsonb(ARRAY[n.day]) AND n.minute>=(w->>'start_minute')::int AND n.minute<(w->>'end_minute')::int ELSE ((w->'weekdays') @> to_jsonb(ARRAY[n.day]) AND n.minute>=(w->>'start_minute')::int) OR ((w->'weekdays') @> to_jsonb(ARRAY[((n.day+5)%7)+1]) AND n.minute<(w->>'end_minute')::int) END)").bind(policy["timezone"].as_str().ok_or(Error::Conflict)?).bind(&policy["allowed_windows"]).bind(now).bind(until).fetch_one(&mut **tx).await?;
        // Subsecond conservative margin: never run across the first disallowed minute.
        if let Some(boundary) = boundary {
            until = until.min(boundary - chrono::Duration::seconds(1));
        }
    }
    require(
        until > now + chrono::Duration::seconds(5),
        "execution_window_too_short",
    )?;
    Ok(json!({"server_time":now,"not_before":now,"execute_before":until}))
}
