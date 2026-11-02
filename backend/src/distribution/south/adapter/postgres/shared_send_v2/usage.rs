//! One durable business intent coordinates resource grants and immutable execution scopes.
use super::*;
use crate::distribution::domain::mobile_execution::usage::*;
use chrono::{DateTime, Utc};

async fn permission(tx: &mut Tx<'_>, a: &Actor, s: Option<Uuid>, admin: bool) -> Result<bool> {
    let platform: bool =
        sqlx::query_scalar("SELECT platform_admin FROM accounts WHERE id=$1 AND active FOR SHARE")
            .bind(a.id)
            .fetch_optional(&mut **tx)
            .await?
            .ok_or(Error::Forbidden)?;
    if !platform {
        authorize(tx, a.id, Some(s.ok_or(Error::Forbidden)?), admin).await?;
    } else if let Some(s) = s {
        require(
            sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM schools WHERE id=$1 AND active)",
            )
            .bind(s)
            .fetch_one(&mut **tx)
            .await?,
            "学校已停用",
        )?;
    }
    Ok(platform)
}
fn scheme(p: &Value) -> Value {
    json!({"id":p["id"],"limits":p["limits"],"timezone":p["timezone"],"allowed_windows":p["allowed_windows"],"daily_max_calls":p["limits"]["school"]["daily_max_calls"],"interval_seconds":p["limits"]["target"]["interval_seconds"]})
}
fn same_scheme(a: &Value, b: &Value) -> bool {
    let mut a = a.clone();
    let mut b = b.clone();
    if let Some(v) = a.as_object_mut() {
        v.remove("id");
    }
    if let Some(v) = b.as_object_mut() {
        v.remove("id");
    }
    a == b
}
async fn options(tx: &mut Tx<'_>, a: &Actor, s: Option<Uuid>) -> Result<Value> {
    let platform = permission(tx, a, s, false).await?;
    let devices:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('id',d.id,'name',d.name,'revision',d.revision,'online',coalesce(cp.state='online' AND cp.last_control_heartbeat_at>clock_timestamp()-interval '30 seconds',false)) FROM execution_devices d LEFT JOIN device_control_presence cp ON cp.id=d.id WHERE d.enabled AND ($1 OR EXISTS(SELECT 1 FROM mobile_shared_school_access x WHERE x.device_id=d.id AND x.school_id=$2 AND x.enabled)) ORDER BY d.name,d.id").bind(platform).bind(s).fetch_all(&mut **tx).await?;
    let schools:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('id',id,'name',name) FROM schools WHERE active AND ($1 OR id=$2) ORDER BY name,id").bind(platform).bind(s).fetch_all(&mut **tx).await?;
    let targets:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('id',id,'name',coalesce(nullif(alias,''),search_text),'search_text',search_text,'revision',locator_revision) FROM mobile_targets WHERE school_id=$1 AND enabled AND archived_at IS NULL ORDER BY alias,search_text,id").bind(s).fetch_all(&mut **tx).await?;
    let p = policy(tx).await?;
    Ok(
        json!({"id":"usage-options","schools":schools,"devices":devices,"targets":targets,"scheme":p.as_ref().map(scheme),"sending_status":business_scheme::status(tx).await?,"business_scheme":if platform {p.clone()} else {None}}),
    )
}
fn selection_input(b: &Value) -> Result<UsageInput> {
    serde_json::from_value(json!({"device_ids":b["device_ids"],"targets":b["targets"].as_array().unwrap().iter().map(|t|json!({"id":t["id"],"name":t["name"],"search_text":t["search_text"]})).collect::<Vec<_>>(),"max_calls":b["max_calls"],"expires_at":b["expires_at"],"auto_targets":b["auto_targets"],"renewal_of":b.get("renewal_of"),"expected_revision":b.get("expected_revision")})).map_err(|_|Error::Conflict)
}
fn date(v: &Value, k: &str) -> Result<DateTime<Utc>> {
    serde_json::from_value(v[k].clone()).map_err(|_| Error::Conflict)
}
fn covers(parent: &Value, input: &Value) -> bool {
    let devices = input["device_ids"].as_array().cloned().unwrap_or_default();
    let targets = input["targets"].as_array().cloned().unwrap_or_default();
    let parent_targets = parent["targets"]
        .as_array()
        .map(|ts| ts.iter().map(|t| t["id"].clone()).collect::<Vec<_>>())
        .unwrap_or_default();
    let unchanged = input["targets"].as_array().is_some_and(|ts| {
        ts.iter().all(|t| {
            parent["targets"].as_array().is_some_and(|original| {
                match original.iter().find(|v| v["id"] == t["id"]) {
                    Some(v) => v == t,
                    None => parent["auto_targets"] == true,
                }
            })
        })
    }) && input["device_revisions"].as_array().is_some_and(|ds| {
        ds.iter().all(|d| {
            parent["device_revisions"]
                .as_array()
                .is_some_and(|original| original.contains(d))
        })
    }) && same_scheme(&parent["scheme"], &input["scheme"]);
    unchanged
        && parent["device_ids"]
            .as_array()
            .is_some_and(|ds| devices.iter().all(|d| ds.contains(d)))
        && (parent["auto_targets"] == true
            || targets.iter().all(|t| parent_targets.contains(&t["id"])))
        && (input["auto_targets"] != true || parent["auto_targets"] == true)
        && input["max_calls"].as_i64().unwrap_or(i64::MAX)
            <= parent["max_calls"].as_i64().unwrap_or(0)
}
async fn event(tx: &mut Tx<'_>, id: Uuid, a: Uuid, action: &str, body: &Value) -> Result<()> {
    sqlx::query(
        "INSERT INTO mobile_usage_events(id,usage_id,actor_id,action,body) VALUES($1,$2,$3,$4,$5)",
    )
    .bind(Uuid::now_v7())
    .bind(id)
    .bind(a)
    .bind(action)
    .bind(body)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
async fn valid_selection(tx: &mut Tx<'_>, a: &Actor, s: Uuid, v: &UsageInput) -> Result<Value> {
    v.validate()?;
    if let Some(id) = v.renewal_of {
        let ok:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_usages WHERE id=$1 AND school_id=$2 AND revision=$3 AND approval='approved' AND state<>'revoked')").bind(id).bind(s).bind(v.expected_revision).fetch_one(&mut **tx).await?;
        require(ok, "原授权已变化，请刷新后重新申请")?;
    }
    let o = options(tx, a, Some(s)).await?;
    for d in &v.device_ids {
        require(
            o["devices"]
                .as_array()
                .is_some_and(|ds| ds.iter().any(|x| x["id"] == json!(d))),
            "手机资源资格已变化",
        )?;
    }
    let mut body = json!(v);
    for t in body["targets"].as_array_mut().ok_or(Error::Conflict)? {
        if !t["id"].is_null() {
            let found = o["targets"]
                .as_array()
                .and_then(|ts| ts.iter().find(|x| x["id"] == t["id"]))
                .ok_or(Error::Policy("目标群资料已变化"))?;
            require(
                found["search_text"] == t["search_text"] && found["name"] == t["name"],
                "目标群资料已变化",
            )?;
            t["revision"] = found["revision"].clone();
        }
    }
    body["scheme_id"] = o["scheme"]["id"].clone();
    body["scheme"] = o["scheme"].clone();
    body["device_revisions"] = json!(
        o["devices"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|d| v.device_ids.iter().any(|id| json!(id) == d["id"]))
            .map(|d| json!({"id":d["id"],"revision":d["revision"]}))
            .collect::<Vec<_>>()
    );
    Ok(body)
}
async fn detail(tx: &mut Tx<'_>, s: Option<Uuid>, id: Option<Uuid>) -> Result<Value> {
    let mut rows:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(u)||jsonb_build_object('school_name',s.name,'delivery',CASE WHEN j.state='ready' THEN 'available' ELSE 'waiting' END,'progress',coalesce(j.reason,'等待处理'),'issues',coalesce((SELECT jsonb_agg(to_jsonb(i)-'context') FROM mobile_usage_issues i WHERE i.usage_id=u.id),'[]'::jsonb),'events',coalesce((SELECT jsonb_agg(to_jsonb(e) ORDER BY e.created_at) FROM mobile_usage_events e WHERE e.usage_id=u.id),'[]'::jsonb)) FROM mobile_usages u JOIN schools s ON s.id=u.school_id LEFT JOIN mobile_usage_jobs j ON j.usage_id=u.id WHERE ($1::uuid IS NULL OR u.school_id=$1) AND ($2::uuid IS NULL OR u.id=$2) ORDER BY u.created_at DESC,u.id DESC").bind(s).bind(id).fetch_all(&mut **tx).await?;
    for row in &mut rows {
        let school = uid(row, "school_id")?;
        let states = readiness::school(tx, school).await?;
        let devices = row["body"]["device_ids"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let targets = row["body"]["targets"]
            .as_array()
            .map(|ts| ts.iter().map(|t| t["id"].clone()).collect::<Vec<_>>())
            .unwrap_or_default();
        row["readiness"]=json!(states.iter().filter(|v|devices.contains(&v["device_id"])&&(row["body"]["auto_targets"]==true||targets.contains(&v["target_id"]))).map(|v|json!({"device_name":v["device_name"],"target_name":v["target_name"],"state":v["state"],"ready":v["ready"],"messages":v["blockers"].as_array().unwrap().iter().map(|b|b["message"].clone()).collect::<Vec<_>>()})).collect::<Vec<_>>());
    }
    Ok(json!({"id":"usages","items":rows}))
}
impl Repository {
    pub async fn usage_read(
        &self,
        a: &Actor,
        s: Option<Uuid>,
        id: Option<Uuid>,
        get_options: bool,
    ) -> Result<Value> {
        let mut tx = self.0.begin().await?;
        guard(&mut tx).await?;
        permission(&mut tx, a, s, false).await?;
        reconcile(&mut tx).await?;
        let result = if get_options {
            options(&mut tx, a, s).await?
        } else {
            detail(&mut tx, s, id).await?
        };
        tx.commit().await?;
        Ok(result)
    }
    pub async fn usage_write(
        &self,
        a: &Actor,
        s: Uuid,
        key: &str,
        command: UsageCommand,
    ) -> Result<Value> {
        crate::business::text(key, 1, 128)?;
        let mut tx = self.0.begin().await?;
        guard(&mut tx).await?;
        let task_preview = matches!(command, UsageCommand::TaskPreview(_));
        let platform = permission(&mut tx, a, Some(s), !task_preview).await?;
        if matches!(&command, UsageCommand::TaskConfirm(_)) {
            authorize(&mut tx, a.id, Some(s), true).await?;
        }
        let scope = format!("{s}:usage");
        let body = json!(command);
        if let Some(r) = replay(&mut tx, a.id, 0, &scope, key, &body).await? {
            return Ok(r);
        }
        let result = match command {
            UsageCommand::Preview(v) => {
                let b = valid_selection(&mut tx, a, s, &v).await?;
                let id = Uuid::now_v7();
                let h = digest(&b)?;
                sqlx::query("INSERT INTO mobile_usage_previews(id,school_id,actor_id,body,digest) VALUES($1,$2,$3,$4,$5)").bind(id).bind(s).bind(a.id).bind(&b).bind(&h).execute(&mut *tx).await?;
                json!({"id":id,"digest":h,"body":b})
            }
            UsageCommand::Confirm(v) => {
                let p:Value=sqlx::query_scalar("SELECT to_jsonb(p) FROM mobile_usage_previews p WHERE id=$1 AND school_id=$2 AND actor_id=$3 AND digest=$4").bind(v.preview_id).bind(s).bind(a.id).bind(v.digest).fetch_optional(&mut *tx).await?.ok_or(Error::Policy("预览已过期，请重新核对"))?;
                if let Some(id) = sqlx::query_scalar::<_, Uuid>(
                    "SELECT usage_id FROM mobile_usage_confirmations WHERE preview_id=$1",
                )
                .bind(v.preview_id)
                .fetch_optional(&mut *tx)
                .await?
                {
                    let result = detail(&mut tx, Some(s), Some(id)).await?;
                    remember(&mut tx, a.id, 0, &scope, key, &body, &result).await?;
                    tx.commit().await?;
                    return Ok(result);
                }
                require(
                    date(&p, "expires_at")? > Utc::now(),
                    "预览已过期，请重新核对",
                )?;
                let mut b = p["body"].clone();
                let input = selection_input(&b)?;
                require(
                    valid_selection(&mut tx, a, s, &input).await? == b,
                    "范围或使用方案已变化，请重新核对",
                )?;
                for t in b["targets"].as_array_mut().unwrap() {
                    if t["id"].is_null() {
                        let id = Uuid::now_v7();
                        sqlx::query("INSERT INTO mobile_targets(id,school_id,alias,search_text,created_by,updated_by) VALUES($1,$2,$3,$4,$5,$5)").bind(id).bind(s).bind(t["name"].as_str()).bind(t["search_text"].as_str()).bind(a.id).execute(&mut *tx).await?;
                        t["id"] = json!(id);
                        t["revision"] = json!(1);
                    }
                }
                // A school never creates another budget by copying an existing approval. Covered requests reuse it.
                let parents:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(u) FROM mobile_usages u WHERE school_id=$1 AND approval='approved' AND state='active' AND expires_at>=$2 ORDER BY created_at").bind(s).bind(input.expires_at).fetch_all(&mut *tx).await?;
                if !platform
                    && input.renewal_of.is_none()
                    && let Some(parent) = parents.iter().find(|u| covers(&u["body"], &b))
                {
                    let id = uid(parent, "id")?;
                    sqlx::query(
                        "INSERT INTO mobile_usage_confirmations(preview_id,usage_id) VALUES($1,$2)",
                    )
                    .bind(v.preview_id)
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
                    event(&mut tx, id, a.id, "reused", &b).await?;
                    reconcile(&mut tx).await?;
                    let r = detail(&mut tx, Some(s), Some(id)).await?;
                    remember(&mut tx, a.id, 0, &scope, key, &body, &r).await?;
                    tx.commit().await?;
                    return Ok(r);
                }
                let budget: Option<Uuid> = if let Some(previous) = input.renewal_of {
                    Some(sqlx::query_scalar("SELECT budget_root FROM mobile_usages WHERE id=$1 AND school_id=$2 AND revision=$3 FOR UPDATE").bind(previous).bind(s).bind(input.expected_revision).fetch_optional(&mut *tx).await?.ok_or(Error::Conflict)?)
                } else {
                    None
                };
                let id = Uuid::now_v7();
                let approval = if platform { "approved" } else { "pending" };
                sqlx::query("INSERT INTO mobile_usages(id,school_id,actor_id,approval,body,confirmed_until,expires_at,approved_by,supersedes,budget_root) VALUES($1,$2,$3,$4,$5,clock_timestamp()+interval '24 hours',$6,$7,$8,$9)").bind(id).bind(s).bind(a.id).bind(approval).bind(&b).bind(input.expires_at).bind(platform.then_some(a.id)).bind(input.renewal_of).bind(budget.unwrap_or(id)).execute(&mut *tx).await?;
                sqlx::query(
                    "INSERT INTO mobile_usage_confirmations(preview_id,usage_id) VALUES($1,$2)",
                )
                .bind(v.preview_id)
                .bind(id)
                .execute(&mut *tx)
                .await?;
                event(&mut tx, id, a.id, approval, &b).await?;
                sqlx::query("INSERT INTO mobile_usage_jobs(usage_id) VALUES($1)")
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
                reconcile(&mut tx).await?;
                detail(&mut tx, Some(s), Some(id)).await?
            }
            UsageCommand::Mutate {
                id,
                operation,
                mutation,
            } => {
                crate::business::text(&mutation.reason, 1, 2000)?;
                let old:Value=sqlx::query_scalar("SELECT to_jsonb(u) FROM mobile_usages u WHERE id=$1 AND school_id=$2 AND revision=$3 FOR UPDATE").bind(id).bind(s).bind(mutation.expected_revision).fetch_optional(&mut *tx).await?.ok_or(Error::Conflict)?;
                require(
                    platform || !matches!(operation.as_str(), "approve" | "decline"),
                    "需要平台审批",
                )?;
                match operation.as_str() {
                    "approve" => {
                        let input = selection_input(&old["body"])?;
                        require(
                            valid_selection(&mut tx, a, s, &input).await? == old["body"],
                            "申请范围或方案已变化，请重新核对后申请",
                        )?;
                        require(
                            old["approval"] == "pending"
                                && old["state"] == "active"
                                && date(&old, "confirmed_until")? > Utc::now()
                                && date(&old, "expires_at")? > Utc::now(),
                            "申请已失效，请重新确认",
                        )?;
                        sqlx::query("UPDATE mobile_usages SET approval='approved',approved_by=$2,revision=revision+1 WHERE id=$1").bind(id).bind(a.id).execute(&mut *tx).await?;
                    }
                    "decline" => {
                        require(old["approval"] == "pending", "申请已处理")?;
                        sqlx::query("UPDATE mobile_usages SET approval='declined',revision=revision+1 WHERE id=$1").bind(id).execute(&mut *tx).await?;
                    }
                    "pause" | "revoke" | "resume" => {
                        require(
                            old["state"] != "revoked" && date(&old, "expires_at")? > Utc::now(),
                            "授权已失效",
                        )?;
                        let state = match operation.as_str() {
                            "pause" => "paused",
                            "revoke" => "revoked",
                            _ => "active",
                        };
                        sqlx::query(
                            "UPDATE mobile_usages SET state=$2,revision=revision+1 WHERE id=$1",
                        )
                        .bind(id)
                        .bind(state)
                        .execute(&mut *tx)
                        .await?;
                    }
                    _ => return Err(Error::Policy("请从原范围重新预览并申请续期")),
                }
                event(&mut tx, id, a.id, &operation, &json!(mutation)).await?;
                reconcile(&mut tx).await?;
                detail(&mut tx, Some(s), Some(id)).await?
            }
            UsageCommand::IssueUpdate {
                id,
                issue_id,
                input,
            } => {
                require(platform, "需要平台处理问题报告")?;
                crate::business::text(&input.response, 1, 2000)?;
                require(
                    matches!(
                        input.state.as_str(),
                        "investigating" | "resolved" | "declined"
                    ),
                    "问题处理状态无效",
                )?;
                let n=sqlx::query("UPDATE mobile_usage_issues SET state=$4,response=$5,revision=revision+1 WHERE id=$1 AND usage_id=$2 AND revision=$3 AND EXISTS(SELECT 1 FROM mobile_usages WHERE id=$2 AND school_id=$6)").bind(issue_id).bind(id).bind(input.expected_revision).bind(&input.state).bind(&input.response).bind(s).execute(&mut *tx).await?.rows_affected();
                require(n == 1, "问题报告已变化，请刷新后重试")?;
                event(
                    &mut tx,
                    id,
                    a.id,
                    "issue_response",
                    &json!({"issue_id":issue_id,"response":input.response,"state":input.state}),
                )
                .await?;
                detail(&mut tx, Some(s), Some(id)).await?
            }
            UsageCommand::Issue { id, description } => {
                crate::business::text(&description, 1, 2000)?;
                let old = detail(&mut tx, Some(s), Some(id)).await?;
                require(
                    !old["items"].as_array().unwrap().is_empty(),
                    "使用记录不存在",
                )?;
                let issue = Uuid::now_v7();
                let context = readiness::school(&mut tx, s).await?;
                sqlx::query("INSERT INTO mobile_usage_issues(id,usage_id,actor_id,description,context) VALUES($1,$2,$3,$4,$5)").bind(issue).bind(id).bind(a.id).bind(description).bind(json!(context)).execute(&mut *tx).await?;
                json!({"id":issue,"state":"submitted"})
            }
            UsageCommand::TaskConfirm(v) => {
                let production:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_v9_previews WHERE id=$1 AND school_id=$2 AND plan->>'stage'='production_allowlist')").bind(v.preview_id).bind(s).fetch_one(&mut *tx).await?;
                require(production, "发送确认必须引用当前业务预览")?;
                tasks::write(&mut tx, a, s, SchoolCommand::Confirm(v)).await?
            }
            UsageCommand::TaskPreview(v) => {
                if let Some(id) = v.draft_id {
                    let old_stage: Option<String> = sqlx::query_scalar(
                        "SELECT stage FROM mobile_v9_task_drafts WHERE id=$1 AND school_id=$2",
                    )
                    .bind(id)
                    .bind(s)
                    .fetch_optional(&mut *tx)
                    .await?;
                    require(
                        old_stage.as_deref() == Some("production_allowlist"),
                        "请选择当前业务草稿",
                    )?;
                }

                reconcile(&mut tx).await?;
                let draft = tasks::write(
                    &mut tx,
                    a,
                    s,
                    SchoolCommand::Draft(
                        v.draft_id,
                        DraftInput {
                            target_ids: v.target_ids,
                            message: v.message,
                            content_type: "text/plain".into(),
                            expected_revision: v.expected_revision,
                            stage: "production_allowlist".into(),
                            between_targets_seconds: v.between_targets_seconds,
                        },
                    ),
                )
                .await?;
                let preview = tasks::write(
                    &mut tx,
                    a,
                    s,
                    SchoolCommand::Preview(PreviewInput {
                        draft_id: uid(&draft, "id")?,
                        expected_revision: draft["revision"].as_i64().ok_or(Error::Conflict)?,
                        selections: v.selections,
                    }),
                )
                .await?;
                json!({"id":preview["id"],"draft":draft,"preview":preview})
            }
        };
        remember(&mut tx, a.id, 0, &scope, key, &body, &result).await?;
        tx.commit().await?;
        Ok(result)
    }
}

