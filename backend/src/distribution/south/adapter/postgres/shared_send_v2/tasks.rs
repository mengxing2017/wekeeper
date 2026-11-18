use super::*;
/// Every phone receives the same business group name; bindings only select authorized devices.
pub(super) async fn raw_candidates(tx: &mut Tx<'_>, target: Uuid) -> Result<Vec<Value>> {
    sqlx::query_scalar(r#"SELECT jsonb_build_object(
 'target_id',t.id,'target_name',t.search_text,'device_name',d.name,'device_id',d.id,'binding_id',b.id,'binding_locator_revision',b.locator_revision,
 'target_locator_revision',t.locator_revision,
 'target',jsonb_build_object('kind','group','search_query',t.search_text,'match_policy','exact_unique','identity_constraints','[]'::jsonb),
 'credential_generation',d.credential_generation,
 'execution',jsonb_build_object('contract','device_owned/1','capability','wecom.group_text'))
 FROM mobile_device_target_bindings b JOIN mobile_targets t ON t.id=b.target_id
 JOIN execution_devices d ON d.id=b.device_id
 WHERE t.id=$1 AND t.enabled AND t.archived_at IS NULL AND b.enabled AND d.enabled
 ORDER BY d.name,d.id"#).bind(target).fetch_all(&mut **tx).await.map_err(Into::into)
}
pub(super) async fn candidates(tx: &mut Tx<'_>, target: Uuid) -> Result<Vec<Value>> {
    raw_candidates(tx, target).await
}
pub(super) fn dependencies(row: &Value) -> Value {
    let mut deps = serde_json::Map::new();
    for k in [
        "target_id",
        "device_id",
        "binding_id",
        "binding_locator_revision",
        "target_locator_revision",
        "target",
        "credential_generation",
    ] {
        deps.insert(k.into(), row[k].clone());
    }
    Value::Object(deps)
}
// Validate platform authorization and frozen dependencies only. Target identity is interpreted
// by the device after delivery; missing execution metadata must not block task acceptance.
pub(super) async fn validate_plan(tx: &mut Tx<'_>, plan: &Value) -> Result<()> {
    require(
        plan["target_name"].as_str().is_some_and(|name| {
            !name.is_empty() && plan["target"]["search_query"].as_str() == Some(name)
        }),
        "retired_target_contract",
    )?;
    require(
        plan["execution"]["contract"] == "device_owned/1"
            && plan["execution"]["capability"] == "wecom.group_text"
            && plan.get("flow").is_none()
            && plan.get("flow_digest").is_none(),
        "execution_contract_replaced",
    )?;
    let choices = raw_candidates(tx, uid(plan, "target_id")?).await?;
    if !choices
        .iter()
        .any(|c| dependencies(c) == dependencies(plan))
    {
        let current = choices.iter().find(|c| c["device_id"] == plan["device_id"]);
        let frozen = dependencies(plan);
        let live = current.map(dependencies).unwrap_or(Value::Null);
        let fields: Vec<_> = frozen
            .as_object()
            .into_iter()
            .flat_map(|m| m.keys())
            .filter(|k| frozen[*k] != live[*k])
            .collect();
        tracing::warn!(device=%plan["device_id"], target=%plan["target_id"],
            comparison_policy="group_target/1", difference_fields=?fields,
            "execution plan dependency mismatch");
        return Err(Error::Policy("plan_dependency_changed"));
    }
    Ok(())
}
pub(super) async fn validate_risk(tx: &mut Tx<'_>, plan: &Value) -> Result<()> {
    let risk:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_v9_attempts a JOIN mobile_v9_task_items i ON i.id=a.item_id WHERE i.target_id=$1 AND i.intent_id=$2 AND a.state IN ('observed_sent','manually_confirmed_sent'))").bind(uid(plan,"target_id")?).bind(plan["intent_id"].as_str().and_then(|v|Uuid::parse_str(v).ok())).fetch_one(&mut **tx).await?;
    require(!risk, "shared_send_review_required")
}
pub(super) async fn write(tx: &mut Tx<'_>, a: &Actor, command: TaskCommand) -> Result<Value> {
    match command {
        TaskCommand::Support(v) => {
            crate::distribution::domain::shared_mobile::validate_text(&v.reason, 1, 2000, 8000)?;
            let bound: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM mobile_device_target_bindings WHERE device_id=$1)",
            )
            .bind(v.device_id)
            .fetch_one(&mut **tx)
            .await?;
            require(bound, "shared_binding_missing")?;
            let id = Uuid::now_v7();
            sqlx::query("INSERT INTO mobile_v9_support_requests(id,device_id,reason,state,created_by) VALUES($1,$2,$3,'submitted',$4)").bind(id).bind(v.device_id).bind(v.reason).bind(a.id).execute(&mut **tx).await?;
            Ok(json!({"id":id,"state":"submitted"}))
        }

        TaskCommand::Inspection(v) => inspections::request(tx, a, v).await,
        TaskCommand::Draft(id, input) => {
            let input = input.normalized()?;
            let count: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM mobile_targets WHERE id=ANY($1) AND archived_at IS NULL",
            )
            .bind(&input.target_ids)
            .fetch_one(&mut **tx)
            .await?;
            require(
                count == input.target_ids.len() as i64,
                "shared_send_targets_invalid",
            )?;
            let id = if let Some(id) = id {
                let n=sqlx::query("UPDATE mobile_v9_task_drafts SET target_ids=$2,message=$3,content_digest=$4,between_targets_seconds=$6,revision=revision+1 WHERE id=$1 AND revision=$5 AND NOT EXISTS(SELECT 1 FROM mobile_v9_tasks t WHERE t.draft_id=$1)").bind(id).bind(input.target_ids).bind(&input.message).bind(hash(&input.message)).bind(input.expected_revision).bind(input.between_targets_seconds).execute(&mut **tx).await?.rows_affected();
                require(n == 1, "revision_conflict")?;
                id
            } else {
                require(input.expected_revision.is_none(), "revision_conflict")?;
                let id = Uuid::now_v7();
                sqlx::query("INSERT INTO mobile_v9_task_drafts(id,intent_id,actor_id,target_ids,message,content_digest,between_targets_seconds) VALUES($1,$2,$3,$4,$5,$6,$7)").bind(id).bind(Uuid::now_v7()).bind(a.id).bind(input.target_ids).bind(&input.message).bind(hash(&input.message)).bind(input.between_targets_seconds).execute(&mut **tx).await?;
                id
            };
            Ok(sqlx::query_scalar(
                "SELECT to_jsonb(d)||jsonb_build_object('targets',coalesce((SELECT jsonb_agg(jsonb_build_object('id',t.id,'name',t.search_text) ORDER BY ids.n) FROM unnest(d.target_ids) WITH ORDINALITY ids(id,n) JOIN mobile_targets t ON t.id=ids.id ),'[]'::jsonb)) FROM mobile_v9_task_drafts d WHERE id=$1",
            )
            .bind(id)
            .fetch_one(&mut **tx)
            .await?)
        }
        TaskCommand::Preview(input) => {
            let d:Value=sqlx::query_scalar("SELECT to_jsonb(d) FROM mobile_v9_task_drafts d WHERE id=$1 AND revision=$2 AND NOT EXISTS(SELECT 1 FROM mobile_v9_tasks t WHERE t.draft_id=d.id)").bind(input.draft_id).bind(input.expected_revision).fetch_optional(&mut **tx).await?.ok_or(Error::Conflict)?;
            let unit: Option<Uuid> = serde_json::from_value(d["business_unit_id"].clone())
                .map_err(|_| Error::Conflict)?;
            let effective = business_scheme::resolve(tx, unit).await?;
            let sending_status = business_scheme::status_for(tx, unit).await?;
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
            let status = readiness::resources(tx).await?;
            let mut items = Vec::new();
            for (position, target) in targets.iter().enumerate() {
                let mut choices = candidates(tx, *target).await?;
                let mut confirmation_issues = std::collections::BTreeMap::new();
                for choice in &mut choices {
                    let issue = match validate_plan(tx, choice).await {
                        Ok(()) => None,
                        Err(Error::Policy(code)) => Some(code.to_owned()),
                        Err(Error::Invalid(message)) => Some(message),
                        Err(error) => return Err(error),
                    };
                    choice["confirmable"] = json!(issue.is_none());
                    if let Some(issue) = issue {
                        confirmation_issues.insert(uid(choice, "device_id")?, issue);
                    }
                }
                let target_info:PreviewTarget = sqlx::query_scalar::<_,Value>("SELECT jsonb_build_object('id',id,'name',search_text,'search_text',search_text,'locator_revision',locator_revision) FROM mobile_targets WHERE id=$1 ").bind(target).fetch_optional(&mut **tx).await?.map(serde_json::from_value).transpose().map_err(|_|Error::Conflict)?.ok_or(Error::NotFound)?;
                let target_status: Vec<Value> = status
                    .iter()
                    .filter(|r| r["target_id"] == json!(target))
                    .cloned()
                    .collect();
                let selection = input.selections.iter().find(|v| v.target_id == *target);
                let chosen =
                    choose_candidate(&choices, &target_status, selection.map(|v| v.device_id));
                let blocker = if chosen.is_none() {
                    Some("请选择已绑定手机")
                } else if let Some(choice) = chosen.filter(|c| c["confirmable"] != true) {
                    Some(
                        match confirmation_issues
                            .get(&uid(choice, "device_id")?)
                            .map(String::as_str)
                        {
                            Some("shared_send_review_required") => {
                                "存在未核对的发送结果，请先核对原任务"
                            }
                            Some("plan_dependency_changed") => {
                                "手机或目标群关联已变化，请更新业务预览"
                            }
                            _ => "当前手机不满足任务受理条件，请核对当前业务使用范围",
                        },
                    )
                } else {
                    None
                };
                items.push(json!({"position":position,"target_id":target,"target":target_info,"selected":chosen,"candidates":choices,"readiness":target_status,"blocker":blocker}));
            }
            let p = Some(effective.clone());
            let id = Uuid::now_v7();
            let plan = json!({"schema":SCHEMA,"intent_id":d["intent_id"],"draft_id":input.draft_id,"draft_revision":input.expected_revision,"between_targets_seconds":d["between_targets_seconds"],"message":d["message"],"content_digest":d["content_digest"],"items":items,"policy_id":p.as_ref().map(|p|&p["id"]),"sending_status":sending_status,"business_unit_id":unit,"scheme_snapshot":effective});
            let h = digest(&plan)?;
            sqlx::query("INSERT INTO mobile_v9_previews(id,draft_id,draft_revision,plan,digest,policy_id) VALUES($1,$2,$3,$4,$5,$6)").bind(id).bind(input.draft_id).bind(input.expected_revision).bind(&plan).bind(&h).bind(p.as_ref().map(policy_id).transpose()?.flatten()).execute(&mut **tx).await?;
            Ok(
                sqlx::query_scalar("SELECT to_jsonb(p) FROM mobile_v9_previews p WHERE id=$1")
                    .bind(id)
                    .fetch_one(&mut **tx)
                    .await?,
            )
        }
        TaskCommand::Confirm(input) => {
            require(input.confirmed, "confirmation_required")?;
            let old: Option<Uuid> = sqlx::query_scalar(
                "SELECT t.id FROM mobile_v9_tasks t JOIN mobile_v9_previews p ON p.id=t.preview_id WHERE t.preview_id=$1 AND p.digest=$2",
            )
            .bind(input.preview_id)
            .bind(&input.preview_digest)
            .fetch_optional(&mut **tx)
            .await?;
            if let Some(id) = old {
                return task(tx, id).await;
            }
            let p:Value=sqlx::query_scalar("SELECT to_jsonb(p) FROM mobile_v9_previews p JOIN mobile_v9_task_drafts d ON d.id=p.draft_id AND d.revision=p.draft_revision WHERE p.id=$1 AND p.expires_at>clock_timestamp() AND p.digest=$2 FOR UPDATE OF d").bind(input.preview_id).bind(input.preview_digest).fetch_optional(&mut **tx).await?.ok_or(Error::Policy("preview_expired"))?;
            let plan = &p["plan"];
            let unit: Option<Uuid> = serde_json::from_value(plan["business_unit_id"].clone())
                .map_err(|_| Error::Conflict)?;
            require(
                digest(&business_scheme::resolve(tx, unit).await?)?
                    == digest(&plan["scheme_snapshot"])?,
                "business_scheme_changed",
            )?;
            let items = plan["items"].as_array().ok_or(Error::Conflict)?;
            for item in items {
                require(
                    item["selected"]["execution"]["contract"] == "device_owned/1",
                    "preview_expired",
                )?;
                validate_plan(tx, &item["selected"]).await?;
                for candidate in item["candidates"]
                    .as_array()
                    .ok_or(Error::Conflict)?
                    .iter()
                    .filter(|c| c["confirmable"] == true)
                {
                    validate_plan(tx, candidate).await?;
                }
                require(items.len() <= 100, "shared_send_targets_invalid")?;
            }
            create_authorized_task(tx, a.id, &p).await
        }
        TaskCommand::Reassign(id, input) => {
            require(
                !input.reason.trim().is_empty() && input.reason.len() <= 2000,
                "shared_send_reason_required",
            )?;
            let old:Value=sqlx::query_scalar("SELECT to_jsonb(a)||jsonb_build_object('plan',mobile_business_snapshot(p.plan),'snapshot_id',p.snapshot_id,'target_id',i.target_id,'task_id',i.task_id) FROM mobile_v9_attempts a JOIN mobile_v9_plans p ON p.id=a.plan_id JOIN mobile_v9_task_items i ON i.id=a.item_id JOIN mobile_v9_tasks t ON t.id=i.task_id WHERE a.id=$1  AND a.current AND NOT t.stopped").bind(id).fetch_optional(&mut **tx).await?.ok_or(Error::NotFound)?;
            require(
                old["revision"] == input.expected_revision
                    && matches!(
                        old["state"].as_str(),
                        Some(
                            "failed_before_call" | "verified_not_sent" | "invalidated" | "unknown"
                        )
                    ),
                "shared_send_review_required",
            )?;

            let budget: Option<Value> = sqlx::query_scalar(
                "SELECT to_jsonb(ds) FROM mobile_v9_dispatch_state ds WHERE item_id=$1 FOR UPDATE",
            )
            .bind(uid(&old, "item_id")?)
            .fetch_optional(&mut **tx)
            .await?;
            if let Some(budget) = &budget {
                require(
                    !crate::distribution::domain::shared_mobile::execution::dispatch_terminal(
                        budget["state"].as_str().unwrap_or(""),
                    ),
                    "dispatch_round_limit_exceeded",
                )?;
            }
            validate_risk(tx, &old["plan"]).await?;
            let target = uid(&old, "target_id")?;
            let mut plan = candidates(tx, target)
                .await?
                .into_iter()
                .find(|x| x["device_id"] == json!(input.device_id))
                .ok_or(Error::Policy("plan_dependency_changed"))?;
            validate_plan(tx, &plan).await?;
            let allowed: Option<Value> = sqlx::query_scalar(
                "SELECT candidates FROM mobile_v9_dispatch_scopes WHERE item_id=$1",
            )
            .bind(uid(&old, "item_id")?)
            .fetch_optional(&mut **tx)
            .await?;
            if let Some(allowed) = &allowed {
                require(
                    allowed.as_array().is_some_and(|cs| {
                        cs.iter().any(|c| dependencies(c) == dependencies(&plan))
                    }),
                    "plan_dependency_changed",
                )?;
            }
            if let (Some(budget), Some(allowed)) = (&budget, &allowed) {
                let candidates = allowed.as_array().ok_or(Error::Conflict)?;
                let origin = budget["round_origin"].as_u64().ok_or(Error::Conflict)? as usize;
                let cursor = budget["candidate_cursor"].as_u64().ok_or(Error::Conflict)? as usize;
                let index = candidates
                    .iter()
                    .position(|c| dependencies(c) == dependencies(&plan))
                    .ok_or(Error::Conflict)?;
                let offset =
                    (index + candidates.len() - origin % candidates.len()) % candidates.len();
                require(
                    offset >= cursor && budget["state"] != "waiting_resources",
                    "dispatch_candidate_already_visited",
                )?;
                sqlx::query("UPDATE mobile_v9_dispatch_state SET candidate_cursor=$2,assigned_at=clock_timestamp(),state='assigned',reason='manual_reassignment',next_retry_at=clock_timestamp()+interval '5 seconds',revision=revision+1 WHERE item_id=$1")
                    .bind(uid(&old,"item_id")?).bind((offset+1) as i32).execute(&mut **tx).await?;
            }
            sqlx::query("UPDATE mobile_execution_grants SET state='superseded' WHERE attempt_id=$1 AND state='active'").bind(id).execute(&mut **tx).await?;
            let aid = Uuid::now_v7();
            let pid = Uuid::now_v7();
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
                "UPDATE mobile_v9_attempts SET current=false,revision=revision+1 WHERE id=$1",
            )
            .bind(id)
            .execute(&mut **tx)
            .await?;
            sqlx::query("INSERT INTO mobile_v9_plans(id,item_id,target_id,device_id,snapshot_id,plan,digest,content_digest) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(pid).bind(uid(&old,"item_id")?).bind(target).bind(input.device_id).bind(uid(&old,"snapshot_id")?).bind(&plan).bind(digest(&plan)?).bind(plan["content_digest"].as_str()).execute(&mut **tx).await?;
            sqlx::query("INSERT INTO mobile_v9_attempts(id,item_id,plan_id,attempt_no,confirmed_by) VALUES($1,$2,$3,$4,$5)").bind(aid).bind(uid(&old,"item_id")?).bind(pid).bind(old["attempt_no"].as_i64().ok_or(Error::Conflict)? as i32+1).bind(a.id).execute(&mut **tx).await?;
            sqlx::query("INSERT INTO mobile_v9_reassignments(id,old_attempt_id,new_attempt_id,actor_id,reason) VALUES($1,$2,$3,$4,$5)").bind(Uuid::now_v7()).bind(id).bind(aid).bind(a.id).bind(input.reason).execute(&mut **tx).await?;
            task(tx, uid(&old, "task_id")?).await
        }
        TaskCommand::Stop(id, input) => {
            require(
                !input.reason.trim().is_empty() && input.reason.len() <= 2000,
                "shared_send_reason_required",
            )?;
            let n=sqlx::query("UPDATE mobile_v9_tasks SET stopped=true,revision=revision+1 WHERE id=$1 AND revision=$2").bind(id).bind(input.expected_revision).execute(&mut **tx).await?.rows_affected();
            require(n == 1, "revision_conflict")?;
            sqlx::query("UPDATE mobile_v9_attempts a SET state='cancelled',revision=revision+1 FROM mobile_v9_task_items i WHERE i.id=a.item_id AND i.task_id=$1 AND a.state='queued'").bind(id).execute(&mut **tx).await?;
            sqlx::query("UPDATE mobile_execution_grants g SET state='superseded' FROM mobile_v9_attempts a JOIN mobile_v9_task_items i ON i.id=a.item_id WHERE i.task_id=$1 AND g.attempt_id=a.id AND g.state='active'").bind(id).execute(&mut **tx).await?;
            task(tx, id).await
        }
        TaskCommand::Review(id, input) => {
            require(
                matches!(
                    input.decision.as_str(),
                    "manually_confirmed_sent" | "verified_not_sent" | "still_unknown"
                ) && !input.evidence.trim().is_empty()
                    && input.evidence.len() <= 2000,
                "shared_send_review_invalid",
            )?;
            let row = sqlx::query(
                "SELECT id,revision,state FROM mobile_v9_attempts WHERE id=$1 FOR UPDATE",
            )
            .bind(id)
            .fetch_optional(&mut **tx)
            .await?
            .ok_or(Error::NotFound)?;
            require(
                row.get::<i64, _>("revision") == input.expected_revision
                    && row.get::<String, _>("state") == "unknown",
                "revision_conflict",
            )?;
            if input.decision == "verified_not_sent" {
                let active:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_v9_invocations WHERE attempt_id=$1 AND execute_before>clock_timestamp())").bind(id).fetch_one(&mut **tx).await?;
                require(!active, "shared_send_review_required")?;
            }
            let rid = Uuid::now_v7();
            sqlx::query("INSERT INTO mobile_v9_reviews(id,attempt_id,reviewer,decision,evidence) VALUES($1,$2,$3,$4,$5)").bind(rid).bind(id).bind(a.id).bind(&input.decision).bind(&input.evidence).execute(&mut **tx).await?;
            if input.decision == "still_unknown" {
                sqlx::query("UPDATE mobile_v9_attempts SET revision=revision+1 WHERE id=$1")
                    .bind(id)
                    .execute(&mut **tx)
                    .await?;
            }
            if input.decision != "still_unknown" {
                sqlx::query(
                    "UPDATE mobile_v9_attempts SET state=$2,revision=revision+1 WHERE id=$1",
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
                sqlx::query("UPDATE mobile_v9_invocations SET state=$2 WHERE attempt_id=$1")
                    .bind(id)
                    .bind(state)
                    .execute(&mut **tx)
                    .await?;
                sqlx::query("UPDATE mobile_v9_quota_reservations SET state=$2 WHERE attempt_id=$1")
                    .bind(id)
                    .bind(state)
                    .execute(&mut **tx)
                    .await?;
                sqlx::query("UPDATE mobile_v9_risk_guards SET state='closed' WHERE attempt_id=$1")
                    .bind(id)
                    .execute(&mut **tx)
                    .await?;
            }
            if input.decision == "manually_confirmed_sent" {
                sqlx::query("UPDATE mobile_v9_dispatch_state SET state='completed',reason='manually_confirmed_sent',next_retry_at=NULL,finished_at=coalesce(finished_at,clock_timestamp()),revision=revision+1,updated_at=clock_timestamp() WHERE item_id=(SELECT item_id FROM mobile_v9_attempts WHERE id=$1)").bind(id).execute(&mut **tx).await?;
                sqlx::query("UPDATE mobile_execution_grants g SET state='superseded' FROM mobile_v9_attempts a WHERE a.item_id=(SELECT item_id FROM mobile_v9_attempts WHERE id=$1) AND g.attempt_id=a.id AND g.state='active'").bind(id).execute(&mut **tx).await?;
            }
            Ok(json!({"id":rid,"attempt_id":id,"decision":input.decision}))
        }
    }
}

