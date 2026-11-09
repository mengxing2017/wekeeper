use super::*;
/// Every phone receives the same business group name; bindings only select authorized devices.
pub(super) async fn raw_candidates(tx: &mut Tx<'_>, s: Uuid, target: Uuid) -> Result<Vec<Value>> {
    sqlx::query_scalar(r#"SELECT jsonb_build_object(
 'target_id',t.id,'target_name',t.search_text,'device_name',d.name,'device_id',d.id,'binding_id',b.id,'binding_locator_revision',b.locator_revision,
 'target_locator_revision',t.locator_revision,
 'target',jsonb_build_object('kind','group','search_query',t.search_text,'match_policy','exact_unique','identity_constraints','[]'::jsonb),
 'access_revision',CASE WHEN b.source='school_pool' THEN (SELECT revision FROM mobile_shared_school_access WHERE school_id=b.school_id AND device_id=b.device_id) ELSE NULL END,
 'credential_generation',d.credential_generation,'delegation_id',m.id,'accepted',coalesce(c.accepted,false),'consent_state',CASE WHEN c.accepted IS NULL THEN 'unknown' WHEN c.accepted THEN 'accepted' ELSE 'revoked' END,
 'execution',jsonb_build_object('contract','device_owned/1','capability','wecom.group_text'))
 FROM mobile_device_target_bindings b JOIN mobile_targets t ON t.id=b.target_id AND t.school_id=b.school_id
 JOIN mobile_workspaces s ON s.id=t.school_id AND s.active JOIN execution_devices d ON d.id=b.device_id
 LEFT JOIN mobile_shared_delegations m ON m.device_id=d.id AND m.credential_generation=d.credential_generation
 LEFT JOIN LATERAL(SELECT * FROM mobile_shared_delegation_decisions c WHERE c.delegation_id=m.id ORDER BY revision DESC LIMIT 1)c ON true
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
        "target",
        "credential_generation",
    ] {
        deps.insert(k.into(), row[k].clone());
    }
    Value::Object(deps)
}
pub(super) async fn grant(tx: &mut Tx<'_>, s: Uuid, row: &Value) -> Result<Option<Value>> {
    // The grant is an execution and metering projection keyed on the phone's real
    // locator identity, never on a generic device or access revision: re-enabling a
    // resource must not permanently invalidate an otherwise valid business scope.
    sqlx::query_scalar("SELECT to_jsonb(g) FROM mobile_v9_scope_grants g WHERE g.school_id=$1 AND $2=ANY(g.device_ids) AND ($3=ANY(g.target_ids) OR g.auto_targets) AND ($4::uuid IS NULL OR g.id=$4) AND g.expires_at>clock_timestamp() AND (EXISTS(SELECT 1 FROM mobile_usage_scopes us JOIN mobile_usages u ON u.id=us.usage_id WHERE us.scope_id=g.id AND u.approval='approved' AND u.state='active' AND u.expires_at>clock_timestamp() AND NOT EXISTS(SELECT 1 FROM mobile_usages n WHERE n.school_id=u.school_id AND n.budget_root=u.budget_root AND n.approval='approved' AND (n.created_at,n.id)>(u.created_at,u.id)) AND EXISTS(SELECT 1 FROM execution_devices ud JOIN mobile_device_target_bindings ub ON ub.device_id=ud.id JOIN mobile_targets ut ON ut.id=ub.target_id WHERE ud.id=$2 AND ut.id=$3 AND ub.school_id=$1 AND us.resource_snapshot=jsonb_build_object('binding_locator_revision',ub.locator_revision,'target_locator_revision',ut.locator_revision,'target',jsonb_build_object('kind','group','search_query',ut.search_text,'match_policy','exact_unique','identity_constraints','[]'::jsonb))))) AND NOT EXISTS(SELECT 1 FROM mobile_v9_scope_revocations r WHERE r.grant_id=g.id) ORDER BY g.created_at DESC,g.id DESC LIMIT 1")
    .bind(s).bind(uid(row,"device_id")?).bind(uid(row,"target_id")?).bind(row.get("scope_id").map(|_|uid(row,"scope_id")).transpose()?).fetch_optional(&mut **tx).await.map_err(Into::into)
}
/// Current business control state for a school/device/target, resolved from the
/// operative approved usage version of each budget root. Pending, declined and
/// superseded applications are history and never act as an implicit disable; only
/// an explicit pause, revocation or a real expiry on the operative version blocks.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum ScopeControl {
    /// No operative approved usage, or an active unexpired one: the effective
    /// binding (platform, or enabled school-pool access) authorizes this target.
    Authorized,
    Paused,
    Revoked,
    Expired,
}

