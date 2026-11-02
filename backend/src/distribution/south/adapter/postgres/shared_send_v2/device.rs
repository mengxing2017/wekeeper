use super::*;
pub(super) async fn read(repo: &Repository, c: &Credential) -> Result<Value> {
    let mut tx = repo.0.begin().await?;
    guard(&mut tx).await?;
    authenticate(&mut tx, c, false).await?;
    usage::reconcile(&mut tx).await?;
    let delegation = delegation::context(&mut tx, c).await?;
    inspections::reconcile(&mut tx, Some(c.device_id), None).await?;
    let work:Vec<Value>=sqlx::query_scalar("SELECT (to_jsonb(c)-'recovery_credential_hash'-'encrypted_response')||jsonb_build_object('attempt_state',a.state,'invocation_id',i.id,'execute_before',i.execute_before) FROM mobile_v9_send_claims c JOIN mobile_v9_attempts a ON a.id=c.attempt_id LEFT JOIN mobile_v9_invocations i ON i.claim_id=c.id WHERE c.device_id=$1 AND (c.state='active' OR a.state='unknown') ORDER BY c.created_at LIMIT 100").bind(c.device_id).fetch_all(&mut *tx).await?;
    let inspections:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('id',i.id,'target_id',i.target_id,'target_name',coalesce(nullif(t.alias,''),t.search_text),'state',i.state,'expires_at',i.expires_at) FROM mobile_v9_send_inspections i JOIN mobile_targets t ON t.id=i.target_id AND t.school_id=i.school_id WHERE i.device_id=$1 AND i.delegation_id=$2 AND i.purpose='automatic' AND i.state='pending' AND i.expires_at>clock_timestamp() AND NOT EXISTS(SELECT 1 FROM mobile_v9_plans p JOIN mobile_v9_attempts a ON a.plan_id=p.id JOIN mobile_v9_task_items ti ON ti.id=a.item_id JOIN mobile_v9_tasks task ON task.id=ti.task_id WHERE p.plan->>'binding_id'=i.binding_id::text AND a.current AND NOT task.stopped AND task.confirmed_until>clock_timestamp() AND (ti.position<>task.cursor_position OR task.next_eligible_at>clock_timestamp())) ORDER BY i.created_at,i.id").bind(c.device_id).bind(uid(&delegation,"id")?).fetch_all(&mut *tx).await?;

    let turns:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('id',a.id,'task_id',t.id,'position',i.position,'next_eligible_at',t.next_eligible_at,'interval_ms',CASE WHEN i.position=0 THEN 0 ELSE greatest((p.plan->>'between_targets_seconds')::bigint*1000,(p.plan->'flow'->'pacing'->>'between_targets_min_ms')::bigint,coalesce((SELECT max((lim.value->>'interval_seconds')::bigint)*1000 FROM mobile_v9_send_policy_versions policy,jsonb_each(policy.limits) lim WHERE policy.revision=(SELECT max(revision) FROM mobile_v9_send_policy_versions)),1000)) END) FROM mobile_v9_tasks t JOIN mobile_v9_task_items i ON i.task_id=t.id AND i.position=t.cursor_position JOIN mobile_v9_attempts a ON a.item_id=i.id AND a.current JOIN mobile_v9_plans p ON p.id=a.plan_id WHERE p.device_id=$1 AND a.state='queued' AND NOT t.stopped AND t.confirmed_until>clock_timestamp() ORDER BY t.created_at,t.id LIMIT 100").bind(c.device_id).fetch_all(&mut *tx).await?;
    let v = json!({"turns":turns,"inspections":inspections,"id":c.device_id,"schema":SCHEMA,"authorization_schema":SCOPE,"protocol_version":9,"single_use_supported":true,"result_recovery_supported":true,"delegation":delegation,"recovery":work});
    tx.commit().await?;
    Ok(v)
}
async fn live_claim(tx: &mut Tx<'_>, c: &Credential, id: Uuid) -> Result<Value> {
    let v:Value=sqlx::query_scalar("SELECT to_jsonb(c)||jsonb_build_object('plan',p.plan,'plan_digest',p.digest,'content_digest',p.content_digest,'task_id',i.task_id) FROM mobile_v9_send_claims c JOIN mobile_v9_attempts a ON a.id=c.attempt_id JOIN mobile_v9_plans p ON p.id=a.plan_id JOIN mobile_v9_task_items i ON i.id=a.item_id JOIN mobile_v9_tasks t ON t.id=i.task_id JOIN mobile_shared_runtime r ON r.device_id=c.device_id JOIN mobile_shared_device_leases l ON l.device_id=c.device_id WHERE c.id=$1 AND c.device_id=$2 AND c.credential_generation=$3 AND c.state='active' AND c.expires_at>clock_timestamp() AND NOT t.stopped AND t.confirmed_until>clock_timestamp() AND t.cursor_position=i.position AND t.next_eligible_at<=clock_timestamp() AND r.runtime_epoch=c.runtime_epoch AND r.ui_ready AND NOT r.paused AND r.observed_at>clock_timestamp()-interval '30 seconds' AND l.holder_id=c.id AND l.fence=c.lease_fence AND l.expires_at>clock_timestamp() AND EXISTS(SELECT 1 FROM device_control_presence cp WHERE cp.id=c.device_id AND cp.state='online' AND NOT cp.desired_paused AND cp.last_control_heartbeat_at>clock_timestamp()-interval '30 seconds')").bind(id).bind(c.device_id).bind(c.generation).fetch_optional(&mut **tx).await?.ok_or(Error::Policy("lease_superseded"))?;
    let school = uid(&v, "school_id")?;
    let confirmed: Uuid =
        sqlx::query_scalar("SELECT confirmed_by FROM mobile_v9_attempts WHERE id=$1")
            .bind(uid(&v, "attempt_id")?)
            .fetch_one(&mut **tx)
            .await?;
    authorize(tx, confirmed, Some(school), true).await?;
    let delegation = delegation::context(tx, c).await?;
    require(
        delegation["decision"]["accepted"] == true,
        "shared_send_consent_required",
    )?;
    tasks::validate_plan(tx, school, &v["plan"]).await?;
    business_scheme::ensure_open(tx).await?;
    tasks::validate_execution_rules(tx, &v["plan"]).await?;
    require(
        !tasks::missing_rules(&v["plan"]),
        "shared_send_adapter_unavailable",
    )?;
    let checked:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_v9_send_inspections i JOIN mobile_v9_runtime_details r ON r.device_id=i.device_id WHERE i.device_id=$1 AND i.binding_id=$2 AND i.dependencies=$3 AND i.state='passed' AND i.purpose='automatic' AND i.expires_at>clock_timestamp() AND i.result->>'ui_context_epoch'=r.ui_context_epoch)").bind(c.device_id).bind(uid(&v["plan"],"binding_id")?).bind(tasks::dependencies(&v["plan"])).fetch_one(&mut **tx).await?;
    require(checked, "shared_send_inspection_required")?;
    Ok(v)
}
async fn quota(
    tx: &mut Tx<'_>,
    s: Uuid,
    device: Uuid,
    target: Uuid,
    attempt: Uuid,
) -> Result<bool> {
    let p = policy(tx)
        .await?
        .ok_or(Error::Policy("shared_send_not_released"))?;
    let frozen:Option<Value>=sqlx::query_scalar("SELECT coalesce(to_jsonb(policy),nullif(u.body->'scheme','null'::jsonb),to_jsonb(task_policy),$2::jsonb) FROM mobile_v9_attempts a JOIN mobile_v9_plans p ON p.id=a.plan_id LEFT JOIN mobile_usage_scopes m ON m.scope_id=(p.plan->>'scope_id')::uuid LEFT JOIN mobile_usages u ON u.id=m.usage_id LEFT JOIN mobile_v9_send_policy_versions policy ON policy.id=(u.body->>'scheme_id')::uuid LEFT JOIN mobile_v9_send_policy_versions task_policy ON task_policy.id=(p.plan->>'policy_id')::uuid WHERE a.id=$1").bind(attempt).bind(default_policy()).fetch_optional(&mut **tx).await?;
    if let Some(original) = frozen.filter(|p| p.is_object())
        && !quota_policy(tx, s, device, target, attempt, &original).await?
    {
        return Ok(false);
    }
    quota_policy(tx, s, device, target, attempt, &p).await
}
async fn quota_policy(
    tx: &mut Tx<'_>,
    s: Uuid,
    device: Uuid,
    target: Uuid,
    attempt: Uuid,
    p: &Value,
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
        let row=sqlx::query("SELECT count(*) FILTER (WHERE state='risk_held' OR recorded_at>clock_timestamp()-make_interval(secs=>$2::double precision)) AS used,coalesce(bool_or(recorded_at>clock_timestamp()-make_interval(secs=>$3::double precision)),false) AS cooling FROM mobile_v9_quota_reservations WHERE dimension_key=$1 AND state<>'released' AND attempt_id<>$4 AND (state='risk_held' OR recorded_at>clock_timestamp()-make_interval(secs=>greatest($2::double precision,$3::double precision)))").bind(key).bind(l.window_seconds as f64).bind(l.interval_seconds as f64).bind(attempt).fetch_one(&mut **tx).await?;
        let daily:i64=sqlx::query_scalar("SELECT count(*) FROM mobile_v9_quota_reservations WHERE dimension_key=$1 AND state<>'released' AND attempt_id<>$2 AND (state='risk_held' OR recorded_at>=(date_trunc('day',clock_timestamp() AT TIME ZONE $3) AT TIME ZONE $3))").bind(key).bind(attempt).bind(timezone).fetch_one(&mut **tx).await?;
        if daily >= l.daily_max_calls
            || row.get::<i64, _>("used") >= l.max_calls
            || row.get::<bool, _>("cooling")
        {
            return Ok(false);
        }
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
            DeviceCommand::Result(_) | DeviceCommand::Runtime(_)
        ),
    )
    .await?;
    expire(&mut tx).await?;
    let (scope, body) = match &command {
        DeviceCommand::Runtime(v) => ("runtime".into(), json!(v)),
        DeviceCommand::Settle(v) => ("settle".into(), json!(v)),
        DeviceCommand::Diagnostic(v) => ("diagnostic".into(), json!(v)),
        DeviceCommand::Claim(v) => ("claim".into(), json!(v)),
        DeviceCommand::Prepare(id, v) => (format!("prepare:{id}"), json!(v)),
        DeviceCommand::Invoke(id, v) => (format!("invoke:{id}"), json!(v)),
        DeviceCommand::Result(v) => ("result".into(), json!(v)),
        DeviceCommand::Consent(v) => ("delegation-decision".into(), json!(v)),
        DeviceCommand::InspectionClaim(v) => ("inspection-claim".into(), json!(v)),
        DeviceCommand::InspectionResult(v) => ("inspection-result".into(), json!(v)),
        DeviceCommand::Renew(id, v) => (format!("renew:{id}"), json!(v)),
        DeviceCommand::Release(id, v) => (format!("release:{id}"), json!(v)),
    };
    if let Some(v) = replay(&mut tx, c.device_id, c.generation, &scope, key, &body).await? {
        if let Some(id) = v["claim_ref"].as_str() {
            let encrypted: Vec<u8> = sqlx::query_scalar(
                "SELECT encrypted_response FROM mobile_v9_send_claims WHERE id=$1",
            )
            .bind(id.parse::<Uuid>().map_err(|_| Error::Conflict)?)
            .fetch_one(&mut *tx)
            .await?;
            return secrets::open(&encrypted);
        }
        if let Some(id) = v["invocation_ref"].as_str() {
            let encrypted: Vec<u8> = sqlx::query_scalar(
                "SELECT encrypted_response FROM mobile_v9_invocations WHERE id=$1",
            )
            .bind(id.parse::<Uuid>().map_err(|_| Error::Conflict)?)
            .fetch_one(&mut *tx)
            .await?;
            return secrets::open(&encrypted);
        }
        return Ok(v);
    }
    let result = match command {
        DeviceCommand::Settle(v) => settlement::settle(&mut tx, c, v).await?,
        DeviceCommand::Diagnostic(v) => settlement::diagnostic(&mut tx, c, v).await?,
        DeviceCommand::InspectionClaim(v) => inspections::claim(&mut tx, c, v).await?,
        DeviceCommand::InspectionResult(v) => inspections::finish(&mut tx, c, v).await?,
        DeviceCommand::Renew(id, v) => mutate_lease(&mut tx, c, id, v, false).await?,
        DeviceCommand::Release(id, v) => mutate_lease(&mut tx, c, id, v, true).await?,
        DeviceCommand::Runtime(v) => {
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
            sqlx::query("INSERT INTO mobile_v9_runtime_details(device_id,credential_generation,ui_context_epoch,conditions,apk_sha256) VALUES($1,$2,$3,$4,$5) ON CONFLICT(device_id) DO UPDATE SET credential_generation=$2,ui_context_epoch=$3,conditions=$4,apk_sha256=$5,observed_at=clock_timestamp()").bind(c.device_id).bind(c.generation).bind(&v.ui_context_epoch).bind(json!(v.conditions)).bind(&v.apk_sha256).execute(&mut *tx).await?;
            // A vanished runtime never resumes an old UI session. Issued calls stay at risk.
            sqlx::query("UPDATE mobile_v9_attempts a SET state='unknown',revision=revision+1 FROM mobile_v9_invocations i JOIN mobile_v9_send_claims cl ON cl.id=i.claim_id WHERE a.id=i.attempt_id AND i.device_id=$1 AND i.state='risk_held' AND a.state='invocation_issued' AND (cl.runtime_epoch<>$2 OR (i.execute_before<clock_timestamp() AND $3))").bind(c.device_id).bind(v.runtime_epoch).bind(v.quiescent).execute(&mut *tx).await?;
            if v.quiescent {
                settlement::advance_reviewed(&mut tx, c.device_id).await?;
                sqlx::query("UPDATE mobile_shared_device_leases l SET expires_at=clock_timestamp() FROM mobile_v9_send_inspections i WHERE l.device_id=$1 AND l.purpose='field_check' AND l.holder_id=i.id AND l.fence=i.lease_fence AND i.runtime_epoch=$2 AND i.state IN ('passed','failed','stale')").bind(c.device_id).bind(v.runtime_epoch).execute(&mut *tx).await?;
                let expired:Vec<Uuid>=sqlx::query_scalar("UPDATE mobile_v9_send_claims c SET state='closed' WHERE c.device_id=$1 AND c.state='active' AND c.expires_at<clock_timestamp() AND NOT EXISTS(SELECT 1 FROM mobile_v9_invocations i WHERE i.claim_id=c.id AND i.state='risk_held') RETURNING attempt_id").bind(c.device_id).fetch_all(&mut *tx).await?;
                for id in expired {
                    sqlx::query("UPDATE mobile_v9_attempts SET state='failed_before_call',revision=revision+1 WHERE id=$1 AND state IN ('leased','prepared')").bind(id).execute(&mut *tx).await?;
                    sqlx::query("UPDATE mobile_v9_quota_reservations SET state='released' WHERE attempt_id=$1 AND state='reserved'").bind(id).execute(&mut *tx).await?;
                }
            }
            json!({"id":c.device_id,"runtime_epoch":v.runtime_epoch,"recorded":true})
        }
        DeviceCommand::Consent(v) => delegation::decide(&mut tx, c, v).await?,
        DeviceCommand::Claim(v) => claim(&mut tx, c, v).await?,
        DeviceCommand::Prepare(id, v) => {
            let cl = live_claim(&mut tx, c, id).await?;
            require(
                cl["attempt_id"] == json!(v.attempt_id)
                    && cl["lease_fence"] == v.lease_fence
                    && cl["plan_digest"] == v.plan_digest
                    && cl["content_digest"] == v.content_digest
                    && cl["plan"]["flow_digest"] == v.flow_digest
                    && v.input_matches
                    && is_digest(&v.page_digest),
                "shared_send_preparation_invalid",
            )?;
            let pid = Uuid::now_v7();
            sqlx::query("INSERT INTO mobile_v9_preparations(id,school_id,claim_id,body) VALUES($1,$2,$3,$4)").bind(pid).bind(uid(&cl,"school_id")?).bind(id).bind(json!(v)).execute(&mut *tx).await?;
            let n=sqlx::query("UPDATE mobile_v9_attempts SET state='prepared',revision=revision+1 WHERE id=$1 AND state IN ('leased','prepared')").bind(v.attempt_id).execute(&mut *tx).await?.rows_affected();
            require(n == 1, "revision_conflict")?;
            json!({"id":pid,"claim_id":id,"prepared":true})
        }
        DeviceCommand::Invoke(id, v) => {
            let cl = live_claim(&mut tx, c, id).await?;
            require(
                cl["lease_fence"] == v.lease_fence
                    && cl["plan_digest"] == v.plan_digest
                    && cl["content_digest"] == v.content_digest,
                "shared_send_preparation_invalid",
            )?;
            let old: Option<Vec<u8>> = sqlx::query_scalar(
                "SELECT encrypted_response FROM mobile_v9_invocations WHERE claim_id=$1",
            )
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
            if let Some(p) = old {
                return secrets::open(&p);
            }
            let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_v9_preparations WHERE id=$1 AND claim_id=$2 AND created_at>clock_timestamp()-interval '5 seconds')").bind(v.preparation_id).bind(id).fetch_one(&mut *tx).await?;
            require(exists, "shared_send_preparation_invalid")?;
            require(
                quota(
                    &mut tx,
                    uid(&cl, "school_id")?,
                    c.device_id,
                    uid(&cl["plan"], "target_id")?,
                    uid(&cl, "attempt_id")?,
                )
                .await?,
                "quota_wait",
            )?;
            // Binding-authorized tasks have no synthetic stage grant. Existing
            // usage budgets still apply whenever this binding has a managed scope.
            if let Some(scope) = tasks::grant(
                &mut tx,
                uid(&cl, "school_id")?,
                &cl["plan"],
                cl["plan"]["stage"].as_str().ok_or(Error::Conflict)?,
            )
            .await?
            {
                let scope_id = uid(&scope, "id")?;
                let budget:bool=sqlx::query_scalar("SELECT (SELECT count(*) FROM mobile_v9_invocations i JOIN mobile_v9_attempts a ON a.id=i.attempt_id JOIN mobile_v9_plans p ON p.id=a.plan_id WHERE p.plan->>'scope_id'=$1::uuid::text )<g.max_calls FROM mobile_v9_scope_grants g WHERE g.id=$1 AND g.expires_at>clock_timestamp()+interval '15 seconds'").bind(scope_id).fetch_optional(&mut *tx).await?.unwrap_or(false);
                require(
                    budget && usage_budget(&mut tx, scope_id).await?,
                    "quota_wait",
                )?;
            }
            let inv = Uuid::now_v7();
            let timing:Value=sqlx::query_scalar("SELECT jsonb_build_object('server_time',clock_timestamp(),'not_before',clock_timestamp(),'execute_before',clock_timestamp()+interval '15 seconds') FROM mobile_v9_send_claims WHERE id=$1 AND expires_at>clock_timestamp()+interval '27 seconds' AND EXISTS(SELECT 1 FROM mobile_v9_tasks t JOIN mobile_v9_task_items i ON i.task_id=t.id JOIN mobile_v9_attempts a ON a.item_id=i.id WHERE a.id=mobile_v9_send_claims.attempt_id AND t.confirmed_until>clock_timestamp()+interval '15 seconds')").bind(id).fetch_optional(&mut *tx).await?.ok_or(Error::Policy("lease_expired"))?;
            let secret = secrets::token();
            let result_credential = secrets::token();
            let permit = json!({"id":inv,"schema":SCHEMA,"invocation_secret":secret,"result_credential":result_credential,"invocation_id":inv,"claim_id":id,"attempt_id":cl["attempt_id"],"device_id":c.device_id,"credential_generation":c.generation,"runtime_epoch":cl["runtime_epoch"],"lease_fence":v.lease_fence,"plan_digest":v.plan_digest,"content_digest":v.content_digest,"timing":timing});
            sqlx::query("INSERT INTO mobile_v9_invocations(id,school_id,attempt_id,claim_id,device_id,credential_generation,lease_fence,execute_before,state,response,encrypted_response,invocation_secret_hash,result_credential_hash) VALUES($1,$2,$3,$4,$5,$6,$7,$8,'risk_held',$9,$10,$11,$12)").bind(inv).bind(uid(&cl,"school_id")?).bind(uid(&cl,"attempt_id")?).bind(id).bind(c.device_id).bind(c.generation).bind(v.lease_fence).bind(timing["execute_before"].as_str().unwrap_or("").parse::<chrono::DateTime<chrono::Utc>>().map_err(|_|Error::Conflict)?).bind(json!({"id":inv,"claim_id":id})).bind(secrets::seal(&permit)?).bind(hash(&secret)).bind(hash(&result_credential)).execute(&mut *tx).await?;
            sqlx::query("INSERT INTO mobile_v9_risk_guards SELECT i.school_id,i.intent_id,i.target_id,a.id,$2,'active' FROM mobile_v9_attempts a JOIN mobile_v9_task_items i ON i.id=a.item_id WHERE a.id=$1 ON CONFLICT(school_id,intent_id,target_id) DO UPDATE SET attempt_id=excluded.attempt_id,invocation_id=excluded.invocation_id,state='active' WHERE mobile_v9_risk_guards.state='closed'").bind(uid(&cl,"attempt_id")?).bind(inv).execute(&mut *tx).await?;
            sqlx::query("UPDATE mobile_v9_attempts SET state='invocation_issued',revision=revision+1 WHERE id=$1").bind(uid(&cl,"attempt_id")?).execute(&mut *tx).await?;
            sqlx::query("UPDATE mobile_v9_quota_reservations SET state='risk_held',recorded_at=clock_timestamp() WHERE attempt_id=$1").bind(uid(&cl,"attempt_id")?).execute(&mut *tx).await?;
            permit
        }
        DeviceCommand::Result(v) => result(&mut tx, c, v).await?,
    };
    let stored = if result.get("recovery_credential").is_some() {
        json!({"claim_ref":result["id"]})
    } else if result.get("invocation_secret").is_some() {
        json!({"invocation_ref":result["id"]})
    } else {
        result.clone()
    };
    remember(
        &mut tx,
        c.device_id,
        c.generation,
        &scope,
        key,
        &body,
        &stored,
    )
    .await?;
    tx.commit().await?;
    Ok(result)
}
async fn claim(tx: &mut Tx<'_>, c: &Credential, v: ClaimRequest) -> Result<Value> {
    require(
        v.eligible_attempt_ids.len() <= 100,
        "shared_send_selection_invalid",
    )?;
    if business_scheme::ensure_open(tx).await.is_err() {
        return Ok(
            json!({"id":c.device_id,"state":"waiting","reason":"shared_send_platform_closed","retry_after":20}),
        );
    }
    let fresh:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_shared_runtime WHERE device_id=$1 AND credential_generation=$2 AND runtime_epoch=$3 AND ui_ready AND quiescent AND NOT paused AND observed_at>clock_timestamp()-interval '30 seconds') AND EXISTS(SELECT 1 FROM device_control_presence WHERE id=$1 AND state='online' AND NOT desired_paused AND last_control_heartbeat_at>clock_timestamp()-interval '30 seconds')").bind(c.device_id).bind(c.generation).bind(v.runtime_epoch).fetch_one(&mut **tx).await?;
    require(fresh, "shared_send_runtime_required")?;
    let busy = inspections::busy(tx, c.device_id).await?;
    if busy {
        return Ok(
            json!({"id":c.device_id,"state":"waiting","reason":"device_busy","retry_after":20}),
        );
    }
    let rows:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(a)||jsonb_build_object('plan',p.plan,'plan_digest',p.digest,'content_digest',p.content_digest,'confirmed_by',a.confirmed_by) FROM mobile_v9_attempts a JOIN mobile_v9_plans p ON p.id=a.plan_id JOIN mobile_v9_task_items i ON i.id=a.item_id JOIN mobile_v9_tasks t ON t.id=i.task_id LEFT JOIN mobile_v9_scheduler_turns turn ON turn.device_id=p.device_id AND turn.school_id=a.school_id WHERE p.device_id=$1 AND a.id=ANY($2) AND a.current AND a.state='queued' AND a.not_before<=clock_timestamp() AND NOT t.stopped AND t.confirmed_until>clock_timestamp() AND i.position=t.cursor_position AND t.next_eligible_at<=clock_timestamp() AND (i.position<>0 OR NOT EXISTS(SELECT 1 FROM mobile_v9_task_items other JOIN mobile_v9_attempts oa ON oa.item_id=other.id AND oa.current JOIN mobile_v9_plans op ON op.id=oa.plan_id LEFT JOIN mobile_shared_runtime rt ON rt.device_id=op.device_id LEFT JOIN mobile_shared_device_leases l ON l.device_id=op.device_id WHERE other.task_id=t.id AND ((l.expires_at>clock_timestamp()) OR NOT coalesce(rt.quiescent,true)))) ORDER BY coalesce(turn.sequence,0),t.created_at,i.position,a.id").bind(c.device_id).bind(&v.eligible_attempt_ids).fetch_all(&mut **tx).await?;
    for row in rows {
        let s = uid(&row, "school_id")?;
        let plan = &row["plan"];
        if authorize(tx, uid(&row, "confirmed_by")?, Some(s), true)
            .await
            .is_err()
            || tasks::validate_plan(tx, s, plan).await.is_err()
        {
            sqlx::query("UPDATE mobile_v9_attempts SET state='invalidated',revision=revision+1 WHERE id=$1 AND state='queued'").bind(uid(&row,"id")?).execute(&mut **tx).await?;
            continue;
        }
        let valid_inspection:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_v9_send_inspections i JOIN mobile_v9_runtime_details r ON r.device_id=i.device_id WHERE i.device_id=$1 AND i.binding_id=$2 AND i.dependencies=$3 AND i.state='passed' AND i.purpose='automatic' AND i.expires_at>clock_timestamp() AND i.result->>'ui_context_epoch'=r.ui_context_epoch)").bind(c.device_id).bind(uid(plan,"binding_id")?).bind(tasks::dependencies(plan)).fetch_one(&mut **tx).await?;
        let accepted = delegation::context(tx, c).await?;
        if (!valid_inspection && !tasks::missing_rules(plan))
            || accepted["decision"]["accepted"] != true
        {
            continue;
        }
        let target = uid(plan, "target_id")?;
        let risk:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_v9_risk_guards WHERE school_id=$1 AND target_id=$2 AND state='active')").bind(s).bind(target).fetch_one(&mut **tx).await?;
        if risk {
            continue;
        }
        let aid = uid(&row, "id")?;
        if !quota(tx, s, c.device_id, target, aid).await? {
            continue;
        }
        let id = Uuid::now_v7();
        let recovery_credential = secrets::token();
        let fence:i64=sqlx::query_scalar("INSERT INTO mobile_shared_device_leases(device_id,fence,holder_id,purpose,expires_at) VALUES($1,1,$2,'send',clock_timestamp()+interval '60 seconds') ON CONFLICT(device_id) DO UPDATE SET fence=mobile_shared_device_leases.fence+1,holder_id=$2,purpose='send',expires_at=clock_timestamp()+interval '60 seconds' RETURNING fence").bind(c.device_id).bind(id).fetch_one(&mut **tx).await?;
        sqlx::query("INSERT INTO mobile_v9_send_claims(id,school_id,attempt_id,device_id,credential_generation,runtime_epoch,lease_fence,state,expires_at,recovery_credential_hash) VALUES($1,$2,$3,$4,$5,$6,$7,'active',clock_timestamp()+interval '60 seconds',$8)").bind(id).bind(s).bind(aid).bind(c.device_id).bind(c.generation).bind(v.runtime_epoch).bind(fence).bind(hash(&recovery_credential)).execute(&mut **tx).await?;
        sqlx::query("UPDATE mobile_v9_attempts SET state='leased',revision=revision+1 WHERE id=$1")
            .bind(aid)
            .execute(&mut **tx)
            .await?;
        sqlx::query("DELETE FROM mobile_v9_scheduler_turns WHERE device_id=$1 AND school_id=$2")
            .bind(c.device_id)
            .bind(s)
            .execute(&mut **tx)
            .await?;
        sqlx::query("INSERT INTO mobile_v9_scheduler_turns(device_id,school_id) VALUES($1,$2)")
            .bind(c.device_id)
            .bind(s)
            .execute(&mut **tx)
            .await?;
        let timing:Value=sqlx::query_scalar("SELECT jsonb_build_object('server_time',clock_timestamp(),'expires_at',expires_at) FROM mobile_v9_send_claims WHERE id=$1").bind(id).fetch_one(&mut **tx).await?;
        let response = json!({"id":id,"recovery_credential":recovery_credential,"schema":SCHEMA,"state":"leased","attempt_id":aid,"device_id":c.device_id,"credential_generation":c.generation,"runtime_epoch":v.runtime_epoch,"lease_fence":fence,"plan":plan,"plan_digest":row["plan_digest"],"content_digest":row["content_digest"],"timing":timing});
        sqlx::query("UPDATE mobile_v9_send_claims SET encrypted_response=$2 WHERE id=$1")
            .bind(id)
            .bind(secrets::seal(&response)?)
            .execute(&mut **tx)
            .await?;
        return Ok(response);
    }
    Ok(json!({"id":c.device_id,"state":"idle","retry_after":20}))
}
pub(super) async fn result(tx: &mut Tx<'_>, c: &Credential, v: ResultEvent) -> Result<Value> {
    v.validate()?;
    let h = digest(&json!(v))?;
    let old: Option<(String, Value)> = sqlx::query_as(
        "SELECT body_hash,receipt FROM mobile_v9_send_events WHERE id=$1 AND device_id=$2",
    )
    .bind(v.event_id)
    .bind(c.device_id)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some((old, r)) = old {
        require(old == h, "result_identity_mismatch")?;
        return Ok(r);
    }
    let cl:Value=sqlx::query_scalar("SELECT to_jsonb(c)||jsonb_build_object('plan_digest',p.digest,'content_digest',p.content_digest,'attempt_state',a.state) FROM mobile_v9_send_claims c JOIN mobile_v9_attempts a ON a.id=c.attempt_id JOIN mobile_v9_plans p ON p.id=a.plan_id WHERE c.id=$1 AND c.device_id=$2").bind(v.claim_id).bind(c.device_id).fetch_optional(&mut **tx).await?.ok_or(Error::Forbidden)?;
    require(
        cl["attempt_id"] == json!(v.attempt_id)
            && cl["credential_generation"] == v.credential_generation
            && cl["lease_fence"] == v.lease_fence
            && cl["plan_digest"] == v.plan_digest
            && cl["content_digest"] == v.content_digest,
        "result_identity_mismatch",
    )?;
    let invocation: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM mobile_v9_invocations WHERE claim_id=$1")
            .bind(v.claim_id)
            .fetch_optional(&mut **tx)
            .await?;
    require(invocation == v.invocation_id, "result_identity_mismatch")?;
    if let Some(id) = invocation {
        let expected: String = sqlx::query_scalar(
            "SELECT result_credential_hash FROM mobile_v9_invocations WHERE id=$1",
        )
        .bind(id)
        .fetch_one(&mut **tx)
        .await?;
        require(
            v.result_credential
                .as_deref()
                .is_some_and(|s| s.len() == 64 && hash(s) == expected),
            "result_identity_mismatch",
        )?;
    } else {
        require(v.result_credential.is_none(), "result_identity_mismatch")?;
    }

    let late = matches!(
        cl["attempt_state"].as_str(),
        Some(
            "verified_not_sent"
                | "manually_confirmed_sent"
                | "observed_sent"
                | "failed_before_call"
                | "unknown"
        )
    );
    let receipt = json!({"id":Uuid::now_v7(),"event_id":v.event_id,"claim_id":v.claim_id,"attempt_id":v.attempt_id,"invocation_id":v.invocation_id,"device_id":c.device_id,"credential_generation":v.credential_generation,"lease_fence":v.lease_fence,"plan_digest":v.plan_digest,"content_digest":v.content_digest,"body_hash":h,"disposition":if late{"archived"}else{v.outcome.as_str()}});
    let mut redacted = json!(v);
    redacted
        .as_object_mut()
        .ok_or(Error::Conflict)?
        .remove("result_credential");
    let encrypted = if invocation.is_some() {
        Some(secrets::seal(&json!(v))?)
    } else {
        None
    };
    sqlx::query("INSERT INTO mobile_v9_send_events(id,school_id,attempt_id,invocation_id,device_id,body,body_hash,receipt,encrypted_body) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)").bind(v.event_id).bind(uid(&cl,"school_id")?).bind(v.attempt_id).bind(v.invocation_id).bind(c.device_id).bind(redacted).bind(h).bind(&receipt).bind(encrypted).execute(&mut **tx).await?;
    if !late {
        sqlx::query("UPDATE mobile_v9_attempts SET state=$2,revision=revision+1 WHERE id=$1")
            .bind(v.attempt_id)
            .bind(&v.outcome)
            .execute(&mut **tx)
            .await?;
        let state = match v.outcome.as_str() {
            "observed_sent" => "consumed",
            "failed_before_call" => "released",
            _ => "risk_held",
        };
        sqlx::query("UPDATE mobile_v9_invocations SET state=$2 WHERE attempt_id=$1")
            .bind(v.attempt_id)
            .bind(state)
            .execute(&mut **tx)
            .await?;
        sqlx::query("UPDATE mobile_v9_quota_reservations SET state=$2 WHERE attempt_id=$1")
            .bind(v.attempt_id)
            .bind(state)
            .execute(&mut **tx)
            .await?;
        if v.outcome != "unknown" {
            sqlx::query("UPDATE mobile_v9_risk_guards SET state='closed' WHERE attempt_id=$1")
                .bind(v.attempt_id)
                .execute(&mut **tx)
                .await?;
        }
    }
    // Closing a claim does not shorten its physical lease or declare the UI thread stopped.
    sqlx::query("UPDATE mobile_v9_send_claims SET state='closed' WHERE id=$1")
        .bind(v.claim_id)
        .execute(&mut **tx)
        .await?;
    Ok(receipt)
}
async fn mutate_lease(
    tx: &mut Tx<'_>,
    c: &Credential,
    id: Uuid,
    v: LeaseMutation,
    release: bool,
) -> Result<Value> {
    let cl = live_claim(tx, c, id).await?;
    require(
        cl["runtime_epoch"] == json!(v.runtime_epoch) && cl["lease_fence"] == v.lease_fence,
        "lease_superseded",
    )?;
    // Issued invocations cannot be released by a lease operation, even after timeout.
    let issued: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_v9_invocations WHERE claim_id=$1)")
            .bind(id)
            .fetch_one(&mut **tx)
            .await?;
    require(!issued, "invocation_already_issued")?;
    if release {
        require(v.quiescent, "runtime_not_quiescent")?;
        sqlx::query("UPDATE mobile_v9_attempts SET state='failed_before_call',revision=revision+1 WHERE id=$1 AND state IN ('leased','prepared')").bind(uid(&cl,"attempt_id")?).execute(&mut **tx).await?;
        sqlx::query("UPDATE mobile_v9_send_claims SET state='closed' WHERE id=$1")
            .bind(id)
            .execute(&mut **tx)
            .await?;
        sqlx::query("UPDATE mobile_v9_quota_reservations SET state='released' WHERE attempt_id=$1 AND state='reserved'").bind(uid(&cl,"attempt_id")?).execute(&mut **tx).await?;
        sqlx::query("UPDATE mobile_shared_device_leases SET expires_at=clock_timestamp() WHERE device_id=$1 AND holder_id=$2 AND fence=$3").bind(c.device_id).bind(id).bind(v.lease_fence).execute(&mut **tx).await?;
    } else {
        // Preparation expires independently; renewal never extends an invocation permit.
        let n=sqlx::query("UPDATE mobile_v9_send_claims SET expires_at=least(created_at+interval '10 minutes',clock_timestamp()+interval '60 seconds') WHERE id=$1 AND created_at>clock_timestamp()-interval '9 minutes' ").bind(id).execute(&mut **tx).await?.rows_affected();
        require(n == 1, "lease_expired")?;
        sqlx::query("UPDATE mobile_shared_device_leases SET expires_at=(SELECT expires_at FROM mobile_v9_send_claims WHERE id=$2) WHERE device_id=$1 AND holder_id=$2 AND fence=$3").bind(c.device_id).bind(id).bind(v.lease_fence).execute(&mut **tx).await?;
    }
    Ok(json!({"id":id,"lease_fence":v.lease_fence,"state":if release{"closed"}else{"active"}}))
}

async fn usage_budget(tx: &mut Tx<'_>, scope: Uuid) -> Result<bool> {
    let budget:Option<bool>=sqlx::query_scalar("SELECT (SELECT count(*) FROM mobile_v9_invocations i JOIN mobile_v9_attempts a ON a.id=i.attempt_id JOIN mobile_v9_plans p ON p.id=a.plan_id JOIN mobile_usage_scopes x ON x.scope_id=(p.plan->>'scope_id')::uuid JOIN mobile_usages family ON family.id=x.usage_id WHERE family.budget_root=u.budget_root)<(u.body->>'max_calls')::bigint FROM mobile_usage_scopes m JOIN mobile_usages u ON u.id=m.usage_id WHERE m.scope_id=$1").bind(scope).fetch_optional(&mut **tx).await?;
    Ok(budget.unwrap_or(true))
}
