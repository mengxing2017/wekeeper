//! Independent v5 delivery/preparation; all actual sends stay in the shared permit ledger.
use super::*;

pub(super) fn is_dispatch(i: &Value) -> bool {
    i["snapshot"]["requirements"]["protocol_version"] == 5
}
fn flow(v: &Value) -> Result<DispatchFlowV1> {
    serde_json::from_value(v.clone()).map_err(|_| Error::Conflict)
}
pub(super) fn policy(i: &Value) -> Result<ExecutionPolicy> {
    serde_json::from_value(i["snapshot"]["configuration"]["configuration"]["policy"].clone())
        .map_err(|_| Error::Conflict)
}
async fn scoped_device(tx: &mut Tx<'_>, school: Uuid, d: Uuid) -> Result<Value> {
    let dev = object(tx, "execution_devices", d).await?;
    require(dev["enabled"] == true, "设备已停用")?;
    let b = binding(tx, d).await?;
    let a = object(tx, "sending_accounts", uid(&b, "sender_account_id")?).await?;
    school_auth(tx, uid(&a, "responsible_actor_id")?, school, true).await?;
    Ok(b)
}
async fn released(tx: &mut Tx<'_>, release: Uuid, school: Uuid, new: bool) -> Result<Value> {
    let r:Value=sqlx::query_scalar("SELECT to_jsonb(r)||jsonb_build_object('flow',v.body,'flow_digest',v.content_digest,'name',v.name) FROM automation_flow_releases r JOIN automation_flow_versions v ON v.id=r.flow_id WHERE r.id=$1 AND r.school_id=$2 AND NOT r.revoked AND (NOT $3 OR r.active)").bind(release).bind(school).bind(new).fetch_optional(&mut **tx).await?.ok_or(Error::Policy("configuration_not_available"))?;
    flow(&r["flow"])?.validate()?;
    require(
        automation_digest(&r["flow"]) == r["flow_digest"],
        "流程摘要不匹配",
    )?;
    Ok(r)
}
async fn receipt(tx: &mut Tx<'_>, d: Uuid) -> Result<BusinessAuthorizationReceipt> {
    let v:Option<Value>=sqlx::query_scalar("SELECT body FROM automation_business_authorizations WHERE device_id=$1 ORDER BY revision DESC LIMIT 1").bind(d).fetch_optional(&mut **tx).await?;
    serde_json::from_value(v.ok_or(Error::Policy("execution_authorization_required"))?)
        .map_err(|_| Error::Conflict)
}
pub(super) async fn authorization_gate(tx: &mut Tx<'_>, d: Uuid) -> Result<()> {
    require(receipt(tx, d).await?.state == "effective", "执行授权已撤销")
}
async fn authorized(
    tx: &mut Tx<'_>,
    d: Uuid,
    school: Uuid,
    account: Uuid,
) -> Result<BusinessAuthorizationReceipt> {
    let r = receipt(tx, d).await?;
    require(
        r.state == "effective"
            && r.scope.device_id == d
            && r.scope.sender_account_id == account
            && r.scope.schools.contains(&school),
        "业务授权范围不匹配",
    )?;
    Ok(r)
}
async fn delivery(tx: &mut Tx<'_>, p: &Principal, did: Uuid) -> Result<(Value, Value)> {
    require(p.protocol() == 5, "执行要求 v5")?;
    let (d, _) = device(tx, p).await?;
    let del = object(tx, "automation_task_deliveries", did).await?;
    require(del["device_id"] == d.to_string(), "设备范围不匹配")?;
    let plan = object(tx, "automation_dispatch_plans", uid(&del, "plan_id")?).await?;
    Ok((del, plan))
}
async fn current_item(tx: &mut Tx<'_>, plan: &Value) -> Result<Value> {
    sqlx::query_scalar("SELECT to_jsonb(i) FROM mobile_delivery_run_items m JOIN mobile_delivery_items i ON i.id=m.item_id JOIN mobile_delivery_run_progress p ON p.id=m.run_id WHERE p.id=$1 AND m.position=p.current_position").bind(uid(plan,"run_id")?).fetch_optional(&mut **tx).await?.ok_or(Error::Policy("no_ready_task"))
}
pub(super) async fn validate(tx: &mut Tx<'_>, i: &Value, actual: Option<Uuid>) -> Result<()> {
    let snap = &i["snapshot"];
    let target = &snap["execution_target"];
    let plan = object(
        tx,
        "automation_dispatch_plans",
        uid(&snap["dispatch_plan"], "id")?,
    )
    .await?;
    require(
        plan["body"] == snap["dispatch_plan"]["body"]
            && plan["content_digest"] == snap["dispatch_plan"]["content_digest"]
            && automation_digest(&plan["body"]) == plan["content_digest"],
        "冻结指令不匹配",
    )?;
    let d = uid(&plan, "device_id")?;
    require(actual.is_none_or(|v| v == d), "设备不匹配")?;
    let b = scoped_device(tx, uid(i, "school_id")?, d).await?;
    require(
        b["sender_account_id"] == i["sender_account_id"]
            && b["revision"] == target["device_binding_revision"],
        "设备账号分配已改变",
    )?;
    let a = object(
        tx,
        "automation_execution_assignments",
        uid(&plan, "assignment_id")?,
    )
    .await?;
    require(
        a["enabled"] == true
            && a["sender_account_id"] == plan["sender_account_id"]
            && a["account_binding_revision"] == plan["body"]["binding_revision"],
        "执行分配已改变",
    )?;
    released(tx, uid(&plan, "release_id")?, uid(i, "school_id")?, false).await?;
    let batch = object(tx, "automation_dispatch_batches", uid(&plan, "batch_id")?).await?;
    require(
        matches!(batch["state"].as_str(), Some("draft" | "confirmed")),
        "批次已暂停或停止",
    )?;
    for (table, key, idkey) in [
        ("sending_accounts", "sender", "sender_account_id"),
        ("delivery_groups", "group", "group_id"),
    ] {
        let live = visible::snapshot_identity(object(tx, table, uid(i, idkey)?).await?)?;
        require(live == snap[key], "账号或群资料已变化")?;
    }
    let mapping:Option<i64>=sqlx::query_scalar("SELECT revision FROM sender_group_bindings WHERE school_id=$1 AND sender_account_id=$2 AND group_id=$3").bind(uid(i,"school_id")?).bind(uid(i,"sender_account_id")?).bind(uid(i,"group_id")?).fetch_optional(&mut **tx).await?;
    require(mapping == snap["binding_revision"].as_i64(), "群归属已变化")?;
    require(plan["run_id"] == target["run_id"], "执行序列不匹配")?;
    let position = snap["sequence"]["position"]
        .as_u64()
        .ok_or(Error::Conflict)?;
    require(
        position > 0
            && plan["body"]["group_ids"][position as usize - 1] == i["group_id"]
            && plan["body"]["text_content"] == i["content"]
            && plan["body"]["sender"] == snap["sender"]
            && plan["body"]["groups"][position as usize - 1] == snap["group"],
        "群顺序或正文不匹配",
    )?;
    if actual.is_some() {
        require(batch["state"] == "confirmed", "批次尚未确认")?;
        require(
            settings(tx, uid(i, "school_id")?).await?["dispatch_enabled"] == true,
            "v5 未启用",
        )?;
        let r = authorized(tx, d, uid(i, "school_id")?, uid(i, "sender_account_id")?).await?;
        let ready:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM automation_dispatch_preparations p JOIN automation_task_deliveries d ON d.id=p.delivery_id WHERE d.plan_id=$1 AND NOT d.cancelled AND p.item_id=$2 AND p.ready AND p.expires_at>clock_timestamp() AND p.credential_generation=(SELECT credential_generation FROM execution_devices WHERE id=$3) AND p.authorization_id=$4 AND p.fence=$5)").bind(uid(&plan,"id")?).bind(uid(i,"id")?).bind(d).bind(r.authorization_id).bind(i["fence"].as_i64().ok_or(Error::Conflict)?+if i["state"]=="queued" {1}else{0}).fetch_one(&mut **tx).await?;
        require(ready, "readiness_required")?;
    }
    Ok(())
}
pub(super) async fn candidate(
    tx: &mut Tx<'_>,
    d: Uuid,
    account: Uuid,
) -> Result<Option<std::result::Result<Value, Value>>> {
    // Pending work on an account is never bypassed by a second device or a newer plan.
    let row:Option<Value>=sqlx::query_scalar("SELECT to_jsonb(p)||jsonb_build_object('device_id',r.device_id,'school_id',r.school_id) FROM mobile_delivery_run_progress p JOIN mobile_delivery_strategy_runs r ON r.id=p.id JOIN mobile_delivery_tasks t ON t.id=r.task_id WHERE (r.device_id=$1 OR r.strategy_snapshot->>'sender_account_id'=$2) AND (t.state='confirmed' OR EXISTS(SELECT 1 FROM mobile_delivery_run_items m JOIN delivery_usage u ON u.source_channel='mobile' AND u.source_id=m.item_id WHERE m.run_id=r.id AND u.state IN ('pending','unknown'))) AND EXISTS(SELECT 1 FROM mobile_delivery_run_items m WHERE m.run_id=r.id AND m.position>=p.current_position) ORDER BY p.confirmed_at NULLS LAST,r.id LIMIT 1").bind(d).bind(account.to_string()).fetch_optional(&mut **tx).await?;
    let Some(row) = row else { return Ok(None) };
    let wait = |reason: &str| json!({"waiting":true,"reason":reason,"retry_after_seconds":30,"server_time":chrono::Utc::now(),"next_eligible_at":row["next_eligible_at"]});
    if row["device_id"] != d.to_string() {
        return Ok(Some(Err(wait("ordered_run_owns_account"))));
    }
    if row["dispatch_plan_id"].is_null() {
        return Ok(Some(Err(wait("unsupported_execution_protocol"))));
    }
    if let Some(reason) = row["pause_reason"].as_str() {
        return Ok(Some(Err(wait(reason))));
    }
    let plan = object(
        tx,
        "automation_dispatch_plans",
        uid(&row, "dispatch_plan_id")?,
    )
    .await?;
    let i = current_item(tx, &plan).await?;
    if i["state"] != "queued" {
        return Ok(Some(Err(wait("previous_group_not_completed"))));
    }
    match configured::validate(tx, &i).await {
        Ok(()) => Ok(Some(Ok(i))),
        Err(Error::Policy(reason)) => Ok(Some(Err(wait(reason)))),
        Err(e) => Err(e),
    }
}
async fn build_preview(
    tx: &mut Tx<'_>,
    school: Uuid,
    input: &DispatchInput,
    origin: &str,
) -> Result<Value> {
    input.validate()?;
    let mut targets = Vec::new();
    for t in &input.targets {
        let mut a = object(tx, "automation_execution_assignments", t.assignment_id).await?;
        require(
            a["school_id"] == school.to_string() && a["enabled"] == true,
            "执行分配不在当前范围",
        )?;
        let b = scoped_device(tx, school, uid(&a, "device_id")?).await?;
        require(
            a["account_binding_revision"] == b["revision"]
                && a["sender_account_id"] == b["sender_account_id"],
            "账号分配已变化",
        )?;
        let release = released(tx, uid(&a, "release_id")?, school, true).await?;
        let f = flow(&release["flow"])?;
        require(
            t.group_ids.len() <= f.configuration.policy.max_targets
                && input.interval_seconds * 1000
                    >= f.configuration
                        .policy
                        .minimum_group_interval_ms
                        .max(f.configuration.baseline.human_group_min_interval_ms),
            "群数量或间隔不满足流程政策及人工基准",
        )?;
        let mut groups = Vec::new();
        for g in &t.group_ids {
            let s = snapshot(
                tx,
                school,
                &ItemInput {
                    group_id: *g,
                    sender_account_id: uid(&a, "sender_account_id")?,
                    source: ContentSource::Text {
                        text_content: input.text_content.clone(),
                    },
                },
                origin,
            )
            .await?;
            require(
                s["group"]["identity_mode"] == "ui_visible"
                    && s["sender"]["identity_mode"] == "ui_visible",
                "需要已核验界面身份资料",
            )?;
            groups.push(s);
        }
        let dev = object(tx, "execution_devices", uid(&a, "device_id")?).await?;
        let account = object(tx, "sending_accounts", uid(&a, "sender_account_id")?).await?;
        a["device_name"] = dev["name"].clone();
        a["account_name"] = account["visible_profile"]["account_name"].clone();
        targets.push(json!({"assignment":a,"binding":b,"release":release,"groups":groups}));
    }
    Ok(json!({"input":input,"targets":targets}))
}
async fn batch_detail(tx: &mut Tx<'_>, s: Uuid, bid: Uuid) -> Result<Value> {
    let mut b = object(tx, "automation_dispatch_batches", bid).await?;
    require(b["school_id"] == s.to_string(), "学校不匹配")?;
    let plans:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(p)||jsonb_build_object('delivery',to_jsonb(d),'progress',to_jsonb(r),'device_name',v.name) FROM automation_dispatch_plans p JOIN automation_task_deliveries d ON d.plan_id=p.id JOIN mobile_delivery_run_progress r ON r.id=p.run_id JOIN execution_devices v ON v.id=p.device_id WHERE p.batch_id=$1 ORDER BY p.id").bind(bid).fetch_all(&mut **tx).await?;
    let mut children = Vec::new();
    for mut p in plans {
        p["items"]=json!(sqlx::query_scalar::<_,Value>("SELECT to_jsonb(i)||jsonb_build_object('position',m.position) FROM mobile_delivery_run_items m JOIN mobile_delivery_items i ON i.id=m.item_id WHERE m.run_id=$1 ORDER BY m.position").bind(uid(&p,"run_id")?).fetch_all(&mut **tx).await?);
        children.push(p);
    }
    b["plans"] = json!(children);
    Ok(b)
}
async fn remaining_targets(
    tx: &mut Tx<'_>,
    batch: Uuid,
    release: Uuid,
) -> Result<Vec<DispatchTarget>> {
    let blocked:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM automation_dispatch_plans p JOIN mobile_delivery_run_items m ON m.run_id=p.run_id JOIN mobile_execution_claims c ON c.item_id=m.item_id WHERE p.batch_id=$1 AND c.state IN ('claimed','prepared','permit_issued')) OR EXISTS(SELECT 1 FROM automation_dispatch_plans p JOIN mobile_delivery_run_items m ON m.run_id=p.run_id JOIN mobile_delivery_items i ON i.id=m.item_id WHERE p.batch_id=$1 AND i.state='unknown')").bind(batch).fetch_one(&mut **tx).await?;
    require(!blocked, "请先恢复或核对未决发送；unknown 不得重建")?;
    let plans: Vec<Value> = sqlx::query_scalar(
        "SELECT to_jsonb(p) FROM automation_dispatch_plans p WHERE batch_id=$1 ORDER BY id",
    )
    .bind(batch)
    .fetch_all(&mut **tx)
    .await?;
    let mut targets = Vec::new();
    for plan in plans {
        let a = object(
            tx,
            "automation_execution_assignments",
            uid(&plan, "assignment_id")?,
        )
        .await?;
        require(
            a["release_id"] == release.to_string(),
            "请先将执行分配更换到所选流程，原计划保持冻结",
        )?;
        let groups:Vec<Uuid>=sqlx::query_scalar("SELECT i.group_id FROM mobile_delivery_run_items m JOIN mobile_delivery_items i ON i.id=m.item_id WHERE m.run_id=$1 AND i.state IN ('queued','cancelled') AND NOT EXISTS(SELECT 1 FROM mobile_send_permits p WHERE p.item_id=i.id) AND NOT EXISTS(SELECT 1 FROM delivery_usage u WHERE u.source_channel='mobile' AND u.source_id=i.id AND u.state<>'not_sent') ORDER BY m.position").bind(uid(&plan,"run_id")?).fetch_all(&mut **tx).await?;
        if !groups.is_empty() {
            targets.push(DispatchTarget {
                assignment_id: uid(&plan, "assignment_id")?,
                group_ids: groups,
            });
        }
    }
    require(!targets.is_empty(), "没有可安全重建的确定未发群")?;
    Ok(targets)
}
async fn confirm(
    tx: &mut Tx<'_>,
    p: &Principal,
    s: &Scope,
    digest: &str,
    reviewed: bool,
    origin: &str,
) -> Result<Value> {
    let batch = scoped_object(tx, s, "automation_dispatch_batches").await?;
    require(reviewed, "需要核对执行范围和正文")?;
    if batch["state"] == "confirmed" {
        return batch_detail(tx, school(s)?, id(s)?).await;
    }
    require(
        batch["state"] == "draft" && batch["preview_digest"] == digest,
        "预览已改变",
    )?;
    let input: DispatchInput =
        serde_json::from_value(batch["input"].clone()).map_err(|_| Error::Conflict)?;
    let mut preview = build_preview(tx, school(s)?, &input, origin).await?;
    if let Some(replacement) = batch["preview"].get("replacement") {
        let old = object(
            tx,
            "automation_dispatch_batches",
            uid(replacement, "batch_id")?,
        )
        .await?;
        revision(
            &old,
            replacement["revision"].as_i64().ok_or(Error::Conflict)?,
        )?;
        require(old["state"] == "paused", "原批次已变化")?;
        require(
            json!(remaining_targets(tx, uid(&old, "id")?, uid(replacement, "release_id")?).await?)
                == json!(input.targets),
            "剩余群已变化",
        )?;
        preview["replacement"] = replacement.clone();
    }
    require(
        automation_digest(&preview) == digest,
        "后台资料已变化，请重新预览",
    )?;
    if let Some(replacement) = preview.get("replacement") {
        let old = uid(replacement, "batch_id")?;
        sqlx::query("UPDATE mobile_delivery_items i SET state='cancelled',revision=revision+1 FROM mobile_delivery_run_items m JOIN automation_dispatch_plans p ON p.run_id=m.run_id WHERE i.id=m.item_id AND p.batch_id=$1 AND i.state='queued'").bind(old).execute(&mut **tx).await?;
        sqlx::query("UPDATE delivery_dedup_reservations r SET active=false FROM mobile_delivery_items i JOIN mobile_delivery_run_items m ON m.item_id=i.id JOIN automation_dispatch_plans p ON p.run_id=m.run_id WHERE r.item_id=i.id AND p.batch_id=$1 AND i.state='cancelled'").bind(old).execute(&mut **tx).await?;
        sqlx::query("UPDATE automation_dispatch_batches SET state='cancelled',revision=revision+1 WHERE id=$1").bind(old).execute(&mut **tx).await?;
        sqlx::query("UPDATE mobile_delivery_tasks t SET state='stopped',revision=revision+1 FROM mobile_delivery_strategy_runs r JOIN automation_dispatch_plans p ON p.run_id=r.id WHERE t.id=r.task_id AND p.batch_id=$1").bind(old).execute(&mut **tx).await?;
    }
    for t in preview["targets"].as_array().ok_or(Error::Conflict)? {
        let a = &t["assignment"];
        let b = &t["binding"];
        let release = &t["release"];
        let run = Uuid::now_v7();
        let task = Uuid::now_v7();
        let pid = Uuid::now_v7();
        let did = Uuid::now_v7();
        let snaps = t["groups"].as_array().ok_or(Error::Conflict)?;
        let group_ids: Vec<Value> = snaps.iter().map(|v| v["group"]["id"].clone()).collect();
        let body = json!({"schema_version":1,"protocol_version":5,"batch_id":id(s)?,"run_id":run,"school_id":school(s)?,"device_id":a["device_id"],"sender_account_id":a["sender_account_id"],"binding_revision":b["revision"],"assignment_id":a["id"],"assignment_revision":a["revision"],"release_id":release["id"],"flow":release["flow"],"flow_digest":release["flow_digest"],"sender":snaps[0]["sender"],"groups":snaps.iter().map(|v|v["group"].clone()).collect::<Vec<_>>(),"group_ids":group_ids,"text_content":normalize(&input.text_content)?,"interval_ms":input.interval_seconds*1000});
        let pd = automation_digest(&body);
        sqlx::query("INSERT INTO mobile_delivery_tasks(id,school_id,creator_id,activity_id) VALUES($1,$2,$3,$4)").bind(task).bind(school(s)?).bind(p.actor()?).bind(&input.name).execute(&mut **tx).await?;
        sqlx::query("INSERT INTO mobile_delivery_strategy_runs(id,school_id,strategy_revision,task_id,device_id,device_binding_revision,strategy_snapshot,created_by) VALUES($1,$2,1,$3,$4,$5,$6,$7)").bind(run).bind(school(s)?).bind(task).bind(uid(a,"device_id")?).bind(b["revision"].as_i64().ok_or(Error::Conflict)?).bind(json!({"sender_account_id":a["sender_account_id"],"dispatch_batch_id":id(s)?})).bind(p.actor()?).execute(&mut **tx).await?;
        sqlx::query("INSERT INTO automation_dispatch_plans(id,batch_id,school_id,assignment_id,release_id,device_id,sender_account_id,run_id,body,content_digest) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)").bind(pid).bind(id(s)?).bind(school(s)?).bind(uid(a,"id")?).bind(uid(release,"id")?).bind(uid(a,"device_id")?).bind(uid(a,"sender_account_id")?).bind(run).bind(&body).bind(&pd).execute(&mut **tx).await?;
        sqlx::query("INSERT INTO mobile_delivery_run_progress(id,dispatch_plan_id,inter_group_interval_ms) VALUES($1,$2,$3)").bind(run).bind(pid).bind(input.interval_seconds*1000).execute(&mut **tx).await?;
        sqlx::query("INSERT INTO automation_task_deliveries(id,plan_id,device_id,generation) VALUES($1,$2,$3,$4)").bind(did).bind(pid).bind(uid(a,"device_id")?).bind(Uuid::now_v7()).execute(&mut **tx).await?;
        let cfg = &release["flow"]["configuration"];
        for (index, snap) in snaps.iter().enumerate() {
            let item = Uuid::now_v7();
            let mut snap = snap.clone();
            snap["execution_target"] = json!({"device_id":a["device_id"],"device_binding_revision":b["revision"],"strategy_id":null,"strategy_revision":1,"run_id":run});
            snap["configuration"] = json!({"id":pid,"configuration":cfg,"content_digest":automation_digest(cfg),"revoked":false});
            snap["dispatch_plan"] = json!({"id":pid,"body":body,"content_digest":pd});
            snap["execution_revision"] = json!(1);
            snap["sequence"] = json!({"run_id":run,"position":index+1,"total":snaps.len(),"group_interval_ms":input.interval_seconds*1000});
            snap["requirements"] = json!({"protocol_version":5,"plan_digest":pd,"configuration_digest":automation_digest(cfg),"no_injection":true,"no_hook":true,"no_private_protocol":true,"no_client_data_modification":true,"local_invocation_consumption_required":true,"no_external_ids":true,"no_resource_ids":true,"ordered_groups_required":true,"human_pacing_required":true});
            sqlx::query("INSERT INTO mobile_delivery_items(id,school_id,task_id,group_id,sender_account_id,content,content_digest,source,snapshot,snapshot_digest) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)").bind(item).bind(school(s)?).bind(task).bind(uid(&snap["group"],"id")?).bind(uid(a,"sender_account_id")?).bind(strv(&snap,"content")?).bind(hash(strv(&snap,"content")?)).bind(json!({"kind":"text","text_content":strv(&snap,"content")?})).bind(&snap).bind(hash(&snap.to_string())).execute(&mut **tx).await?;
            sqlx::query("INSERT INTO mobile_delivery_run_items(item_id,school_id,run_id,position) VALUES($1,$2,$3,$4)").bind(item).bind(school(s)?).bind(run).bind((index+1)as i32).execute(&mut **tx).await?;
        }
        let check = super::preview(tx, task, origin).await?;
        confirm_task(
            tx,
            p,
            &Scope {
                school: s.school,
                id: Some(task),
            },
            TaskConfirmation {
                expected_revision: 1,
                preview_digest: strv(&check, "preview_digest")?.into(),
                content_review: true,
                scope_acceptance: true,
                substantive_difference_reviewed: false,
                shared_text_scope_acceptance: Some(true),
                evidence_ref: format!("dispatch-batch:{}", id(s)?),
            },
            origin,
            true,
        )
        .await?;
    }
    sqlx::query("UPDATE automation_dispatch_batches SET state='confirmed',revision=revision+1,confirmed_at=clock_timestamp() WHERE id=$1").bind(id(s)?).execute(&mut **tx).await?;
    batch_detail(tx, school(s)?, id(s)?).await
}