/// Called under the existing shared guard: repeatable after crashes and product updates.
pub(super) async fn reconcile(tx: &mut Tx<'_>) -> Result<()> {
    let usages:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(u) FROM mobile_usages u WHERE approval='approved' AND state='active' AND expires_at>clock_timestamp() ORDER BY id").fetch_all(&mut **tx).await?;
    for u in usages {
        let id = uid(&u, "id")?;
        let s = uid(&u, "school_id")?;
        let b = &u["body"];
        let mut reason = "";
        let p = policy(tx).await?;
        if reason.is_empty()
            && p.as_ref().is_none_or(|p| {
                p["mode"] != "enabled"
                    || !p["allowed_stages"]
                        .as_array()
                        .is_some_and(|st| st.contains(&json!("production_allowlist")))
            })
        {
            reason = "等待平台启用使用方案";
        }
        if reason.is_empty()
            && (b["scheme"].is_null()
                || p.as_ref()
                    .is_some_and(|p| !same_scheme(&b["scheme"], &scheme(p))))
        {
            reason = "使用方案已提供或变化，请重新核对后申请";
        }
        if reason.is_empty() {
            let devices: Vec<Uuid> =
                serde_json::from_value(b["device_ids"].clone()).map_err(|_| Error::Conflict)?;
            let mut targets = b["targets"].as_array().cloned().unwrap_or_default();
            if b["auto_targets"] == true {
                targets=sqlx::query_scalar("SELECT jsonb_build_object('id',id,'search_text',search_text,'revision',locator_revision) FROM mobile_targets WHERE school_id=$1 AND enabled AND archived_at IS NULL ORDER BY id").bind(s).fetch_all(&mut **tx).await?;
            }
            for device in devices {
                for t in &targets {
                    let target = uid(t, "id")?;
                    let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_usage_scopes WHERE usage_id=$1 AND release_id IS NULL AND target_id=$2 AND device_id=$3)").bind(id).bind(target).bind(device).fetch_one(&mut **tx).await?;
                    if exists {
                        continue;
                    }
                    let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM execution_devices d,schools s,mobile_targets t WHERE d.id=$1 AND d.enabled AND s.id=$2 AND s.active AND t.id=$3 AND t.school_id=s.id AND t.enabled AND t.archived_at IS NULL AND t.locator_revision=$4)").bind(device).bind(s).bind(target).bind(t["revision"].as_i64().unwrap_or(1)).fetch_one(&mut **tx).await?;
                    let device_revision: i64 =
                        sqlx::query_scalar("SELECT revision FROM execution_devices WHERE id=$1")
                            .bind(device)
                            .fetch_one(&mut **tx)
                            .await?;
                    let device_ok = b["device_revisions"].as_array().is_some_and(|ds| {
                        ds.iter()
                            .any(|x| x["id"] == json!(device) && x["revision"] == device_revision)
                    });
                    if !valid || !device_ok {
                        reason = "资源已变化，请重新核对";
                        continue;
                    }
                    let old_access:Option<(bool,i64)>=sqlx::query_as("SELECT enabled,revision FROM mobile_shared_school_access WHERE device_id=$1 AND school_id=$2").bind(device).bind(s).fetch_optional(&mut **tx).await?;
                    if old_access.is_some_and(|x| !x.0) {
                        reason = "手机使用资格已停用，请平台核对";
                        continue;
                    }
                    let actor = uid(&u, "approved_by")?;
                    sqlx::query("INSERT INTO mobile_shared_school_access(device_id,school_id,enabled,revision,updated_by) VALUES($1,$2,true,1,$3) ON CONFLICT DO NOTHING").bind(device).bind(s).bind(actor).execute(&mut **tx).await?;
                    let old_binding:Option<bool>=sqlx::query_scalar("SELECT enabled FROM mobile_device_target_bindings WHERE device_id=$1 AND target_id=$2").bind(device).bind(target).fetch_optional(&mut **tx).await?;
                    let actual_search:Option<String>=sqlx::query_scalar("SELECT coalesce(search_text_override,(SELECT search_text FROM mobile_targets WHERE id=$2)) FROM mobile_device_target_bindings WHERE device_id=$1 AND target_id=$2").bind(device).bind(target).fetch_optional(&mut **tx).await?;
                    if actual_search
                        .as_ref()
                        .is_some_and(|v| json!(v) != t["search_text"])
                    {
                        reason = "手机群搜索名已变化，请重新核对";
                        continue;
                    }
                    if old_binding == Some(false) {
                        reason = "目标绑定已停用，请重新核对";
                        continue;
                    }
                    sqlx::query("INSERT INTO mobile_device_target_bindings(id,school_id,device_id,target_id,created_by,updated_by,source) VALUES($1,$2,$3,$4,$5,$5,'school_pool') ON CONFLICT DO NOTHING").bind(Uuid::now_v7()).bind(s).bind(device).bind(target).bind(actor).execute(&mut **tx).await?;
                    let revision:i64=sqlx::query_scalar("SELECT revision FROM mobile_shared_school_access WHERE device_id=$1 AND school_id=$2").bind(device).bind(s).fetch_one(&mut **tx).await?;
                    let scope = Uuid::now_v7();
                    sqlx::query("INSERT INTO mobile_v9_scope_grants(id,school_id,release_id,acceptance_id,stage,device_ids,target_ids,auto_targets,max_calls,expires_at,reason,created_by,resource_revisions) VALUES($1,$2,$3,$4,'production_allowlist',ARRAY[$5]::uuid[],ARRAY[$6]::uuid[],false,$7,$8,'使用授权自动关联',$9,$10)").bind(scope).bind(s).bind(None::<Uuid>).bind(None::<Uuid>).bind(device).bind(target).bind(b["max_calls"].as_i64().unwrap_or(1)).bind(date(&u,"expires_at")?).bind(actor).bind(json!({device.to_string():revision})).execute(&mut **tx).await?;
                    let snapshot:Value=sqlx::query_scalar("SELECT jsonb_build_object('device_revision',d.revision,'binding_locator_revision',b.locator_revision,'target_locator_revision',CASE WHEN b.search_text_override IS NULL THEN t.locator_revision ELSE NULL END,'search_text',coalesce(b.search_text_override,t.search_text)) FROM mobile_device_target_bindings b JOIN execution_devices d ON d.id=b.device_id JOIN mobile_targets t ON t.id=b.target_id WHERE b.device_id=$1 AND b.target_id=$2").bind(device).bind(target).fetch_one(&mut **tx).await?;
                    sqlx::query("INSERT INTO mobile_usage_scopes(scope_id,usage_id,release_id,target_id,device_id,resource_snapshot) VALUES($1,$2,$3,$4,$5,$6)").bind(scope).bind(id).bind(None::<Uuid>).bind(target).bind(device).bind(snapshot).execute(&mut **tx).await?;
                }
            }
        }
        sqlx::query("INSERT INTO mobile_usage_jobs(usage_id,state,reason) VALUES($1,$2,$3) ON CONFLICT(usage_id) DO UPDATE SET state=$2,reason=$3,updated_at=clock_timestamp()").bind(id).bind(if reason.is_empty(){"ready"}else{"waiting"}).bind(if reason.is_empty(){"已批准，手机就绪及目标检查通过后可执行"}else{reason}).execute(&mut **tx).await?;
    }
    Ok(())
}

impl Repository {
    pub async fn reconcile_usages(&self) -> Result<()> {
        let mut tx = self.0.begin().await?;
        guard(&mut tx).await?;
        reconcile(&mut tx).await?;
        inspections::reconcile(&mut tx, None, None).await?;
        tx.commit().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_future_groups_share_budget_but_changed_locators_require_approval() {
        let parent = json!({"device_ids":["phone"],"device_revisions":[{"id":"phone","revision":1}],"targets":[{"id":"old","name":"原群","search_text":"原群","revision":1}],"max_calls":10,"auto_targets":true,"scheme":{"interval_seconds":30}});
        let mut input = parent.clone();
        input["targets"] = json!([{"id":"new","name":"新群","search_text":"新群","revision":1}]);
        input["auto_targets"] = json!(false);
        assert!(covers(&parent, &input));
        input["targets"] = parent["targets"].clone();
        input["targets"][0]["revision"] = json!(2);
        assert!(!covers(&parent, &input));
        input["targets"] = parent["targets"].clone();
        input["max_calls"] = json!(11);
        assert!(!covers(&parent, &input));
        input["max_calls"] = json!(10);
        input["device_revisions"][0]["revision"] = json!(2);
        assert!(!covers(&parent, &input));
    }
}