// Connection availability only breaks ties between candidates with valid business scope.
fn choose_candidate<'a>(
    choices: &'a [Value],
    status: &[Value],
    requested: Option<Uuid>,
) -> Option<&'a Value> {
    if let Some(device) = requested {
        return choices.iter().find(|c| c["device_id"] == json!(device));
    }
    choices
        .iter()
        .filter(|c| c["confirmable"] == true)
        .find(|c| {
            status
                .iter()
                .any(|r| r["device_id"] == c["device_id"] && r["ready"] == true)
        })
        .or_else(|| choices.iter().find(|c| c["confirmable"] == true))
        .or_else(|| choices.first())
}

#[cfg(test)]
mod candidate_selection_tests {
    use super::*;
    #[test]
    fn valid_offline_scope_wins_over_ready_but_unauthorized_phone() {
        let choices = vec![
            json!({"device_id":Uuid::now_v7(),"confirmable":false}),
            json!({"device_id":Uuid::now_v7(),"confirmable":true}),
        ];
        let status = vec![json!({"device_id":choices[0]["device_id"],"ready":true})];
        assert_eq!(choose_candidate(&choices, &status, None), Some(&choices[1]));
        // A deliberate selection must never be silently replaced.
        assert_eq!(
            choose_candidate(
                &choices,
                &status,
                Some(uid(&choices[0], "device_id").unwrap())
            ),
            Some(&choices[0])
        );
    }
}

