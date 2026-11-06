use super::*;
pub(super) async fn quota(
    tx: &mut Tx<'_>,
    s: Uuid,
    device: Uuid,
    target: Uuid,
    attempt: Uuid,
    reserve: bool,
) -> Result<bool> {
    let p = policy(tx)
        .await?
        .ok_or(Error::Policy("shared_send_not_released"))?;
    let frozen:Option<Value>=sqlx::query_scalar("SELECT coalesce(to_jsonb(policy),nullif(u.body->'scheme','null'::jsonb),to_jsonb(task_policy),$2::jsonb) FROM mobile_v9_attempts a JOIN mobile_v9_plans p ON p.id=a.plan_id LEFT JOIN mobile_usage_scopes m ON m.scope_id=(p.plan->>'scope_id')::uuid LEFT JOIN mobile_usages u ON u.id=m.usage_id LEFT JOIN mobile_v9_send_policy_versions policy ON policy.id=(u.body->>'scheme_id')::uuid LEFT JOIN mobile_v9_send_policy_versions task_policy ON task_policy.id=(p.plan->>'policy_id')::uuid WHERE a.id=$1").bind(attempt).bind(default_policy()).fetch_optional(&mut **tx).await?;
    if let Some(original) = frozen.filter(|p| p.is_object())
        && !quota_policy(tx, s, device, target, attempt, &original, false).await?
    {
        return Ok(false);
    }
    if !quota_policy(tx, s, device, target, attempt, &p, false).await? {
        return Ok(false);
    }
    if reserve {
        quota_policy(tx, s, device, target, attempt, &p, true).await
    } else {
        Ok(true)
    }
}
pub(super) async fn frozen_policy(tx: &mut Tx<'_>, attempt: Uuid) -> Result<Value> {
    Ok(sqlx::query_scalar("SELECT coalesce(to_jsonb(policy),nullif(u.body->'scheme','null'::jsonb),to_jsonb(task_policy),$2::jsonb) FROM mobile_v9_attempts a JOIN mobile_v9_plans p ON p.id=a.plan_id LEFT JOIN mobile_usage_scopes m ON m.scope_id=(p.plan->>'scope_id')::uuid LEFT JOIN mobile_usages u ON u.id=m.usage_id LEFT JOIN mobile_v9_send_policy_versions policy ON policy.id=(u.body->>'scheme_id')::uuid LEFT JOIN mobile_v9_send_policy_versions task_policy ON task_policy.id=(p.plan->>'policy_id')::uuid WHERE a.id=$1").bind(attempt).bind(default_policy()).fetch_one(&mut **tx).await?)
}
async fn quota_policy(
    tx: &mut Tx<'_>,
    s: Uuid,
    device: Uuid,
    target: Uuid,
    attempt: Uuid,
    p: &Value,
    reserve: bool,
) -> Result<bool> {
    let limits: Limits = serde_json::from_value(p["limits"].clone())
        .map_err(|_| Error::Policy("shared_send_policy_invalid"))?;
    limits.validate()?;
    let timezone = p["timezone"]
        .as_str()
        .ok_or(Error::Policy("shared_send_policy_invalid"))?;
    let permitted:bool=sqlx::query_scalar("WITH local AS (SELECT clock_timestamp() AT TIME ZONE $1 AS t), n AS (SELECT extract(isodow FROM t)::int AS day,(extract(hour FROM t)*60+extract(minute FROM t))::int AS minute FROM local) SELECT EXISTS(SELECT 1 FROM n,jsonb_array_elements($2::jsonb) w WHERE CASE WHEN (w->>'start_minute')::int<(w->>'end_minute')::int THEN (w->'weekdays') @> to_jsonb(ARRAY[n.day]) AND n.minute>=(w->>'start_minute')::int AND n.minute<(w->>'end_minute')::int ELSE ((w->'weekdays') @> to_jsonb(ARRAY[n.day]) AND n.minute>=(w->>'start_minute')::int) OR ((w->'weekdays') @> to_jsonb(ARRAY[((n.day+5)%7)+1]) AND n.minute<(w->>'end_minute')::int) END)").bind(timezone).bind(&p["allowed_windows"]).fetch_one(&mut **tx).await?;
    if !permitted {
        return Ok(false);
    }

    let keys = [
        ("global".to_owned(), limits.global),
        (format!("school:{s}"), limits.school),
        (format!("device:{device}"), limits.device),
        (format!("target:{s}:{target}"), limits.target),
    ];
    for (key, l) in &keys {
        let row=sqlx::query("SELECT count(*) FILTER (WHERE recorded_at>clock_timestamp()-make_interval(secs=>$2::double precision)) AS used,coalesce(bool_or(recorded_at>clock_timestamp()-make_interval(secs=>$3::double precision)),false) AS cooling FROM mobile_v9_quota_reservations WHERE dimension_key=$1 AND state<>'released' AND attempt_id<>$4 AND (recorded_at>clock_timestamp()-make_interval(secs=>greatest($2::double precision,$3::double precision)))").bind(key).bind(l.window_seconds as f64).bind(l.interval_seconds as f64).bind(attempt).fetch_one(&mut **tx).await?;
        let daily:i64=sqlx::query_scalar("SELECT count(*) FROM mobile_v9_quota_reservations WHERE dimension_key=$1 AND state<>'released' AND attempt_id<>$2 AND (recorded_at>=(date_trunc('day',clock_timestamp() AT TIME ZONE $3) AT TIME ZONE $3))").bind(key).bind(attempt).bind(timezone).fetch_one(&mut **tx).await?;
        if daily >= l.daily_max_calls
            || row.get::<i64, _>("used") >= l.max_calls
            || row.get::<bool, _>("cooling")
        {
            return Ok(false);
        }
    }
    if !reserve {
        return Ok(true);
    }
    for (key, _) in keys {
        sqlx::query("INSERT INTO mobile_v9_quota_reservations(attempt_id,dimension_key,state) VALUES($1,$2,'reserved') ON CONFLICT(attempt_id,dimension_key) DO UPDATE SET state='reserved',recorded_at=clock_timestamp() WHERE mobile_v9_quota_reservations.state='released'").bind(attempt).bind(key).execute(&mut **tx).await?;
    }
    Ok(true)
}
pub(super) async fn write(
    repo: &Repository,
    c: &Credential,
    key: &str,
    command: DeviceCommand,
) -> Result<Value> {
    let mut tx = repo.0.begin().await?;
    guard(&mut tx).await?;
    authenticate(
        &mut tx,
        c,
        !matches!(
            &command,
            DeviceCommand::Runtime(_) | DeviceCommand::Consent(_) | DeviceCommand::Diagnostic(_)
        ),
    )
    .await?;
    let (scope, body) = match &command {
        DeviceCommand::Deliver(v) => ("task-delivery", json!(v)),
        DeviceCommand::Grant(v) => ("execution-grant", json!(v)),
        DeviceCommand::Receipt(v) => ("execution-receipt", json!(v)),
        DeviceCommand::Runtime(v) => ("runtime", json!(v)),
        DeviceCommand::Consent(v) => ("delegation-decision", json!(v)),
        DeviceCommand::Diagnostic(v) => ("diagnostic", json!(v)),
    };
    if let Some(mut old) = replay(&mut tx, c.device_id, c.generation, scope, key, &body).await? {
        if let Some(id) = old["grant_ref"].as_str() {
            let encrypted: Vec<u8> =
                sqlx::query_scalar("SELECT envelope FROM mobile_execution_grants WHERE id=$1")
                    .bind(id.parse::<Uuid>().map_err(|_| Error::Conflict)?)
                    .fetch_one(&mut *tx)
                    .await?;
            old = secrets::open(&encrypted)?;
        }
        if matches!(scope, "execution-grant" | "task-delivery") && old["state"] == "granted" {
            let active:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_execution_grants g LEFT JOIN mobile_v9_attempts a ON a.id=g.attempt_id LEFT JOIN mobile_v9_task_items i ON i.id=a.item_id LEFT JOIN mobile_v9_tasks t ON t.id=i.task_id WHERE g.id=$1 AND g.state='active' AND g.execute_before>clock_timestamp() AND (g.kind='inspection' OR (a.current AND NOT t.stopped)))").bind(uid(&old,"id")?).fetch_one(&mut *tx).await?;
            if !active {
                return command_response(scope, json!({"id":old["id"],"state":"superseded"}));
            }
        }
        return command_response(scope, old);
    }
    let result = match command {
        DeviceCommand::Deliver(v) => execution::deliver(&mut tx, c, v).await?,
        DeviceCommand::Grant(v) => execution::grant(&mut tx, c, v).await?,
        DeviceCommand::Receipt(v) => execution::receipt(&mut tx, c, v).await?,
        DeviceCommand::Consent(v) => delegation::decide(&mut tx, c, v).await?,
        DeviceCommand::Diagnostic(v) => diagnostics::diagnostic(&mut tx, c, v).await?,
        DeviceCommand::Runtime(v) => {
            if let Some(reason) = &v.ready_reason {
                require(
                    !v.ui_ready
                        && !reason.is_empty()
                        && reason.len() <= 100
                        && reason
                            .bytes()
                            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'),
                    "send_protocol_unsupported",
                )?;
            }
            if let Some(preparation) = &v.preparation {
                preparation.validate()?;
            }
            require(
                v.schema == SCHEMA && !v.runtime_epoch.is_nil(),
                "send_protocol_unsupported",
            )?;
            sqlx::query("INSERT INTO mobile_shared_runtime(device_id,credential_generation,runtime_epoch,ui_ready,paused,quiescent) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT(device_id) DO UPDATE SET credential_generation=$2,runtime_epoch=$3,ui_ready=$4,paused=$5,quiescent=$6,observed_at=clock_timestamp()").bind(c.device_id).bind(c.generation).bind(v.runtime_epoch).bind(v.ui_ready).bind(v.paused).bind(v.quiescent).execute(&mut *tx).await?;
            require(
                is_digest(&v.apk_sha256)
                    && !v.ui_context_epoch.is_empty()
                    && v.ui_context_epoch.len() <= 256,
                "send_protocol_unsupported",
            )?;
            sqlx::query("INSERT INTO mobile_v9_runtime_details(device_id,credential_generation,ui_context_epoch,conditions,apk_sha256,preparation,ready_reason) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT(device_id) DO UPDATE SET credential_generation=$2,ui_context_epoch=$3,conditions=$4,apk_sha256=$5,preparation=$6,ready_reason=$7,observed_at=clock_timestamp()").bind(c.device_id).bind(c.generation).bind(&v.ui_context_epoch).bind(json!(v.conditions)).bind(&v.apk_sha256).bind(json!(v.preparation)).bind(&v.ready_reason).execute(&mut *tx).await?;
            json!({"id":c.device_id,"runtime_epoch":v.runtime_epoch,"recorded":true})
        }
    };
    if matches!(scope, "execution-grant" | "task-delivery") {
        tracing::info!(device=%c.device_id, idempotency_key_digest=%hash(key),
            state=%result["state"], stage="grant_decision", protocol=10,
            "execution grant decision");
    }
    // Grant responses contain secrets and must not enter the plaintext idempotency table.
    let stored =
        if matches!(scope, "execution-grant" | "task-delivery") && result["state"] == "granted" {
            json!({"grant_ref":result["id"]})
        } else {
            result.clone()
        };
    remember(
        &mut tx,
        c.device_id,
        c.generation,
        scope,
        key,
        &body,
        &stored,
    )
    .await?;
    tx.commit().await?;
    command_response(scope, result)
}

fn command_response(scope: &str, value: Value) -> Result<Value> {
    if scope == "task-delivery" {
        execution::delivery_response(value)
    } else {
        grant_response(value)
    }
}

// Rejections are committed together with attempt state and idempotency before returning 409.
fn grant_response(value: Value) -> Result<Value> {
    if value["state"] == "rejected" && value["reason"] == "plan_dependency_changed" {
        Err(Error::Policy("plan_dependency_changed"))
    } else {
        Ok(value)
    }
}
