//! Visible UI targets and reusable strategies. No external system identifiers or selectors.
use super::*;

pub(super) fn snapshot_identity(mut v: Value) -> Result<Value> {
    if v["identity_mode"] == "ui_visible" {
        if v["verification_state"] != "reviewed" {
            return Err(Error::Policy("profile_review_required"));
        }
        let mut o = serde_json::Map::new();
        for key in [
            "id",
            "school_id",
            "name",
            "client_kind",
            "identity_mode",
            "visible_profile",
            "profile_revision",
            "verification_state",
            "enabled",
        ] {
            if let Some(x) = v.get(key) {
                o.insert(key.into(), x.clone());
            }
        }
        Ok(Value::Object(o))
    } else {
        // Preserve the exact pre-migration shape of legacy snapshots.
        for key in [
            "identity_mode",
            "visible_profile",
            "profile_revision",
            "verification_state",
        ] {
            v.as_object_mut().ok_or(Error::Conflict)?.remove(key);
        }
        Ok(v)
    }
}
pub(super) fn requirements(set: &Value, protocol: u32) -> Value {
    let mut r = json!({"protocol_version":protocol,"client_version":set["approved_client_version"],"script_version":set["approved_script_version"],"baseline_version":set["approved_baseline_version"],"no_injection":true,"no_hook":true,"no_private_protocol":true,"no_client_data_modification":true,"local_invocation_consumption_required":true});
    if protocol == 2 {
        r["no_external_ids"] = json!(true);
        r["no_resource_ids"] = json!(true);
    }
    r
}
#[allow(clippy::too_many_arguments)]
pub(super) async fn create_account(
    tx: &mut Tx<'_>,
    p: &Principal,
    kind: String,
    namespace: Option<String>,
    key: Option<String>,
    actor: Uuid,
    evidence: String,
    profile: Option<AccountProfile>,
) -> Result<Value> {
    platform(tx, p).await?;
    require(
        kind == "wecom" && namespace.is_none() && key.is_none(),
        "界面模式不接受外部身份标识",
    )?;
    let profile = profile
        .ok_or(Error::Policy("target_profile_missing"))?
        .normalized()?;
    let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sending_accounts WHERE identity_mode='ui_visible' AND visible_profile->>'company_name'=$1 AND visible_profile->>'account_name'=$2)").bind(&profile.company_name).bind(&profile.account_name).fetch_one(&mut **tx).await?;
    if exists {
        return Err(Error::Policy("duplicate_visible_target"));
    }
    let id = Uuid::now_v7();
    sqlx::query("INSERT INTO sending_accounts(id,client_kind,identity_mode,visible_profile,responsible_actor_id,identity_evidence_ref) VALUES($1,'wecom','ui_visible',$2,$3,$4)").bind(id).bind(json!(profile)).bind(actor).bind(text(&evidence,1,500)?).execute(&mut **tx).await?;
    object(tx, "sending_accounts", id).await
}
#[allow(clippy::too_many_arguments)]
pub(super) async fn create_group(
    tx: &mut Tx<'_>,
    p: &Principal,
    s: &Scope,
    name: String,
    kind: String,
    namespace: Option<String>,
    key: Option<String>,
    evidence: String,
    official: Option<Uuid>,
    profile: Option<GroupProfile>,
) -> Result<Value> {
    school_auth(tx, p.actor()?, school(s)?, true).await?;
    require(
        kind == "wecom" && namespace.is_none() && key.is_none(),
        "界面模式不接受外部身份标识",
    )?;
    let name = label(&name)?;
    let profile = profile
        .ok_or(Error::Policy("target_profile_missing"))?
        .normalized()?;
    ensure_unique_group(tx, school(s)?, &name, None).await?;
    let id = Uuid::now_v7();
    sqlx::query("INSERT INTO delivery_groups(id,school_id,name,client_kind,identity_mode,visible_profile,identity_evidence_ref,wecom_group_id) VALUES($1,$2,$3,'wecom','ui_visible',$4,$5,$6)").bind(id).bind(school(s)?).bind(name).bind(json!(profile)).bind(text(&evidence,1,500)?).bind(official).execute(&mut **tx).await?;
    object(tx, "delivery_groups", id).await
}
async fn ensure_unique_group(
    tx: &mut Tx<'_>,
    school: Uuid,
    name: &str,
    except: Option<Uuid>,
) -> Result<()> {
    let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM delivery_groups WHERE school_id=$1 AND name=$2 AND identity_mode='ui_visible' AND ($3::uuid IS NULL OR id<>$3))").bind(school).bind(name).bind(except).fetch_one(&mut **tx).await?;
    if exists {
        Err(Error::Policy("duplicate_visible_target"))
    } else {
        Ok(())
    }
}
pub(super) async fn update_account(
    tx: &mut Tx<'_>,
    p: &Principal,
    s: &Scope,
    rev: i64,
    profile: AccountProfile,
    reason: String,
) -> Result<Value> {
    platform(tx, p).await?;
    text(&reason, 1, 1000)?;
    let old = object(tx, "sending_accounts", id(s)?).await?;
    revision(&old, rev)?;
    require(old["identity_mode"] == "ui_visible", "目标不是界面模式")?;
    let profile = profile.normalized()?;
    let duplicate:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sending_accounts WHERE id<>$1 AND identity_mode='ui_visible' AND visible_profile->>'company_name'=$2 AND visible_profile->>'account_name'=$3)").bind(id(s)?).bind(&profile.company_name).bind(&profile.account_name).fetch_one(&mut **tx).await?;
    if duplicate {
        return Err(Error::Policy("duplicate_visible_target"));
    }
    invalidate_account(tx, id(s)?).await?;
    sqlx::query("UPDATE sending_accounts SET visible_profile=$2,profile_revision=profile_revision+1,verification_state='pending',revision=revision+1 WHERE id=$1").bind(id(s)?).bind(json!(profile)).execute(&mut **tx).await?;
    object(tx, "sending_accounts", id(s)?).await
}
async fn invalidate_account(tx: &mut Tx<'_>, a: Uuid) -> Result<()> {
    let devices: Vec<Uuid> = sqlx::query_scalar(
        "SELECT device_id FROM device_account_bindings WHERE sender_account_id=$1",
    )
    .bind(a)
    .fetch_all(&mut **tx)
    .await?;
    for d in devices {
        invalidate(tx, d).await?;
    }
    Ok(())
}
#[allow(clippy::too_many_arguments)]
pub(super) async fn update_group(
    tx: &mut Tx<'_>,
    p: &Principal,
    s: &Scope,
    rev: i64,
    name: String,
    profile: GroupProfile,
    reason: String,
) -> Result<Value> {
    school_auth(tx, p.actor()?, school(s)?, true).await?;
    text(&reason, 1, 1000)?;
    let old = scoped_object(tx, s, "delivery_groups").await?;
    revision(&old, rev)?;
    require(old["identity_mode"] == "ui_visible", "目标不是界面模式")?;
    let name = label(&name)?;
    let profile = profile.normalized()?;
    ensure_unique_group(tx, school(s)?, &name, Some(id(s)?)).await?;
    let accounts: Vec<Uuid> =
        sqlx::query_scalar("SELECT sender_account_id FROM sender_group_bindings WHERE group_id=$1")
            .bind(id(s)?)
            .fetch_all(&mut **tx)
            .await?;
    for a in accounts {
        invalidate_account(tx, a).await?;
    }
    sqlx::query("UPDATE delivery_groups SET name=$2,visible_profile=$3,profile_revision=profile_revision+1,verification_state='pending',revision=revision+1 WHERE id=$1").bind(id(s)?).bind(name).bind(json!(profile)).execute(&mut **tx).await?;
    object(tx, "delivery_groups", id(s)?).await
}
pub(super) async fn review(
    tx: &mut Tx<'_>,
    p: &Principal,
    s: &Scope,
    group: bool,
    rev: i64,
    evidence: String,
) -> Result<Value> {
    let table = if group {
        school_auth(tx, p.actor()?, school(s)?, true).await?;
        "delivery_groups"
    } else {
        platform(tx, p).await?;
        "sending_accounts"
    };
    let old = if group {
        scoped_object(tx, s, table).await?
    } else {
        object(tx, table, id(s)?).await?
    };
    revision(&old, rev)?;
    require(old["identity_mode"] == "ui_visible", "目标不是界面模式")?;
    sqlx::query(&format!("UPDATE {table} SET verification_state='reviewed',identity_evidence_ref=$2,revision=revision+1 WHERE id=$1")).bind(id(s)?).bind(text(&evidence,1,500)?).execute(&mut **tx).await?;
    object(tx, table, id(s)?).await
}
pub(super) async fn strategy_target(
    tx: &mut Tx<'_>,
    s: Uuid,
    input: &StrategyInput,
) -> Result<Value> {
    input.validate()?;
    for group in input.groups() {
        let i = ItemInput {
            group_id: group,
            sender_account_id: input.sender_account_id,
            source: ContentSource::Text {
                text_content: input.text_content.clone(),
            },
        };
        let snap = snapshot(tx, s, &i, "").await?;
        require(
            snap["group"]["identity_mode"] == "ui_visible"
                && snap["sender"]["identity_mode"] == "ui_visible",
            "策略需要界面模式账号与群",
        )?;
    }
    let b = binding(tx, input.device_id).await?;
    if b["sender_account_id"] != input.sender_account_id.to_string() {
        return Err(Error::Policy("device_binding_changed"));
    }
    let d = object(tx, "execution_devices", input.device_id).await?;
    require(d["enabled"] == true, "设备尚未启用")?;
    if d["execution_protocol_version"]
        != if input.configuration_release_id.is_some() {
            3
        } else {
            2
        }
    {
        return Err(Error::Policy("unsupported_execution_protocol"));
    }
    Ok(b)
}
pub(super) async fn strategy(
    tx: &mut Tx<'_>,
    p: &Principal,
    s: &Scope,
    rev: Option<i64>,
    input: StrategyInput,
) -> Result<Value> {
    school_auth(tx, p.actor()?, school(s)?, true).await?;
    // Disabling an existing strategy remains possible after its binding/profile becomes invalid.
    if input.enabled || rev.is_none() {
        strategy_target(tx, school(s)?, &input).await?;
    } else {
        input.validate()?;
    }
    let id = if let Some(rev) = rev {
        let old = scoped_object(tx, s, "mobile_delivery_strategies").await?;
        revision(&old, rev)?;
        id(s)?
    } else {
        Uuid::now_v7()
    };
    if rev.is_some() {
        sqlx::query("UPDATE mobile_delivery_strategies SET name=$2,sender_account_id=$3,group_id=$4,device_id=$5,text_content=$6,enabled=$7,delivery_mode=$8,targets=$9,inter_group_interval_seconds=$10,configuration_release_id=$11,revision=revision+1,updated_at=clock_timestamp() WHERE id=$1").bind(id).bind(label(&input.name)?).bind(input.sender_account_id).bind(input.group_id).bind(input.device_id).bind(normalize(&input.text_content)?).bind(input.enabled).bind(&input.delivery_mode).bind(json!(input.targets)).bind(input.inter_group_interval_seconds).bind(input.configuration_release_id).execute(&mut **tx).await?;
    } else {
        sqlx::query("INSERT INTO mobile_delivery_strategies(id,school_id,name,sender_account_id,group_id,device_id,text_content,enabled,created_by,delivery_mode,targets,inter_group_interval_seconds,configuration_release_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)").bind(id).bind(school(s)?).bind(label(&input.name)?).bind(input.sender_account_id).bind(input.group_id).bind(input.device_id).bind(normalize(&input.text_content)?).bind(input.enabled).bind(p.actor()?).bind(&input.delivery_mode).bind(json!(input.targets)).bind(input.inter_group_interval_seconds).bind(input.configuration_release_id).execute(&mut **tx).await?;
    }
    sqlx::query("DELETE FROM mobile_delivery_strategy_targets WHERE strategy_id=$1")
        .bind(id)
        .execute(&mut **tx)
        .await?;
    for (position, group) in input.groups().iter().enumerate() {
        sqlx::query("INSERT INTO mobile_delivery_strategy_targets(strategy_id,school_id,position,group_id) VALUES($1,$2,$3,$4)").bind(id).bind(school(s)?).bind((position+1)as i32).bind(group).execute(&mut **tx).await?;
    }
    object(tx, "mobile_delivery_strategies", id).await
}
pub(super) async fn run(
    tx: &mut Tx<'_>,
    p: &Principal,
    s: &Scope,
    rev: i64,
    origin: &str,
) -> Result<Value> {
    school_auth(tx, p.actor()?, school(s)?, true).await?;
    let strategy = scoped_object(tx, s, "mobile_delivery_strategies").await?;
    revision(&strategy, rev)?;
    require(
        strategy["automation_binding_id"].is_null(),
        "新版业务请通过业务向导重新预览确认",
    )?;
    require(strategy["enabled"] == true, "策略已停用")?;
    let input: StrategyInput = StrategyInput {
        name: strv(&strategy, "name")?.into(),
        sender_account_id: uid(&strategy, "sender_account_id")?,
        group_id: strategy["group_id"]
            .as_str()
            .and_then(|s| Uuid::parse_str(s).ok()),
        delivery_mode: strv(&strategy, "delivery_mode")?.into(),
        targets: serde_json::from_value(strategy["targets"].clone())
            .map_err(|_| Error::Conflict)?,
        inter_group_interval_seconds: strategy["inter_group_interval_seconds"].as_i64(),
        configuration_release_id: strategy["configuration_release_id"]
            .as_str()
            .and_then(|s| Uuid::parse_str(s).ok()),
        device_id: uid(&strategy, "device_id")?,
        text_content: strv(&strategy, "text_content")?.into(),
        enabled: true,
        trigger_mode: "manual_once".into(),
    };
    let b = strategy_target(tx, school(s)?, &input).await?;
    if input.configuration_release_id.is_some() {
        return configured::run(tx, p, s, strategy, input, b, origin).await;
    }
    let set = settings(tx, school(s)?).await?;
    for field in [
        "approved_client_version",
        "approved_script_version",
        "approved_baseline_version",
    ] {
        text(strv(&set, field)?, 1, 100)?;
    }
    let run = Uuid::now_v7();
    let task = save_draft(
        tx,
        p,
        &Scope {
            school: s.school,
            id: None,
        },
        Draft {
            expected_revision: None,
            activity_id: format!("strategy-run:{run}"),
            items: vec![ItemInput {
                group_id: input.group_id.ok_or(Error::Conflict)?,
                sender_account_id: input.sender_account_id,
                source: ContentSource::Text {
                    text_content: input.text_content,
                },
            }],
        },
        origin,
    )
    .await?;
    let target = json!({"device_id":input.device_id,"device_binding_revision":b["revision"],"strategy_id":strategy["id"],"strategy_revision":rev,"run_id":run});
    let mut item = items(tx, uid(&task, "id")?).await?.remove(0);
    item["snapshot"]["execution_target"] = target;
    item["snapshot"]["requirements"] = requirements(&set, 2);
    sqlx::query("UPDATE mobile_delivery_items SET snapshot=$2,snapshot_digest=$3 WHERE id=$1")
        .bind(uid(&item, "id")?)
        .bind(&item["snapshot"])
        .bind(hash(&item["snapshot"].to_string()))
        .execute(&mut **tx)
        .await?;
    sqlx::query("INSERT INTO mobile_delivery_strategy_runs(id,school_id,strategy_id,strategy_revision,task_id,device_id,device_binding_revision,strategy_snapshot,created_by) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)").bind(run).bind(school(s)?).bind(id(s)?).bind(rev).bind(uid(&task,"id")?).bind(input.device_id).bind(b["revision"].as_i64().ok_or(Error::Conflict)?).bind(strategy).bind(p.actor()?).execute(&mut **tx).await?;
    object(tx, "mobile_delivery_strategy_runs", run).await
}
pub(super) async fn validate_execution(
    tx: &mut Tx<'_>,
    i: &Value,
    device: Option<Uuid>,
    protocol: u32,
) -> Result<()> {
    if protocol == 5 {
        return dispatch::validate(tx, i, device).await;
    }
    let ui = i["snapshot"]["group"]["identity_mode"] == "ui_visible";
    if !ui {
        return if protocol == 1 {
            Ok(())
        } else {
            Err(Error::Policy("unsupported_execution_protocol"))
        };
    }
    if ![2, 3, 4].contains(&protocol)
        || i["snapshot"]["requirements"]["protocol_version"] != protocol
    {
        return Err(Error::Policy("unsupported_execution_protocol"));
    }
    let t = &i["snapshot"]["execution_target"];
    let d = uid(t, "device_id").map_err(|_| Error::Policy("target_profile_missing"))?;
    if device.is_some_and(|actual| actual != d) {
        return Err(Error::Policy("device_binding_changed"));
    }
    let device_row = object(tx, "execution_devices", d).await?;
    if device_row["enabled"] != true || device_row["execution_protocol_version"] != protocol {
        return Err(Error::Policy("unsupported_execution_protocol"));
    }
    let b = binding(tx, d).await?;
    if b["revision"] != t["device_binding_revision"]
        || b["sender_account_id"] != i["sender_account_id"]
    {
        return Err(Error::Policy("device_binding_changed"));
    }
    let group_binding:Option<i64>=sqlx::query_scalar("SELECT revision FROM sender_group_bindings WHERE group_id=$1 AND sender_account_id=$2 AND school_id=$3").bind(uid(i,"group_id")?).bind(uid(i,"sender_account_id")?).bind(uid(i,"school_id")?).fetch_optional(&mut **tx).await?;
    if group_binding != i["snapshot"]["binding_revision"].as_i64() {
        return Err(Error::Policy("binding_changed"));
    }
    let run: Option<Value> = sqlx::query_scalar(
        "SELECT to_jsonb(r) FROM mobile_delivery_strategy_runs r WHERE task_id=$1 AND device_id=$2",
    )
    .bind(uid(i, "task_id")?)
    .bind(d)
    .fetch_optional(&mut **tx)
    .await?;
    let run = run.ok_or(Error::Policy("target_profile_missing"))?;
    if run["id"] != t["run_id"]
        || run["strategy_id"] != t["strategy_id"]
        || run["strategy_revision"] != t["strategy_revision"]
        || run["device_binding_revision"] != b["revision"]
    {
        return Err(Error::Policy("device_binding_changed"));
    }
    for (table, snap, idkey) in [
        ("sending_accounts", "sender", "sender_account_id"),
        ("delivery_groups", "group", "group_id"),
    ] {
        let live = snapshot_identity(object(tx, table, uid(i, idkey)?).await?)?;
        if live != i["snapshot"][snap] {
            return Err(Error::Policy("binding_changed"));
        }
    }
    if protocol == 4 {
        automation::validate_manifest(tx, i).await?;
        if device.is_some() {
            automation::require_readiness(tx, i).await?;
        }
        return configured::validate_configuration(tx, i).await;
    }
    if protocol == 3 {
        return configured::validate_configuration(tx, i).await;
    }
    let set = settings(tx, uid(i, "school_id")?).await?;
    if requirements(&set, 2) != i["snapshot"]["requirements"] {
        return Err(Error::Policy("protocol_unsupported"));
    }
    Ok(())
}
pub(super) fn verification(
    i: &Value,
    v: Option<&VisibleVerification>,
    protocol: u32,
) -> Result<()> {
    if protocol == 1 {
        return require(v.is_none(), "旧协议不接受界面核验摘要");
    }
    v.ok_or(Error::Policy("profile_review_required"))?.validate(
        i["snapshot"]["sender"]["profile_revision"]
            .as_i64()
            .ok_or(Error::Conflict)?,
        i["snapshot"]["group"]["profile_revision"]
            .as_i64()
            .ok_or(Error::Conflict)?,
    )
}
pub(super) async fn query(
    tx: &mut Tx<'_>,
    p: &Principal,
    s: &Scope,
    q: Query,
    limit: i64,
    offset: i64,
) -> Result<Value> {
    school_auth(tx, p.actor()?, school(s)?, true).await?;
    match q {
 Query::Targets => {
 let mut accounts:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(a) FROM sending_accounts a JOIN school_memberships m ON m.account_id=a.responsible_actor_id WHERE m.school_id=$1 AND m.role='admin' ORDER BY a.id LIMIT $2 OFFSET $3").bind(school(s)?).bind(limit).bind(offset).fetch_all(&mut **tx).await?;
 let mut devices:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(d)||jsonb_build_object('sender_account_id',b.sender_account_id,'binding_revision',b.revision) FROM execution_devices d JOIN device_account_bindings b ON b.device_id=d.id JOIN sending_accounts a ON a.id=b.sender_account_id JOIN school_memberships m ON m.account_id=a.responsible_actor_id WHERE m.school_id=$1 AND m.role='admin' ORDER BY d.id LIMIT $2 OFFSET $3").bind(school(s)?).bind(limit).bind(offset).fetch_all(&mut **tx).await?;
 let page_size=(limit-1) as usize;
            let accounts_has_more=accounts.len()>page_size;
            let devices_has_more=devices.len()>page_size;
            accounts.truncate(page_size);devices.truncate(page_size);
            Ok(json!({"id":school(s)?,"accounts":accounts,"devices":devices,"accounts_has_more":accounts_has_more,"devices_has_more":devices_has_more}))
 },
 Query::Strategy => scoped_object(tx,s,"mobile_delivery_strategies").await,
 Query::Strategies => Ok(json!(sqlx::query_scalar::<_,Value>("SELECT to_jsonb(t) FROM mobile_delivery_strategies t WHERE school_id=$1 AND automation_binding_id IS NULL ORDER BY id LIMIT $2 OFFSET $3").bind(school(s)?).bind(limit).bind(offset).fetch_all(&mut **tx).await?)),
 Query::StrategyRuns => {scoped_object(tx,s,"mobile_delivery_strategies").await?;Ok(json!(sqlx::query_scalar::<_,Value>("SELECT to_jsonb(r)||jsonb_build_object('task_state',t.state) FROM mobile_delivery_strategy_runs r JOIN mobile_delivery_tasks t ON t.id=r.task_id WHERE r.school_id=$1 AND r.strategy_id=$2 ORDER BY r.created_at DESC,r.id DESC LIMIT $3 OFFSET $4").bind(school(s)?).bind(id(s)?).bind(limit).bind(offset).fetch_all(&mut **tx).await?))},
 _=>Err(Error::NotFound)
 }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn visible_wire_identity_has_no_external_or_control_ids() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../../../../../api-contract/fixtures/mobile-visible-claim-v2.json"
        ))
        .unwrap();
        let account = &fixture["data"]["attributes"]["sender_binding"];
        let mut db = account.clone();
        for key in [
            "identity_namespace",
            "external_key",
            "corp_id",
            "chat_id",
            "resource_id",
            "responsible_actor_id",
        ] {
            db[key] = json!("must-not-leave-server");
        }
        assert_eq!(snapshot_identity(db).unwrap(), *account);
        let mut pending = account.clone();
        pending["verification_state"] = json!("pending");
        assert!(snapshot_identity(pending).is_err());
    }
    #[test]
    fn legacy_snapshot_shape_remains_byte_stable_after_migration() {
        let old = json!({"id":"legacy","identity_namespace":"demo","external_key":"group","enabled":true});
        let mut migrated = old.clone();
        migrated["identity_mode"] = json!("legacy_external");
        migrated["visible_profile"] = Value::Null;
        migrated["profile_revision"] = json!(1);
        migrated["verification_state"] = json!("pending");
        assert_eq!(
            snapshot_identity(migrated).unwrap().to_string(),
            old.to_string()
        );
    }
}