pub(super) async fn scope_control(
    tx: &mut Tx<'_>,
    s: Uuid,
    device: Uuid,
    target: Uuid,
) -> Result<ScopeControl> {
    let usages = operative_usages(tx, s, device, target).await?;
    Ok(scope_decision(&usages))
}

fn scope_decision(usages: &[Value]) -> ScopeControl {
    let control_time = |u: &Value| {
        u["control_at"]
            .as_str()
            .and_then(|s| s.parse::<chrono::DateTime<chrono::Utc>>().ok())
            .unwrap_or(chrono::DateTime::<chrono::Utc>::MIN_UTC)
    };
    let latest_live = usages
        .iter()
        .filter(|u| {
            u["state"] == "active" && u["expired"] == false && u["projection_revoked"] != true
        })
        .map(control_time)
        .max();
    // A later explicit reopening can replace an older revoked business root.
    // Conversely, a pause/revocation performed after that reopening still wins.
    let current_disable = |u: &&Value| latest_live.is_none_or(|t| control_time(u) >= t);
    if usages
        .iter()
        .filter(current_disable)
        .any(|u| u["state"] == "revoked" || u["projection_revoked"] == true)
    {
        ScopeControl::Revoked
    } else if usages
        .iter()
        .filter(current_disable)
        .any(|u| u["state"] == "paused")
    {
        ScopeControl::Paused
    } else if latest_live.is_some() {
        ScopeControl::Authorized
    } else if usages
        .iter()
        .any(|u| u["state"] == "active" && u["expired"] == true)
    {
        ScopeControl::Expired
    } else {
        ScopeControl::Authorized
    }
}

/// Resolve current approved versions before matching resources, so a superseded
/// record cannot keep controlling a target removed from its successor.
pub(super) async fn operative_usages(
    tx: &mut Tx<'_>,
    school: Uuid,
    device: Uuid,
    target: Uuid,
) -> Result<Vec<Value>> {
    sqlx::query_scalar("SELECT to_jsonb(u)||jsonb_build_object('expired',u.expires_at<=clock_timestamp(),'control_at',greatest(u.created_at,coalesce((SELECT max(e.created_at) FROM mobile_usage_events e JOIN mobile_usages eu ON eu.id=e.usage_id WHERE eu.school_id=u.school_id AND eu.budget_root=u.budget_root AND e.action IN ('pause','resume','revoke','approve','approved')),u.created_at),coalesce((SELECT max(r.created_at) FROM mobile_usage_scopes m JOIN mobile_v9_scope_revocations r ON r.grant_id=m.scope_id WHERE m.usage_id=u.id AND m.device_id=$2 AND m.target_id=$3),u.created_at)),'projection_revoked',EXISTS(SELECT 1 FROM mobile_usage_scopes m JOIN mobile_v9_scope_revocations r ON r.grant_id=m.scope_id WHERE m.usage_id=u.id AND m.device_id=$2 AND m.target_id=$3)) FROM mobile_usages u WHERE u.school_id=$1 AND u.approval='approved' AND (u.body->'device_ids' @> jsonb_build_array($2::uuid) AND (u.body->'auto_targets'='true'::jsonb OR EXISTS(SELECT 1 FROM jsonb_array_elements(u.body->'targets') t WHERE t->>'id'=$3::uuid::text))) AND NOT EXISTS(SELECT 1 FROM mobile_usages n WHERE n.school_id=u.school_id AND n.budget_root=u.budget_root AND n.approval='approved' AND (n.created_at,n.id)>(u.created_at,u.id))")
        .bind(school).bind(device).bind(target).fetch_all(&mut **tx).await.map_err(Into::into)
}

