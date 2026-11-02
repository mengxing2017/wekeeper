use super::*;
/// Only platform-owned resources. Effective locator dependencies omit unused default names.
pub(super) async fn raw_candidates(tx: &mut Tx<'_>, s: Uuid, target: Uuid) -> Result<Vec<Value>> {
    sqlx::query_scalar(r#"SELECT jsonb_build_object(
 'target_id',t.id,'target_name',coalesce(nullif(t.alias,''),t.search_text),'device_name',d.name,'device_id',d.id,'binding_id',b.id,'binding_locator_revision',b.locator_revision,
 'target_locator_revision',CASE WHEN b.search_text_override IS NULL THEN t.locator_revision ELSE NULL END,
 'search_text',coalesce(b.search_text_override,t.search_text),'search_source',CASE WHEN b.search_text_override IS NULL THEN 'default' ELSE 'override' END,
 'access_revision',CASE WHEN b.source='school_pool' THEN (SELECT revision FROM mobile_shared_school_access WHERE school_id=b.school_id AND device_id=b.device_id) ELSE NULL END,
 'credential_generation',d.credential_generation,'delegation_id',m.id,'accepted',coalesce(c.accepted,false),
 'release_id',r.id,'flow_digest',r.content_digest,'flow',r.flow)
 FROM mobile_device_target_bindings b JOIN mobile_targets t ON t.id=b.target_id AND t.school_id=b.school_id
 JOIN schools s ON s.id=t.school_id AND s.active JOIN execution_devices d ON d.id=b.device_id
 LEFT JOIN mobile_shared_delegations m ON m.device_id=d.id AND m.credential_generation=d.credential_generation
 LEFT JOIN LATERAL(SELECT * FROM mobile_shared_delegation_decisions c WHERE c.delegation_id=m.id ORDER BY revision DESC LIMIT 1)c ON true
 LEFT JOIN LATERAL(SELECT r.* FROM mobile_v9_adapter_releases r WHERE NOT EXISTS(SELECT 1 FROM mobile_v9_adapter_withdrawals w WHERE w.release_id=r.id) ORDER BY r.created_at DESC,r.id DESC LIMIT 1) r ON true
 WHERE t.school_id=$1 AND t.id=$2 AND t.enabled AND t.archived_at IS NULL AND b.enabled AND d.enabled
 AND (b.source='platform' OR EXISTS(SELECT 1 FROM mobile_shared_school_access a WHERE a.school_id=b.school_id AND a.device_id=b.device_id AND a.enabled)) ORDER BY d.name,d.id"#).bind(s).bind(target).fetch_all(&mut **tx).await.map_err(Into::into)
}
pub(super) async fn candidates(tx: &mut Tx<'_>, s: Uuid, target: Uuid) -> Result<Vec<Value>> {
    raw_candidates(tx, s, target).await
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
        "release_id",
        "flow_digest",
    ] {
        deps.insert(k.into(), row[k].clone());
    }
    Value::Object(deps)
}
pub(super) async fn grant(
    tx: &mut Tx<'_>,
    s: Uuid,
    row: &Value,
    stage: &str,
) -> Result<Option<Value>> {
    sqlx::query_scalar("SELECT to_jsonb(g) FROM mobile_v9_scope_grants g WHERE g.school_id=$1 AND $2=ANY(g.device_ids) AND ($3=ANY(g.target_ids) OR g.auto_targets) AND (g.release_id IS NULL OR g.release_id=$4) AND g.stage=$5 AND ($6::uuid IS NULL OR g.id=$6) AND g.expires_at>clock_timestamp() AND (g.stage<>'production_allowlist' OR EXISTS(SELECT 1 FROM mobile_usage_scopes us JOIN mobile_usages u ON u.id=us.usage_id WHERE us.scope_id=g.id AND u.approval='approved' AND u.state='active' AND u.expires_at>clock_timestamp() AND EXISTS(SELECT 1 FROM mobile_v9_send_policy_versions bp WHERE bp.id=(SELECT id FROM mobile_v9_send_policy_versions ORDER BY revision DESC LIMIT 1) AND (u.body->'scheme')-'id'=jsonb_build_object('limits',bp.limits,'timezone',bp.timezone,'allowed_windows',bp.allowed_windows,'daily_max_calls',bp.limits->'school'->'daily_max_calls','interval_seconds',bp.limits->'target'->'interval_seconds')) AND EXISTS(SELECT 1 FROM execution_devices ud JOIN mobile_device_target_bindings ub ON ub.device_id=ud.id JOIN mobile_targets ut ON ut.id=ub.target_id WHERE ud.id=$2 AND ut.id=$3 AND ub.school_id=$1 AND us.resource_snapshot=jsonb_build_object('device_revision',ud.revision,'binding_locator_revision',ub.locator_revision,'target_locator_revision',CASE WHEN ub.search_text_override IS NULL THEN ut.locator_revision ELSE NULL END,'search_text',coalesce(ub.search_text_override,ut.search_text))))) AND NOT EXISTS(SELECT 1 FROM mobile_v9_scope_revocations r WHERE r.grant_id=g.id) AND (NOT (g.resource_revisions ? $2::uuid::text) OR (g.resource_revisions->>$2::uuid::text)::bigint=coalesce((SELECT revision FROM mobile_shared_school_access WHERE device_id=$2 AND school_id=$1 AND enabled),0)) ORDER BY g.created_at DESC,g.id DESC LIMIT 1")
    .bind(s).bind(uid(row,"device_id")?).bind(uid(row,"target_id")?).bind(row["release_id"].as_str().and_then(|v|v.parse::<Uuid>().ok())).bind(stage).bind(row.get("scope_id").map(|_|uid(row,"scope_id")).transpose()?).fetch_optional(&mut **tx).await.map_err(Into::into)
}
/// A business task can be delivered even when this product build lacks its UI rules.
pub(super) fn missing_rules(plan: &Value) -> bool {
    ["release_id", "flow", "flow_digest"]
        .iter()
        .all(|k| plan.get(*k).is_some_and(Value::is_null))
}
pub(super) async fn validate_plan(tx: &mut Tx<'_>, s: Uuid, plan: &Value) -> Result<()> {
    if plan["stage"] == "production_allowlist" {
        business_scheme::ensure_open(tx).await?;
    }

    let choices = raw_candidates(tx, s, uid(plan, "target_id")?).await?;
    let frozen = plan.get("attempt_id").is_some();
    require(
        choices.iter().any(|c| {
            let mut current = c.clone();
            if frozen {
                current["release_id"] = plan["release_id"].clone();
                current["flow_digest"] = plan["flow_digest"].clone();
            }
            dependencies(&current) == dependencies(plan)
        }),
        "plan_dependency_changed",
    )?;
    if frozen && !missing_rules(plan) {
        let release:Option<Value>=sqlx::query_scalar("SELECT jsonb_build_object('flow',r.flow,'digest',r.content_digest) FROM mobile_v9_adapter_releases r WHERE r.id=$1 AND NOT EXISTS(SELECT 1 FROM mobile_v9_adapter_withdrawals w WHERE w.release_id=r.id)").bind(uid(plan,"release_id")?).fetch_optional(&mut **tx).await?;
        require(
            release
                .is_some_and(|r| r["flow"] == plan["flow"] && r["digest"] == plan["flow_digest"]),
            "plan_dependency_changed",
        )?;
    }
    if !missing_rules(plan) {
        let flow: crate::distribution::domain::mobile_execution::FlowV3 =
            serde_json::from_value(plan["flow"].clone())
                .map_err(|_| Error::Policy("flow_invalid"))?;
        flow.validate()?;
        require(
            digest(&plan["flow"])? == plan["flow_digest"],
            "plan_dependency_changed",
        )?;
    }
    let p = policy(tx)
        .await?
        .ok_or(Error::Policy("shared_send_not_released"))?;
    let stage = plan["stage"]
        .as_str()
        .ok_or(Error::Policy("shared_send_stage_invalid"))?;
    require(
        p["mode"] == "enabled"
            && p["allowed_stages"]
                .as_array()
                .is_some_and(|a| a.contains(&json!(stage))),
        "shared_send_not_released",
    )?;
    let g = grant(tx, s, plan, stage)
        .await?
        .ok_or(Error::Policy("shared_send_scope_not_accepted"))?;
    require(
        plan.get("scope_id").is_none_or(|id| id == &g["id"]),
        "plan_dependency_changed",
    )?;
    let risk:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_v9_invocations WHERE device_id=$1 AND state='risk_held') OR EXISTS(SELECT 1 FROM mobile_shared_invocations WHERE device_id=$1 AND state='risk_held') OR EXISTS(SELECT 1 FROM mobile_shared_risk_guards WHERE school_id=$2 AND target_id=$3 AND state='active') OR EXISTS(SELECT 1 FROM mobile_v9_risk_guards WHERE school_id=$2 AND target_id=$3 AND state='active')").bind(uid(plan,"device_id")?).bind(s).bind(uid(plan,"target_id")?).fetch_one(&mut **tx).await?;
    require(!risk, "shared_send_review_required")
}
pub(super) async fn write(
    tx: &mut Tx<'_>,
    a: &Actor,
    s: Uuid,
    command: SchoolCommand,
) -> Result<Value> {
    match command {
        SchoolCommand::Support(v) => {
            crate::distribution::domain::mobile_execution::validate_text(&v.reason, 1, 2000, 8000)?;
            require(v.school_id == Some(s), "shared_send_resource_invalid")?;
            let bound:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_device_target_bindings WHERE school_id=$1 AND device_id=$2)").bind(s).bind(v.device_id).fetch_one(&mut **tx).await?;
            require(bound, "shared_binding_missing")?;
            let id = Uuid::now_v7();
            sqlx::query("INSERT INTO mobile_v9_support_requests(id,device_id,school_id,reason,state,created_by) VALUES($1,$2,$3,$4,'submitted',$5)").bind(id).bind(v.device_id).bind(s).bind(v.reason).bind(a.id).execute(&mut **tx).await?;
            Ok(json!({"id":id,"state":"submitted"}))
        }

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
                let n=sqlx::query("UPDATE mobile_v9_task_drafts SET target_ids=$3,message=$4,content_digest=$5,stage=$7,between_targets_seconds=$8,revision=revision+1 WHERE school_id=$1 AND id=$2 AND revision=$6 AND NOT EXISTS(SELECT 1 FROM mobile_v9_tasks t WHERE t.draft_id=$2)").bind(s).bind(id).bind(input.target_ids).bind(&input.message).bind(hash(&input.message)).bind(input.expected_revision).bind(&input.stage).bind(input.between_targets_seconds).execute(&mut **tx).await?.rows_affected();
                require(n == 1, "revision_conflict")?;
                id
            } else {
                require(input.expected_revision.is_none(), "revision_conflict")?;
                let id = Uuid::now_v7();
                sqlx::query("INSERT INTO mobile_v9_task_drafts(id,school_id,intent_id,actor_id,target_ids,message,content_digest,stage,between_targets_seconds) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)").bind(id).bind(s).bind(Uuid::now_v7()).bind(a.id).bind(input.target_ids).bind(&input.message).bind(hash(&input.message)).bind(&input.stage).bind(input.between_targets_seconds).execute(&mut **tx).await?;
                id
            };
            Ok(sqlx::query_scalar(
                "SELECT to_jsonb(d)||jsonb_build_object('targets',coalesce((SELECT jsonb_agg(jsonb_build_object('id',t.id,'name',coalesce(nullif(t.alias,''),t.search_text)) ORDER BY ids.n) FROM unnest(d.target_ids) WITH ORDINALITY ids(id,n) JOIN mobile_targets t ON t.id=ids.id AND t.school_id=d.school_id),'[]'::jsonb)) FROM mobile_v9_task_drafts d WHERE id=$1",
            )
            .bind(id)
            .fetch_one(&mut **tx)
            .await?)
        }
        SchoolCommand::Preview(input) => {
            let sending_status = business_scheme::status(tx).await?;
            let d:Value=sqlx::query_scalar("SELECT to_jsonb(d) FROM mobile_v9_task_drafts d WHERE school_id=$1 AND id=$2 AND revision=$3 AND NOT EXISTS(SELECT 1 FROM mobile_v9_tasks t WHERE t.draft_id=d.id)").bind(s).bind(input.draft_id).bind(input.expected_revision).fetch_optional(&mut **tx).await?.ok_or(Error::Conflict)?;
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
                let mut choices = candidates(tx, s, *target).await?;
                for choice in &mut choices {
                    choice["stage"] = d["stage"].clone();
                    if let Some(g) =
                        grant(tx, s, choice, d["stage"].as_str().ok_or(Error::Conflict)?).await?
                    {
                        choice["scope_id"] = g["id"].clone();
                    }
                    choice["confirmable"] = json!(validate_plan(tx, s, choice).await.is_ok());
                }
                let target_info:PreviewTarget = sqlx::query_scalar::<_,Value>("SELECT jsonb_build_object('id',id,'name',coalesce(nullif(alias,''),search_text),'search_text',search_text,'locator_revision',locator_revision) FROM mobile_targets WHERE id=$1 AND school_id=$2").bind(target).bind(s).fetch_optional(&mut **tx).await?.map(serde_json::from_value).transpose().map_err(|_|Error::Conflict)?.ok_or(Error::NotFound)?;
                let target_status: Vec<Value> = status
                    .iter()
                    .filter(|r| r["target_id"] == json!(target))
                    .cloned()
                    .collect();
                let selection = input.selections.iter().find(|v| v.target_id == *target);
                let chosen = if let Some(v) = selection {
                    choices
                        .iter()
                        .find(|c| c["device_id"] == json!(v.device_id))
                } else {
                    choices
                        .iter()
                        .find(|c| {
                            target_status
                                .iter()
                                .any(|r| r["device_id"] == c["device_id"] && r["ready"] == true)
                        })
                        .or_else(|| choices.first())
                };
                let blocker = if d["stage"] == "production_allowlist" && !sending_status.can_confirm
                {
                    Some(sending_status.message.as_str())
                } else if chosen.is_none() {
                    Some("请选择已绑定手机")
                } else if chosen.is_some_and(|c| c["confirmable"] != true) {
                    Some("请查看本校手机使用状态，核对使用授权和手机是否就绪")
                } else {
                    None
                };
                items.push(json!({"position":position,"target_id":target,"target":target_info,"selected":chosen,"candidates":choices,"readiness":target_status,"blocker":blocker}));
            }
            let p = policy(tx).await?;
            let id = Uuid::now_v7();
            let plan = json!({"schema":SCHEMA,"school_id":s,"intent_id":d["intent_id"],"draft_id":input.draft_id,"draft_revision":input.expected_revision,"stage":d["stage"],"between_targets_seconds":d["between_targets_seconds"],"message":d["message"],"content_digest":d["content_digest"],"items":items,"policy_id":p.as_ref().map(|p|&p["id"]),"sending_status":sending_status});
            let h = digest(&plan)?;
            sqlx::query("INSERT INTO mobile_v9_previews(id,school_id,draft_id,draft_revision,plan,digest,policy_id) VALUES($1,$2,$3,$4,$5,$6,$7)").bind(id).bind(s).bind(input.draft_id).bind(input.expected_revision).bind(&plan).bind(&h).bind(p.as_ref().map(|p|uid(p,"id")).transpose()?).execute(&mut **tx).await?;
            Ok(
                sqlx::query_scalar("SELECT to_jsonb(p) FROM mobile_v9_previews p WHERE id=$1")
                    .bind(id)
                    .fetch_one(&mut **tx)
                    .await?,
            )
        }
        SchoolCommand::Confirm(input) => {
            require(input.confirmed, "confirmation_required")?;
            let old: Option<Uuid> = sqlx::query_scalar(
                "SELECT t.id FROM mobile_v9_tasks t JOIN mobile_v9_previews p ON p.id=t.preview_id WHERE t.school_id=$1 AND t.preview_id=$2 AND p.digest=$3",
            )
            .bind(s)
            .bind(input.preview_id)
            .bind(&input.preview_digest)
            .fetch_optional(&mut **tx)
            .await?;
            if let Some(id) = old {
                return task(tx, s, id).await;
            }
            let p:Value=sqlx::query_scalar("SELECT to_jsonb(p) FROM mobile_v9_previews p JOIN mobile_v9_task_drafts d ON d.id=p.draft_id AND d.revision=p.draft_revision WHERE p.school_id=$1 AND p.id=$2 AND p.expires_at>clock_timestamp() AND p.digest=$3 FOR UPDATE OF d").bind(s).bind(input.preview_id).bind(input.preview_digest).fetch_optional(&mut **tx).await?.ok_or(Error::Policy("preview_expired"))?;
            let plan = &p["plan"];
            if plan["stage"] == "production_allowlist" {
                business_scheme::ensure_open(tx).await?;
            }
            let current = policy(tx)
                .await?
                .ok_or(Error::Policy("shared_send_not_released"))?;
            require(current["id"] == p["policy_id"], "plan_dependency_changed")?;
            let items = plan["items"].as_array().ok_or(Error::Conflict)?;
            for item in items {
                validate_plan(tx, s, &item["selected"]).await?;
                require(
                    items.len() as u64
                        <= item["selected"]["flow"]["max_targets"]
                            .as_u64()
                            .unwrap_or(100),
                    "shared_send_targets_invalid",
                )?;
            }
            let id = Uuid::now_v7();
            let snap = Uuid::now_v7();
            let message = plan["message"].as_str().ok_or(Error::Conflict)?;
            sqlx::query("INSERT INTO distribution_message_snapshots(id,school_id,channel,content,content_hash) VALUES($1,$2,'mobile',$3,$4)").bind(snap).bind(s).bind(message).bind(hash(message)).execute(&mut **tx).await?;
            sqlx::query("INSERT INTO mobile_v9_tasks(id,school_id,draft_id,intent_id,preview_id,confirmed_by,confirmed_until) VALUES($1,$2,$3,$4,$5,$6,clock_timestamp()+make_interval(secs=>$7::double precision))").bind(id).bind(s).bind(uid(plan,"draft_id")?).bind(uid(plan,"intent_id")?).bind(input.preview_id).bind(a.id).bind(current["confirmation_seconds"].as_i64().ok_or(Error::Conflict)? as f64).execute(&mut **tx).await?;
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
                frozen["stage"] = plan["stage"].clone();
                let minimum = ["global", "school", "device", "target"]
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
                frozen["policy_id"] = p["policy_id"].clone();
                frozen["selection_policy"] = json!("first_group_result");
                sqlx::query("INSERT INTO mobile_v9_task_items(id,school_id,task_id,intent_id,target_id,position) VALUES($1,$2,$3,$4,$5,$6)").bind(iid).bind(s).bind(id).bind(uid(plan,"intent_id")?).bind(uid(item,"target_id")?).bind(item["position"].as_i64().ok_or(Error::Conflict)? as i32).execute(&mut **tx).await?;
                sqlx::query("INSERT INTO mobile_v9_plans(id,school_id,item_id,target_id,device_id,snapshot_id,plan,digest,content_digest) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)").bind(pid).bind(s).bind(iid).bind(uid(item,"target_id")?).bind(uid(&frozen,"device_id")?).bind(snap).bind(&frozen).bind(digest(&frozen)?).bind(plan["content_digest"].as_str()).execute(&mut **tx).await?;
                sqlx::query("INSERT INTO mobile_v9_attempts(id,school_id,item_id,plan_id,attempt_no,confirmed_by) VALUES($1,$2,$3,$4,1,$5)").bind(aid).bind(s).bind(iid).bind(pid).bind(a.id).execute(&mut **tx).await?;
            }
            task(tx, s, id).await
        }
        SchoolCommand::Reassign(id, input) => {
            require(
                !input.reason.trim().is_empty() && input.reason.len() <= 2000,
                "shared_send_reason_required",
            )?;
            let old:Value=sqlx::query_scalar("SELECT to_jsonb(a)||jsonb_build_object('plan',p.plan,'snapshot_id',p.snapshot_id,'target_id',i.target_id,'task_id',i.task_id) FROM mobile_v9_attempts a JOIN mobile_v9_plans p ON p.id=a.plan_id JOIN mobile_v9_task_items i ON i.id=a.item_id JOIN mobile_v9_tasks t ON t.id=i.task_id WHERE a.id=$1 AND a.school_id=$2 AND a.current AND NOT t.stopped").bind(id).bind(s).fetch_optional(&mut **tx).await?.ok_or(Error::NotFound)?;
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
            plan["stage"] = old["plan"]["stage"].clone();
            let stage = plan["stage"]
                .as_str()
                .ok_or(Error::Policy("shared_send_stage_invalid"))?;
            let scope = grant(tx, s, &plan, stage)
                .await?
                .ok_or(Error::Policy("shared_send_scope_not_accepted"))?;
            plan["scope_id"] = scope["id"].clone();
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
                "stage",
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
            sqlx::query("INSERT INTO mobile_v9_plans(id,school_id,item_id,target_id,device_id,snapshot_id,plan,digest,content_digest) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)").bind(pid).bind(s).bind(uid(&old,"item_id")?).bind(target).bind(input.device_id).bind(uid(&old,"snapshot_id")?).bind(&plan).bind(digest(&plan)?).bind(plan["content_digest"].as_str()).execute(&mut **tx).await?;
            sqlx::query("INSERT INTO mobile_v9_attempts(id,school_id,item_id,plan_id,attempt_no,confirmed_by) VALUES($1,$2,$3,$4,$5,$6)").bind(aid).bind(s).bind(uid(&old,"item_id")?).bind(pid).bind(old["attempt_no"].as_i64().ok_or(Error::Conflict)? as i32+1).bind(a.id).execute(&mut **tx).await?;
            sqlx::query("INSERT INTO mobile_v9_reassignments(id,school_id,old_attempt_id,new_attempt_id,actor_id,reason) VALUES($1,$2,$3,$4,$5,$6)").bind(Uuid::now_v7()).bind(s).bind(id).bind(aid).bind(a.id).bind(input.reason).execute(&mut **tx).await?;
            task(tx, s, uid(&old, "task_id")?).await
        }
        SchoolCommand::Stop(id, input) => {
            require(
                !input.reason.trim().is_empty() && input.reason.len() <= 2000,
                "shared_send_reason_required",
            )?;
            let n=sqlx::query("UPDATE mobile_v9_tasks SET stopped=true,revision=revision+1 WHERE school_id=$1 AND id=$2 AND revision=$3").bind(s).bind(id).bind(input.expected_revision).execute(&mut **tx).await?.rows_affected();
            require(n == 1, "revision_conflict")?;
            sqlx::query("UPDATE mobile_v9_attempts a SET state='cancelled',revision=revision+1 FROM mobile_v9_task_items i WHERE i.id=a.item_id AND i.task_id=$1 AND a.state='queued'").bind(id).execute(&mut **tx).await?;
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
            let row=sqlx::query("SELECT id,revision,state FROM mobile_v9_attempts WHERE school_id=$1 AND id=$2 FOR UPDATE").bind(s).bind(id).fetch_optional(&mut **tx).await?.ok_or(Error::NotFound)?;
            require(
                row.get::<i64, _>("revision") == input.expected_revision
                    && row.get::<String, _>("state") == "unknown",
                "revision_conflict",
            )?;
            let rid = Uuid::now_v7();
            sqlx::query("INSERT INTO mobile_v9_reviews(id,school_id,attempt_id,reviewer,decision,evidence) VALUES($1,$2,$3,$4,$5,$6)").bind(rid).bind(s).bind(id).bind(a.id).bind(&input.decision).bind(&input.evidence).execute(&mut **tx).await?;
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
            Ok(json!({"id":rid,"attempt_id":id,"decision":input.decision}))
        }
    }
}

#[cfg(test)]
mod rule_availability_tests {
    use super::*;
    #[test]
    fn absent_rules_are_distinct_from_corrupt_or_partial_rules() {
        let missing = json!({"release_id":null,"flow":null,"flow_digest":null});
        assert!(missing_rules(&missing));
        assert!(!missing_rules(&json!({})));
        let mut partial = missing.clone();
        partial["flow_digest"] = json!("invalid");
        assert!(!missing_rules(&partial));
        partial = missing;
        partial["flow"] = json!({"invalid":true});
        assert!(!missing_rules(&partial));
    }
}
