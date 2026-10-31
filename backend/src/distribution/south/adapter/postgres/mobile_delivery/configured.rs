//! Versioned configuration and strictly ordered runs; uses the same delivery transaction guard.
use super::*;

pub(super) fn is_configured(i: &Value) -> bool {
    matches!(
        i["snapshot"]["requirements"]["protocol_version"].as_u64(),
        Some(3 | 4)
    )
}
pub(super) fn config(i: &Value) -> Result<ConfigurationInput> {
    serde_json::from_value(i["snapshot"]["configuration"]["configuration"].clone())
        .map_err(|_| Error::Policy("configuration_missing"))
}
pub(super) async fn release(
    tx: &mut Tx<'_>,
    release: Uuid,
    school: Uuid,
    device: Uuid,
    new_run: bool,
) -> Result<Value> {
    let r: Value=sqlx::query_scalar("SELECT to_jsonb(r)||jsonb_build_object('configuration',v.configuration,'content_digest',v.content_digest,'name',v.name) FROM client_rule_releases r JOIN client_rule_bundle_versions v ON v.id=r.configuration_id WHERE r.id=$1 AND r.school_id=$2 AND (r.device_id IS NULL OR r.device_id=$3) AND NOT r.revoked AND (NOT $4 OR r.active)").bind(release).bind(school).bind(device).bind(new_run).fetch_optional(&mut **tx).await?.ok_or(Error::Policy("configuration_not_available"))?;
    let c: ConfigurationInput =
        serde_json::from_value(r["configuration"].clone()).map_err(|_| Error::Conflict)?;
    c.validate()?;
    require(
        hash(&r["configuration"].to_string()) == strv(&r, "content_digest")?,
        "规则包摘要不匹配",
    )?;
    Ok(r)
}
pub(super) async fn create(
    tx: &mut Tx<'_>,
    p: &Principal,
    input: ConfigurationInput,
) -> Result<Value> {
    let actor = platform(tx, p).await?;
    input.validate()?;
    let id = Uuid::now_v7();
    let body = json!(input);
    sqlx::query("INSERT INTO client_rule_bundle_versions(id,name,configuration,content_digest,created_by) VALUES($1,$2,$3,$4,$5)").bind(id).bind(strv(&body,"name")?).bind(&body).bind(hash(&body.to_string())).bind(actor).execute(&mut **tx).await?;
    object(tx, "client_rule_bundle_versions", id).await
}
pub(super) async fn publish(
    tx: &mut Tx<'_>,
    p: &Principal,
    s: &Scope,
    rev: i64,
    school: Uuid,
    device_id: Option<Uuid>,
    evidence: &str,
) -> Result<Value> {
    let actor = platform(tx, p).await?;
    text(evidence, 1, 500)?;
    let v = object(tx, "client_rule_bundle_versions", id(s)?).await?;
    revision(&v, rev)?;
    let c: ConfigurationInput =
        serde_json::from_value(v["configuration"].clone()).map_err(|_| Error::Conflict)?;
    c.validate()?;
    sqlx::query("SELECT id FROM schools WHERE id=$1 AND active")
        .bind(school)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(Error::Forbidden)?;
    if let Some(d) = device_id {
        let b = binding(tx, d).await?;
        let a = object(tx, "sending_accounts", uid(&b, "sender_account_id")?).await?;
        school_auth(tx, uid(&a, "responsible_actor_id")?, school, true).await?;
    }
    // Supersede only the same environment/scope. Other installed versions remain usable.
    sqlx::query("UPDATE client_rule_releases r SET active=false,revision=r.revision+1 FROM client_rule_bundle_versions v WHERE r.configuration_id=v.id AND r.school_id=$1 AND r.device_id IS NOT DISTINCT FROM $2 AND r.active AND v.configuration->'rules'->'client'=$3").bind(school).bind(device_id).bind(&v["configuration"]["rules"]["client"]).execute(&mut **tx).await?;
    let release = Uuid::now_v7();
    sqlx::query("INSERT INTO client_rule_releases(id,configuration_id,school_id,device_id,evidence_ref,created_by) VALUES($1,$2,$3,$4,$5,$6)").bind(release).bind(id(s)?).bind(school).bind(device_id).bind(evidence).bind(actor).execute(&mut **tx).await?;
    object(tx, "client_rule_releases", release).await
}
pub(super) async fn revoke(
    tx: &mut Tx<'_>,
    p: &Principal,
    s: &Scope,
    rev: i64,
    reason: &str,
) -> Result<Value> {
    platform(tx, p).await?;
    text(reason, 1, 1000)?;
    let old = object(tx, "client_rule_releases", id(s)?).await?;
    revision(&old, rev)?;
    sqlx::query(
        "UPDATE client_rule_releases SET active=false,revoked=true,revision=revision+1 WHERE id=$1",
    )
    .bind(id(s)?)
    .execute(&mut **tx)
    .await?;
    object(tx, "client_rule_releases", id(s)?).await
}
pub(super) async fn resolve(
    tx: &mut Tx<'_>,
    p: &Principal,
    package: &str,
    version: &str,
    code: i64,
    engine: &str,
    capabilities: &[String],
) -> Result<Value> {
    require(p.protocol() == 3, "配置解析要求 v3")?;
    let (d, _) = device(tx, p).await?;
    let b = binding(tx, d).await?;
    require(
        engine == ENGINE_VERSION
            && ENGINE_CAPABILITIES
                .iter()
                .all(|c| capabilities.iter().any(|v| v == c)),
        "引擎能力不支持",
    )?;
    // A confirmed run keeps its frozen policy even after a newer release supersedes it.
    let owned:Option<Value>=sqlx::query_scalar("SELECT to_jsonb(p)||jsonb_build_object('school_id',r.school_id) FROM mobile_delivery_run_progress p JOIN mobile_delivery_strategy_runs r ON r.id=p.id JOIN mobile_delivery_tasks t ON t.id=r.task_id WHERE r.device_id=$1 AND t.state='confirmed' AND EXISTS(SELECT 1 FROM mobile_delivery_run_items m WHERE m.run_id=r.id AND m.position>=p.current_position) ORDER BY p.confirmed_at NULLS LAST,r.id LIMIT 1").bind(d).fetch_optional(&mut **tx).await?;
    if let Some(owned) = owned {
        let r = release(
            tx,
            uid(&owned, "configuration_release_id")?,
            uid(&owned, "school_id")?,
            d,
            false,
        )
        .await?;
        let client = &r["configuration"]["rules"]["client"];
        require(
            client["package_name"] == package
                && client["version_name"] == version
                && client["version_code"] == code,
            "在途任务规则不匹配安装版本，请先修订剩余项",
        )?;
        return Ok(r);
    }
    let rows:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(r)||jsonb_build_object('configuration',v.configuration,'content_digest',v.content_digest,'name',v.name) FROM client_rule_releases r JOIN client_rule_bundle_versions v ON v.id=r.configuration_id JOIN sending_accounts a ON a.id=$1 JOIN school_memberships m ON m.account_id=a.responsible_actor_id AND m.school_id=r.school_id AND m.role='admin' WHERE r.active AND NOT r.revoked AND (r.device_id IS NULL OR r.device_id=$2) AND v.configuration->'rules'->'client'->>'package_name'=$3 AND v.configuration->'rules'->'client'->>'version_name'=$4 AND (v.configuration->'rules'->'client'->>'version_code')::bigint=$5 ORDER BY r.id").bind(uid(&b,"sender_account_id")?).bind(d).bind(package).bind(version).bind(code).fetch_all(&mut **tx).await?;
    let specific: Vec<_> = rows
        .iter()
        .filter(|r| r["device_id"] == d.to_string())
        .collect();
    let candidates: Vec<_> = if specific.is_empty() {
        rows.iter().collect()
    } else {
        specific
    };
    require(
        candidates.len() == 1,
        "无唯一兼容的已发布规则，请在 WeKeeper 配置适配",
    )?;
    let r = candidates[0].clone();
    sqlx::query("UPDATE execution_devices SET execution_protocol_version=3,supported_protocol_versions=ARRAY[1,2,3] WHERE id=$1").bind(d).execute(&mut **tx).await?;
    Ok(r)
}
pub(super) async fn download(tx: &mut Tx<'_>, p: &Principal, s: &Scope) -> Result<Value> {
    let (d, _) = device(tx, p).await?;
    let r = object(tx, "client_rule_releases", id(s)?).await?;
    let b = binding(tx, d).await?;
    let a = object(tx, "sending_accounts", uid(&b, "sender_account_id")?).await?;
    school_auth(
        tx,
        uid(&a, "responsible_actor_id")?,
        uid(&r, "school_id")?,
        true,
    )
    .await?;
    release(tx, id(s)?, uid(&r, "school_id")?, d, false).await
}
pub(super) async fn progress(tx: &mut Tx<'_>, p: &Principal, s: &Scope) -> Result<Value> {
    school_auth(tx, p.actor()?, school(s)?, true).await?;
    let r = scoped_object(tx, s, "mobile_delivery_strategy_runs").await?;
    let mut v = object(tx, "mobile_delivery_run_progress", uid(&r, "id")?).await?;
    let members:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(i)||jsonb_build_object('position',r.position) FROM mobile_delivery_run_items r JOIN mobile_delivery_items i ON i.id=r.item_id WHERE r.run_id=$1 ORDER BY r.position").bind(id(s)?).fetch_all(&mut **tx).await?;
    v["items"] = json!(members);
    v["server_time"] = json!(db_now(tx).await?);
    Ok(v)
}
pub(super) async fn validate(tx: &mut Tx<'_>, i: &Value) -> Result<()> {
    let run = uid(&i["snapshot"]["execution_target"], "run_id")?;
    let progress = object(tx, "mobile_delivery_run_progress", run).await?;
    require(
        progress["pause_reason"].is_null(),
        "顺序执行已暂停，需要处理当前项",
    )?;
    let pos: i32 = sqlx::query_scalar(
        "SELECT position FROM mobile_delivery_run_items WHERE item_id=$1 AND run_id=$2",
    )
    .bind(uid(i, "id")?)
    .bind(run)
    .fetch_one(&mut **tx)
    .await?;
    if progress["current_position"] != pos {
        return Err(Error::Policy("out_of_order"));
    }
    validate_configuration(tx, i).await?;
    let ready:bool=sqlx::query_scalar("SELECT next_eligible_at IS NULL OR next_eligible_at<=clock_timestamp() FROM mobile_delivery_run_progress WHERE id=$1").bind(run).fetch_one(&mut **tx).await?;
    if !ready {
        return Err(Error::Policy("group_interval_active"));
    }
    let previous:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_delivery_run_items r JOIN mobile_delivery_items i ON i.id=r.item_id WHERE r.run_id=$1 AND r.position<$2 AND i.state NOT IN ('ui_observed_sent','manually_sent'))").bind(run).bind(pos).fetch_one(&mut **tx).await?;
    if previous {
        return Err(Error::Policy("previous_group_not_completed"));
    }
    Ok(())
}
pub(super) async fn validate_configuration(tx: &mut Tx<'_>, i: &Value) -> Result<()> {
    automation::validate_manifest(tx, i).await?;
    let progress = object(
        tx,
        "mobile_delivery_run_progress",
        uid(&i["snapshot"]["execution_target"], "run_id")?,
    )
    .await?;
    let r = release(
        tx,
        uid(&progress, "configuration_release_id")?,
        uid(i, "school_id")?,
        uid(&i["snapshot"]["execution_target"], "device_id")?,
        false,
    )
    .await?;
    if r["content_digest"] != i["snapshot"]["configuration"]["content_digest"]
        || progress["execution_revision"] != i["snapshot"]["execution_revision"]
    {
        return Err(Error::Policy("configuration_changed"));
    }
    Ok(())
}
pub(super) async fn advance(tx: &mut Tx<'_>, i: &Value, sent: bool) -> Result<()> {
    if !is_configured(i) {
        return Ok(());
    }
    let run = uid(&i["snapshot"]["execution_target"], "run_id")?;
    if !sent {
        sqlx::query("UPDATE mobile_delivery_run_progress SET pause_reason='unknown_requires_review',revision=revision+1 WHERE id=$1").bind(run).execute(&mut **tx).await?;
        return Ok(());
    }
    let p = config(i)?.policy;
    sqlx::query("UPDATE mobile_delivery_run_progress p SET current_position=p.current_position+1,last_completed_at=i.observed_at,next_eligible_at=i.observed_at+greatest(p.inter_group_interval_ms,$3,$4)*interval '1 millisecond',revision=p.revision+1 FROM mobile_delivery_run_items r JOIN mobile_delivery_items i ON i.id=r.item_id WHERE p.id=$1 AND r.item_id=$2 AND r.run_id=p.id AND r.position=p.current_position").bind(run).bind(uid(i,"id")?).bind(p.account_cooldown_ms).bind(p.content_cooldown_ms).execute(&mut **tx).await?;
    Ok(())
}
pub(super) async fn run(
    tx: &mut Tx<'_>,
    p: &Principal,
    s: &Scope,
    strategy: Value,
    input: StrategyInput,
    b: Value,
    origin: &str,
) -> Result<Value> {
    let release = release(
        tx,
        input
            .configuration_release_id
            .ok_or(Error::Policy("configuration_missing"))?,
        school(s)?,
        input.device_id,
        true,
    )
    .await?;
    let c: ConfigurationInput =
        serde_json::from_value(release["configuration"].clone()).map_err(|_| Error::Conflict)?;
    let groups = input.groups();
    require(groups.len() <= c.policy.max_targets, "目标数超过已发布政策")?;
    let interval = input
        .inter_group_interval_seconds
        .map(|v| v * 1000)
        .unwrap_or(c.policy.default_group_interval_ms);
    require(
        interval
            >= c.policy
                .minimum_group_interval_ms
                .max(c.baseline.human_group_min_interval_ms),
        "群间等待不能低于政策和人工基准",
    )?;
    let run = Uuid::now_v7();
    let task = Uuid::now_v7();
    sqlx::query("INSERT INTO mobile_delivery_tasks(id,school_id,creator_id,activity_id) VALUES($1,$2,$3,$4)").bind(task).bind(school(s)?).bind(p.actor()?).bind(format!("strategy-run:{run}")).execute(&mut **tx).await?;
    sqlx::query("INSERT INTO mobile_delivery_strategy_runs(id,school_id,strategy_id,strategy_revision,task_id,device_id,device_binding_revision,strategy_snapshot,created_by) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)").bind(run).bind(school(s)?).bind(id(s)?).bind(strategy["revision"].as_i64().ok_or(Error::Conflict)?).bind(task).bind(input.device_id).bind(b["revision"].as_i64().ok_or(Error::Conflict)?).bind(&strategy).bind(p.actor()?).execute(&mut **tx).await?;
    sqlx::query("INSERT INTO mobile_delivery_run_progress(id,configuration_release_id,inter_group_interval_ms) VALUES($1,$2,$3)").bind(run).bind(uid(&release,"id")?).bind(interval).execute(&mut **tx).await?;
    for (index, group) in groups.iter().enumerate() {
        let item = Uuid::now_v7();
        let source = ContentSource::Text {
            text_content: input.text_content.clone(),
        };
        let mut snap = snapshot(
            tx,
            school(s)?,
            &ItemInput {
                group_id: *group,
                sender_account_id: input.sender_account_id,
                source: source.clone(),
            },
            origin,
        )
        .await?;
        snap["execution_target"] = json!({"device_id":input.device_id,"device_binding_revision":b["revision"],"strategy_id":strategy["id"],"strategy_revision":strategy["revision"],"run_id":run});
        snap["configuration"] = release.clone();
        snap["execution_revision"] = json!(1);
        snap["sequence"] = json!({"position":index+1,"total":groups.len(),"group_interval_ms":interval,"run_id":run});
        snap["requirements"] = requirements(&c, &release);
        let content = strv(&snap, "content")?;
        sqlx::query("INSERT INTO mobile_delivery_items(id,school_id,task_id,group_id,sender_account_id,content,content_digest,source,snapshot,snapshot_digest) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)").bind(item).bind(school(s)?).bind(task).bind(group).bind(input.sender_account_id).bind(content).bind(hash(content)).bind(json!(source)).bind(&snap).bind(hash(&snap.to_string())).execute(&mut **tx).await?;
        sqlx::query("INSERT INTO mobile_delivery_run_items(item_id,school_id,run_id,position) VALUES($1,$2,$3,$4)").bind(item).bind(school(s)?).bind(run).bind((index+1)as i32).execute(&mut **tx).await?;
    }
    object(tx, "mobile_delivery_strategy_runs", run).await
}
fn requirements(c: &ConfigurationInput, r: &Value) -> Value {
    json!({"protocol_version":3,"client_version":c.rules.client.version_name,"script_version":c.rules.engine_version,"baseline_version":c.baseline.version,"no_injection":true,"no_hook":true,"no_private_protocol":true,"no_client_data_modification":true,"local_invocation_consumption_required":true,"no_external_ids":true,"no_resource_ids":true,"ordered_groups_required":true,"human_pacing_required":true,"configuration_digest":r["content_digest"]})
}
/// None means no configured run owns this device/account. Some never falls through to round robin.
pub(super) async fn candidate(
    tx: &mut Tx<'_>,
    d: Uuid,
    account: Uuid,
    protocol: u32,
) -> Result<Option<std::result::Result<Value, Value>>> {
    let application_ids = if protocol == 4 {
        automation::authorized_application_ids(tx, d, account).await?
    } else {
        Vec::new()
    };
    let row:Option<Value>=sqlx::query_scalar("SELECT to_jsonb(p)||jsonb_build_object('device_id',r.device_id,'school_id',r.school_id,'task_id',r.task_id,'automation_manifest_id',r.automation_manifest_id) FROM mobile_delivery_run_progress p JOIN mobile_delivery_strategy_runs r ON r.id=p.id JOIN mobile_delivery_tasks t ON t.id=r.task_id WHERE (t.state='confirmed' OR EXISTS(SELECT 1 FROM mobile_delivery_run_items m JOIN delivery_usage u ON u.source_channel='mobile' AND u.source_id=m.item_id WHERE m.run_id=r.id AND u.state IN ('pending','unknown'))) AND (r.device_id=$1 OR r.strategy_snapshot->>'sender_account_id'=$2) AND EXISTS(SELECT 1 FROM mobile_delivery_run_items m WHERE m.run_id=r.id AND m.position>=p.current_position) AND ($3::int<>4 OR r.device_id<>$1 OR r.automation_manifest_id IS NULL OR p.current_position>1 OR EXISTS(SELECT 1 FROM mobile_delivery_run_items started JOIN mobile_execution_claims c ON c.item_id=started.item_id WHERE started.run_id=r.id) OR EXISTS(SELECT 1 FROM mobile_delivery_run_items m JOIN delivery_usage u ON u.source_channel='mobile' AND u.source_id=m.item_id WHERE m.run_id=r.id AND u.state IN ('pending','unknown')) OR EXISTS(SELECT 1 FROM automation_execution_manifests am WHERE am.id=r.automation_manifest_id AND am.body#>>'{resolution,binding,id}'=ANY($4::text[]))) ORDER BY p.confirmed_at NULLS LAST,r.id LIMIT 1").bind(d).bind(account.to_string()).bind(protocol as i32).bind(&application_ids).fetch_optional(&mut **tx).await?;
    let Some(row) = row else { return Ok(None) };
    let reason = if row["device_id"] != d.to_string() {
        Some("ordered_run_owns_account")
    } else if protocol
        != if row["automation_manifest_id"].is_null() {
            3
        } else {
            4
        }
    {
        Some("unsupported_execution_protocol")
    } else {
        row["pause_reason"].as_str()
    };
    let server_time = db_now(tx).await?;
    let wait = |reason: &str| json!({"waiting":true,"server_time":server_time,"retry_after_seconds":30,"next_eligible_at":row["next_eligible_at"],"run_id":row["id"],"position":row["current_position"],"reason":reason});
    if let Some(reason) = reason {
        return Ok(Some(Err(wait(reason))));
    }
    let item:Value=sqlx::query_scalar("SELECT to_jsonb(i) FROM mobile_delivery_run_items r JOIN mobile_delivery_items i ON i.id=r.item_id WHERE r.run_id=$1 AND r.position=$2").bind(uid(&row,"id")?).bind(row["current_position"].as_i64().ok_or(Error::Conflict)? as i32).fetch_one(&mut **tx).await?;
    if item["state"] != "queued" {
        return Ok(Some(Err(wait("previous_group_not_completed"))));
    }
    match validate(tx, &item).await {
        Ok(()) => Ok(Some(Ok(item))),
        Err(Error::Policy(reason)) => Ok(Some(Err(wait(reason)))),
        Err(e) => Err(e),
    }
}
pub(super) async fn revision_preview(
    tx: &mut Tx<'_>,
    p: &Principal,
    s: &Scope,
    target: Option<Uuid>,
) -> Result<Value> {
    let v = progress(tx, p, s).await?;
    let run = scoped_object(tx, s, "mobile_delivery_strategy_runs").await?;
    let new_release = if let Some(target) = target {
        release(tx, target, school(s)?, uid(&run, "device_id")?, true).await?
    } else {
        Value::Null
    };
    let digest=hash(&json!({"new_release":new_release,"run":s.id,"revision":v["revision"],"items":v["items"],"configuration_release_id":v["configuration_release_id"]}).to_string());
    Ok(
        json!({"id":s.id,"revision":v["revision"],"preview_digest":digest,"progress":v,"new_release":new_release}),
    )
}
pub(super) async fn revise(
    tx: &mut Tx<'_>,
    p: &Principal,
    s: &Scope,
    expected: i64,
    release_id: Uuid,
    digest: &str,
    evidence: &str,
) -> Result<Value> {
    school_auth(tx, p.actor()?, school(s)?, true).await?;
    text(evidence, 1, 500)?;
    let preview = revision_preview(tx, p, s, Some(release_id)).await?;
    require(preview["preview_digest"] == digest, "执行修订预览已变化")?;
    let old = &preview["progress"];
    revision(old, expected)?;
    let run = scoped_object(tx, s, "mobile_delivery_strategy_runs").await?;
    let task = object(tx, "mobile_delivery_tasks", uid(&run, "task_id")?).await?;
    require(task["state"] == "confirmed", "仅可修订已确认的未完成任务")?;
    let blocked:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_delivery_run_items r JOIN mobile_delivery_items i ON i.id=r.item_id WHERE r.run_id=$1 AND r.position>=$2 AND (i.state<>'queued' OR EXISTS(SELECT 1 FROM mobile_send_permits p WHERE p.item_id=i.id) OR EXISTS(SELECT 1 FROM mobile_execution_claims c WHERE c.item_id=i.id AND c.state IN ('claimed','prepared','permit_issued'))))").bind(id(s)?).bind(old["current_position"].as_i64().ok_or(Error::Conflict)? as i32).fetch_one(&mut **tx).await?;
    require(!blocked, "仅可修订没有活动领取、许可或不确定结果的剩余项")?;
    let r = release(tx, release_id, school(s)?, uid(&run, "device_id")?, true).await?;
    let c: ConfigurationInput =
        serde_json::from_value(r["configuration"].clone()).map_err(|_| Error::Conflict)?;
    require(
        old["items"].as_array().ok_or(Error::Conflict)?.len() <= c.policy.max_targets,
        "目标数超过新政策容量",
    )?;
    let rev = old["execution_revision"].as_i64().ok_or(Error::Conflict)? + 1;
    let interval = old["inter_group_interval_ms"]
        .as_i64()
        .ok_or(Error::Conflict)?
        .max(c.policy.minimum_group_interval_ms)
        .max(c.baseline.human_group_min_interval_ms);
    let pending:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(i) FROM mobile_delivery_run_items r JOIN mobile_delivery_items i ON i.id=r.item_id WHERE r.run_id=$1 AND r.position>=$2").bind(id(s)?).bind(old["current_position"].as_i64().ok_or(Error::Conflict)? as i32).fetch_all(&mut **tx).await?;
    for mut i in pending {
        i["snapshot"]["configuration"] = r.clone();
        i["snapshot"]["execution_revision"] = json!(rev);
        i["snapshot"]["requirements"] = requirements(&c, &r);
        i["snapshot"]["sequence"]["group_interval_ms"] = json!(interval);
        sqlx::query("UPDATE mobile_delivery_items SET snapshot=$2,snapshot_digest=$3,revision=revision+1 WHERE id=$1").bind(uid(&i,"id")?).bind(&i["snapshot"]).bind(hash(&i["snapshot"].to_string())).execute(&mut **tx).await?;
    }
    // Keep every prior snapshot in the append-only revision evidence, never rewrite completed items.
    sqlx::query("INSERT INTO mobile_delivery_run_execution_revisions(id,run_id,revision,configuration_release_id,preview_digest,evidence_ref,created_by,previous_progress) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(Uuid::now_v7()).bind(id(s)?).bind(rev).bind(release_id).bind(digest).bind(evidence).bind(p.actor()?).bind(old).execute(&mut **tx).await?;
    sqlx::query("UPDATE mobile_delivery_run_progress SET configuration_release_id=$2,execution_revision=$3,inter_group_interval_ms=$4,pause_reason=NULL,revision=revision+1,next_eligible_at=CASE WHEN last_completed_at IS NULL THEN next_eligible_at ELSE greatest(next_eligible_at,last_completed_at+greatest($4::bigint,$5::bigint,$6::bigint)*interval '1 millisecond') END WHERE id=$1").bind(id(s)?).bind(release_id).bind(rev).bind(interval).bind(c.policy.account_cooldown_ms).bind(c.policy.content_cooldown_ms).execute(&mut **tx).await?;
    progress(tx, p, s).await
}