pub(super) async fn apply(
    tx: &mut Tx<'_>,
    p: &Principal,
    s: &Scope,
    c: DispatchCommand,
    enabled: bool,
    origin: &str,
) -> Result<Value> {
    if let Some(school) = s.school {
        school_auth(tx, p.actor()?, school, true).await?;
    }
    match c {
        DispatchCommand::RequestValidation {
            device_id,
            flow_id,
            manual_application,
            kind,
            path_key,
            launcher_index,
        } => {
            platform(tx, p).await?;
            let b = scoped_device(tx, school(s)?, device_id).await?;
            require(
                matches!(
                    kind,
                    ValidationKind::InspectRoles
                        | ValidationKind::OpenApplication
                        | ValidationKind::MeasureManual
                ),
                "验证类型不支持",
            )?;
            if let Some(fid) = flow_id {
                let f = object(tx, "automation_flow_versions", fid).await?;
                let flow = flow(&f["body"])?;
                flow.validate()?;
                require(manual_application.is_none(), "已选择流程，不重复声明应用")?;
                require(
                    path_key == "generic" || flow.specializations.iter().any(|v| v.key == path_key),
                    "路径不存在",
                )?;
                require(
                    (kind == ValidationKind::OpenApplication) == launcher_index.is_some(),
                    "桌面验证须选择候选",
                )?;
                if let Some(index) = launcher_index {
                    require(
                        index < flow.launchers.len() && path_key == "generic",
                        "桌面候选无效",
                    )?;
                }
            } else {
                require(
                    kind == ValidationKind::MeasureManual
                        && path_key == "generic"
                        && launcher_index.is_none(),
                    "只有人工计时可先于流程建立",
                )?;
                let app = manual_application
                    .as_ref()
                    .ok_or(Error::Invalid("请选择人工计时应用".into()))?;
                label(&app.label)?;
                validate_package(&app.client.package_name)?;
            }
            let dev = object(tx, "execution_devices", device_id).await?;
            let jid = Uuid::now_v7();
            sqlx::query("INSERT INTO automation_flow_validations(id,school_id,device_id,flow_id,kind,path_key,launcher_index,generation,credential_generation,created_by,manual_application) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)").bind(jid).bind(school(s)?).bind(device_id).bind(flow_id).bind(kind.key()).bind(path_key).bind(launcher_index.map(|v|v as i32)).bind(Uuid::now_v7()).bind(dev["credential_generation"].as_i64().ok_or(Error::Conflict)?).bind(p.actor()?).bind(manual_application.map(|v|json!(v))).execute(&mut **tx).await?;
            let _ = b;
            object(tx, "automation_flow_validations", jid).await
        }
        DispatchCommand::AcceptValidation {
            expected_revision,
            generation,
            authorization_id,
        } => {
            let (d, g) = device(tx, p).await?;
            require(p.protocol() == 5, "维护验证要求 v5")?;
            let j = object(tx, "automation_flow_validations", id(s)?).await?;
            require(
                j["device_id"] == d.to_string()
                    && j["credential_generation"] == g
                    && j["generation"] == generation.to_string()
                    && j["state"] == "queued",
                "维护验证代次已变化",
            )?;
            revision(&j, expected_revision)?;
            platform(tx, &Principal::Actor(uid(&j, "created_by")?)).await?;
            let b = scoped_device(tx, uid(&j, "school_id")?, d).await?;
            let auth =
                authorized(tx, d, uid(&j, "school_id")?, uid(&b, "sender_account_id")?).await?;
            require(
                auth.authorization_id == authorization_id,
                "业务范围授权已变化",
            )?;
            let busy:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_execution_claims WHERE device_id=$1 AND state IN ('claimed','prepared','permit_issued')) OR EXISTS(SELECT 1 FROM automation_dispatch_preparations q JOIN automation_task_deliveries d ON d.id=q.delivery_id WHERE d.device_id=$1 AND q.expires_at>clock_timestamp()) OR EXISTS(SELECT 1 FROM automation_flow_validations WHERE device_id=$1 AND state='accepted' AND expires_at>clock_timestamp())").bind(d).fetch_one(&mut **tx).await?;
            require(!busy, "设备正忙，请先恢复或结束当前操作")?;
            let n=sqlx::query("UPDATE automation_flow_validations SET state='accepted',authorization_id=$2,revision=revision+1 WHERE id=$1 AND expires_at>clock_timestamp()").bind(id(s)?).bind(authorization_id).execute(&mut **tx).await?.rows_affected();
            require(n == 1, "验证已过期")?;
            let updated = object(tx, "automation_flow_validations", id(s)?).await?;
            validation_document(tx, &updated).await
        }
        DispatchCommand::ValidationEvent { event } => {
            let (d, g) = device(tx, p).await?;
            let j = object(tx, "automation_flow_validations", id(s)?).await?;
            require(
                p.protocol() == 5 && j["device_id"] == d.to_string(),
                "验证不属于当前设备",
            )?;
            let body = json!(event);
            let old:Option<Value>=sqlx::query_scalar("SELECT body FROM automation_flow_validation_events WHERE job_id=$1 AND event_id=$2").bind(id(s)?).bind(event.event_id).fetch_optional(&mut **tx).await?;
            if let Some(old) = old {
                require(old == body, "验证事件冲突")?;
                return validation_document(tx, &j).await;
            }
            let kind = match j["kind"].as_str() {
                Some("inspect_roles") => ValidationKind::InspectRoles,
                Some("open_application") => ValidationKind::OpenApplication,
                Some("measure_manual") => ValidationKind::MeasureManual,
                _ => return Err(Error::Conflict),
            };
            event.validate(kind)?;
            require(
                j["credential_generation"] == g
                    && j["generation"] == event.generation.to_string()
                    && j["state"] == "accepted",
                "维护验证已取消或过期",
            )?;
            revision(&j, event.expected_revision)?;
            let live: bool = sqlx::query_scalar(
                "SELECT expires_at>clock_timestamp() FROM automation_flow_validations WHERE id=$1",
            )
            .bind(id(s)?)
            .fetch_one(&mut **tx)
            .await?;
            require(live, "维护验证已过期")?;
            if event.outcome == "succeeded" {
                platform(tx, &Principal::Actor(uid(&j, "created_by")?)).await?;
                let b = scoped_device(tx, uid(&j, "school_id")?, d).await?;
                let auth =
                    authorized(tx, d, uid(&j, "school_id")?, uid(&b, "sender_account_id")?).await?;
                require(
                    j["authorization_id"] == auth.authorization_id.to_string(),
                    "授权范围已改变",
                )?;
            }
            if event.outcome == "succeeded" && kind == ValidationKind::InspectRoles {
                let doc = validation_document(tx, &j).await?;
                let r = &doc["components"][0]["body"]["rules"];
                for key in r["controls"]
                    .as_object()
                    .ok_or(Error::Conflict)?
                    .keys()
                    .cloned()
                    .chain(
                        r["pages"]
                            .as_object()
                            .ok_or(Error::Conflict)?
                            .keys()
                            .map(|v| format!("page:{v}")),
                    )
                {
                    require(
                        event.role_matches.get(&key) == Some(&1),
                        "页面角色必须完整唯一核验",
                    )?;
                }
            }
            require(
                matches!(
                    event.outcome.as_str(),
                    "succeeded" | "failed" | "interrupted"
                ),
                "仅提交固定终态",
            )?;
            sqlx::query("INSERT INTO automation_flow_validation_events(job_id,event_id,body) VALUES($1,$2,$3)").bind(id(s)?).bind(event.event_id).bind(&body).execute(&mut **tx).await?;
            sqlx::query("UPDATE automation_flow_validations SET state=$2,result=$3,revision=revision+1 WHERE id=$1").bind(id(s)?).bind(event.outcome).bind(body).execute(&mut **tx).await?;
            let updated = object(tx, "automation_flow_validations", id(s)?).await?;
            validation_document(tx, &updated).await
        }
        DispatchCommand::CancelValidation { expected_revision } => {
            platform(tx, p).await?;
            let j = scoped_object(tx, s, "automation_flow_validations").await?;
            revision(&j, expected_revision)?;
            require(
                matches!(j["state"].as_str(), Some("queued" | "accepted")),
                "验证已结束",
            )?;
            sqlx::query("UPDATE automation_flow_validations SET state='cancelled',revision=revision+1 WHERE id=$1").bind(id(s)?).execute(&mut **tx).await?;
            object(tx, "automation_flow_validations", id(s)?).await
        }
        DispatchCommand::CreatePacing {
            name,
            school_id,
            validation_ids,
        } => {
            let actor = platform(tx, p).await?;
            school_auth(tx, actor, school_id, true).await?;
            label(&name)?;
            let unique: std::collections::BTreeSet<_> = validation_ids.iter().collect();
            require(
                !validation_ids.is_empty()
                    && validation_ids.len() <= 100
                    && unique.len() == validation_ids.len(),
                "人工样本缺失或重复",
            )?;
            let mut timings = Vec::new();
            for id in &validation_ids {
                let j = if let Some(v) = sqlx::query_scalar::<_, Value>(
                    "SELECT to_jsonb(j) FROM automation_flow_validations j WHERE id=$1",
                )
                .bind(id)
                .fetch_optional(&mut **tx)
                .await?
                {
                    v
                } else {
                    object(tx, "device_validation_jobs", *id).await?
                };
                require(
                    j["school_id"] == school_id.to_string()
                        && j["kind"] == "measure_manual"
                        && j["state"] == "succeeded",
                    "请选择真实成功人工计时记录",
                )?;
                let timing: ManualTiming = serde_json::from_value(j["result"]["timing"].clone())
                    .map_err(|_| Error::Conflict)?;
                for v in [
                    timing.group_interval_ms,
                    timing.navigation_ms,
                    timing.input_codepoint_ms,
                    timing.input_settle_ms,
                    timing.observation_ms,
                ] {
                    require((1..=31_536_000_000).contains(&v), "人工计时无效")?;
                }
                timings.push(timing);
            }
            let id = Uuid::now_v7();
            let baseline = ManualBaseline {
                version: id.to_string(),
                samples: timings.len(),
                human_group_min_interval_ms: timings
                    .iter()
                    .map(|v| v.group_interval_ms)
                    .max()
                    .unwrap_or(0),
                navigation_min_ms: timings.iter().map(|v| v.navigation_ms).max().unwrap_or(0),
                input_codepoint_min_ms: timings
                    .iter()
                    .map(|v| v.input_codepoint_ms)
                    .max()
                    .unwrap_or(0),
                input_settle_min_ms: timings.iter().map(|v| v.input_settle_ms).max().unwrap_or(0),
                observation_min_ms: timings.iter().map(|v| v.observation_ms).max().unwrap_or(0),
                evidence_ref: format!("pacing:{id}"),
            };
            sqlx::query("INSERT INTO automation_pacing_profiles(id,name,school_id,validation_ids,baseline,created_by) VALUES($1,$2,$3,$4,$5,$6)").bind(id).bind(name).bind(school_id).bind(json!(validation_ids)).bind(json!(baseline)).bind(actor).execute(&mut **tx).await?;
            object(tx, "automation_pacing_profiles", id).await
        }
        DispatchCommand::CreateFlow {
            flow: f,
            pacing_profile_id,
        } => {
            let actor = platform(tx, p).await?;
            f.validate()?;
            let pacing = object(tx, "automation_pacing_profiles", pacing_profile_id).await?;
            require(
                json!(f.configuration.baseline) == pacing["baseline"],
                "人工基准不能自行声明或修改",
            )?;
            let id = Uuid::now_v7();
            let body = json!(f);
            sqlx::query("INSERT INTO automation_flow_versions(id,name,body,content_digest,pacing_profile_id,created_by) VALUES($1,$2,$3,$4,$5,$6)").bind(id).bind(strv(&body["configuration"],"name")?).bind(&body).bind(automation_digest(&body)).bind(pacing_profile_id).bind(actor).execute(&mut **tx).await?;
            object(tx, "automation_flow_versions", id).await
        }
        DispatchCommand::PublishFlow {
            school_id,
            validation_id,
        } => {
            let actor = platform(tx, p).await?;
            school_auth(tx, actor, school_id, true).await?;
            let f = object(tx, "automation_flow_versions", id(s)?).await?;
            flow(&f["body"])?.validate()?;
            let fid = id(s)?;
            let own: Option<Value> = sqlx::query_scalar(
                "SELECT to_jsonb(j) FROM automation_flow_validations j WHERE id=$1",
            )
            .bind(validation_id)
            .fetch_optional(&mut **tx)
            .await?;
            let (legacy, new) = if let Some(j) = own {
                require(
                    j["school_id"] == school_id.to_string()
                        && j["flow_id"] == fid.to_string()
                        && j["kind"] == "inspect_roles"
                        && j["path_key"] == "generic"
                        && j["state"] == "succeeded",
                    "请选择该通用流程的真实成功验证",
                )?;
                let complete = flow(&f["body"])?;
                for candidate in &complete.specializations {
                    require(
                        flow_evidence(
                            tx,
                            fid,
                            school_id,
                            "inspect_roles",
                            &candidate.key,
                            None,
                            None,
                        )
                        .await?,
                        "专项分支尚未验证",
                    )?;
                }
                for (index, launcher) in complete.launchers.iter().enumerate() {
                    for (path, steps) in [
                        ("search", &launcher.search_steps),
                        ("all_apps", &launcher.all_apps_steps),
                    ] {
                        if !steps.is_empty() {
                            require(
                                flow_evidence(
                                    tx,
                                    fid,
                                    school_id,
                                    "open_application",
                                    "generic",
                                    Some(index as i32),
                                    Some(path),
                                )
                                .await?,
                                "完整桌面路径尚未验证",
                            )?;
                        }
                    }
                }
                (None, Some(validation_id))
            } else {
                let j = object(tx, "device_validation_jobs", validation_id).await?;
                require(
                    j["school_id"] == school_id.to_string()
                        && j["kind"] == "inspect_roles"
                        && j["state"] == "succeeded",
                    "发布需要真实成功页面角色验证",
                )?;
                require(
                    flow(&f["body"])?.specializations.is_empty(),
                    "专项流程请使用 v5 验证入口",
                )?;
                let ids: Vec<Uuid> = serde_json::from_value(j["component_ids"].clone())
                    .map_err(|_| Error::Conflict)?;
                let mut verified = false;
                for cid in ids {
                    let c = object(tx, "automation_component_versions", cid).await?;
                    if c["kind"] == "workflow" {
                        let mut rules = c["body"]["rules"].clone();
                        rules["client"] = f["body"]["configuration"]["rules"]["client"].clone();
                        rules["schema_version"] = json!(2);
                        rules["engine_version"] = json!("otter-ui/2");
                        verified = rules == f["body"]["configuration"]["rules"];
                    }
                }
                require(verified, "角色证据与通用规则不一致")?;
                // Legacy evidence can seed a reusable flow only when each launcher path was actually exercised.
                let jobs:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(j) FROM device_validation_jobs j WHERE school_id=$1 AND kind='open_application' AND state='succeeded'").bind(school_id).fetch_all(&mut **tx).await?;
                for launcher in &flow(&f["body"])?.launchers {
                    for (path, steps) in [
                        ("search", &launcher.search_steps),
                        ("all_apps", &launcher.all_apps_steps),
                    ] {
                        if steps.is_empty() {
                            continue;
                        }
                        let mut found = false;
                        for j in &jobs {
                            if j["result"]["path"] != path {
                                continue;
                            }
                            let ids: Vec<Uuid> = serde_json::from_value(j["component_ids"].clone())
                                .map_err(|_| Error::Conflict)?;
                            for cid in ids {
                                let c = object(tx, "automation_component_versions", cid).await?;
                                if c["kind"] == "launcher" {
                                    let mut v = c["body"]["flow"].clone();
                                    v["client"] = json!(launcher.client);
                                    found |= v == json!(launcher);
                                }
                            }
                        }
                        require(found, "桌面路径证据缺失，请使用 v5 流程验证")?;
                    }
                }
                (Some(validation_id), None)
            };
            let rid = Uuid::now_v7();
            sqlx::query("INSERT INTO automation_flow_releases(id,flow_id,school_id,validation_id,flow_validation_id,created_by) VALUES($1,$2,$3,$4,$5,$6)").bind(rid).bind(fid).bind(school_id).bind(legacy).bind(new).bind(actor).execute(&mut **tx).await?;
            object(tx, "automation_flow_releases", rid).await
        }
        DispatchCommand::RevokeFlow {
            expected_revision,
            reason,
        } => {
            platform(tx, p).await?;
            text(&reason, 1, 1000)?;
            let old = object(tx, "automation_flow_releases", id(s)?).await?;
            revision(&old, expected_revision)?;
            sqlx::query("UPDATE automation_flow_releases SET revoked=true,active=false,revision=revision+1 WHERE id=$1").bind(id(s)?).execute(&mut **tx).await?;
            object(tx, "automation_flow_releases", id(s)?).await
        }
        DispatchCommand::SetEnabled {
            enabled,
            reviewed_history,
        } => {
            require(!enabled || reviewed_history, "开启前请核对历史未决发送")?;
            sqlx::query("INSERT INTO mobile_delivery_settings(school_id,enabled,dispatch_enabled,history_reviewed,evidence_ref) VALUES($1,$2,$2,$3,'business-v5-history-review') ON CONFLICT(school_id) DO UPDATE SET dispatch_enabled=$2,enabled=mobile_delivery_settings.enabled OR $2,history_reviewed=mobile_delivery_settings.history_reviewed OR $3,revision=mobile_delivery_settings.revision+1").bind(school(s)?).bind(enabled).bind(reviewed_history).execute(&mut **tx).await?;
            settings(tx, school(s)?).await
        }
        DispatchCommand::SaveGroup { name, device_ids } => {
            label(&name)?;
            require(
                !device_ids.is_empty() && device_ids.len() <= 100,
                "请选择设备",
            )?;
            let gid = Uuid::now_v7();
            sqlx::query("INSERT INTO automation_device_groups(id,school_id,name) VALUES($1,$2,$3)")
                .bind(gid)
                .bind(school(s)?)
                .bind(name)
                .execute(&mut **tx)
                .await?;
            for d in device_ids {
                scoped_device(tx, school(s)?, d).await?;
                sqlx::query(
                    "INSERT INTO automation_device_group_members(group_id,device_id) VALUES($1,$2)",
                )
                .bind(gid)
                .bind(d)
                .execute(&mut **tx)
                .await?;
            }
            object(tx, "automation_device_groups", gid).await
        }
        DispatchCommand::Assign {
            mut device_ids,
            group_id,
            release_id,
        } => {
            released(tx, release_id, school(s)?, true).await?;
            if let Some(gid) = group_id {
                let g = object(tx, "automation_device_groups", gid).await?;
                require(g["school_id"] == school(s)?.to_string(), "设备组不在学校")?;
                device_ids.extend(
                    sqlx::query_scalar::<_, Uuid>(
                        "SELECT device_id FROM automation_device_group_members WHERE group_id=$1",
                    )
                    .bind(gid)
                    .fetch_all(&mut **tx)
                    .await?,
                );
            }
            device_ids.sort();
            device_ids.dedup();
            require(
                !device_ids.is_empty() && device_ids.len() <= 100,
                "请选择设备范围",
            )?;
            let mut out = Vec::new();
            for d in device_ids {
                let b = scoped_device(tx, school(s)?, d).await?;
                let aid:Uuid=sqlx::query_scalar("INSERT INTO automation_execution_assignments(id,school_id,device_id,sender_account_id,account_binding_revision,release_id) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT(school_id,device_id) DO UPDATE SET release_id=EXCLUDED.release_id,sender_account_id=EXCLUDED.sender_account_id,account_binding_revision=EXCLUDED.account_binding_revision,enabled=true,revision=automation_execution_assignments.revision+1 RETURNING id").bind(Uuid::now_v7()).bind(school(s)?).bind(d).bind(uid(&b,"sender_account_id")?).bind(b["revision"].as_i64().ok_or(Error::Conflict)?).bind(release_id).fetch_one(&mut **tx).await?;
                out.push(object(tx, "automation_execution_assignments", aid).await?);
            }
            Ok(json!({"id":Uuid::now_v7(),"assignments":out}))
        }
        DispatchCommand::Preview { input } => {
            let preview = build_preview(tx, school(s)?, &input, origin).await?;
            let bid = Uuid::now_v7();
            sqlx::query("INSERT INTO automation_dispatch_batches(id,school_id,name,state,input,preview,preview_digest,created_by) VALUES($1,$2,$3,'draft',$4,$5,$6,$7)").bind(bid).bind(school(s)?).bind(&input.name).bind(json!(input)).bind(&preview).bind(automation_digest(&preview)).bind(p.actor()?).execute(&mut **tx).await?;
            object(tx, "automation_dispatch_batches", bid).await
        }
        DispatchCommand::Confirm {
            preview_digest,
            reviewed,
        } => confirm(tx, p, s, &preview_digest, reviewed, origin).await,
        DispatchCommand::ControlBatch {
            expected_revision,
            command,
        } => {
            let b = scoped_object(tx, s, "automation_dispatch_batches").await?;
            revision(&b, expected_revision)?;
            let state = match command.as_str() {
                "pause" => "paused",
                "resume" => "confirmed",
                "stop" => "cancelled",
                _ => return Err(Error::Invalid("控制动作无效".into())),
            };
            require(
                b["state"] != "draft" && b["state"] != "cancelled",
                "批次状态不允许",
            )?;
            sqlx::query(
                "UPDATE automation_dispatch_batches SET state=$2,revision=revision+1 WHERE id=$1",
            )
            .bind(id(s)?)
            .bind(state)
            .execute(&mut **tx)
            .await?;
            sqlx::query("UPDATE automation_dispatch_preparations q SET expires_at=clock_timestamp() FROM automation_task_deliveries d JOIN automation_dispatch_plans p ON p.id=d.plan_id WHERE q.delivery_id=d.id AND p.batch_id=$1").bind(id(s)?).execute(&mut **tx).await?;
            if state == "cancelled" {
                let devices:Vec<Uuid>=sqlx::query_scalar("SELECT DISTINCT c.device_id FROM mobile_execution_claims c JOIN mobile_delivery_run_items m ON m.item_id=c.item_id JOIN automation_dispatch_plans p ON p.run_id=m.run_id WHERE p.batch_id=$1 AND c.state IN ('claimed','prepared','permit_issued')").bind(id(s)?).fetch_all(&mut **tx).await?;
                for d in devices {
                    invalidate(tx, d).await?;
                }
                sqlx::query("UPDATE mobile_delivery_items i SET state='cancelled',revision=revision+1 FROM mobile_delivery_run_items m JOIN automation_dispatch_plans p ON p.run_id=m.run_id WHERE i.id=m.item_id AND p.batch_id=$1 AND i.state IN ('queued','claimed','prepared')").bind(id(s)?).execute(&mut **tx).await?;
                sqlx::query("UPDATE delivery_dedup_reservations r SET active=false FROM mobile_delivery_items i JOIN mobile_delivery_run_items m ON m.item_id=i.id JOIN automation_dispatch_plans p ON p.run_id=m.run_id WHERE r.item_id=i.id AND p.batch_id=$1 AND i.state='cancelled'").bind(id(s)?).execute(&mut **tx).await?;
                sqlx::query("UPDATE mobile_delivery_tasks t SET state='stopped',revision=revision+1 FROM mobile_delivery_strategy_runs r JOIN automation_dispatch_plans p ON p.run_id=r.id WHERE t.id=r.task_id AND p.batch_id=$1").bind(id(s)?).execute(&mut **tx).await?;
            }
            batch_detail(tx, school(s)?, id(s)?).await
        }
        DispatchCommand::Receipt { plan_digest } => {
            let (_, plan) = delivery(tx, p, id(s)?).await?;
            require(plan["content_digest"] == plan_digest, "计划摘要不一致")?;
            sqlx::query("UPDATE automation_task_deliveries SET received_at=coalesce(received_at,clock_timestamp()) WHERE id=$1").bind(id(s)?).execute(&mut **tx).await?;
            object(tx, "automation_task_deliveries", id(s)?).await
        }
        DispatchCommand::PreparationLease {
            plan_digest,
            authorization_id,
        } => {
            require(enabled, "手机发送总开关未开启")?;
            let (del, plan) = delivery(tx, p, id(s)?).await?;
            require(
                !del["received_at"].is_null()
                    && del["cancelled"] == false
                    && plan["content_digest"] == plan_digest,
                "投递未接收或已取消",
            )?;
            let (d, g) = device(tx, p).await?;
            let auth = authorized(
                tx,
                d,
                uid(&plan, "school_id")?,
                uid(&plan, "sender_account_id")?,
            )
            .await?;
            require(auth.authorization_id == authorization_id, "授权已改变")?;
            let i = current_item(tx, &plan).await?;
            require(i["state"] == "queued", "当前群已开始，请恢复原请求")?;
            match candidate(tx, d, uid(&plan, "sender_account_id")?).await? {
                Some(Ok(next)) => require(next["id"] == i["id"], "请先处理较早的任务")?,
                _ => return Err(Error::Policy("no_ready_task")),
            }

            configured::validate(tx, &i).await?;
            active_school(tx, uid(&plan, "school_id")?).await?;
            require(
                settings(tx, uid(&plan, "school_id")?).await?["dispatch_enabled"] == true,
                "v5 未启用",
            )?;
            let blocked:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_execution_claims WHERE device_id=$1 AND state IN ('claimed','prepared','permit_issued')) OR EXISTS(SELECT 1 FROM delivery_usage WHERE sender_account_id=$2 AND state IN ('pending','unknown'))").bind(d).bind(uid(&plan,"sender_account_id")?).fetch_one(&mut **tx).await?;
            require(!blocked, "先恢复未决发送")?;
            let maintenance:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM automation_flow_validations WHERE device_id=$1 AND state='accepted' AND expires_at>clock_timestamp())").bind(d).fetch_one(&mut **tx).await?;
            require(!maintenance, "维护任务正在使用界面")?;
            let lease = Uuid::now_v7();
            let generation = Uuid::now_v7();
            sqlx::query("INSERT INTO automation_dispatch_preparations(id,delivery_id,item_id,credential_generation,generation,authorization_id,fence) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT(delivery_id,item_id) DO UPDATE SET id=EXCLUDED.id,credential_generation=EXCLUDED.credential_generation,generation=EXCLUDED.generation,authorization_id=EXCLUDED.authorization_id,fence=EXCLUDED.fence,ready=false,path_key='generic',path_digest=NULL,stage='preparing_app',expires_at=clock_timestamp()+interval '2 minutes'").bind(lease).bind(id(s)?).bind(uid(&i,"id")?).bind(g).bind(generation).bind(auth.authorization_id).bind(i["fence"].as_i64().ok_or(Error::Conflict)?+1).execute(&mut **tx).await?;
            let mut q = object(tx, "automation_dispatch_preparations", lease).await?;
            q["server_time"] = json!(db_now(tx).await?);
            q["plan"] = plan;
            q["delivery_id"] = del["id"].clone();
            Ok(q)
        }
        DispatchCommand::Stage {
            generation,
            item_id,
            stage,
            path_key,
        } => {
            let (_, plan) = delivery(tx, p, id(s)?).await?;
            let auth = authorized(
                tx,
                uid(&plan, "device_id")?,
                uid(&plan, "school_id")?,
                uid(&plan, "sender_account_id")?,
            )
            .await?;
            let q:Value=sqlx::query_scalar("SELECT to_jsonb(q) FROM automation_dispatch_preparations q WHERE q.delivery_id=$1 AND q.item_id=$2 AND q.generation=$3 AND q.expires_at>clock_timestamp()").bind(id(s)?).bind(item_id).bind(generation).fetch_optional(&mut **tx).await?.ok_or(Error::Policy("readiness_expired"))?;
            require(
                q["authorization_id"] == auth.authorization_id.to_string(),
                "授权已变化",
            )?;
            let (_, credential) = device(tx, p).await?;
            require(q["credential_generation"] == credential, "准备凭据已变化")?;
            let item = current_item(tx, &plan).await?;
            require(item["id"] == item_id.to_string(), "当前群已变化")?;
            validate(tx, &item, None).await?;
            let issued: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM mobile_send_permits WHERE item_id=$1)",
            )
            .bind(item_id)
            .fetch_one(&mut **tx)
            .await?;
            require(!issued, "许可申请后路径已冻结")?;
            let f = flow(&plan["body"]["flow"])?;
            require(
                path_key == "generic" || f.specializations.iter().any(|v| v.key == path_key),
                "路径不存在",
            )?;
            require(
                matches!(stage.as_str(), "preparing_app" | "ready"),
                "阶段无效",
            )?;
            let pd = automation_digest(
                &json!({"plan_digest":plan["content_digest"],"path_key":path_key}),
            );
            sqlx::query("UPDATE automation_dispatch_preparations SET stage=$2,path_key=$3,path_digest=$4,ready=$5,expires_at=clock_timestamp()+interval '2 minutes' WHERE id=$1").bind(uid(&q,"id")?).bind(&stage).bind(path_key).bind(pd).bind(stage=="ready").execute(&mut **tx).await?;
            object(tx, "automation_dispatch_preparations", uid(&q, "id")?).await
        }
        DispatchCommand::Prepare {
            fence,
            snapshot_digest,
            observed_account_id,
            observed_group_id,
            plan_digest,
            path_digest,
            checks_passed,
            verification,
        } => {
            let (_, i) = ready_claim(tx, p, s, fence).await?;
            visible::verification(&i, Some(&verification), 5)?;
            validate_snapshot(tx, &i, origin).await?;
            require(
                checks_passed
                    && i["snapshot_digest"] == snapshot_digest
                    && i["sender_account_id"] == observed_account_id.to_string()
                    && i["group_id"] == observed_group_id.to_string()
                    && i["snapshot"]["dispatch_plan"]["content_digest"] == plan_digest,
                "准备核验不匹配",
            )?;
            let path:Option<String>=sqlx::query_scalar("SELECT q.path_digest FROM automation_dispatch_preparations q JOIN automation_task_deliveries d ON d.id=q.delivery_id WHERE d.plan_id=$1 AND q.item_id=$2 AND q.ready AND q.expires_at>clock_timestamp()").bind(uid(&i["snapshot"]["dispatch_plan"],"id")?).bind(uid(&i,"id")?).fetch_optional(&mut **tx).await?;
            require(path.as_deref() == Some(&path_digest), "执行路径已变化")?;
            let set = active_school(tx, uid(&i, "school_id")?).await?;
            let prep = Uuid::now_v7();
            sqlx::query("UPDATE mobile_execution_claims SET state='prepared',preparation_id=$2,preparation_settings_revision=$3 WHERE id=$1").bind(id(s)?).bind(prep).bind(set["revision"].as_i64().ok_or(Error::Conflict)?).execute(&mut **tx).await?;
            sqlx::query(
                "UPDATE mobile_delivery_items SET state='prepared',revision=revision+1 WHERE id=$1",
            )
            .bind(uid(&i, "id")?)
            .execute(&mut **tx)
            .await?;
            Ok(json!({"id":prep,"claim_id":id(s)?,"fence":fence,"snapshot_digest":snapshot_digest}))
        }
        DispatchCommand::Failure { failure } => {
            failure.validate()?;
            let (_, plan) = delivery(tx, p, id(s)?).await?;
            require(plan["content_digest"] == failure.plan_digest, "计划不匹配")?;
            let i = object(tx, "mobile_delivery_items", failure.item_id).await?;
            require(
                i["snapshot"]["execution_target"]["run_id"] == plan["run_id"],
                "群项不属于计划",
            )?;
            let body = json!(failure);
            let old:Option<Value>=sqlx::query_scalar("SELECT to_jsonb(f)||jsonb_build_object('occurrences',count(*) OVER(PARTITION BY fingerprint)) FROM automation_execution_failures f WHERE delivery_id=$1 AND event_id=$2").bind(id(s)?).bind(failure.event_id).fetch_optional(&mut **tx).await?;
            if let Some(old) = old {
                require(old["body"] == body, "失败事件冲突")?;
                return Ok(old);
            }
            let maybe: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM mobile_send_permits WHERE item_id=$1)",
            )
            .bind(failure.item_id)
            .fetch_one(&mut **tx)
            .await?;
            require(
                !matches!(
                    i["state"].as_str(),
                    Some("ui_observed_sent" | "manually_sent")
                ),
                "成功项不能反馈执行失败",
            )?;
            require(
                (!maybe && failure.send_state == "not_sent")
                    || (maybe && failure.send_state == "unknown" && i["state"] == "unknown"),
                "发送状态必须先按原结果协议闭合",
            )?;
            let current = current_item(tx, &plan).await?;
            require(
                current["id"] == i["id"]
                    && (i["fence"] == failure.fence
                        || (i["state"] == "queued"
                            && i["fence"].as_i64().is_some_and(|n| n + 1 == failure.fence))),
                "失败代次或群进度已变化",
            )?;
            let fid = Uuid::now_v7();
            let fingerprint = automation_digest(
                &json!({"app":"wecom","flow":plan["body"]["flow_digest"],"path":failure.path_key,"step":failure.step,"reason":failure.reason}),
            );
            sqlx::query("INSERT INTO automation_execution_failures(id,delivery_id,school_id,event_id,body,fingerprint) VALUES($1,$2,$3,$4,$5,$6)").bind(fid).bind(id(s)?).bind(uid(&plan,"school_id")?).bind(failure.event_id).bind(body).bind(fingerprint).execute(&mut **tx).await?;
            sqlx::query("UPDATE mobile_delivery_run_progress SET pause_reason=$2,revision=revision+1 WHERE id=$1").bind(uid(&plan,"run_id")?).bind(if maybe{"unknown_requires_review"}else{"execution_failed_not_sent"}).execute(&mut **tx).await?;
            sqlx::query("UPDATE automation_dispatch_preparations SET expires_at=clock_timestamp() WHERE delivery_id=$1").bind(id(s)?).execute(&mut **tx).await?;
            object(tx, "automation_execution_failures", fid).await
        }
        DispatchCommand::Diagnostic {
            failure_id,
            event_id,
            versions,
        } => {
            delivery(tx, p, id(s)?).await?;
            let f = object(tx, "automation_execution_failures", failure_id).await?;
            require(
                f["delivery_id"] == id(s)?.to_string(),
                "失败不属于本设备投递",
            )?;
            let body = json!({"failure_id":failure_id,"versions":versions});
            let old: Option<Value> = sqlx::query_scalar(
                "SELECT body FROM automation_dispatch_events WHERE delivery_id=$1 AND event_id=$2",
            )
            .bind(id(s)?)
            .bind(event_id)
            .fetch_optional(&mut **tx)
            .await?;
            if let Some(old) = old {
                require(old == body, "诊断事件冲突")?;
            } else {
                sqlx::query("INSERT INTO automation_dispatch_events(id,delivery_id,event_id,body) VALUES($1,$2,$3,$4)").bind(Uuid::now_v7()).bind(id(s)?).bind(event_id).bind(body).execute(&mut **tx).await?;
            }
            Ok(json!({"id":event_id}))
        }
        DispatchCommand::Authorization { receipt: r } => {
            let (d, _) = device(tx, p).await?;
            let scope = &r.scope;
            require(
                scope.format == "business_application_scope/2"
                    && scope.origin == origin
                    && scope.device_id == d
                    && scope.app_key == "wecom"
                    && scope.capability == "group_text"
                    && r.revision > 0
                    && matches!(r.state.as_str(), "effective" | "revoked")
                    && ((r.state == "revoked") == r.revoked_at.is_some()),
                "业务授权回执无效",
            )?;
            require(
                !scope.schools.is_empty() && scope.schools.len() <= 100,
                "业务学校范围无效",
            )?;
            if r.state == "effective" {
                let b = binding(tx, d).await?;
                require(
                    scope.sender_account_id == uid(&b, "sender_account_id")?,
                    "authorization_context_changed",
                )?;
                if let Some(expected) = r.binding_revision {
                    require(b["revision"] == expected, "authorization_context_changed")?;
                }
                for school in &scope.schools {
                    scoped_device(tx, *school, d).await?;
                }
            } else {
                // Withdrawal remains possible after account or school access changed.
                // The device can only withdraw its own previously recorded grant.
                let old:Option<Value> = sqlx::query_scalar("SELECT body FROM automation_business_authorizations WHERE device_id=$1 AND authorization_id=$2 ORDER BY revision DESC LIMIT 1")
                    .bind(d).bind(r.authorization_id).fetch_optional(&mut **tx).await?;
                if let Some(old) = old {
                    require(
                        old["scope"] == json!(scope) && old["granted_at"] == json!(r.granted_at),
                        "业务授权回执无效",
                    )?;
                }
                // A local grant may have been revoked before its first upload.
                // Such a withdrawal grants no execution rights and still advances the fence.
            }
            let max:i64=sqlx::query_scalar("SELECT coalesce(max(revision),0) FROM automation_business_authorizations WHERE device_id=$1").bind(d).fetch_one(&mut **tx).await?;
            require(r.revision > max, "授权回执代次过旧")?;
            sqlx::query("INSERT INTO automation_business_authorizations(device_id,revision,authorization_id,body) VALUES($1,$2,$3,$4)").bind(d).bind(r.revision).bind(r.authorization_id).bind(json!(r)).execute(&mut **tx).await?;
            Ok(json!({"id":r.authorization_id,"revision":r.revision}))
        }
        DispatchCommand::Heartbeat {
            binding_revision,
            sender_account_id,
            permissions_ready,
        } => {
            let (d, _) = device(tx, p).await?;
            let b = binding(tx, d).await?;
            require(
                b["revision"] == binding_revision
                    && b["sender_account_id"] == sender_account_id.to_string(),
                "设备账号已变化",
            )?;
            sqlx::query("UPDATE execution_devices SET last_seen_at=clock_timestamp(),execution_protocol_version=5,supported_protocol_versions=ARRAY[1,2,3,4,5] WHERE id=$1").bind(d).execute(&mut **tx).await?;
            Ok(
                json!({"id":d,"binding":b,"permissions_ready_declared":permissions_ready,"server_time":db_now(tx).await?,"heartbeat_interval_seconds":30}),
            )
        }
        DispatchCommand::RebuildRemaining {
            expected_revision,
            release_id,
            reviewed,
        } => {
            require(reviewed, "需要核对确定未发的剩余群")?;
            let old = scoped_object(tx, s, "automation_dispatch_batches").await?;
            revision(&old, expected_revision)?;
            require(old["state"] == "paused", "请先暂停原批次")?;
            released(tx, release_id, school(s)?, true).await?;
            let targets = remaining_targets(tx, id(s)?, release_id).await?;
            let original: DispatchInput =
                serde_json::from_value(old["input"].clone()).map_err(|_| Error::Conflict)?;
            let input = DispatchInput {
                name: original.name,
                text_content: original.text_content,
                interval_seconds: original.interval_seconds,
                targets,
            };
            let mut preview = build_preview(tx, school(s)?, &input, origin).await?;
            preview["replacement"] =
                json!({"batch_id":id(s)?,"revision":expected_revision,"release_id":release_id});
            let bid = Uuid::now_v7();
            let root = old["scope_owner_id"]
                .as_str()
                .map(Uuid::parse_str)
                .transpose()
                .map_err(|_| Error::Conflict)?
                .unwrap_or(id(s)?);
            sqlx::query("INSERT INTO automation_dispatch_batches(id,school_id,name,state,input,preview,preview_digest,created_by,scope_owner_id) VALUES($1,$2,$3,'draft',$4,$5,$6,$7,$8)").bind(bid).bind(school(s)?).bind(&input.name).bind(json!(input)).bind(&preview).bind(automation_digest(&preview)).bind(p.actor()?).bind(root).execute(&mut **tx).await?;
            object(tx, "automation_dispatch_batches", bid).await
        }
    }
}

