//! Automation uses the existing delivery transaction, guard, authorization and idempotency ledger.
use super::*;
use std::collections::BTreeMap;

async fn scoped_device(tx: &mut Tx<'_>, s: Uuid, d: Uuid) -> Result<Value> {
    sqlx::query_scalar("SELECT to_jsonb(d)||jsonb_build_object('sender_account_id',b.sender_account_id,'binding_revision',b.revision) FROM execution_devices d JOIN device_account_bindings b ON b.device_id=d.id JOIN sending_accounts a ON a.id=b.sender_account_id JOIN school_memberships m ON m.account_id=a.responsible_actor_id AND m.school_id=$1 AND m.role='admin' WHERE d.id=$2")
        .bind(s).bind(d).fetch_optional(&mut **tx).await?.ok_or(Error::NotFound)
}
async fn authorization_context(tx: &mut Tx<'_>, p: &Principal) -> Result<Value> {
    require(p.protocol() == 4, "授权要求 v4")?;
    let (d, _) = device(tx, p).await?;
    let b = binding(tx, d).await?;
    let account = object(tx, "sending_accounts", uid(&b, "sender_account_id")?).await?;
    let actor = uid(&account, "responsible_actor_id")?;
    let schools:Vec<Value> = sqlx::query_scalar("SELECT jsonb_build_object('school_id',s.id,'name',s.name) FROM schools s JOIN school_memberships m ON m.school_id=s.id JOIN accounts a ON a.id=m.account_id WHERE m.account_id=$1 AND m.role='admin' AND s.active AND a.active ORDER BY s.id LIMIT 100").bind(actor).fetch_all(&mut **tx).await?;
    let dev = object(tx, "execution_devices", d).await?;
    let authorization_revision:i64=sqlx::query_scalar("SELECT COALESCE(max(local_revision),0) FROM device_execution_authorizations WHERE device_id=$1").bind(d).fetch_one(&mut **tx).await?;
    let environment_generation: i64 = sqlx::query_scalar(
        "SELECT COALESCE(max(generation),0) FROM device_environment_snapshots WHERE device_id=$1",
    )
    .bind(d)
    .fetch_one(&mut **tx)
    .await?;
    let mut scopes = Vec::new();
    let mut summaries = Vec::new();
    for school in schools {
        let sid = uid(&school, "school_id")?;
        let apps:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('binding_id',b.id,'app_key',a.app_key,'instance_ref',b.instance_ref,'label',a.name) FROM device_app_bindings b JOIN automation_apps a ON a.id=b.app_id WHERE b.school_id=$1 AND b.device_id=$2 AND b.sender_account_id=$3 AND b.account_binding_revision=$4 AND a.enabled AND a.app_key='wecom' ORDER BY b.id").bind(sid).bind(d).bind(uid(&b,"sender_account_id")?).bind(b["revision"].as_i64().ok_or(Error::Conflict)?).fetch_all(&mut **tx).await?;
        scopes.push(json!({"school_id":sid,"applications":apps.iter().map(|a|json!({"binding_id":a["binding_id"],"app_key":a["app_key"],"instance_ref":a["instance_ref"]})).collect::<Vec<_>>()}));
        summaries.push(json!({"school_id":sid,"name":school["name"],"applications":apps}));
    }
    require(!scopes.is_empty(), "账号尚无有效单位管理员范围")?;
    Ok(
        json!({"id":d,"authorization_revision":authorization_revision,"environment_generation":environment_generation,"extension":AUTHORIZATION_EXTENSION,"device_id":d,"device_name":dev["name"],"account_name":account["visible_profile"]["account_name"].as_str().unwrap_or("已绑定业务账号"),"responsible_actor_id":actor,"schools":summaries,"scope":{"device_id":d,"sender_account_id":b["sender_account_id"],"schools":scopes,"purpose":"company_business/1","data_policy":"minimal_visible_results/1","object_scope":"approved_school_groups/1"}}),
    )
}
async fn active_authorization(tx: &mut Tx<'_>, d: Uuid) -> Result<ExecutionAuthorizationReceipt> {
    let row:Option<Value>=sqlx::query_scalar("SELECT body FROM device_execution_authorizations WHERE device_id=$1 ORDER BY local_revision DESC LIMIT 1").bind(d).fetch_optional(&mut **tx).await?;
    let r: ExecutionAuthorizationReceipt =
        serde_json::from_value(row.ok_or(Error::Policy("execution_authorization_required"))?)
            .map_err(|_| Error::Conflict)?;
    require(r.state == "effective", "执行授权已撤销")?;
    Ok(r)
}
// Selection may skip untouched tasks outside consent, but pending/unknown work still owns the account.
pub(super) async fn authorized_application_ids(
    tx: &mut Tx<'_>,
    d: Uuid,
    account: Uuid,
) -> Result<Vec<String>> {
    let receipt = active_authorization(tx, d).await?;
    require(
        receipt.scope.device_id == d && receipt.scope.sender_account_id == account,
        "authorization_scope_required",
    )?;
    Ok(receipt
        .scope
        .schools
        .iter()
        .flat_map(|s| s.applications.iter().map(|a| a.binding_id.to_string()))
        .collect())
}
pub(super) async fn authorization_gate(tx: &mut Tx<'_>, device: Uuid, protocol: u32) -> Result<()> {
    if protocol == 5 {
        return dispatch::authorization_gate(tx, device).await;
    }
    let state:Option<String>=sqlx::query_scalar("SELECT body->>'state' FROM device_execution_authorizations WHERE device_id=$1 ORDER BY local_revision DESC LIMIT 1").bind(device).fetch_optional(&mut **tx).await?;
    if let Some(state) = state {
        require(
            state == "effective" && protocol == 4,
            "execution_authorization_required",
        )?;
    } else {
        require(protocol < 4, "execution_authorization_required")?;
    }
    Ok(())
}
async fn authorized(
    tx: &mut Tx<'_>,
    d: Uuid,
    school: Uuid,
    app: Option<&Value>,
    maintenance: bool,
) -> Result<ExecutionAuthorizationReceipt> {
    let r = active_authorization(tx, d).await?;
    let b = binding(tx, d).await?;
    let account = object(tx, "sending_accounts", uid(&b, "sender_account_id")?).await?;
    school_auth(tx, uid(&account, "responsible_actor_id")?, school, true).await?;
    let a = app
        .map(|a| -> Result<AuthorizedApplication> {
            Ok(AuthorizedApplication {
                binding_id: uid(a, "id")?,
                app_key: "wecom".into(),
                instance_ref: strv(a, "instance_ref")?.into(),
            })
        })
        .transpose()?;
    require(
        r.scope.device_id == d
            && r.scope.sender_account_id == uid(&b, "sender_account_id")?
            && r.scope.covers(school, a.as_ref())
            && (!maintenance || r.maintenance),
        "authorization_scope_required",
    )?;
    Ok(r)
}
async fn latest_environment(tx: &mut Tx<'_>, d: Uuid) -> Result<Value> {
    sqlx::query_scalar("SELECT to_jsonb(e) FROM device_environment_snapshots e JOIN execution_devices d ON d.id=e.device_id WHERE e.device_id=$1 AND e.credential_generation=d.credential_generation AND e.received_at>clock_timestamp()-interval '10 minutes' ORDER BY e.generation DESC LIMIT 1")
        .bind(d).fetch_optional(&mut **tx).await?.ok_or(Error::Policy("automation_environment_required"))
}
fn report(e: &Value) -> Result<EnvironmentReport> {
    serde_json::from_value(e["body"].clone()).map_err(|_| Error::Conflict)
}
fn selected<'a>(e: &'a EnvironmentReport, b: &Value) -> Result<&'a InstalledApplication> {
    e.applications
        .iter()
        .find(|a| {
            Some(a.instance_ref.as_str()) == b["instance_ref"].as_str()
                && a.instance_visibility == "unique"
        })
        .ok_or(Error::Policy("automation_instance_ambiguous"))
}
fn environment_digest(e: &EnvironmentReport, b: &Value) -> Result<String> {
    Ok(automation_digest(
        &json!({"launcher":e.launcher,"application":selected(e,b)?,"engine_version":e.engine_version,"capabilities":e.capabilities.iter().collect::<std::collections::BTreeSet<_>>()}),
    ))
}
async fn app_binding(tx: &mut Tx<'_>, s: Uuid, bid: Uuid) -> Result<Value> {
    let b = object(tx, "device_app_bindings", bid).await?;
    require(b["school_id"] == s.to_string(), "应用不在当前学校")?;
    let d = scoped_device(tx, s, uid(&b, "device_id")?).await?;
    if d["enabled"] != true
        || d["binding_revision"] != b["account_binding_revision"]
        || d["sender_account_id"] != b["sender_account_id"]
    {
        return Err(Error::Policy("binding_changed"));
    }
    Ok(b)
}
async fn no_device_operation(
    tx: &mut Tx<'_>,
    d: Uuid,
    except: Option<Uuid>,
    include_queued: bool,
) -> Result<()> {
    let busy:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_execution_claims WHERE device_id=$1 AND state IN ('claimed','prepared','permit_issued')) OR EXISTS(SELECT 1 FROM device_validation_jobs WHERE device_id=$1 AND (state IN ('accepted','running') OR ($3 AND state IN ('queued','offered'))) AND expires_at>clock_timestamp() AND ($2::uuid IS NULL OR id<>$2)) OR EXISTS(SELECT 1 FROM delivery_usage u JOIN device_account_bindings b ON b.sender_account_id=u.sender_account_id WHERE b.device_id=$1 AND u.state IN ('pending','unknown'))")
        .bind(d).bind(except).bind(include_queued).fetch_one(&mut **tx).await?;
    require(!busy, "手机正在执行、等待验证或有待核对结果，请先处理")
}
pub(super) async fn validation_gate(tx: &mut Tx<'_>, d: Uuid) -> Result<()> {
    let active:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM device_validation_jobs WHERE device_id=$1 AND state IN ('accepted','running') AND expires_at>clock_timestamp())").bind(d).fetch_one(&mut **tx).await?;
    require(!active, "手机正在等待或执行验证")
}
fn kind(v: &Value) -> Result<ValidationKind> {
    serde_json::from_value(v["kind"].clone()).map_err(|_| Error::Conflict)
}
async fn valid_job(tx: &mut Tx<'_>, p: &Principal, jid: Uuid) -> Result<Value> {
    require(p.protocol() == 4, "验证要求 v4 手机端")?;
    let (d, g) = device(tx, p).await?;
    let job = object(tx, "device_validation_jobs", jid).await?;
    if job["device_id"] != d.to_string() || job["credential_generation"] != g {
        return Err(Error::NotFound);
    }
    school_auth(tx, uid(&job, "created_by")?, uid(&job, "school_id")?, true).await?;
    Ok(job)
}
async fn job_current(tx: &mut Tx<'_>, job: &Value) -> Result<()> {
    let fresh: bool = sqlx::query_scalar(
        "SELECT expires_at>clock_timestamp() FROM device_validation_jobs WHERE id=$1",
    )
    .bind(uid(job, "id")?)
    .fetch_one(&mut **tx)
    .await?;
    require(fresh, "验证已过期，请重新发起")?;
    if let Some(eid) = job["environment_id"].as_str() {
        let b = app_binding(tx, uid(job, "school_id")?, uid(job, "app_binding_id")?).await?;
        let e = latest_environment(tx, uid(job, "device_id")?).await?;
        require(
            e["id"] == eid && b["revision"] == job["binding_revision"],
            "验证环境或绑定已变化",
        )?;
    }
    Ok(())
}
async fn components(tx: &mut Tx<'_>, ids: &[Uuid]) -> Result<Vec<Value>> {
    require(ids.len() <= 4, "一次最多四种组件")?;
    let mut out = Vec::new();
    let mut kinds = std::collections::BTreeSet::new();
    for id in ids {
        let c = object(tx, "automation_component_versions", *id).await?;
        require(
            kinds.insert(strv(&c, "kind")?.to_owned()),
            "同类组件不能重复",
        )?;
        out.push(c);
    }
    Ok(out)
}
async fn successful_job(tx: &mut Tx<'_>, s: Uuid, id: Uuid) -> Result<Value> {
    let j = object(tx, "device_validation_jobs", id).await?;
    require(
        j["school_id"] == s.to_string() && j["state"] == "succeeded",
        "需要本学校成功验证记录",
    )?;
    Ok(j)
}
async fn baseline(
    tx: &mut Tx<'_>,
    s: Uuid,
    b: &Value,
    environment: &str,
    ids: &[Uuid],
) -> Result<ManualBaseline> {
    let mut samples = Vec::new();
    for id in ids {
        let j = successful_job(tx, s, *id).await?;
        require(
            kind(&j)? == ValidationKind::MeasureManual
                && j["app_binding_id"] == b["id"]
                && j["binding_revision"] == b["revision"],
            "人工样本绑定不一致",
        )?;
        let je = object(
            tx,
            "device_environment_snapshots",
            uid(&j, "environment_id")?,
        )
        .await?;
        require(
            environment_digest(&report(&je)?, b)? == environment,
            "人工样本环境不一致",
        )?;
        samples.push(
            serde_json::from_value::<ManualTiming>(j["result"]["timing"].clone())
                .map_err(|_| Error::Policy("automation_timing_required"))?,
        );
    }
    require(!samples.is_empty(), "缺少人工计时样本")?;
    // Conservative maximum prevents selecting a fast sample to undercut a measured human pace.
    Ok(ManualBaseline {
        version: automation_digest(&json!(ids)),
        samples: samples.len(),
        human_group_min_interval_ms: samples
            .iter()
            .map(|v| v.group_interval_ms)
            .max()
            .unwrap_or(0),
        navigation_min_ms: samples.iter().map(|v| v.navigation_ms).max().unwrap_or(0),
        input_codepoint_min_ms: samples
            .iter()
            .map(|v| v.input_codepoint_ms)
            .max()
            .unwrap_or(0),
        input_settle_min_ms: samples.iter().map(|v| v.input_settle_ms).max().unwrap_or(0),
        observation_min_ms: samples.iter().map(|v| v.observation_ms).max().unwrap_or(0),
        evidence_ref: format!("automation-timing:{}", automation_digest(&json!(ids))),
    })
}
async fn resolve(tx: &mut Tx<'_>, s: Uuid, bid: Uuid, capability: Uuid) -> Result<Value> {
    let b = app_binding(tx, s, bid).await?;
    let cap = object(tx, "automation_capabilities", capability).await?;
    let app = object(tx, "automation_apps", uid(&b, "app_id")?).await?;
    require(
        cap["app_id"] == b["app_id"]
            && cap["handler"] == "wecom_group_text"
            && app["enabled"] == true,
        "该应用业务尚不支持",
    )?;
    let e = latest_environment(tx, uid(&b, "device_id")?).await?;
    let er = report(&e)?;
    require(
        er.supports_execution(),
        "手机引擎或通知、无障碍、解锁状态未就绪",
    )?;
    require(
        selected(&er, &b)?.app_key == "wecom",
        "安装候选不属于企业微信",
    )?;
    let fingerprint = environment_digest(&er, &b)?;
    let rows:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(r)||jsonb_build_object('component',to_jsonb(v)) FROM automation_component_releases r JOIN automation_component_versions v ON v.id=r.component_id WHERE r.school_id=$1 AND (r.device_id IS NULL OR r.device_id=$2) AND r.environment_digest=$3 AND r.active AND NOT r.revoked ORDER BY r.kind,r.id")
        .bind(s).bind(uid(&b,"device_id")?).bind(&fingerprint).fetch_all(&mut **tx).await?;
    let mut releases = BTreeMap::new();
    for kind in ["launcher", "workflow", "policy", "baseline"] {
        let all: Vec<_> = rows.iter().filter(|v| v["kind"] == kind).collect();
        let specific: Vec<_> = all
            .iter()
            .copied()
            .filter(|v| !v["device_id"].is_null())
            .collect();
        let candidates = if specific.is_empty() { all } else { specific };
        require(
            candidates.len() == 1,
            "尚无唯一适配方案，请联系维护人员完成验证发布",
        )?;
        releases.insert(kind, candidates[0].clone());
    }
    let rules: UiRules =
        serde_json::from_value(releases["workflow"]["component"]["body"]["rules"].clone())
            .map_err(|_| Error::Conflict)?;
    let flow: LauncherFlow =
        serde_json::from_value(releases["launcher"]["component"]["body"]["flow"].clone())
            .map_err(|_| Error::Conflict)?;
    flow.validate()?;
    require(
        json!(flow.client) == json!(er.launcher)
            && json!(rules.client) == json!(selected(&er, &b)?.client),
        "适配版本与实际安装不一致",
    )?;
    let policy: ExecutionPolicy =
        serde_json::from_value(releases["policy"]["component"]["body"]["policy"].clone())
            .map_err(|_| Error::Conflict)?;
    let ids: Vec<Uuid> =
        serde_json::from_value(releases["baseline"]["component"]["body"]["validation_ids"].clone())
            .map_err(|_| Error::Conflict)?;
    let base = baseline(tx, s, &b, &fingerprint, &ids).await?;
    let config = ConfigurationInput {
        name: "自动组装企业微信流程".into(),
        rules,
        policy,
        baseline: base,
        evidence_ref: format!("automation-binding:{bid}"),
    };
    config.validate()?;
    require(
        flow.search_steps
            .iter()
            .chain(&flow.all_apps_steps)
            .all(|s| s.delay_ms >= config.baseline.navigation_min_ms),
        "桌面操作节奏不得快于人工基准",
    )?;
    let opened_environments:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(e) FROM device_validation_jobs j JOIN device_environment_snapshots e ON e.id=j.environment_id WHERE j.app_binding_id=$1 AND j.binding_revision=$2 AND j.kind='open_application' AND j.state='succeeded' AND j.component_ids @> $3::jsonb")
        .bind(bid).bind(b["revision"].as_i64().ok_or(Error::Conflict)?).bind(json!([releases["launcher"]["component_id"]])).fetch_all(&mut **tx).await?;
    let opened = opened_environments.iter().any(|e| {
        report(e)
            .and_then(|e| environment_digest(&e, &b))
            .is_ok_and(|d| d == fingerprint)
    });
    require(opened, "请先在手机接受并完成测试打开")?;
    Ok(
        json!({"binding":b,"environment_id":e["id"],"environment_generation":e["generation"],"environment_digest":fingerprint,"capability":cap,"components":releases,"configuration":config,"protocol_version":4,"engine_version":AUTOMATION_ENGINE}),
    )
}
pub(super) async fn apply(
    tx: &mut Tx<'_>,
    p: &Principal,
    s: &Scope,
    c: AutomationCommand,
    enabled: bool,
    origin: &str,
) -> Result<Value> {
    sqlx::query("UPDATE device_validation_jobs SET state='expired',revision=revision+1 WHERE expires_at<=clock_timestamp() AND state IN ('queued','offered','accepted','running')").execute(&mut **tx).await?;
    match c {
        AutomationCommand::RecordAuthorization { receipt } => {
            require(p.protocol() == 4, "授权要求 v4")?;
            let (d, _) = device(tx, p).await?;
            receipt.validate()?;
            require(
                receipt.scope.device_id == d
                    && receipt.scope.origin == origin.trim_end_matches('/'),
                "授权来源或设备不一致",
            )?;
            let previous:Option<Value>=sqlx::query_scalar("SELECT body FROM device_execution_authorizations WHERE device_id=$1 ORDER BY local_revision DESC LIMIT 1").bind(d).fetch_optional(&mut **tx).await?;
            if let Some(old) = previous {
                let rev = old["revision"].as_i64().ok_or(Error::Conflict)?;
                if receipt.revision == rev {
                    require(json!(*receipt) == old, "授权版本内容变化")?;
                    return Ok(json!({"id":receipt.authorization_id,"revision":rev}));
                }
                require(receipt.revision > rev, "授权版本过旧，禁止恢复已撤销授权")?;
            }
            if receipt.state == "effective" {
                let context = authorization_context(tx, p).await?;
                require(
                    receipt.scope.sender_account_id == uid(&context["scope"], "sender_account_id")?,
                    "授权账号已变化",
                )?;
                let current: Vec<AuthorizedSchool> =
                    serde_json::from_value(context["scope"]["schools"].clone())
                        .map_err(|_| Error::Conflict)?;
                for scope in &receipt.scope.schools {
                    let found = current
                        .iter()
                        .find(|c| c.school_id == scope.school_id)
                        .ok_or(Error::Policy("authorization_scope_required"))?;
                    require(
                        scope
                            .applications
                            .iter()
                            .all(|a| found.applications.contains(a)),
                        "授权应用已变化",
                    )?;
                }
            }
            sqlx::query("INSERT INTO device_execution_authorizations(device_id,local_revision,authorization_id,body) VALUES($1,$2,$3,$4)").bind(d).bind(receipt.revision).bind(receipt.authorization_id).bind(json!(*receipt)).execute(&mut **tx).await?;
            Ok(json!({"id":receipt.authorization_id,"revision":receipt.revision}))
        }
        AutomationCommand::RejectValidation {
            expected_revision,
            generation,
        } => {
            let j = valid_job(tx, p, id(s)?).await?;
            revision(&j, expected_revision)?;
            require(
                j["generation"] == generation.to_string()
                    && matches!(j["state"].as_str(), Some("queued" | "offered")),
                "验证已终结或代次变化",
            )?;
            sqlx::query("UPDATE device_validation_jobs SET state='declined',decision_source='local_decline',revision=revision+1 WHERE id=$1").bind(id(s)?).execute(&mut **tx).await?;
            object(tx, "device_validation_jobs", id(s)?).await
        }
        AutomationCommand::AcceptStandingValidation {
            expected_revision,
            generation,
            authorization_id,
        } => {
            let j = valid_job(tx, p, id(s)?).await?;
            revision(&j, expected_revision)?;
            job_current(tx, &j).await?;
            require(
                j["generation"] == generation.to_string()
                    && matches!(j["state"].as_str(), Some("queued" | "offered")),
                "验证代次变化或已接受",
            )?;
            let b = if j["app_binding_id"].is_null() {
                None
            } else {
                Some(app_binding(tx, uid(&j, "school_id")?, uid(&j, "app_binding_id")?).await?)
            };
            let maintenance = matches!(
                kind(&j)?,
                ValidationKind::InspectRoles | ValidationKind::MeasureManual
            );
            if maintenance {
                platform(tx, &Principal::Actor(uid(&j, "created_by")?)).await?;
            }
            let r = authorized(
                tx,
                uid(&j, "device_id")?,
                uid(&j, "school_id")?,
                b.as_ref(),
                maintenance,
            )
            .await?;
            require(r.authorization_id == authorization_id, "执行授权已变化")?;
            no_device_operation(tx, uid(&j, "device_id")?, Some(id(s)?), true).await?;
            sqlx::query("UPDATE device_validation_jobs SET state='accepted',accepted_at=clock_timestamp(),authorization_id=$2,decision_source='standing_authorization',revision=revision+1 WHERE id=$1").bind(id(s)?).bind(authorization_id).execute(&mut **tx).await?;
            object(tx, "device_validation_jobs", id(s)?).await
        }
        AutomationCommand::ReportEnvironment { report: r } => {
            require(p.protocol() == 4, "环境上报要求 v4")?;
            let (d, g) = device(tx, p).await?;
            r.validate()?;
            let last: Option<i64> = sqlx::query_scalar(
                "SELECT max(generation) FROM device_environment_snapshots WHERE device_id=$1",
            )
            .bind(d)
            .fetch_one(&mut **tx)
            .await?;
            require(last.is_none_or(|n| r.generation > n), "环境代次必须递增")?;
            let eid = Uuid::now_v7();
            let body = json!(r);
            sqlx::query("INSERT INTO device_environment_snapshots(id,device_id,generation,credential_generation,body,content_digest) VALUES($1,$2,$3,$4,$5,$6)").bind(eid).bind(d).bind(r.generation).bind(g).bind(&body).bind(automation_digest(&body)).execute(&mut **tx).await?;
            sqlx::query("UPDATE execution_devices SET last_seen_at=clock_timestamp(),execution_protocol_version=4,supported_protocol_versions=ARRAY[1,2,3,4] WHERE id=$1").bind(d).execute(&mut **tx).await?;
            object(tx, "device_environment_snapshots", eid).await
        }
        AutomationCommand::BindApplication {
            device_id: d,
            app_id,
            environment_id,
            instance_ref,
            expected_revision,
        } => {
            let school = school(s)?;
            let actor = p.actor()?;
            let dev = scoped_device(tx, school, d).await?;
            let e = latest_environment(tx, d).await?;
            require(
                e["id"] == environment_id.to_string(),
                "环境已变化，请重新检测",
            )?;
            let app = object(tx, "automation_apps", app_id).await?;
            require(app["enabled"] == true, "应用尚未支持")?;
            let er = report(&e)?;
            let candidate = er
                .applications
                .iter()
                .find(|a| {
                    a.instance_ref == instance_ref
                        && a.app_key == app["app_key"].as_str().unwrap_or("")
                })
                .ok_or(Error::NotFound)?;
            require(
                candidate.instance_visibility == "unique",
                "工作资料或分身实例无法可靠区分",
            )?;
            let account = object(tx, "sending_accounts", uid(&dev, "sender_account_id")?).await?;
            require(
                account["client_kind"] == app["app_key"]
                    && account["identity_mode"] == "ui_visible"
                    && account["verification_state"] == "reviewed",
                "请先绑定已核对的企业微信账号",
            )?;
            no_device_operation(tx, d, None, true).await?;
            let old:Option<Value>=sqlx::query_scalar("SELECT to_jsonb(b) FROM device_app_bindings b WHERE school_id=$1 AND device_id=$2 AND app_id=$3").bind(school).bind(d).bind(app_id).fetch_optional(&mut **tx).await?;
            let bid = if let Some(old) = old {
                revision(&old, expected_revision)?;
                uid(&old, "id")?
            } else {
                require(expected_revision == 0, "绑定已变化")?;
                Uuid::now_v7()
            };
            sqlx::query("INSERT INTO device_app_bindings(id,school_id,device_id,app_id,sender_account_id,account_binding_revision,instance_ref,environment_id,created_by) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT(school_id,device_id,app_id) DO UPDATE SET sender_account_id=EXCLUDED.sender_account_id,account_binding_revision=EXCLUDED.account_binding_revision,instance_ref=EXCLUDED.instance_ref,environment_id=EXCLUDED.environment_id,revision=device_app_bindings.revision+1")
                .bind(bid).bind(school).bind(d).bind(app_id).bind(uid(&dev,"sender_account_id")?).bind(dev["binding_revision"].as_i64().ok_or(Error::Conflict)?).bind(instance_ref).bind(environment_id).bind(actor).execute(&mut **tx).await?;
            object(tx, "device_app_bindings", bid).await
        }
        AutomationCommand::RequestValidation {
            device_id: d,
            app_binding_id,
            kind: k,
            mut component_ids,
        } => {
            let school = school(s)?;
            let actor = p.actor()?;
            let dev = scoped_device(tx, school, d).await?;
            require(
                dev["enabled"] == true
                    && dev["supported_protocol_versions"]
                        .as_array()
                        .is_some_and(|v| v.contains(&json!(4))),
                "等待支持 v4 的新版手机端",
            )?;
            no_device_operation(tx, d, None, true).await?;
            let navigating:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM automation_readiness_tickets t JOIN mobile_delivery_items i ON i.id=t.item_id WHERE t.device_id=$1 AND t.expires_at>clock_timestamp() AND i.state='queued')").bind(d).fetch_one(&mut **tx).await?;
            require(!navigating, "手机正在准备应用，请停止准备或等待票据到期")?;
            if matches!(
                k,
                ValidationKind::InspectRoles | ValidationKind::MeasureManual
            ) {
                platform(tx, p).await?;
            }
            let b = if let Some(bid) = app_binding_id {
                let b = app_binding(tx, school, bid).await?;
                require(b["device_id"] == d.to_string(), "手机与应用绑定不一致")?;
                Some(b)
            } else {
                require(k == ValidationKind::InspectEnvironment, "请选择已设置应用")?;
                None
            };
            let e = if b.is_some() {
                Some(latest_environment(tx, d).await?)
            } else {
                None
            };
            if let Some(e) = &e {
                let er = report(e)?;
                require(er.supports_execution(), "手机权限或引擎未就绪")?;
                if matches!(
                    k,
                    ValidationKind::InspectRoles | ValidationKind::MeasureManual
                ) {
                    require(er.maintenance_mode, "需要手机维护模式")?;
                }
            }
            if k == ValidationKind::OpenApplication {
                if !component_ids.is_empty() {
                    platform(tx, p).await?;
                } else {
                    let b = b.as_ref().ok_or(Error::NotFound)?;
                    let er = report(e.as_ref().ok_or(Error::NotFound)?)?;
                    let digest = environment_digest(&er, b)?;
                    let rows:Vec<Uuid>=sqlx::query_scalar("SELECT component_id FROM automation_component_releases WHERE school_id=$1 AND (device_id IS NULL OR device_id=$2) AND environment_digest=$3 AND kind='launcher' AND active AND NOT revoked ORDER BY device_id NULLS LAST")
                        .bind(school).bind(d).bind(digest).fetch_all(&mut **tx).await?;
                    require(
                        !rows.is_empty(),
                        "尚无桌面方案，请联系维护人员先完成试用验证",
                    )?;
                    component_ids.push(rows[0]);
                }
            }
            if !component_ids.is_empty() && k != ValidationKind::OpenApplication {
                platform(tx, p).await?;
            }
            if k == ValidationKind::InspectEnvironment {
                require(component_ids.is_empty(), "环境检查不接受操作组件")?;
            }
            let cs = components(tx, &component_ids).await?;
            if k == ValidationKind::OpenApplication {
                require(
                    cs.len() == 1 && cs[0]["kind"] == "launcher",
                    "测试打开只接受一个桌面组件",
                )?;
            }
            if k == ValidationKind::InspectRoles {
                require(
                    cs.iter().any(|c| c["kind"] == "workflow"),
                    "角色验证须选择业务组件",
                )?;
            }
            let jid = Uuid::now_v7();
            sqlx::query("INSERT INTO device_validation_jobs(id,school_id,device_id,app_binding_id,kind,component_ids,environment_id,binding_revision,generation,credential_generation,created_by,expires_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,clock_timestamp()+interval '10 minutes')")
                .bind(jid).bind(school).bind(d).bind(app_binding_id).bind(k.key()).bind(json!(component_ids)).bind(e.as_ref().map(|v|uid(v,"id")).transpose()?).bind(b.as_ref().and_then(|v|v["revision"].as_i64())).bind(Uuid::now_v7()).bind(dev["credential_generation"].as_i64().ok_or(Error::Conflict)?).bind(actor).execute(&mut **tx).await?;
            object(tx, "device_validation_jobs", jid).await
        }
        AutomationCommand::CancelValidation { expected_revision } => {
            let j = scoped_object(tx, s, "device_validation_jobs").await?;
            p.actor()?;
            revision(&j, expected_revision)?;
            require(
                ["queued", "offered", "accepted", "running"].contains(&strv(&j, "state")?),
                "验证已终结",
            )?;
            sqlx::query("UPDATE device_validation_jobs SET state='cancelled',revision=revision+1 WHERE id=$1").bind(id(s)?).execute(&mut **tx).await?;
            object(tx, "device_validation_jobs", id(s)?).await
        }
        AutomationCommand::AcceptValidation {
            expected_revision,
            generation,
        } => {
            let j = valid_job(tx, p, id(s)?).await?;
            revision(&j, expected_revision)?;
            job_current(tx, &j).await?;
            require(
                j["generation"] == generation.to_string()
                    && matches!(j["state"].as_str(), Some("queued" | "offered")),
                "验证代次无效或已接受",
            )?;
            no_device_operation(tx, uid(&j, "device_id")?, Some(id(s)?), true).await?;
            let b = if j["app_binding_id"].is_null() {
                None
            } else {
                Some(app_binding(tx, uid(&j, "school_id")?, uid(&j, "app_binding_id")?).await?)
            };
            authorized(
                tx,
                uid(&j, "device_id")?,
                uid(&j, "school_id")?,
                b.as_ref(),
                matches!(
                    kind(&j)?,
                    ValidationKind::InspectRoles | ValidationKind::MeasureManual
                ),
            )
            .await?;
            sqlx::query("UPDATE device_validation_jobs SET state='accepted',accepted_at=clock_timestamp(),decision_source='local_action',revision=revision+1 WHERE id=$1").bind(id(s)?).execute(&mut **tx).await?;
            object(tx, "device_validation_jobs", id(s)?).await
        }
        AutomationCommand::ReportValidation { event: e } => {
            let j = valid_job(tx, p, id(s)?).await?;
            e.validate(kind(&j)?)?;
            if let Some(old) = sqlx::query_scalar::<_, Value>(
                "SELECT body FROM device_validation_events WHERE job_id=$1 AND event_id=$2",
            )
            .bind(id(s)?)
            .bind(e.event_id)
            .fetch_optional(&mut **tx)
            .await?
            {
                require(old == json!(e), "事件内容已变化")?;
                return Ok(j);
            }
            require(
                j["generation"] == e.generation.to_string(),
                "验证代次不一致",
            )?;
            let current = job_current(tx, &j).await.is_ok()
                && matches!(j["state"].as_str(), Some("accepted" | "running"));
            if current {
                revision(&j, e.expected_revision)?;
                if e.outcome == "succeeded" && kind(&j)? == ValidationKind::InspectRoles {
                    let ids: Vec<Uuid> = serde_json::from_value(j["component_ids"].clone())
                        .map_err(|_| Error::Conflict)?;
                    for component in components(tx, &ids).await? {
                        if component["kind"] == "workflow" {
                            let rules: UiRules =
                                serde_json::from_value(component["body"]["rules"].clone())
                                    .map_err(|_| Error::Conflict)?;
                            for role in rules.controls.keys() {
                                require(
                                    e.role_matches.get(role) == Some(&1),
                                    "业务角色验证不完整",
                                )?;
                            }
                            for page in rules.pages.keys() {
                                require(
                                    e.role_matches.get(&format!("page:{page}")) == Some(&1),
                                    "业务页面验证不完整",
                                )?;
                            }
                        }
                    }
                }
            }
            sqlx::query("INSERT INTO device_validation_events(id,job_id,event_id,body,applied) VALUES($1,$2,$3,$4,$5)").bind(Uuid::now_v7()).bind(id(s)?).bind(e.event_id).bind(json!(e)).bind(current).execute(&mut **tx).await?;
            if current {
                sqlx::query("UPDATE device_validation_jobs SET state=$2,result=$3,revision=revision+1 WHERE id=$1").bind(id(s)?).bind(&e.outcome).bind(json!(e)).execute(&mut **tx).await?;
            }
            object(tx, "device_validation_jobs", id(s)?).await
        }
        AutomationCommand::CreateComponent { name, body } => {
            let actor = platform(tx, p).await?;
            label(&name)?;
            body.validate()?;
            let cid = Uuid::now_v7();
            let value = json!(body);
            sqlx::query("INSERT INTO automation_component_versions(id,name,kind,body,content_digest,created_by) VALUES($1,$2,$3,$4,$5,$6)").bind(cid).bind(name).bind(body.kind()).bind(&value).bind(automation_digest(&value)).bind(actor).execute(&mut **tx).await?;
            object(tx, "automation_component_versions", cid).await
        }
        AutomationCommand::PublishComponent {
            school_id,
            device_id,
            validation_id,
            expected_revision,
        } => {
            let actor = platform(tx, p).await?;
            school_auth(tx, actor, school_id, true).await?;
            let c = object(tx, "automation_component_versions", id(s)?).await?;
            revision(&c, expected_revision)?;
            let j = successful_job(tx, school_id, validation_id).await?;
            let b = app_binding(tx, school_id, uid(&j, "app_binding_id")?).await?;
            require(
                j["binding_revision"] == b["revision"]
                    && device_id.is_none_or(|d| j["device_id"] == d.to_string()),
                "发布试用绑定不一致",
            )?;
            let e = latest_environment(tx, uid(&j, "device_id")?).await?;
            let er = report(&e)?;
            require(e["id"] == j["environment_id"], "验证环境已变化")?;
            let digest = environment_digest(&er, &b)?;
            let body: ComponentBody =
                serde_json::from_value(c["body"].clone()).map_err(|_| Error::Conflict)?;
            body.validate()?;
            if let ComponentBody::Baseline { validation_ids } = &body {
                baseline(tx, school_id, &b, &digest, validation_ids).await?;
                require(
                    validation_ids.contains(&validation_id),
                    "基准发布须关联所选人工样本",
                )?;
            } else {
                require(
                    j["component_ids"]
                        .as_array()
                        .is_some_and(|ids| ids.contains(&c["id"])),
                    "验证没有覆盖该版本",
                )?;
                require(
                    match &body {
                        ComponentBody::Launcher { flow } => {
                            kind(&j)? == ValidationKind::OpenApplication
                                && json!(flow.client) == json!(er.launcher)
                                && match j["result"]["path"].as_str() {
                                    Some("search") => !flow.search_steps.is_empty(),
                                    Some("all_apps") => !flow.all_apps_steps.is_empty(),
                                    _ => false,
                                }
                        }
                        ComponentBody::Workflow { rules } => {
                            kind(&j)? == ValidationKind::InspectRoles
                                && json!(rules.client) == json!(selected(&er, &b)?.client)
                        }
                        ComponentBody::Policy { .. } => kind(&j)? == ValidationKind::InspectRoles,
                        _ => false,
                    },
                    "验证种类、路径或精确安装版本不匹配",
                )?;
            }
            if let ComponentBody::Launcher { flow } = &body {
                for (path, steps) in [
                    ("search", &flow.search_steps),
                    ("all_apps", &flow.all_apps_steps),
                ] {
                    if steps.is_empty() {
                        continue;
                    }
                    let verified:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM device_validation_jobs WHERE school_id=$1 AND app_binding_id=$2 AND binding_revision=$3 AND environment_id=$4 AND kind='open_application' AND state='succeeded' AND result->>'path'=$5 AND component_ids @> $6::jsonb)").bind(school_id).bind(uid(&b,"id")?).bind(b["revision"].as_i64().ok_or(Error::Conflict)?).bind(uid(&e,"id")?).bind(path).bind(json!([c["id"]])).fetch_one(&mut **tx).await?;
                    require(verified, "每条已启用桌面路径均须在当前环境完成测试")?;
                }
            }
            // All writes share the delivery guard, including publication and confirmation.
            sqlx::query("UPDATE automation_component_releases SET active=false,revision=revision+1 WHERE school_id=$1 AND device_id IS NOT DISTINCT FROM $2 AND environment_digest=$3 AND kind=$4 AND active").bind(school_id).bind(device_id).bind(&digest).bind(strv(&c,"kind")?).execute(&mut **tx).await?;
            let rid = Uuid::now_v7();
            sqlx::query("INSERT INTO automation_component_releases(id,component_id,kind,school_id,device_id,environment_digest,validation_id,created_by) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(rid).bind(id(s)?).bind(strv(&c,"kind")?).bind(school_id).bind(device_id).bind(digest).bind(validation_id).bind(actor).execute(&mut **tx).await?;
            object(tx, "automation_component_releases", rid).await
        }
        AutomationCommand::RevokeRelease {
            expected_revision,
            reason,
        } => {
            platform(tx, p).await?;
            text(&reason, 1, 1000)?;
            let r = object(tx, "automation_component_releases", id(s)?).await?;
            revision(&r, expected_revision)?;
            sqlx::query("UPDATE automation_component_releases SET active=false,revoked=true,revision=revision+1 WHERE id=$1").bind(id(s)?).execute(&mut **tx).await?;
            object(tx, "automation_component_releases", id(s)?).await
        }
        AutomationCommand::PreviewBusiness { input } => {
            preview_business(tx, p, s, input, origin).await
        }
        AutomationCommand::ConfirmBusiness {
            expected_revision,
            preview_digest,
            reviewed,
        } => {
            let v = scoped_object(tx, s, "automation_business_previews").await?;
            revision(&v, expected_revision)?;
            require(
                v["created_by"] == p.actor()?.to_string() && reviewed,
                "请复核账号、设备、群顺序和正文后确认",
            )?;
            let fresh:bool=sqlx::query_scalar("SELECT expires_at>clock_timestamp() AND confirmed_at IS NULL FROM automation_business_previews WHERE id=$1").bind(id(s)?).fetch_one(&mut **tx).await?;
            require(fresh, "预览过期或已确认，请重新预览")?;
            let input: BusinessInput =
                serde_json::from_value(v["input"].clone()).map_err(|_| Error::Conflict)?;
            let resolution =
                resolve(tx, school(s)?, input.app_binding_id, input.capability_id).await?;
            if resolution != v["resolution"] || v["preview_digest"] != preview_digest {
                return Err(Error::Policy("revision_conflict"));
            }
            let run = object(tx, "mobile_delivery_strategy_runs", uid(&v, "run_id")?).await?;
            let scope = Scope {
                school: s.school,
                id: Some(uid(&run, "task_id")?),
            };
            let task = object(tx, "mobile_delivery_tasks", id(&scope)?).await?;
            confirm_task(
                tx,
                p,
                &scope,
                TaskConfirmation {
                    expected_revision: task["revision"].as_i64().ok_or(Error::Conflict)?,
                    preview_digest: strv(&v["task_preview"], "preview_digest")?.into(),
                    content_review: true,
                    scope_acceptance: true,
                    shared_text_scope_acceptance: Some(true),
                    substantive_difference_reviewed: true,
                    evidence_ref: format!("automation-preview:{}", id(s)?),
                },
                origin,
                true,
            )
            .await?;
            sqlx::query("UPDATE automation_business_previews SET confirmed_at=clock_timestamp(),revision=revision+1 WHERE id=$1").bind(id(s)?).execute(&mut **tx).await?;
            object(tx, "mobile_delivery_strategy_runs", uid(&v, "run_id")?).await
        }
        AutomationCommand::PrepareExecution {} => prepare_execution(tx, p, enabled, origin).await,
        AutomationCommand::ReportPreparation { generation, stage } => {
            require(p.protocol() == 4, "准备要求 v4")?;
            let (d, g) = device(tx, p).await?;
            require(
                [
                    "checking_environment",
                    "returning_to_launcher",
                    "matching_launcher",
                    "searching_application",
                    "browsing_all_applications",
                    "opening_application",
                    "verifying_application",
                ]
                .contains(&stage.as_str()),
                "准备阶段无效",
            )?;
            let t = object(tx, "automation_readiness_tickets", id(s)?).await?;
            let authorization = active_authorization(tx, d).await?;
            require(
                t["authorization_id"] == authorization.authorization_id.to_string(),
                "执行授权已变化",
            )?;
            require(
                t["device_id"] == d.to_string()
                    && t["credential_generation"] == g
                    && t["generation"] == generation.to_string(),
                "准备票据不匹配",
            )?;
            let i = object(tx, "mobile_delivery_items", uid(&t, "item_id")?).await?;
            require(i["state"] == "queued", "当前项已变化")?;
            configured::validate(tx, &i).await?;
            validate_manifest(tx, &i).await?;
            let current = latest_environment(tx, d).await?;
            require(current["id"] == t["environment_id"], "准备环境已变化")?;
            validation_gate(tx, d).await?;
            let updated=sqlx::query("UPDATE automation_readiness_tickets SET stage=$2 WHERE id=$1 AND NOT ready AND expires_at>clock_timestamp()").bind(id(s)?).bind(stage).execute(&mut **tx).await?;
            require(updated.rows_affected() == 1, "准备票据过期或已完成")?;
            object(tx, "automation_readiness_tickets", id(s)?).await
        }
        AutomationCommand::ApplicationReady {
            generation,
            manifest_digest,
            environment_id,
        } => {
            let (d, g) = device(tx, p).await?;
            require(p.protocol() == 4, "准备要求 v4")?;
            let t = object(tx, "automation_readiness_tickets", id(s)?).await?;
            let authorization = active_authorization(tx, d).await?;
            require(
                t["authorization_id"] == authorization.authorization_id.to_string(),
                "执行授权已变化",
            )?;
            require(
                t["device_id"] == d.to_string()
                    && t["credential_generation"] == g
                    && t["generation"] == generation.to_string()
                    && t["environment_id"] == environment_id.to_string(),
                "准备票据不匹配",
            )?;
            let m = object(
                tx,
                "automation_execution_manifests",
                uid(&t, "manifest_id")?,
            )
            .await?;
            require(m["content_digest"] == manifest_digest, "清单摘要不匹配")?;
            let e = latest_environment(tx, d).await?;
            require(e["id"] == t["environment_id"], "环境代次已变化")?;
            validation_gate(tx, d).await?;
            let i = object(tx, "mobile_delivery_items", uid(&t, "item_id")?).await?;
            configured::validate(tx, &i).await?;
            validate_manifest(tx, &i).await?;
            require(i["state"] == "queued", "当前项已变化")?;
            let updated=sqlx::query("UPDATE automation_readiness_tickets SET ready=true,stage='ready' WHERE id=$1 AND expires_at>clock_timestamp()").bind(id(s)?).execute(&mut **tx).await?;
            require(updated.rows_affected() == 1, "准备票据已过期")?;
            object(tx, "automation_readiness_tickets", id(s)?).await
        }
    }
}
async fn preview_business(
    tx: &mut Tx<'_>,
    p: &Principal,
    s: &Scope,
    mut input: BusinessInput,
    origin: &str,
) -> Result<Value> {
    input.validate()?;
    input.text_content = normalize(&input.text_content)?;
    let school = school(s)?;
    let resolution = resolve(tx, school, input.app_binding_id, input.capability_id).await?;
    let b = &resolution["binding"];
    let d = uid(b, "device_id")?;
    let config: ConfigurationInput =
        serde_json::from_value(resolution["configuration"].clone()).map_err(|_| Error::Conflict)?;
    require(
        input.group_ids.len() <= config.policy.max_targets
            && input.interval_seconds * 1000
                >= config
                    .policy
                    .minimum_group_interval_ms
                    .max(config.baseline.human_group_min_interval_ms),
        "群间等待低于人工基准或目标数超限",
    )?;
    let active:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_delivery_strategy_runs r JOIN mobile_delivery_tasks t ON t.id=r.task_id WHERE r.device_id=$1 AND t.state='confirmed')").bind(d).fetch_one(&mut **tx).await?;
    require(!active, "手机已有确认中的业务，请先完成或停止")?;
    validation_gate(tx, d).await?;
    // Compile the existing group handler's typed configuration. This private compatibility release is
    // frozen into the new manifest; it is never offered as an administrator-selectable adaptation.
    let cid = Uuid::now_v7();
    let config_body = json!(config);
    sqlx::query("INSERT INTO client_rule_bundle_versions(id,name,configuration,content_digest,created_by) VALUES($1,$2,$3,$4,$5)").bind(cid).bind(&config.name).bind(&config_body).bind(hash(&config_body.to_string())).bind(p.actor()?).execute(&mut **tx).await?;
    let cv = object(tx, "client_rule_bundle_versions", cid).await?;
    let cr = Uuid::now_v7();
    sqlx::query("INSERT INTO client_rule_releases(id,configuration_id,school_id,device_id,evidence_ref,created_by) VALUES($1,$2,$3,$4,$5,$6)").bind(cr).bind(uid(&cv,"id")?).bind(school).bind(d).bind(format!("automation-binding:{}",input.app_binding_id)).bind(p.actor()?).execute(&mut **tx).await?;
    let sid = Uuid::now_v7();
    let single = input.group_ids.len() == 1;
    let groups: Vec<StrategyTarget> = input
        .group_ids
        .iter()
        .map(|group_id| StrategyTarget {
            group_id: *group_id,
        })
        .collect();
    let si = StrategyInput {
        name: input.name.clone(),
        sender_account_id: uid(b, "sender_account_id")?,
        device_id: d,
        group_id: if single {
            Some(input.group_ids[0])
        } else {
            None
        },
        delivery_mode: if single {
            "single_group"
        } else {
            "ordered_groups"
        }
        .into(),
        targets: if single { vec![] } else { groups },
        inter_group_interval_seconds: Some(input.interval_seconds),
        configuration_release_id: Some(cr),
        text_content: input.text_content.clone(),
        enabled: true,
        trigger_mode: "manual_once".into(),
    };
    si.validate()?;
    // snapshot() validates each account/group authorization before any draft is committed.
    for group in &input.group_ids {
        let snap = snapshot(
            tx,
            school,
            &ItemInput {
                group_id: *group,
                sender_account_id: si.sender_account_id,
                source: ContentSource::Text {
                    text_content: input.text_content.clone(),
                },
            },
            origin,
        )
        .await?;
        require(
            snap["group"]["identity_mode"] == "ui_visible"
                && snap["sender"]["identity_mode"] == "ui_visible",
            "业务仅接受已核对的界面模式群与账号",
        )?;
    }
    sqlx::query("INSERT INTO mobile_delivery_strategies(id,school_id,name,sender_account_id,group_id,device_id,text_content,enabled,created_by,delivery_mode,targets,inter_group_interval_seconds,configuration_release_id,automation_binding_id,automation_capability_id) VALUES($1,$2,$3,$4,$5,$6,$7,true,$8,$9,$10,$11,$12,$13,$14)")
        .bind(sid).bind(school).bind(&si.name).bind(si.sender_account_id).bind(si.group_id).bind(d).bind(&si.text_content).bind(p.actor()?).bind(&si.delivery_mode).bind(json!(si.targets)).bind(input.interval_seconds).bind(cr).bind(input.app_binding_id).bind(input.capability_id).execute(&mut **tx).await?;
    for (i, g) in input.group_ids.iter().enumerate() {
        sqlx::query("INSERT INTO mobile_delivery_strategy_targets(strategy_id,school_id,position,group_id) VALUES($1,$2,$3,$4)").bind(sid).bind(school).bind((i+1)as i32).bind(g).execute(&mut **tx).await?;
    }
    let strategy = object(tx, "mobile_delivery_strategies", sid).await?;
    let scope = Scope {
        school: Some(school),
        id: Some(sid),
    };
    let account_binding = binding(tx, d).await?;
    let run = configured::run(tx, p, &scope, strategy, si, account_binding, origin).await?;
    let mid = Uuid::now_v7();
    let rid = uid(&run, "id")?;
    let manifest = json!({"schema_version":1,"protocol_version":4,"engine_version":AUTOMATION_ENGINE,"run_id":rid,"resolution":resolution,"business":input});
    let md = automation_digest(&manifest);
    sqlx::query("INSERT INTO automation_execution_manifests(id,school_id,app_binding_id,body,content_digest) VALUES($1,$2,$3,$4,$5)").bind(mid).bind(school).bind(input.app_binding_id).bind(&manifest).bind(&md).execute(&mut **tx).await?;
    sqlx::query("UPDATE mobile_delivery_strategy_runs SET automation_manifest_id=$2 WHERE id=$1")
        .bind(rid)
        .bind(mid)
        .execute(&mut **tx)
        .await?;
    let items: Vec<Value> = sqlx::query_scalar(
        "SELECT to_jsonb(i) FROM mobile_delivery_items i WHERE task_id=$1 ORDER BY id",
    )
    .bind(uid(&run, "task_id")?)
    .fetch_all(&mut **tx)
    .await?;
    for mut i in items {
        i["snapshot"]["requirements"]["protocol_version"] = json!(4);
        i["snapshot"]["requirements"]["automation_engine_version"] = json!(AUTOMATION_ENGINE);
        i["snapshot"]["requirements"]["manifest_digest"] = json!(md);
        i["snapshot"]["automation_manifest"] =
            json!({"id":mid,"content_digest":md,"body":manifest});
        sqlx::query("UPDATE mobile_delivery_items SET snapshot=$2,snapshot_digest=$3 WHERE id=$1")
            .bind(uid(&i, "id")?)
            .bind(&i["snapshot"])
            .bind(hash(&i["snapshot"].to_string()))
            .execute(&mut **tx)
            .await?;
    }
    let tp = preview(tx, uid(&run, "task_id")?, origin).await?;
    let pid = Uuid::now_v7();
    let pd = automation_digest(
        &json!({"input":input,"resolution":resolution,"task_preview":tp,"manifest_digest":md}),
    );
    sqlx::query("INSERT INTO automation_business_previews(id,school_id,created_by,input,resolution,preview_digest,run_id,task_preview) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(pid).bind(school).bind(p.actor()?).bind(json!(input)).bind(resolution).bind(pd).bind(rid).bind(tp).execute(&mut **tx).await?;
    object(tx, "automation_business_previews", pid).await
}
pub(super) async fn validate_manifest(tx: &mut Tx<'_>, i: &Value) -> Result<()> {
    if i["snapshot"]["requirements"]["protocol_version"] != 4 {
        return Ok(());
    }
    let frozen = &i["snapshot"]["automation_manifest"];
    let m = object(tx, "automation_execution_manifests", uid(frozen, "id")?).await?;
    require(
        m["body"] == frozen["body"]
            && m["content_digest"] == frozen["content_digest"]
            && m["content_digest"] == automation_digest(&m["body"]),
        "执行清单已变化",
    )?;
    let r = &m["body"]["resolution"];
    let b = app_binding(tx, uid(&m, "school_id")?, uid(&m, "app_binding_id")?).await?;
    require(b == r["binding"], "应用绑定已变化，请重新预览")?;
    let e = latest_environment(tx, uid(&b, "device_id")?).await?;
    let er = report(&e)?;
    require(
        er.supports_execution() && environment_digest(&er, &b)? == r["environment_digest"],
        "实际安装环境或权限已变化",
    )?;
    for frozen in r["components"].as_object().ok_or(Error::Conflict)?.values() {
        let live = object(tx, "automation_component_releases", uid(frozen, "id")?).await?;
        require(
            live["revoked"] == false && live["component_id"] == frozen["component_id"],
            "适配已撤销",
        )?;
        // A superseding release does not rewrite an already frozen run. Revocation still blocks.
    }
    Ok(())
}
pub(super) async fn require_readiness(tx: &mut Tx<'_>, i: &Value) -> Result<()> {
    let d = uid(&i["snapshot"]["execution_target"], "device_id")?;
    validation_gate(tx, d).await?;
    let binding = &i["snapshot"]["automation_manifest"]["body"]["resolution"]["binding"];
    let authorization = authorized(tx, d, uid(i, "school_id")?, Some(binding), false).await?;
    let environment = latest_environment(tx, d).await?;
    let ready:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM automation_readiness_tickets t JOIN execution_devices d ON d.id=t.device_id WHERE t.device_id=$1 AND t.item_id=$2 AND t.manifest_id=$3 AND t.environment_id=$4 AND t.credential_generation=d.credential_generation AND t.authorization_id=$5 AND t.ready AND t.expires_at>clock_timestamp())")
        .bind(d).bind(uid(i,"id")?).bind(uid(&i["snapshot"]["automation_manifest"],"id")?).bind(uid(&environment,"id")?).bind(authorization.authorization_id).fetch_one(&mut **tx).await?;
    if ready {
        Ok(())
    } else {
        Err(Error::Policy("automation_readiness_required"))
    }
}
async fn prepare_execution(
    tx: &mut Tx<'_>,
    p: &Principal,
    enabled: bool,
    origin: &str,
) -> Result<Value> {
    require(p.protocol() == 4, "准备票据要求 v4")?;
    if !enabled {
        return Err(Error::Unavailable);
    }
    let (d, g) = device(tx, p).await?;
    no_device_operation(tx, d, None, false).await?;
    let b = binding(tx, d).await?;
    let candidate = configured::candidate(tx, d, uid(&b, "sender_account_id")?, 4).await?;
    let i = match candidate {
        Some(Ok(i)) => i,
        Some(Err(wait)) => return Ok(wait),
        None => {
            return Ok(json!({"waiting":true,"reason":"no_ready_task","retry_after_seconds":30}));
        }
    };
    let app = &i["snapshot"]["automation_manifest"]["body"]["resolution"]["binding"];
    let authorization = authorized(tx, d, uid(&i, "school_id")?, Some(app), false).await?;
    validate_manifest(tx, &i).await?;
    validate_snapshot(tx, &i, origin).await?;
    restrictions(tx, &i).await?;
    let m = &i["snapshot"]["automation_manifest"];
    let previous:Option<Value>=sqlx::query_scalar("SELECT to_jsonb(t) FROM automation_readiness_tickets t WHERE device_id=$1 AND item_id=$2 AND credential_generation=$3 AND environment_id=(SELECT id FROM device_environment_snapshots WHERE device_id=$1 ORDER BY generation DESC LIMIT 1) AND expires_at>clock_timestamp() ORDER BY expires_at DESC LIMIT 1").bind(d).bind(uid(&i,"id")?).bind(g).fetch_optional(&mut **tx).await?;
    if let Some(mut previous) = previous {
        previous["manifest"] = m.clone();
        return Ok(previous);
    }
    let environment = latest_environment(tx, d).await?;
    let tid = Uuid::now_v7();
    sqlx::query("INSERT INTO automation_readiness_tickets(id,device_id,item_id,manifest_id,environment_id,credential_generation,generation,authorization_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8)")
        .bind(tid).bind(d).bind(uid(&i,"id")?).bind(uid(m,"id")?).bind(uid(&environment,"id")?).bind(g).bind(Uuid::now_v7()).bind(authorization.authorization_id).execute(&mut **tx).await?;
    let mut out = object(tx, "automation_readiness_tickets", tid).await?;
    out["manifest"] = m.clone();
    Ok(out)
}
pub(super) async fn query(
    tx: &mut Tx<'_>,
    p: &Principal,
    s: &Scope,
    q: AutomationQuery,
    limit: i64,
    offset: i64,
) -> Result<Value> {
    match q {
        AutomationQuery::AuthorizationContext => authorization_context(tx,p).await,
        AutomationQuery::Apps => {
            p.actor()?;
            let rows:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(a)||jsonb_build_object('capabilities',COALESCE((SELECT jsonb_agg(to_jsonb(c) ORDER BY c.id) FROM automation_capabilities c WHERE c.app_id=a.id),'[]'::jsonb)) FROM automation_apps a ORDER BY a.id").fetch_all(&mut **tx).await?; Ok(json!(rows))
        }
        AutomationQuery::Applications => {
            let d=scoped_device(tx,school(s)?,id(s)?).await?;
            let e=latest_environment(tx,id(s)?).await;
            match e { Ok(e)=>Ok(json!({"id":id(s)?,"device":d,"environment":e,"status":"reported","next_step":"选择已安装应用，检查权限并测试打开"})),Err(Error::Policy(_))=>Ok(json!({"id":id(s)?,"device":d,"environment":null,"status":"waiting_environment","next_step":"等待支持 v4 的新版 Otter 检查手机环境；当前手机端尚未支持时请勿反复配对"})),Err(e)=>Err(e) }
        }
        AutomationQuery::Bindings => {
            let rows:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(b)||jsonb_build_object('device_name',d.name,'app_name',a.name,'account_name',s.visible_profile->>'account_name') FROM device_app_bindings b JOIN execution_devices d ON d.id=b.device_id JOIN automation_apps a ON a.id=b.app_id JOIN sending_accounts s ON s.id=b.sender_account_id WHERE b.school_id=$1 ORDER BY b.id LIMIT $2 OFFSET $3").bind(school(s)?).bind(limit).bind(offset).fetch_all(&mut **tx).await?; Ok(json!(rows))
        }
        AutomationQuery::Validations => {
            Ok(json!(sqlx::query_scalar::<_,Value>("SELECT to_jsonb(j)||jsonb_build_object('state',CASE WHEN j.expires_at<=clock_timestamp() AND j.state IN ('queued','offered','accepted','running') THEN 'expired' ELSE j.state END) FROM device_validation_jobs j WHERE j.school_id=$1 ORDER BY j.created_at DESC,j.id DESC LIMIT $2 OFFSET $3").bind(school(s)?).bind(limit).bind(offset).fetch_all(&mut **tx).await?))
        }
        AutomationQuery::Validation => {
            p.actor()?; let mut j=scoped_object(tx,s,"device_validation_jobs").await?;
            let events:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(e) FROM device_validation_events e WHERE job_id=$1 ORDER BY created_at,id LIMIT 200").bind(id(s)?).fetch_all(&mut **tx).await?; j["events"]=json!(events);
            let expired:bool=sqlx::query_scalar("SELECT expires_at<=clock_timestamp() FROM device_validation_jobs WHERE id=$1").bind(id(s)?).fetch_one(&mut **tx).await?;
            if expired && matches!(j["state"].as_str(),Some("queued"|"offered"|"accepted"|"running")) { j["state"]=json!("expired"); }
            Ok(j)
        }
        AutomationQuery::Components | AutomationQuery::Releases => {
            platform(tx,p).await?; let table=if matches!(q,AutomationQuery::Components){"automation_component_versions"}else{"automation_component_releases"};
            Ok(json!(sqlx::query_scalar::<_,Value>(&format!("SELECT to_jsonb(c) FROM {table} c ORDER BY created_at DESC,id DESC LIMIT $1 OFFSET $2")).bind(limit).bind(offset).fetch_all(&mut **tx).await?))
        }
        AutomationQuery::Runs => {
            Ok(json!(sqlx::query_scalar::<_,Value>("SELECT to_jsonb(r)||jsonb_build_object('task_state',t.state,'name',r.strategy_snapshot->>'name') FROM mobile_delivery_strategy_runs r JOIN mobile_delivery_tasks t ON t.id=r.task_id WHERE r.school_id=$1 AND r.automation_manifest_id IS NOT NULL AND t.state<>'draft' ORDER BY r.created_at DESC,r.id DESC LIMIT $2 OFFSET $3").bind(school(s)?).bind(limit).bind(offset).fetch_all(&mut **tx).await?))
        }
        AutomationQuery::Run => {
            let r=scoped_object(tx,s,"mobile_delivery_strategy_runs").await?;
            require(!r["automation_manifest_id"].is_null(),"不是新版业务记录")?;
            let mut progress=configured::progress(tx,p,s).await?;
            progress["task_id"]=r["task_id"].clone();
            let preparation:Option<Value>=sqlx::query_scalar("SELECT jsonb_build_object('stage',t.stage,'expires_at',t.expires_at) FROM automation_readiness_tickets t JOIN mobile_delivery_run_items i ON i.item_id=t.item_id WHERE i.run_id=$1 AND i.position=$2 AND t.expires_at>clock_timestamp() ORDER BY t.expires_at DESC LIMIT 1").bind(id(s)?).bind(progress["current_position"].as_i64().ok_or(Error::Conflict)? as i32).fetch_optional(&mut **tx).await?;
            progress["preparation"]=json!(preparation); progress["manifest"]=object(tx,"automation_execution_manifests",uid(&r,"automation_manifest_id")?).await?;
            Ok(progress)
        }
        AutomationQuery::ValidationJobs => {
            require(p.protocol()==4,"验证要求 v4")?; let (d,g)=device(tx,p).await?;
            let mut jobs:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(j) FROM device_validation_jobs j WHERE device_id=$1 AND credential_generation=$2 AND state IN ('queued','offered','accepted','running') AND expires_at>clock_timestamp() ORDER BY created_at,id LIMIT 1").bind(d).bind(g).fetch_all(&mut **tx).await?;
            for j in &mut jobs { school_auth(tx,uid(j,"created_by")?,uid(j,"school_id")?,true).await?; if matches!(kind(j)?,ValidationKind::InspectRoles|ValidationKind::MeasureManual){platform(tx,&Principal::Actor(uid(j,"created_by")?)).await?;} let ids:Vec<Uuid>=serde_json::from_value(j["component_ids"].clone()).map_err(|_|Error::Conflict)?; j["components"]=json!(components(tx,&ids).await?);
                if !j["app_binding_id"].is_null() {
                    let b=app_binding(tx,uid(j,"school_id")?,uid(j,"app_binding_id")?).await?;
                    let e=object(tx,"device_environment_snapshots",uid(j,"environment_id")?).await?;
                    j["application"]=json!(selected(&report(&e)?,&b)?);
                    j["launcher"]=e["body"]["launcher"].clone();
                }
            }
            Ok(json!(jobs))
        }
        AutomationQuery::Resolution => {
            let b=object(tx,"device_app_bindings",id(s)?).await?; if let Principal::Actor(actor)=p { school_auth(tx,*actor,school(s)?,true).await?; require(b["school_id"]==school(s)?.to_string(),"绑定不属于本学校")?; } else { require(p.protocol()==4,"解析要求 v4")?; let (d,_)=device(tx,p).await?; require(b["device_id"]==d.to_string(),"绑定不属于当前手机")?; }
            let capability:Uuid=sqlx::query_scalar("SELECT id FROM automation_capabilities WHERE app_id=$1 AND handler='wecom_group_text'").bind(uid(&b,"app_id")?).fetch_one(&mut **tx).await?;
            let mut r=resolve(tx,uid(&b,"school_id")?,id(s)?,capability).await?; r["id"]=json!(id(s)?); Ok(r)
        }
    }
}