#[cfg(test)]
mod group_target_tests {
    use super::*;
    #[test]
    fn kotlin_handoff_projection_canonical_and_digest_match() {
        let vectors: Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../api-contract/fixtures/group-target-dependencies.json"
        )))
        .unwrap();
        for case in vectors["cases"].as_array().unwrap() {
            let projected = dependencies(&case["plan"]);
            assert_eq!(
                canonical(&projected).unwrap(),
                case["expected_canonical_utf8"]
            );
            assert_eq!(digest(&projected).unwrap(), case["expected_sha256"]);
        }
    }
}

/// Shared persistence for an already-authorized immutable plan. Only the manual
/// command validates a human Confirmation; source authorization is a distinct use case.
pub(super) async fn create_authorized_task(
    tx: &mut Tx<'_>,
    principal: Uuid,
    p: &Value,
) -> Result<Value> {
    let plan = &p["plan"];
    let unit: Option<Uuid> =
        serde_json::from_value(plan["business_unit_id"].clone()).map_err(|_| Error::Conflict)?;
    let current = plan["scheme_snapshot"].clone();
    require(current.is_object(), "shared_send_policy_invalid")?;
    let items = plan["items"].as_array().ok_or(Error::Conflict)?;
    let id = Uuid::now_v7();
    let snap = p["source_snapshot_id"]
        .as_str()
        .and_then(|v| v.parse::<Uuid>().ok())
        .unwrap_or_else(Uuid::now_v7);
    let message = plan["message"].as_str().ok_or(Error::Conflict)?;
    if p.get("source_snapshot_id").is_none() {
        sqlx::query("INSERT INTO distribution_message_snapshots(id,channel,content,content_hash) VALUES($1,'mobile',$2,$3)").bind(snap).bind(message).bind(hash(message)).execute(&mut **tx).await?;
    } else {
        require(sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM distribution_message_snapshots WHERE id=$1  AND channel='mobile' AND content=$2 AND content_hash=$3)").bind(snap).bind(message).bind(hash(message)).fetch_one(&mut **tx).await?,"source_snapshot_mismatch")?;
    }
    // Initial reconciliation runs below, so persist the actual authorization
    // kind before creating attempts; an automatic task is never briefly manual.
    let authorization_kind =
        if plan["authorization_kind"] == "source_route" && p.get("source_snapshot_id").is_some() {
            "source_route"
        } else {
            "manual"
        };
    sqlx::query("INSERT INTO mobile_v9_tasks(id,draft_id,intent_id,preview_id,confirmed_by,confirmed_until,authorization_kind,business_unit_id,scheme_snapshot) VALUES($1,$2,$3,$4,$5,clock_timestamp()+make_interval(secs=>$6::double precision),$7,$8,$9)").bind(id).bind(uid(plan,"draft_id")?).bind(uid(plan,"intent_id")?).bind(uid(p,"id")?).bind(principal).bind(current["confirmation_seconds"].as_i64().ok_or(Error::Conflict)? as f64).bind(authorization_kind).bind(unit).bind(&current).execute(&mut **tx).await?;
    if let Some(not_before) = plan["not_before"].as_str() {
        sqlx::query("UPDATE mobile_v9_tasks SET next_eligible_at=greatest(next_eligible_at,$2::text::timestamptz) WHERE id=$1").bind(id).bind(not_before).execute(&mut **tx).await?;
    }
    for item in items {
        let iid = Uuid::now_v7();
        let pid = Uuid::now_v7();
        let aid = Uuid::now_v7();
        let mut frozen = item["selected"].clone();
        frozen["schema"] = json!(SCHEMA);

        frozen["intent_id"] = plan["intent_id"].clone();
        frozen["item_id"] = json!(iid);
        frozen["attempt_id"] = json!(aid);
        frozen["message"] = json!(message);
        frozen["content_digest"] = plan["content_digest"].clone();
        let minimum = ["total", "device", "target"]
            .iter()
            .filter_map(|k| current["limits"][k]["interval_seconds"].as_i64())
            .max()
            .unwrap_or(1);
        frozen["between_targets_seconds"] = json!(
            plan["between_targets_seconds"]
                .as_i64()
                .ok_or(Error::Conflict)?
                .max(minimum)
        );
        frozen["policy_id"] = current["id"].clone();
        frozen["selection_policy"] = json!("exact_unique");
        sqlx::query("INSERT INTO mobile_v9_task_items(id,task_id,intent_id,target_id,position) VALUES($1,$2,$3,$4,$5)").bind(iid).bind(id).bind(uid(plan,"intent_id")?).bind(uid(item,"target_id")?).bind(item["position"].as_i64().ok_or(Error::Conflict)? as i32).execute(&mut **tx).await?;
        sqlx::query("INSERT INTO mobile_v9_plans(id,item_id,target_id,device_id,snapshot_id,plan,digest,content_digest) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(pid).bind(iid).bind(uid(item,"target_id")?).bind(uid(&frozen,"device_id")?).bind(snap).bind(&frozen).bind(digest(&frozen)?).bind(plan["content_digest"].as_str()).execute(&mut **tx).await?;
        sqlx::query("INSERT INTO mobile_v9_attempts(id,item_id,plan_id,attempt_no,confirmed_by) VALUES($1,$2,$3,1,$4)").bind(aid).bind(iid).bind(pid).bind(principal).execute(&mut **tx).await?;
        let mut pool: Vec<Value> = item["candidates"]
            .as_array()
            .ok_or(Error::Conflict)?
            .iter()
            .filter(|c| c["confirmable"] == true)
            .cloned()
            .collect();
        pool.sort_by_key(|c| c["device_id"] != item["selected"]["device_id"]);
        require(!pool.is_empty(), "shared_send_selection_invalid")?;
        sqlx::query("INSERT INTO mobile_v9_dispatch_scopes(item_id,candidates) VALUES($1,$2)")
            .bind(iid)
            .bind(json!(pool))
            .execute(&mut **tx)
            .await?;
        sqlx::query("INSERT INTO mobile_v9_dispatch_state(item_id,candidate_cursor) VALUES($1,1)")
            .bind(iid)
            .execute(&mut **tx)
            .await?;
        records::ensure_task(tx, id, iid, uid(item, "target_id")?, snap, &frozen).await?;
        dispatch::reconcile(tx, iid).await?;
    }
    task(tx, id).await
}