/// A platform binding (or enabled school-pool access) already authorizes this
/// school/device/target. The internal usage grant is an execution and metering
/// projection, not an extra opening step: a missing or drifted projection never
/// blocks a currently authorized task. Only a real pause, revocation or expiry on
/// the operative business usage version is enforced here.
pub(super) async fn validate_scope(tx: &mut Tx<'_>, s: Uuid, plan: &Value) -> Result<()> {
    // Current explicit controls always win, even if an older grant still matches.
    match scope_control(tx, s, uid(plan, "device_id")?, uid(plan, "target_id")?).await? {
        ScopeControl::Authorized => Ok(()),
        ScopeControl::Paused => Err(Error::Policy("shared_send_usage_paused")),
        ScopeControl::Revoked => Err(Error::Policy("shared_send_usage_revoked")),
        ScopeControl::Expired => Err(Error::Policy("shared_send_usage_expired")),
    }
}

// Validate platform authorization and frozen dependencies only. Target identity is interpreted
// by the device after delivery; missing execution metadata must not block task acceptance.
pub(super) async fn validate_plan(tx: &mut Tx<'_>, s: Uuid, plan: &Value) -> Result<()> {
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
    let choices = raw_candidates(tx, s, uid(plan, "target_id")?).await?;
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
    validate_scope(tx, s, plan).await?;
    Ok(())
}
pub(super) async fn validate_risk(tx: &mut Tx<'_>, s: Uuid, plan: &Value) -> Result<()> {
    let risk:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_v9_attempts a JOIN mobile_v9_task_items i ON i.id=a.item_id WHERE i.school_id=$1 AND i.target_id=$2 AND i.intent_id=$3 AND a.state IN ('observed_sent','manually_confirmed_sent'))").bind(s).bind(uid(plan,"target_id")?).bind(plan["intent_id"].as_str().and_then(|v|Uuid::parse_str(v).ok())).fetch_one(&mut **tx).await?;
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
            crate::distribution::domain::shared_mobile::validate_text(&v.reason, 1, 2000, 8000)?;
            require(v.school_id == Some(s), "shared_send_resource_invalid")?;
            let bound:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_device_target_bindings WHERE school_id=$1 AND device_id=$2)").bind(s).bind(v.device_id).fetch_one(&mut **tx).await?;
            require(bound, "shared_binding_missing")?;
            let id = Uuid::now_v7();
            sqlx::query("INSERT INTO mobile_v9_support_requests(id,device_id,school_id,reason,state,created_by) VALUES($1,$2,$3,$4,'submitted',$5)").bind(id).bind(v.device_id).bind(s).bind(v.reason).bind(a.id).execute(&mut **tx).await?;
            Ok(json!({"id":id,"state":"submitted"}))
        }

        SchoolCommand::PoolBinding(v) => {
            let allowed:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_shared_school_access a JOIN mobile_workspaces s ON s.id=a.school_id JOIN execution_devices d ON d.id=a.device_id JOIN mobile_targets t ON t.school_id=a.school_id WHERE a.school_id=$1 AND a.device_id=$2 AND a.enabled AND a.revision=$3 AND s.active AND d.enabled AND t.id=$4 AND t.enabled AND t.archived_at IS NULL)").bind(s).bind(v.device_id).bind(v.expected_access_revision).bind(v.target_id).fetch_one(&mut **tx).await?;
            require(allowed, "shared_pool_access_required")?;
            // Re-saving an enabled binding does not change its authorization revision.
            let id:Option<Uuid>=sqlx::query_scalar("INSERT INTO mobile_device_target_bindings(id,school_id,device_id,target_id,created_by,updated_by,source) VALUES($1,$2,$3,$4,$5,$5,'school_pool') ON CONFLICT(device_id,target_id) DO UPDATE SET enabled=true,updated_by=$5,revision=mobile_device_target_bindings.revision+CASE WHEN NOT mobile_device_target_bindings.enabled THEN 1 ELSE 0 END,locator_revision=mobile_device_target_bindings.locator_revision+CASE WHEN NOT mobile_device_target_bindings.enabled THEN 1 ELSE 0 END WHERE mobile_device_target_bindings.source='school_pool' RETURNING id").bind(Uuid::now_v7()).bind(s).bind(v.device_id).bind(v.target_id).bind(a.id).fetch_optional(&mut **tx).await?;
            let id = id.ok_or(Error::Policy("shared_binding_platform_managed"))?;
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
                let n=sqlx::query("UPDATE mobile_v9_task_drafts SET target_ids=$3,message=$4,content_digest=$5,between_targets_seconds=$7,revision=revision+1 WHERE school_id=$1 AND id=$2 AND revision=$6 AND NOT EXISTS(SELECT 1 FROM mobile_v9_tasks t WHERE t.draft_id=$2)").bind(s).bind(id).bind(input.target_ids).bind(&input.message).bind(hash(&input.message)).bind(input.expected_revision).bind(input.between_targets_seconds).execute(&mut **tx).await?.rows_affected();
                require(n == 1, "revision_conflict")?;
                id
            } else {
                require(input.expected_revision.is_none(), "revision_conflict")?;
                let id = Uuid::now_v7();
                sqlx::query("INSERT INTO mobile_v9_task_drafts(id,school_id,intent_id,actor_id,target_ids,message,content_digest,between_targets_seconds) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(id).bind(s).bind(Uuid::now_v7()).bind(a.id).bind(input.target_ids).bind(&input.message).bind(hash(&input.message)).bind(input.between_targets_seconds).execute(&mut **tx).await?;
                id
            };
            Ok(sqlx::query_scalar(
                "SELECT to_jsonb(d)||jsonb_build_object('targets',coalesce((SELECT jsonb_agg(jsonb_build_object('id',t.id,'name',t.search_text) ORDER BY ids.n) FROM unnest(d.target_ids) WITH ORDINALITY ids(id,n) JOIN mobile_targets t ON t.id=ids.id AND t.school_id=d.school_id),'[]'::jsonb)) FROM mobile_v9_task_drafts d WHERE id=$1",
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
                let mut confirmation_issues = std::collections::BTreeMap::new();
                for choice in &mut choices {
                    if let Some(g) = grant(tx, s, choice).await? {
                        choice["scope_id"] = g["id"].clone();
                    }
                    let issue = match validate_plan(tx, s, choice).await {
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
                let target_info:PreviewTarget = sqlx::query_scalar::<_,Value>("SELECT jsonb_build_object('id',id,'name',search_text,'search_text',search_text,'locator_revision',locator_revision) FROM mobile_targets WHERE id=$1 AND school_id=$2").bind(target).bind(s).fetch_optional(&mut **tx).await?.map(serde_json::from_value).transpose().map_err(|_|Error::Conflict)?.ok_or(Error::NotFound)?;
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
                            Some("shared_send_usage_paused") => {
                                "当前工作空间此手机与目标群的使用范围已暂停，请在使用管理恢复后再确认"
                            }
                            Some("shared_send_usage_revoked") => {
                                "当前工作空间此手机与目标群的使用范围已撤销，请在使用管理核对"
                            }
                            Some("shared_send_usage_expired") => {
                                "当前工作空间此手机与目标群的使用范围已到期，请在使用管理办理续期"
                            }
                            Some("shared_send_review_required") => {
                                "存在未核对的发送结果，请先核对原任务"
                            }
                            Some("plan_dependency_changed") => {
                                "手机或目标群关联已变化，请更新业务预览"
                            }
                            _ => "当前手机不满足任务受理条件，请核对当前工作空间使用范围",
                        },
                    )
                } else {
                    None
                };
                items.push(json!({"position":position,"target_id":target,"target":target_info,"selected":chosen,"candidates":choices,"readiness":target_status,"blocker":blocker}));
            }
            let p = policy(tx).await?;
            let id = Uuid::now_v7();
            let plan = json!({"dispatch_mode":"candidate_pool","schema":SCHEMA,"school_id":s,"intent_id":d["intent_id"],"draft_id":input.draft_id,"draft_revision":input.expected_revision,"between_targets_seconds":d["between_targets_seconds"],"message":d["message"],"content_digest":d["content_digest"],"items":items,"policy_id":p.as_ref().map(|p|&p["id"]),"sending_status":sending_status});
            let h = digest(&plan)?;
            sqlx::query("INSERT INTO mobile_v9_previews(id,school_id,draft_id,draft_revision,plan,digest,policy_id) VALUES($1,$2,$3,$4,$5,$6,$7)").bind(id).bind(s).bind(input.draft_id).bind(input.expected_revision).bind(&plan).bind(&h).bind(p.as_ref().map(policy_id).transpose()?.flatten()).execute(&mut **tx).await?;
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
            policy(tx)
                .await?
                .ok_or(Error::Policy("shared_send_not_released"))?;
            require(plan["dispatch_mode"] == "candidate_pool", "preview_expired")?;
            let items = plan["items"].as_array().ok_or(Error::Conflict)?;
            for item in items {
                require(
                    item["selected"]["execution"]["contract"] == "device_owned/1",
                    "preview_expired",
                )?;
                validate_plan(tx, s, &item["selected"]).await?;
                for candidate in item["candidates"]
                    .as_array()
                    .ok_or(Error::Conflict)?
                    .iter()
                    .filter(|c| c["confirmable"] == true)
                {
                    validate_plan(tx, s, candidate).await?;
                }
                require(items.len() <= 100, "shared_send_targets_invalid")?;
            }
            create_authorized_task(tx, a.id, s, &p).await
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
            validate_risk(tx, s, &old["plan"]).await?;
            let target = uid(&old, "target_id")?;
            let mut plan = candidates(tx, s, target)
                .await?
                .into_iter()
                .find(|x| x["device_id"] == json!(input.device_id))
                .ok_or(Error::Policy("plan_dependency_changed"))?;
            if let Some(scope) = grant(tx, s, &plan).await? {
                plan["scope_id"] = scope["id"].clone();
            }
            validate_plan(tx, s, &plan).await?;
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
            sqlx::query("UPDATE mobile_execution_grants g SET state='superseded' FROM mobile_v9_attempts a JOIN mobile_v9_task_items i ON i.id=a.item_id WHERE i.task_id=$1 AND g.attempt_id=a.id AND g.state='active'").bind(id).execute(&mut **tx).await?;
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
            if input.decision == "verified_not_sent" {
                let active:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_v9_invocations WHERE attempt_id=$1 AND execute_before>clock_timestamp())").bind(id).fetch_one(&mut **tx).await?;
                require(!active, "shared_send_review_required")?;
            }
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
            if input.decision == "manually_confirmed_sent" {
                sqlx::query("UPDATE mobile_v9_dispatch_state SET state='completed',reason='manually_confirmed_sent',next_retry_at=NULL,finished_at=coalesce(finished_at,clock_timestamp()),revision=revision+1,updated_at=clock_timestamp() WHERE item_id=(SELECT item_id FROM mobile_v9_attempts WHERE id=$1)").bind(id).execute(&mut **tx).await?;
                sqlx::query("UPDATE mobile_execution_grants g SET state='superseded' FROM mobile_v9_attempts a WHERE a.item_id=(SELECT item_id FROM mobile_v9_attempts WHERE id=$1) AND g.attempt_id=a.id AND g.state='active'").bind(id).execute(&mut **tx).await?;
            }
            Ok(json!({"id":rid,"attempt_id":id,"decision":input.decision}))
        }
    }
}