async fn flow_evidence(
    tx: &mut Tx<'_>,
    flow: Uuid,
    school: Uuid,
    kind: &str,
    path: &str,
    index: Option<i32>,
    navigation: Option<&str>,
) -> Result<bool> {
    Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM automation_flow_validations WHERE flow_id=$1 AND school_id=$2 AND kind=$3 AND path_key=$4 AND launcher_index IS NOT DISTINCT FROM $5 AND state='succeeded' AND ($6::text IS NULL OR result->>'path'=$6))").bind(flow).bind(school).bind(kind).bind(path).bind(index).bind(navigation).fetch_one(&mut **tx).await?)
}
async fn validation_document(tx: &mut Tx<'_>, j: &Value) -> Result<Value> {
    let mut j = object(tx, "automation_flow_validations", uid(j, "id")?).await?;
    if j["flow_id"].is_null() {
        j["application"] = j["manual_application"].clone();
        j["components"] = json!([]);
        j["protocol_version"] = json!(5);
        let b = binding(tx, uid(&j, "device_id")?).await?;
        j["sender_account_id"] = b["sender_account_id"].clone();
        return Ok(j);
    }
    let f = object(tx, "automation_flow_versions", uid(&j, "flow_id")?).await?;
    let flow = flow(&f["body"])?;
    let rules = if j["path_key"] == "generic" {
        json!(flow.configuration.rules)
    } else {
        json!(
            flow.specializations
                .iter()
                .find(|v| j["path_key"] == v.key)
                .ok_or(Error::Conflict)?
                .rules
        )
    };
    let component = json!({"kind":"workflow","rules":rules});
    j["components"] = json!([{"id":f["id"],"kind":"workflow","body":component,"content_digest":automation_digest(&component)}]);
    j["flow"] = f["body"].clone();
    j["protocol_version"] = json!(5);
    j["application"] = json!({"client":rules["client"],"label":flow.application_label});
    let b = binding(tx, uid(&j, "device_id")?).await?;
    j["sender_account_id"] = b["sender_account_id"].clone();
    Ok(j)
}
pub(super) async fn query(
    tx: &mut Tx<'_>,
    p: &Principal,
    s: &Scope,
    q: DispatchQuery,
    limit: i64,
    offset: i64,
) -> Result<Value> {
    if let Some(school) = s.school {
        school_auth(tx, p.actor()?, school, true).await?;
    }
    match q {
        DispatchQuery::Validations|DispatchQuery::DeviceValidations=>{
            let d=if matches!(q,DispatchQuery::DeviceValidations){require(p.protocol()==5,"维护要求 v5")?;let d=device(tx,p).await?.0;authorization_gate(tx,d).await?;Some(d)}else{platform(tx,p).await?;None};
            let rows:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(j) FROM automation_flow_validations j WHERE ($1::uuid IS NULL OR j.device_id=$1) AND ($2::uuid IS NULL OR school_id=$2) AND ($1::uuid IS NULL OR (state IN ('queued','accepted') AND expires_at>clock_timestamp())) ORDER BY created_at DESC,id DESC LIMIT $3 OFFSET $4").bind(d).bind(s.school).bind(limit).bind(offset).fetch_all(&mut **tx).await?;
            let mut out=Vec::new();for j in rows {
                if let Some(d)=d {let b=binding(tx,d).await?; if authorized(tx,d,uid(&j,"school_id")?,uid(&b,"sender_account_id")?).await.is_err(){continue;}}
                out.push(validation_document(tx,&j).await?);
            }Ok(json!(out))
        }

        DispatchQuery::Flows=>{platform(tx,p).await?;Ok(json!(sqlx::query_scalar::<_,Value>("SELECT to_jsonb(v) FROM automation_flow_versions v ORDER BY created_at DESC,id DESC LIMIT $1 OFFSET $2").bind(limit).bind(offset).fetch_all(&mut **tx).await?))}
        DispatchQuery::Pacing=>{platform(tx,p).await?;Ok(json!(sqlx::query_scalar::<_,Value>("SELECT to_jsonb(v) FROM automation_pacing_profiles v ORDER BY created_at DESC,id DESC LIMIT $1 OFFSET $2").bind(limit).bind(offset).fetch_all(&mut **tx).await?))}
        DispatchQuery::Releases=>{if s.school.is_none(){platform(tx,p).await?;}Ok(json!(sqlx::query_scalar::<_,Value>("SELECT to_jsonb(r)||jsonb_build_object('flow',v.body,'name',v.name,'flow_digest',v.content_digest) FROM automation_flow_releases r JOIN automation_flow_versions v ON v.id=r.flow_id WHERE ($1::uuid IS NULL OR r.school_id=$1) ORDER BY r.created_at DESC,r.id DESC LIMIT $2 OFFSET $3").bind(s.school).bind(limit).bind(offset).fetch_all(&mut **tx).await?))}
        DispatchQuery::DeviceGroups=>Ok(json!(sqlx::query_scalar::<_,Value>("SELECT to_jsonb(g)||jsonb_build_object('device_ids',coalesce((SELECT jsonb_agg(m.device_id ORDER BY m.device_id) FROM automation_device_group_members m WHERE m.group_id=g.id),'[]'::jsonb)) FROM automation_device_groups g WHERE school_id=$1 ORDER BY id LIMIT $2 OFFSET $3").bind(school(s)?).bind(limit).bind(offset).fetch_all(&mut **tx).await?)),
        DispatchQuery::Assignments=>Ok(json!(sqlx::query_scalar::<_,Value>("SELECT to_jsonb(a)||jsonb_build_object('device_name',d.name,'account_name',s.visible_profile->>'account_name','flow_name',v.name,'flow',v.body,'online',p.state='online','protocol',d.execution_protocol_version) FROM automation_execution_assignments a JOIN execution_devices d ON d.id=a.device_id JOIN device_control_presence p ON p.id=d.id JOIN sending_accounts s ON s.id=a.sender_account_id JOIN automation_flow_releases r ON r.id=a.release_id JOIN automation_flow_versions v ON v.id=r.flow_id WHERE a.school_id=$1 ORDER BY a.id LIMIT $2 OFFSET $3").bind(school(s)?).bind(limit).bind(offset).fetch_all(&mut **tx).await?)),
        DispatchQuery::Batches=>Ok(json!(sqlx::query_scalar::<_,Value>("SELECT to_jsonb(b)-'preview' FROM automation_dispatch_batches b WHERE school_id=$1 ORDER BY created_at DESC,id DESC LIMIT $2 OFFSET $3").bind(school(s)?).bind(limit).bind(offset).fetch_all(&mut **tx).await?)),
        DispatchQuery::Batch=>batch_detail(tx,school(s)?,id(s)?).await,
        DispatchQuery::Failures=>{if s.school.is_none(){platform(tx,p).await?;}Ok(json!(sqlx::query_scalar::<_,Value>("SELECT issue.body FROM (SELECT DISTINCT ON(fingerprint) to_jsonb(f)||jsonb_build_object('occurrences',count(*) OVER(PARTITION BY fingerprint)) AS body,created_at,id FROM automation_execution_failures f WHERE ($1::uuid IS NULL OR school_id=$1) ORDER BY fingerprint,created_at DESC,id DESC) issue ORDER BY issue.created_at DESC,issue.id DESC LIMIT $2 OFFSET $3").bind(s.school).bind(limit).bind(offset).fetch_all(&mut **tx).await?))}
        DispatchQuery::Inbox=>{
            require(p.protocol()==5,"收件箱要求 v5")?;let(d,_)=device(tx,p).await?;authorization_gate(tx,d).await?;let auth=receipt(tx,d).await?;let b=binding(tx,d).await?;
            Ok(json!(sqlx::query_scalar::<_,Value>("SELECT to_jsonb(d)||jsonb_build_object('plan',to_jsonb(p),'batch_state',b.state,'progress',to_jsonb(q)) FROM automation_task_deliveries d JOIN automation_dispatch_plans p ON p.id=d.plan_id JOIN automation_dispatch_batches b ON b.id=p.batch_id JOIN mobile_delivery_run_progress q ON q.id=p.run_id WHERE d.device_id=$1 AND p.sender_account_id=$4 AND p.school_id=ANY($5) AND EXISTS(SELECT 1 FROM schools school JOIN school_memberships member ON member.school_id=school.id JOIN sending_accounts account ON account.responsible_actor_id=member.account_id JOIN accounts actor ON actor.id=member.account_id WHERE school.id=p.school_id AND school.active AND member.role='admin' AND actor.active AND account.id=p.sender_account_id AND account.enabled) AND NOT d.cancelled AND b.state IN ('confirmed','paused') AND EXISTS(SELECT 1 FROM mobile_delivery_run_items m WHERE m.run_id=p.run_id AND m.position>=q.current_position) ORDER BY p.created_at,p.id LIMIT least($2,1) OFFSET $3").bind(d).bind(limit).bind(offset).bind(uid(&b,"sender_account_id")?).bind(auth.scope.schools).fetch_all(&mut **tx).await?))
        }
        DispatchQuery::SetupStatus => {
            require(p.protocol() == 5, "设置要求 v5")?;
            let (d, _) = device(tx, p).await?;
            let dev = object(tx, "execution_devices", d).await?;
            // No consent or binding prerequisite: a newly paired device must see
            // what the administrator still needs to prepare.
            let b: Option<Value> = sqlx::query_scalar("SELECT to_jsonb(b) FROM device_account_bindings b JOIN sending_accounts a ON a.id=b.sender_account_id WHERE b.device_id=$1 AND a.enabled")
                .bind(d).fetch_optional(&mut **tx).await?;
            let mut result = DeviceSetupStatus { id:d, protocol_version:5, checked_at:db_now(tx).await?, device_name:strv(&dev,"name")?.into(), account_name:None, sender_account_id:None, binding_revision:None, scope_source:"responsible_actor_school_admin".into(), schools:vec![] };
            if let Some(b) = b {
                let account = object(tx,"sending_accounts",uid(&b,"sender_account_id")?).await?;
                result.account_name=account["visible_profile"]["account_name"].as_str().map(str::to_owned);
                result.sender_account_id=Some(uid(&b,"sender_account_id")?);
                result.binding_revision=b["revision"].as_i64();
                let rows:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('id',s.id,'name',s.name,'dispatch_enabled',coalesce(settings.dispatch_enabled,false),'assignment_ready',EXISTS(SELECT 1 FROM automation_execution_assignments x JOIN automation_flow_releases r ON r.id=x.release_id WHERE x.school_id=s.id AND x.device_id=$2 AND x.sender_account_id=$3 AND x.account_binding_revision=$4 AND x.enabled AND r.active AND NOT r.revoked)) FROM schools s JOIN school_memberships m ON m.school_id=s.id JOIN accounts actor ON actor.id=m.account_id LEFT JOIN mobile_delivery_settings settings ON settings.school_id=s.id WHERE m.account_id=$1 AND m.role='admin' AND s.active AND actor.active ORDER BY s.id LIMIT 100")
                    .bind(uid(&account,"responsible_actor_id")?).bind(d).bind(result.sender_account_id).bind(result.binding_revision).fetch_all(&mut **tx).await?;
                result.schools=rows.into_iter().map(serde_json::from_value).collect::<std::result::Result<_,_>>().map_err(|_|Error::Conflict)?;
            }
            Ok(json!(result))
        }
        DispatchQuery::AuthorizationContext=>{
            require(p.protocol()==5,"授权要求 v5")?;let(d,_)=device(tx,p).await?;let b=binding(tx,d).await?;let a=object(tx,"sending_accounts",uid(&b,"sender_account_id")?).await?;
            let schools:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('id',s.id,'name',s.name) FROM schools s JOIN school_memberships m ON m.school_id=s.id JOIN accounts a ON a.id=m.account_id WHERE m.account_id=$1 AND m.role='admin' AND s.active AND a.active ORDER BY s.id LIMIT 100").bind(uid(&a,"responsible_actor_id")?).fetch_all(&mut **tx).await?;
            let revision:i64=sqlx::query_scalar("SELECT coalesce(max(revision),0) FROM automation_business_authorizations WHERE device_id=$1").bind(d).fetch_one(&mut **tx).await?;
            let dev=object(tx,"execution_devices",d).await?;
            Ok(json!({"id":d,"extension":"business_application_scope/2","device_name":dev["name"],"account_name":a["visible_profile"]["account_name"],"authorization_revision":revision,"binding_revision":b["revision"],"scope_source":"responsible_actor_school_admin","schools":schools,"scope":{"format":"business_application_scope/2","origin":null,"device_id":d,"sender_account_id":b["sender_account_id"],"schools":schools.iter().map(|v|v["id"].clone()).collect::<Vec<_>>(),"app_key":"wecom","capability":"group_text"}}))
        }
    }
}
