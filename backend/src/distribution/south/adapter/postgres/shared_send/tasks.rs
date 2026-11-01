use super::*;
/// Only platform-owned resources. Effective locator dependencies omit unused default names.
pub(super) async fn raw_candidates(tx: &mut Tx<'_>, s: Uuid, target: Uuid) -> Result<Vec<Value>> {
    let rows:Vec<Value>=sqlx::query_scalar(r#"SELECT jsonb_build_object(
 'target_id',t.id,'target_name',coalesce(nullif(t.alias,''),t.search_text),'device_name',d.name,'device_id',d.id,'binding_id',b.id,'binding_locator_revision',b.locator_revision,
 'target_locator_revision',CASE WHEN b.search_text_override IS NULL THEN t.locator_revision ELSE NULL END,
 'search_text',coalesce(b.search_text_override,t.search_text),'search_source',CASE WHEN b.search_text_override IS NULL THEN 'default' ELSE 'override' END,
 'access_revision',CASE WHEN b.source='school_pool' THEN (SELECT revision FROM mobile_shared_school_access WHERE school_id=b.school_id AND device_id=b.device_id) ELSE NULL END,'credential_generation',d.credential_generation,'delegation_id',m.id,'decision_id',c.id,'decision_revision',c.revision,
 'environment_snapshot_id',e.id,'release_id',r.id,'flow_digest',r.content_digest,
 'flow',r.flow,'environment',e.snapshot,'scope_digest',m.scope_digest)
 FROM mobile_device_target_bindings b JOIN mobile_targets t ON t.id=b.target_id AND t.school_id=b.school_id
 JOIN schools s ON s.id=t.school_id AND s.active JOIN execution_devices d ON d.id=b.device_id
 JOIN mobile_shared_delegations m ON m.device_id=d.id AND m.credential_generation=d.credential_generation
 JOIN mobile_device_environment_observations obs ON obs.device_id=d.id
 JOIN mobile_device_environments e ON e.id=obs.snapshot_id AND e.device_id=d.id AND e.credential_generation=d.credential_generation
 JOIN mobile_send_adapter_releases r ON r.environment=e.snapshot-'instance_ref'
 JOIN LATERAL(SELECT * FROM mobile_shared_delegation_decisions c WHERE c.delegation_id=m.id ORDER BY revision DESC LIMIT 1)c ON c.accepted
 WHERE t.school_id=$1 AND t.id=$2 AND t.enabled AND t.archived_at IS NULL AND b.enabled AND d.enabled
 AND (b.source='platform' OR EXISTS(SELECT 1 FROM mobile_shared_school_access a WHERE a.school_id=b.school_id AND a.device_id=b.device_id AND a.enabled))
 AND obs.observed_at>clock_timestamp()-interval '5 minutes'
 AND NOT EXISTS(SELECT 1 FROM mobile_send_adapter_withdrawals w WHERE w.release_id=r.id)
 AND (SELECT count(*) FROM mobile_send_adapter_releases r2 WHERE r2.environment=r.environment AND NOT EXISTS(SELECT 1 FROM mobile_send_adapter_withdrawals w WHERE w.release_id=r2.id))=1
 ORDER BY d.id"#).bind(s).bind(target).fetch_all(&mut **tx).await?;
    let mut valid = Vec::new();
    for row in rows {
        let flow: crate::distribution::domain::mobile_execution::FlowV2 =
            match serde_json::from_value(row["flow"].clone()) {
                Ok(f) => f,
                Err(_) => continue,
            };
        if flow.validate_shared_send().is_ok() && digest(&row["flow"])? == row["flow_digest"] {
            valid.push(row)
        }
    }
    Ok(valid)
}
pub(super) async fn candidates(tx: &mut Tx<'_>, s: Uuid, target: Uuid) -> Result<Vec<Value>> {
    let rows = raw_candidates(tx, s, target).await?;
    let mut valid = Vec::new();
    let mut devices = std::collections::BTreeSet::new();
    for mut row in rows {
        let deps = dependencies(&row);
        let inspection:Option<Uuid>=sqlx::query_scalar("SELECT id FROM mobile_shared_send_inspections WHERE school_id=$1 AND device_id=$2 AND target_id=$3 AND state='passed' AND expires_at>clock_timestamp() AND dependencies=$4 ORDER BY id DESC LIMIT 1").bind(s).bind(uid(&row,"device_id")?).bind(target).bind(&deps).fetch_optional(&mut **tx).await?;
        if let Some(id) = inspection
            && devices.insert(uid(&row, "device_id")?)
        {
            row["inspection_id"] = json!(id);
            valid.push(row);
        }
    }
    Ok(valid)
}
pub(super) fn dependencies(row: &Value) -> Value {
    let mut deps = serde_json::Map::new();
    for k in [
        "target_id",
        "device_id",
        "binding_id",
        "binding_locator_revision",
        "access_revision",
        "target_locator_revision",
        "search_text",
        "search_source",
        "credential_generation",
        "delegation_id",
        "decision_id",
        "decision_revision",
        "environment_snapshot_id",
        "release_id",
        "flow_digest",
        "scope_digest",
    ] {
        deps.insert(k.into(), row[k].clone());
    }
    Value::Object(deps)
}
pub(super) async fn validate_plan(tx: &mut Tx<'_>, s: Uuid, plan: &Value) -> Result<()> {
    let choices = candidates(tx, s, uid(plan, "target_id")?).await?;
    require(
        choices
            .iter()
            .any(|c| dependencies(c) == dependencies(plan)),
        "plan_dependency_changed",
    )?;
    let p = policy(tx)
        .await?
        .ok_or(Error::Policy("shared_send_not_released"))?;
    require(
        matches!(
            p["mode"].as_str(),
            Some("test_send" | "production_allowlist")
        ),
        "shared_send_not_released",
    )?;
    let allowed:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_shared_release_scopes WHERE device_id=$1 AND school_id=$2 AND target_id=$3 AND release_id=$4 AND environment_snapshot_id=$5 AND stage=$6 AND expires_at>clock_timestamp())").bind(uid(plan,"device_id")?).bind(s).bind(uid(plan,"target_id")?).bind(uid(plan,"release_id")?).bind(uid(plan,"environment_snapshot_id")?).bind(p["mode"].as_str()).fetch_one(&mut **tx).await?;
    require(allowed, "shared_send_scope_not_accepted")
}
pub(super) async fn write(
    tx: &mut Tx<'_>,
    a: &Actor,
    s: Uuid,
    command: SchoolCommand,
) -> Result<Value> {
    match command {
        SchoolCommand::PoolBinding(v) => {
            if let Some(ref text) = v.search_text_override {
                crate::distribution::domain::mobile_execution::validate_text(text, 1, 256, 1024)?;
            }
            let allowed:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_shared_school_access a JOIN schools s ON s.id=a.school_id JOIN execution_devices d ON d.id=a.device_id JOIN mobile_targets t ON t.school_id=a.school_id WHERE a.school_id=$1 AND a.device_id=$2 AND a.enabled AND a.revision=$3 AND s.active AND d.enabled AND t.id=$4 AND t.enabled AND t.archived_at IS NULL)").bind(s).bind(v.device_id).bind(v.expected_access_revision).bind(v.target_id).fetch_one(&mut **tx).await?;
            require(allowed, "shared_pool_access_required")?;
            let id:Option<Uuid>=sqlx::query_scalar("INSERT INTO mobile_device_target_bindings(id,school_id,device_id,target_id,search_text_override,created_by,updated_by,source) VALUES($1,$2,$3,$4,$5,$6,$6,'school_pool') ON CONFLICT(device_id,target_id) DO UPDATE SET enabled=true,search_text_override=$5,updated_by=$6,revision=mobile_device_target_bindings.revision+1,locator_revision=mobile_device_target_bindings.locator_revision+1 WHERE mobile_device_target_bindings.source='school_pool' RETURNING id").bind(Uuid::now_v7()).bind(s).bind(v.device_id).bind(v.target_id).bind(v.search_text_override).bind(a.id).fetch_optional(&mut **tx).await?;
            let id = id.ok_or(Error::Policy("shared_binding_platform_managed"))?;
            inspections::reconcile(tx, Some(v.device_id), None).await?;
            Ok(json!({"id":id,"state":"automatic_check_queued"}))
        }
        SchoolCommand::Unbind(id, v) => {
            let n=sqlx::query("UPDATE mobile_device_target_bindings SET enabled=false,revision=revision+1,locator_revision=locator_revision+1,updated_by=$4,updated_at=clock_timestamp() WHERE id=$1 AND school_id=$2 AND revision=$3 AND source='school_pool'").bind(id).bind(s).bind(v.expected_revision).bind(a.id).execute(&mut **tx).await?.rows_affected();
            require(n == 1, "revision_conflict")?;
            Ok(json!({"id":id,"enabled":false}))
        }
        SchoolCommand::Inspection(v) => inspections::request(tx, a, s, v).await,
        SchoolCommand::Draft(id, input) => {
            let input = input.normalized()?;
            let count:i64=sqlx::query_scalar("SELECT count(*) FROM mobile_targets WHERE school_id=$1 AND id=ANY($2) AND archived_at IS NULL").bind(s).bind(&input.target_ids).fetch_one(&mut **tx).await?;
            require(
                count == input.target_ids.len() as i64,
                "shared_send_targets_invalid",
            )?;
            let id = if let Some(id) = id {
                let n=sqlx::query("UPDATE mobile_shared_task_drafts SET target_ids=$3,message=$4,content_digest=$5,revision=revision+1 WHERE school_id=$1 AND id=$2 AND revision=$6 AND NOT EXISTS(SELECT 1 FROM mobile_shared_tasks t WHERE t.draft_id=$2)").bind(s).bind(id).bind(input.target_ids).bind(&input.message).bind(hash(&input.message)).bind(input.expected_revision).execute(&mut **tx).await?.rows_affected();
                require(n == 1, "revision_conflict")?;
                id
            } else {
                require(input.expected_revision.is_none(), "revision_conflict")?;
                let id = Uuid::now_v7();
                sqlx::query("INSERT INTO mobile_shared_task_drafts(id,school_id,intent_id,actor_id,target_ids,message,content_digest) VALUES($1,$2,$3,$4,$5,$6,$7)").bind(id).bind(s).bind(Uuid::now_v7()).bind(a.id).bind(input.target_ids).bind(&input.message).bind(hash(&input.message)).execute(&mut **tx).await?;
                id
            };
            Ok(sqlx::query_scalar(
                "SELECT to_jsonb(d) FROM mobile_shared_task_drafts d WHERE id=$1",
            )
            .bind(id)
            .fetch_one(&mut **tx)
            .await?)
        }
        SchoolCommand::Preview(input) => {
            let d:Value=sqlx::query_scalar("SELECT to_jsonb(d) FROM mobile_shared_task_drafts d WHERE school_id=$1 AND id=$2 AND revision=$3 AND NOT EXISTS(SELECT 1 FROM mobile_shared_tasks t WHERE t.draft_id=d.id)").bind(s).bind(input.draft_id).bind(input.expected_revision).fetch_optional(&mut **tx).await?.ok_or(Error::Conflict)?;
            let targets: Vec<Uuid> =
                serde_json::from_value(d["target_ids"].clone()).map_err(|_| Error::Conflict)?;
            require(
                input.selections.len() <= targets.len()
                    && input
                        .selections
                        .iter()
                        .all(|v| targets.contains(&v.target_id))
                    && input
                        .selections
                        .iter()
                        .map(|v| v.target_id)
                        .collect::<std::collections::BTreeSet<_>>()
                        .len()
                        == input.selections.len(),
                "shared_send_selection_invalid",
            )?;
            let status = readiness::school(tx, s).await?;
            let mut items = Vec::new();
            for (position, target) in targets.iter().enumerate() {
                let choices = candidates(tx, s, *target).await?;
                let selection = input.selections.iter().find(|v| v.target_id == *target);
                let chosen = if let Some(v) = selection {
                    choices
                        .iter()
                        .find(|c| c["device_id"] == json!(v.device_id))
                } else {
                    choices.first()
                };
                items.push(json!({"position":position,"target_id":target,"selected":chosen,"candidates":choices,"readiness":status.iter().filter(|r|r["target_id"]==json!(target)).collect::<Vec<_>>(),"blocker":if chosen.is_none(){Some("target_inspection_or_binding_required")}else{None}}));
            }
            let p = policy(tx).await?;
            let id = Uuid::now_v7();
            let plan = json!({"schema":SCHEMA,"school_id":s,"intent_id":d["intent_id"],"draft_id":input.draft_id,"draft_revision":input.expected_revision,"message":d["message"],"content_digest":d["content_digest"],"items":items,"policy_id":p.as_ref().map(|p|&p["id"])});
            let h = digest(&plan)?;
            sqlx::query("INSERT INTO mobile_shared_previews(id,school_id,draft_id,draft_revision,plan,digest,policy_id) VALUES($1,$2,$3,$4,$5,$6,$7)").bind(id).bind(s).bind(input.draft_id).bind(input.expected_revision).bind(&plan).bind(&h).bind(p.as_ref().map(|p|uid(p,"id")).transpose()?).execute(&mut **tx).await?;
            Ok(
                sqlx::query_scalar("SELECT to_jsonb(p) FROM mobile_shared_previews p WHERE id=$1")
                    .bind(id)
                    .fetch_one(&mut **tx)
                    .await?,
            )
        }
        SchoolCommand::Confirm(input) => {
            require(input.confirmed, "confirmation_required")?;
            let old: Option<Uuid> = sqlx::query_scalar(
                "SELECT t.id FROM mobile_shared_tasks t JOIN mobile_shared_previews p ON p.id=t.preview_id WHERE t.school_id=$1 AND t.preview_id=$2 AND p.digest=$3",
            )
            .bind(s)
            .bind(input.preview_id)
            .bind(&input.preview_digest)
            .fetch_optional(&mut **tx)
            .await?;
            if let Some(id) = old {
                return task(tx, s, id).await;
            }
            let p:Value=sqlx::query_scalar("SELECT to_jsonb(p) FROM mobile_shared_previews p JOIN mobile_shared_task_drafts d ON d.id=p.draft_id AND d.revision=p.draft_revision WHERE p.school_id=$1 AND p.id=$2 AND p.expires_at>clock_timestamp() AND p.digest=$3 FOR UPDATE OF d").bind(s).bind(input.preview_id).bind(input.preview_digest).fetch_optional(&mut **tx).await?.ok_or(Error::Policy("preview_expired"))?;
            let plan = &p["plan"];
            let current = policy(tx)
                .await?
                .ok_or(Error::Policy("shared_send_not_released"))?;
            require(current["id"] == p["policy_id"], "plan_dependency_changed")?;
            let items = plan["items"].as_array().ok_or(Error::Conflict)?;
            for item in items {
                validate_plan(tx, s, &item["selected"]).await?;
            }
            let id = Uuid::now_v7();
            let snap = Uuid::now_v7();
            let message = plan["message"].as_str().ok_or(Error::Conflict)?;
            sqlx::query("INSERT INTO distribution_message_snapshots(id,school_id,channel,content,content_hash) VALUES($1,$2,'mobile',$3,$4)").bind(snap).bind(s).bind(message).bind(hash(message)).execute(&mut **tx).await?;
            sqlx::query("INSERT INTO mobile_shared_tasks(id,school_id,draft_id,intent_id,preview_id,confirmed_by) VALUES($1,$2,$3,$4,$5,$6)").bind(id).bind(s).bind(uid(plan,"draft_id")?).bind(uid(plan,"intent_id")?).bind(input.preview_id).bind(a.id).execute(&mut **tx).await?;
            for item in items {
                let iid = Uuid::now_v7();
                let pid = Uuid::now_v7();
                let aid = Uuid::now_v7();
                let mut frozen = item["selected"].clone();
                frozen["schema"] = json!(SCHEMA);
                frozen["school_id"] = json!(s);
                frozen["intent_id"] = plan["intent_id"].clone();
                frozen["item_id"] = json!(iid);
                frozen["attempt_id"] = json!(aid);
                frozen["message"] = json!(message);
                frozen["content_digest"] = plan["content_digest"].clone();
                frozen["selection_policy"] = json!("first_group_result");
                sqlx::query("INSERT INTO mobile_shared_task_items(id,school_id,task_id,intent_id,target_id,position) VALUES($1,$2,$3,$4,$5,$6)").bind(iid).bind(s).bind(id).bind(uid(plan,"intent_id")?).bind(uid(item,"target_id")?).bind(item["position"].as_i64().ok_or(Error::Conflict)? as i32).execute(&mut **tx).await?;
                sqlx::query("INSERT INTO mobile_shared_plans(id,school_id,item_id,target_id,device_id,snapshot_id,plan,digest,content_digest) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)").bind(pid).bind(s).bind(iid).bind(uid(item,"target_id")?).bind(uid(&frozen,"device_id")?).bind(snap).bind(&frozen).bind(digest(&frozen)?).bind(plan["content_digest"].as_str()).execute(&mut **tx).await?;
                sqlx::query("INSERT INTO mobile_shared_attempts(id,school_id,item_id,plan_id,attempt_no,confirmed_by) VALUES($1,$2,$3,$4,1,$5)").bind(aid).bind(s).bind(iid).bind(pid).bind(a.id).execute(&mut **tx).await?;
            }
            task(tx, s, id).await
        }
        SchoolCommand::Reassign(id, input) => {
            require(
                !input.reason.trim().is_empty() && input.reason.len() <= 2000,
                "shared_send_reason_required",
            )?;
            let old:Value=sqlx::query_scalar("SELECT to_jsonb(a)||jsonb_build_object('plan',p.plan,'snapshot_id',p.snapshot_id,'target_id',i.target_id,'task_id',i.task_id) FROM mobile_shared_attempts a JOIN mobile_shared_plans p ON p.id=a.plan_id JOIN mobile_shared_task_items i ON i.id=a.item_id JOIN mobile_shared_tasks t ON t.id=i.task_id WHERE a.id=$1 AND a.school_id=$2 AND a.current AND NOT t.stopped").bind(id).bind(s).fetch_optional(&mut **tx).await?.ok_or(Error::NotFound)?;
            require(
                old["revision"] == input.expected_revision
                    && matches!(
                        old["state"].as_str(),
                        Some("failed_before_call" | "verified_not_sent" | "invalidated")
                    ),
                "shared_send_review_required",
            )?;
            let old_device = uid(&old["plan"], "device_id")?;
            // Even a manual not-sent decision cannot override a still-running physical owner.
            require(!inspections::busy(tx, old_device).await?, "device_busy")?;
            let target = uid(&old, "target_id")?;
            let mut plan = candidates(tx, s, target)
                .await?
                .into_iter()
                .find(|x| x["device_id"] == json!(input.device_id))
                .ok_or(Error::Policy("plan_dependency_changed"))?;
            validate_plan(tx, s, &plan).await?;
            let aid = Uuid::now_v7();
            let pid = Uuid::now_v7();
            for k in [
                "schema",
                "school_id",
                "intent_id",
                "item_id",
                "message",
                "content_digest",
                "selection_policy",
            ] {
                plan[k] = old["plan"][k].clone();
            }
            plan["attempt_id"] = json!(aid);
            sqlx::query(
                "UPDATE mobile_shared_attempts SET current=false,revision=revision+1 WHERE id=$1",
            )
            .bind(id)
            .execute(&mut **tx)
            .await?;
            sqlx::query("INSERT INTO mobile_shared_plans(id,school_id,item_id,target_id,device_id,snapshot_id,plan,digest,content_digest) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)").bind(pid).bind(s).bind(uid(&old,"item_id")?).bind(target).bind(input.device_id).bind(uid(&old,"snapshot_id")?).bind(&plan).bind(digest(&plan)?).bind(plan["content_digest"].as_str()).execute(&mut **tx).await?;
            sqlx::query("INSERT INTO mobile_shared_attempts(id,school_id,item_id,plan_id,attempt_no,confirmed_by) VALUES($1,$2,$3,$4,$5,$6)").bind(aid).bind(s).bind(uid(&old,"item_id")?).bind(pid).bind(old["attempt_no"].as_i64().ok_or(Error::Conflict)? as i32+1).bind(a.id).execute(&mut **tx).await?;
            sqlx::query("INSERT INTO mobile_shared_reassignments(id,school_id,old_attempt_id,new_attempt_id,actor_id,reason) VALUES($1,$2,$3,$4,$5,$6)").bind(Uuid::now_v7()).bind(s).bind(id).bind(aid).bind(a.id).bind(input.reason).execute(&mut **tx).await?;
            task(tx, s, uid(&old, "task_id")?).await
        }
        SchoolCommand::Stop(id, input) => {
            require(
                !input.reason.trim().is_empty() && input.reason.len() <= 2000,
                "shared_send_reason_required",
            )?;
            let n=sqlx::query("UPDATE mobile_shared_tasks SET stopped=true,revision=revision+1 WHERE school_id=$1 AND id=$2 AND revision=$3").bind(s).bind(id).bind(input.expected_revision).execute(&mut **tx).await?.rows_affected();
            require(n == 1, "revision_conflict")?;
            sqlx::query("UPDATE mobile_shared_attempts a SET state='cancelled',revision=revision+1 FROM mobile_shared_task_items i WHERE i.id=a.item_id AND i.task_id=$1 AND a.state='queued'").bind(id).execute(&mut **tx).await?;
            task(tx, s, id).await
        }
        SchoolCommand::Review(id, input) => {
            require(
                matches!(
                    input.decision.as_str(),
                    "manually_confirmed_sent" | "verified_not_sent" | "still_unknown"
                ) && !input.evidence.trim().is_empty()
                    && input.evidence.len() <= 2000,
                "shared_send_review_invalid",
            )?;
            let row=sqlx::query("SELECT id,revision,state FROM mobile_shared_attempts WHERE school_id=$1 AND id=$2 FOR UPDATE").bind(s).bind(id).fetch_optional(&mut **tx).await?.ok_or(Error::NotFound)?;
            require(
                row.get::<i64, _>("revision") == input.expected_revision
                    && row.get::<String, _>("state") == "unknown",
                "revision_conflict",
            )?;
            let rid = Uuid::now_v7();
            sqlx::query("INSERT INTO mobile_shared_reviews(id,school_id,attempt_id,reviewer,decision,evidence) VALUES($1,$2,$3,$4,$5,$6)").bind(rid).bind(s).bind(id).bind(a.id).bind(&input.decision).bind(&input.evidence).execute(&mut **tx).await?;
            if input.decision == "still_unknown" {
                sqlx::query("UPDATE mobile_shared_attempts SET revision=revision+1 WHERE id=$1")
                    .bind(id)
                    .execute(&mut **tx)
                    .await?;
            }
            if input.decision != "still_unknown" {
                sqlx::query(
                    "UPDATE mobile_shared_attempts SET state=$2,revision=revision+1 WHERE id=$1",
                )
                .bind(id)
                .bind(&input.decision)
                .execute(&mut **tx)
                .await?;
                let state = if input.decision == "verified_not_sent" {
                    "released"
                } else {
                    "consumed"
                };
                sqlx::query("UPDATE mobile_shared_invocations SET state=$2 WHERE attempt_id=$1")
                    .bind(id)
                    .bind(state)
                    .execute(&mut **tx)
                    .await?;
                sqlx::query(
                    "UPDATE mobile_shared_quota_reservations SET state=$2 WHERE attempt_id=$1",
                )
                .bind(id)
                .bind(state)
                .execute(&mut **tx)
                .await?;
                sqlx::query(
                    "UPDATE mobile_shared_risk_guards SET state='closed' WHERE attempt_id=$1",
                )
                .bind(id)
                .execute(&mut **tx)
                .await?;
            }
            Ok(json!({"id":rid,"attempt_id":id,"decision":input.decision}))
        }
    }
}