// Runtime readiness only breaks ties between candidates with valid business scope.
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
mod scope_control_tests {
    use super::*;
    #[test]
    fn explicit_controls_win_over_another_live_grant() {
        let active = json!({"state":"active","expired":false});
        assert_eq!(
            scope_decision(&[active.clone(), json!({"state":"paused"})]),
            ScopeControl::Paused
        );
        assert_eq!(
            scope_decision(&[active.clone(), json!({"state":"revoked"})]),
            ScopeControl::Revoked
        );
        assert_eq!(
            scope_decision(&[
                active,
                json!({"state":"active","expired":false,"projection_revoked":true})
            ]),
            ScopeControl::Revoked
        );
    }
    #[test]
    fn reopening_replaces_an_older_revocation_but_a_later_pause_still_wins() {
        let reopened =
            json!({"state":"active","expired":false,"control_at":"2026-11-03T12:00:00Z"});
        let revoked = json!({"state":"revoked","control_at":"2026-11-02T12:00:00Z"});
        assert_eq!(
            scope_decision(&[revoked, reopened.clone()]),
            ScopeControl::Authorized
        );
        let paused = json!({"state":"paused","control_at":"2026-11-03T13:00:00Z"});
        assert_eq!(scope_decision(&[reopened, paused]), ScopeControl::Paused);
    }
    #[test]
    fn live_business_outweighs_expired_history_but_not_an_explicit_disable() {
        let expired = json!({"state":"active","expired":true});
        assert_eq!(scope_decision(&[]), ScopeControl::Authorized);
        assert_eq!(
            scope_decision(std::slice::from_ref(&expired)),
            ScopeControl::Expired
        );
        assert_eq!(
            scope_decision(&[expired, json!({"state":"active","expired":false})]),
            ScopeControl::Authorized
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
    s: Uuid,
    p: &Value,
) -> Result<Value> {
    let plan = &p["plan"];
    let current = policy(tx).await?.ok_or(Error::Unavailable)?;
    let items = plan["items"].as_array().ok_or(Error::Conflict)?;
    let id = Uuid::now_v7();
    let snap = Uuid::now_v7();
    let message = plan["message"].as_str().ok_or(Error::Conflict)?;
    sqlx::query("INSERT INTO distribution_message_snapshots(id,school_id,channel,content,content_hash) VALUES($1,$2,'mobile',$3,$4)").bind(snap).bind(s).bind(message).bind(hash(message)).execute(&mut **tx).await?;
    sqlx::query("INSERT INTO mobile_v9_tasks(id,school_id,draft_id,intent_id,preview_id,confirmed_by,confirmed_until,dispatch_mode) VALUES($1,$2,$3,$4,$5,$6,clock_timestamp()+make_interval(secs=>$7::double precision),'candidate_pool')").bind(id).bind(s).bind(uid(plan,"draft_id")?).bind(uid(plan,"intent_id")?).bind(uid(p,"id")?).bind(principal).bind(current["confirmation_seconds"].as_i64().ok_or(Error::Conflict)? as f64).execute(&mut **tx).await?;
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
        frozen["selection_policy"] = json!("exact_unique");
        sqlx::query("INSERT INTO mobile_v9_task_items(id,school_id,task_id,intent_id,target_id,position) VALUES($1,$2,$3,$4,$5,$6)").bind(iid).bind(s).bind(id).bind(uid(plan,"intent_id")?).bind(uid(item,"target_id")?).bind(item["position"].as_i64().ok_or(Error::Conflict)? as i32).execute(&mut **tx).await?;
        sqlx::query("INSERT INTO mobile_v9_plans(id,school_id,item_id,target_id,device_id,snapshot_id,plan,digest,content_digest) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)").bind(pid).bind(s).bind(iid).bind(uid(item,"target_id")?).bind(uid(&frozen,"device_id")?).bind(snap).bind(&frozen).bind(digest(&frozen)?).bind(plan["content_digest"].as_str()).execute(&mut **tx).await?;
        sqlx::query("INSERT INTO mobile_v9_attempts(id,school_id,item_id,plan_id,attempt_no,confirmed_by) VALUES($1,$2,$3,$4,1,$5)").bind(aid).bind(s).bind(iid).bind(pid).bind(principal).execute(&mut **tx).await?;
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
        dispatch::reconcile(tx, iid).await?;
    }
    task(tx, s, id).await
}
