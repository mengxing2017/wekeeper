//! One durable business intent coordinates resource grants and immutable execution scopes.
use super::*;
use crate::distribution::domain::shared_mobile::usage::*;
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
                "SELECT EXISTS(SELECT 1 FROM mobile_workspaces WHERE id=$1 AND active)",
            )
            .bind(s)
            .fetch_one(&mut **tx)
            .await?,
            "移动分发工作空间已停用",
        )?;
    }
    Ok(platform)
}
fn scheme(p: &Value) -> Value {
    json!({"id":p["id"],"limits":p["limits"],"timezone":p["timezone"],"allowed_windows":p["allowed_windows"],"daily_max_calls":p["limits"]["school"]["daily_max_calls"],"interval_seconds":p["limits"]["target"]["interval_seconds"]})
}
fn same_resource_scope(a: &Value, b: &Value) -> bool {
    usage_locators::comparison_body(a) == usage_locators::comparison_body(b)
}

async fn options(tx: &mut Tx<'_>, a: &Actor, s: Option<Uuid>) -> Result<Value> {
    let platform = permission(tx, a, s, false).await?;
    let devices:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('id',d.id,'name',d.name,'revision',d.revision,'online',coalesce(cp.state IN ('online','suspect'),false)) FROM execution_devices d LEFT JOIN device_control_presence cp ON cp.id=d.id WHERE d.enabled AND ($1 OR EXISTS(SELECT 1 FROM mobile_shared_school_access x WHERE x.device_id=d.id AND x.school_id=$2 AND x.enabled)) ORDER BY d.name,d.id").bind(platform).bind(s).fetch_all(&mut **tx).await?;
    let schools:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('id',id,'name',name) FROM mobile_workspaces WHERE active AND ($1 OR id=$2) ORDER BY name,id").bind(platform).bind(s).fetch_all(&mut **tx).await?;
    let targets:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('id',id,'name',search_text,'search_text',search_text,'revision',locator_revision) FROM mobile_targets WHERE school_id=$1 AND enabled AND archived_at IS NULL ORDER BY search_text,id").bind(s).fetch_all(&mut **tx).await?;
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
                    Some(v) => {
                        if let (Some(old), Some(new)) =
                            (parent["locators"].as_array(), input["locators"].as_array())
                        {
                            new.iter()
                                .filter(|l| l["target_id"] == t["id"])
                                .all(|l| old.iter().any(|o| usage_locators::same_locator(o, l)))
                        } else {
                            v["search_text"] == t["search_text"] && v["revision"] == t["revision"]
                        }
                    }
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
    });
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
        let ok:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_usages WHERE id=$1 AND school_id=$2 AND revision=$3 AND state<>'revoked')").bind(id).bind(s).bind(v.expected_revision).fetch_one(&mut **tx).await?;
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
        if t["id"].is_null() {
            let matching: Vec<_> = o["targets"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|x| x["search_text"] == t["search_text"] && x["name"] == t["name"])
                .collect();
            require(matching.len() <= 1, "存在多个同名目标群，请选择已有群")?;
            if let Some(found) = matching.first() {
                t["id"] = found["id"].clone();
            }
        }
        if t["id"].is_string() {
            let found = o["targets"]
                .as_array()
                .and_then(|ts| ts.iter().find(|x| x["id"] == t["id"]))
                .ok_or(Error::Policy("目标群资料已变化"))?;
            t["name"] = found["name"].clone();
            t["search_text"] = found["search_text"].clone();
            t["revision"] = found["revision"].clone();
        }
    }
    usage_locators::resolve(tx, s, &mut body).await?;
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
    let mut rows:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(u)||jsonb_build_object('school_name',s.name,'delivery',CASE WHEN j.state='ready' THEN 'available' ELSE 'waiting' END,'progress',coalesce(j.reason,'等待处理'),'issues',coalesce((SELECT jsonb_agg(to_jsonb(i)-'context') FROM mobile_usage_issues i WHERE i.usage_id=u.id),'[]'::jsonb),'events',coalesce((SELECT jsonb_agg(to_jsonb(e) ORDER BY e.created_at) FROM mobile_usage_events e WHERE e.usage_id=u.id),'[]'::jsonb)) FROM mobile_usages u JOIN mobile_workspaces s ON s.id=u.school_id LEFT JOIN mobile_usage_jobs j ON j.usage_id=u.id WHERE ($1::uuid IS NULL OR u.school_id=$1) AND ($2::uuid IS NULL OR u.id=$2) ORDER BY u.created_at DESC,u.id DESC").bind(s).bind(id).fetch_all(&mut **tx).await?;
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
        row["readiness"]=json!(states.iter().filter(|v|devices.contains(&v["device_id"])&&(row["body"]["auto_targets"]==true||targets.contains(&v["target_id"]))).map(|v|json!({"device_name":v["device_name"],"target_name":v["target_name"],"state":v["state"],"ready":v["ready"],"messages":v["blockers"].as_array().unwrap().iter().chain(v["execution_conditions"].as_array().unwrap().iter()).map(|b|b["message"].clone()).collect::<Vec<_>>()})).collect::<Vec<_>>());
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
        permission(&mut tx, a, s, false).await?;
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
                if let Some(source) = v.renewal_of {
                    let branched:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_usages WHERE supersedes=$1 AND school_id=$2)").bind(source).bind(s).fetch_one(&mut *tx).await?;
                    require(!branched, "已有后续申请，请从最新记录继续操作")?;
                }
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
                    same_resource_scope(&valid_selection(&mut tx, a, s, &input).await?, &b),
                    "范围或使用方案已变化，请重新核对",
                )?;
                for t in b["targets"].as_array_mut().unwrap() {
                    if t["id"].is_null() {
                        let id = Uuid::now_v7();
                        sqlx::query("INSERT INTO mobile_targets(id,school_id,search_text,created_by,updated_by) VALUES($1,$2,$3,$4,$4)").bind(id).bind(s).bind(t["search_text"].as_str()).bind(a.id).execute(&mut *tx).await?;
                        t["id"] = json!(id);
                        t["revision"] = json!(1);
                    }
                }
                // New target IDs are now known; preserve each phone's resolved locator.
                let resolved_targets = b["targets"].as_array().unwrap().clone();
                for l in b["locators"].as_array_mut().unwrap() {
                    l["target_id"] = resolved_targets[l["target_index"].as_u64().unwrap() as usize]
                        ["id"]
                        .clone();
                }
                // Repeated openings share the same authorization and budget, for every role.
                let parents:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(u) FROM mobile_usages u WHERE school_id=$1 AND approval='approved' AND state='active' AND expires_at>=$2 ORDER BY created_at").bind(s).bind(input.expires_at).fetch_all(&mut *tx).await?;
                if input.renewal_of.is_none()
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
                    reconcile_selected(&mut tx, id).await?;
                    let r = detail(&mut tx, Some(s), Some(id)).await?;
                    remember(&mut tx, a.id, 0, &scope, key, &body, &r).await?;
                    tx.commit().await?;
                    return Ok(r);
                }
                let previous: Option<Value> = if let Some(previous) = input.renewal_of {
                    let branched:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_usages WHERE supersedes=$1 AND school_id=$2)").bind(previous).bind(s).fetch_one(&mut *tx).await?;
                    require(!branched, "已有后续申请，请从最新记录继续操作")?;
                    Some(sqlx::query_scalar("SELECT to_jsonb(u) FROM mobile_usages u WHERE id=$1 AND school_id=$2 AND revision=$3 FOR UPDATE").bind(previous).bind(s).bind(input.expected_revision).fetch_optional(&mut *tx).await?.ok_or(Error::Conflict)?)
                } else {
                    let existing: Vec<Value> = sqlx::query_scalar(
                        "SELECT body FROM mobile_usages WHERE school_id=$1 AND state<>'revoked'",
                    )
                    .bind(s)
                    .fetch_all(&mut *tx)
                    .await?;
                    require(
                        !existing
                            .iter()
                            .any(|u| usage_locators::same_bindings(u, &b)),
                        "此使用范围已存在，请从原记录续期、调整范围或重新申请",
                    )?;
                    None
                };
                if let Some(ref old) = previous
                    && old["approval"] == "approved"
                    && usage_locators::same_bindings(&old["body"], &b)
                {
                    require(
                        input.expires_at >= date(old, "expires_at")?,
                        "续期不能缩短原使用有效期",
                    )?;
                    require(
                        input.expires_at > date(old, "expires_at")?
                            || old["body"]["max_calls"] != b["max_calls"]
                            || !covers(&old["body"], &b),
                        "使用范围和额度未扩大，请延长有效期后续期，无需重复开通",
                    )?;
                }
                let id = Uuid::now_v7();
                let approval = if platform { "approved" } else { "pending" };
                sqlx::query("INSERT INTO mobile_usages(id,school_id,actor_id,approval,body,confirmed_until,expires_at,approved_by,supersedes,budget_root,state) VALUES($1,$2,$3,$4,$5,clock_timestamp()+interval '24 hours',$6,$7,$8,$9,$10)").bind(id).bind(s).bind(a.id).bind(approval).bind(&b).bind(input.expires_at).bind(platform.then_some(a.id)).bind(input.renewal_of).bind(previous.as_ref().map(|u|uid(u,"budget_root")).transpose()?.unwrap_or(id)).bind(if previous.as_ref().is_some_and(|u|u["state"]=="paused"){"paused"}else{"active"}).execute(&mut *tx).await?;
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
                reconcile_selected(&mut tx, id).await?;
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
                        let has_child:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_usages WHERE supersedes=$1 AND school_id=$2)").bind(id).bind(s).fetch_one(&mut *tx).await?;
                        require(!has_child, "请审批最新申请，当前申请已有后续版本")?;
                        let mut input = selection_input(&old["body"])?;
                        input.renewal_of = None;
                        input.expected_revision = None;
                        require(
                            same_resource_scope(
                                &valid_selection(&mut tx, a, s, &input).await?,
                                &old["body"],
                            ),
                            "申请范围或方案已变化，请重新核对后申请",
                        )?;
                        require(
                            old["approval"] == "pending"
                                && old["state"] != "revoked"
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
                            "UPDATE mobile_usages SET state=$2,revision=revision+1 WHERE budget_root=$1 AND school_id=$3 AND state<>'revoked'",
                        )
                        .bind(uid(&old,"budget_root")?)
                        .bind(state)
                        .bind(s)
                        .execute(&mut *tx)
                        .await?;
                    }
                    _ => return Err(Error::Policy("请从原范围重新预览并申请续期")),
                }
                event(&mut tx, id, a.id, &operation, &json!(mutation)).await?;
                reconcile_selected(&mut tx, id).await?;
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
                let exists: bool = sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM mobile_v9_previews WHERE id=$1 AND school_id=$2)",
                )
                .bind(v.preview_id)
                .bind(s)
                .fetch_one(&mut *tx)
                .await?;
                require(exists, "发送确认必须引用当前业务预览")?;
                tasks::write(&mut tx, a, s, SchoolCommand::Confirm(v)).await?
            }
            UsageCommand::TaskPreview(v) => {
                if let Some(id) = v.draft_id {
                    let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_v9_task_drafts WHERE id=$1 AND school_id=$2)").bind(id).bind(s).fetch_one(&mut *tx).await?;
                    require(exists, "请选择当前业务草稿")?;
                }

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

/// Serialize this desired intent only; a busy intent is retried on the next worker cycle.
async fn reconcile_selected(tx: &mut Tx<'_>, id: Uuid) -> Result<()> {
    let u:Option<Value>=sqlx::query_scalar("SELECT to_jsonb(u) FROM mobile_usages u WHERE id=$1 AND approval='approved' AND state='active' AND expires_at>clock_timestamp() AND NOT EXISTS(SELECT 1 FROM mobile_usages n WHERE n.school_id=u.school_id AND n.budget_root=u.budget_root AND n.approval='approved' AND (n.created_at,n.id)>(u.created_at,u.id)) FOR UPDATE OF u SKIP LOCKED").bind(id).fetch_optional(&mut **tx).await?;
    if let Some(u) = u {
        reconcile_usage(tx, &u).await?;
    }
    Ok(())
}
async fn reconcile_usage(tx: &mut Tx<'_>, u: &Value) -> Result<()> {
    let id = uid(u, "id")?;
    let school = uid(u, "school_id")?;
    let body = &u["body"];
    let devices: Vec<Uuid> =
        serde_json::from_value(body["device_ids"].clone()).map_err(|_| Error::Conflict)?;
    let mut targets = body["targets"].as_array().cloned().unwrap_or_default();
    if body["auto_targets"] == true {
        targets=sqlx::query_scalar("SELECT jsonb_build_object('id',id,'name',search_text,'search_text',search_text,'revision',locator_revision) FROM mobile_targets WHERE school_id=$1 AND enabled AND archived_at IS NULL ORDER BY id").bind(school).fetch_all(&mut **tx).await?;
    }
    let total = devices.len() * targets.len();
    let mut ready = 0;
    let mut failures = Vec::new();
    if total == 0 {
        failures.push("当前没有可用目标群，请调整使用范围".into());
    }
    for device in devices {
        let device_name: String =
            sqlx::query_scalar("SELECT name FROM execution_devices WHERE id=$1")
                .bind(device)
                .fetch_one(&mut **tx)
                .await?;
        for target in &targets {
            let target_id = uid(target, "id")?;
            let label = format!(
                "{} / {}",
                device_name,
                target["name"].as_str().unwrap_or("目标群")
            );
            // Business membership follows resource identity and enabled state.
            // Search locators are execution dependencies, checked against each frozen
            // task separately; they are not a reason to reopen business authorization.
            let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM execution_devices d JOIN mobile_workspaces s ON s.id=$2 JOIN mobile_targets t ON t.school_id=s.id WHERE d.id=$1 AND d.enabled AND s.active AND t.id=$3 AND t.enabled AND t.archived_at IS NULL)").bind(device).bind(school).bind(target_id).fetch_one(&mut **tx).await?;
            if !valid {
                failures.push(format!("{label}：工作空间、手机或目标群已停用"));
                continue;
            }
            let access:Option<bool>=sqlx::query_scalar("SELECT enabled FROM mobile_shared_school_access WHERE device_id=$1 AND school_id=$2").bind(device).bind(school).fetch_optional(&mut **tx).await?;
            if access == Some(false) {
                failures.push(format!("{label}：手机使用资格已停用"));
                continue;
            }
            let binding:Option<Value>=sqlx::query_scalar("SELECT to_jsonb(b)||jsonb_build_object('effective_search_text',t.search_text) FROM mobile_device_target_bindings b JOIN mobile_targets t ON t.id=b.target_id WHERE b.school_id=$1 AND b.device_id=$2 AND b.target_id=$3").bind(school).bind(device).bind(target_id).fetch_optional(&mut **tx).await?;
            if binding.as_ref().is_some_and(|b| b["enabled"] != true) {
                failures.push(format!("{label}：目标绑定已停用"));
                continue;
            }
            let snapshot:Option<Value>=sqlx::query_scalar("SELECT jsonb_build_object('binding_locator_revision',b.locator_revision,'target_locator_revision',t.locator_revision,'target',jsonb_build_object('kind','group','search_query',t.search_text,'match_policy','exact_unique','identity_constraints','[]'::jsonb)) FROM mobile_device_target_bindings b JOIN execution_devices d ON d.id=b.device_id JOIN mobile_targets t ON t.id=b.target_id WHERE b.school_id=$1 AND b.device_id=$2 AND b.target_id=$3").bind(school).bind(device).bind(target_id).fetch_optional(&mut **tx).await?;
            let existing:Option<Value>=sqlx::query_scalar("SELECT jsonb_build_object('scope_id',m.scope_id,'snapshot',m.resource_snapshot,'revoked',EXISTS(SELECT 1 FROM mobile_v9_scope_revocations r WHERE r.grant_id=m.scope_id),'active',EXISTS(SELECT 1 FROM mobile_v9_scope_grants g WHERE g.id=m.scope_id AND g.expires_at>clock_timestamp() AND NOT EXISTS(SELECT 1 FROM mobile_v9_scope_revocations r2 WHERE r2.grant_id=g.id))) FROM mobile_usage_scopes m WHERE m.usage_id=$1 AND m.release_id IS NULL AND m.target_id=$2 AND m.device_id=$3").bind(id).bind(target_id).bind(device).fetch_optional(&mut **tx).await?;
            if let Some(existing) = existing {
                // An explicit platform revocation of this projection is a real disable.
                if existing["revoked"] == true {
                    failures.push(format!("{label}：已关联使用范围已撤销，请在使用管理核对"));
                    continue;
                }
                let Some(current) = snapshot.as_ref() else {
                    failures.push(format!("{label}：目标绑定已变化，请重新预览调整范围"));
                    continue;
                };
                if current == &existing["snapshot"] && existing["active"] == true {
                    ready += 1;
                    continue;
                }
                // Business membership is valid; project the current execution locator.
                // Frozen task plans and their dependency checks remain unchanged. Heal
                // the mutable scope snapshot in place and keep the same scope_id, so the
                // budget root and cumulative metering are never reset. The grant row is
                // immutable evidence and its expiry already tracks this usage; a renewal
                // is a new usage that reconciles its own new grant under the same root.
                let scope_id = uid(&existing, "scope_id")?;
                sqlx::query(
                    "UPDATE mobile_usage_scopes SET resource_snapshot=$2 WHERE scope_id=$1",
                )
                .bind(scope_id)
                .bind(current)
                .execute(&mut **tx)
                .await?;
                ready += 1;
                continue;
            }
            let actor = uid(u, "approved_by")?;
            sqlx::query("INSERT INTO mobile_shared_school_access(device_id,school_id,enabled,revision,updated_by) VALUES($1,$2,true,1,$3) ON CONFLICT DO NOTHING").bind(device).bind(school).bind(actor).execute(&mut **tx).await?;
            sqlx::query("INSERT INTO mobile_device_target_bindings(id,school_id,device_id,target_id,created_by,updated_by,source) VALUES($1,$2,$3,$4,$5,$5,'school_pool') ON CONFLICT DO NOTHING").bind(Uuid::now_v7()).bind(school).bind(device).bind(target_id).bind(actor).execute(&mut **tx).await?;
            let revision:i64=sqlx::query_scalar("SELECT revision FROM mobile_shared_school_access WHERE device_id=$1 AND school_id=$2").bind(device).bind(school).fetch_one(&mut **tx).await?;
            let scope = Uuid::now_v7();
            sqlx::query("INSERT INTO mobile_v9_scope_grants(id,school_id,release_id,acceptance_id,stage,device_ids,target_ids,auto_targets,max_calls,expires_at,reason,created_by,resource_revisions) VALUES($1,$2,NULL,NULL,'production_allowlist',ARRAY[$3]::uuid[],ARRAY[$4]::uuid[],false,$5,$6,'使用授权自动关联',$7,$8)").bind(scope).bind(school).bind(device).bind(target_id).bind(body["max_calls"].as_i64().unwrap_or(1)).bind(date(u,"expires_at")?).bind(actor).bind(json!({device.to_string():revision})).execute(&mut **tx).await?;
            let snapshot:Value=sqlx::query_scalar("SELECT jsonb_build_object('binding_locator_revision',b.locator_revision,'target_locator_revision',t.locator_revision,'target',jsonb_build_object('kind','group','search_query',t.search_text,'match_policy','exact_unique','identity_constraints','[]'::jsonb)) FROM mobile_device_target_bindings b JOIN execution_devices d ON d.id=b.device_id JOIN mobile_targets t ON t.id=b.target_id WHERE b.device_id=$1 AND b.target_id=$2").bind(device).bind(target_id).fetch_one(&mut **tx).await?;
            sqlx::query("INSERT INTO mobile_usage_scopes(scope_id,usage_id,release_id,target_id,device_id,resource_snapshot) VALUES($1,$2,NULL,$3,$4,$5)").bind(scope).bind(id).bind(target_id).bind(device).bind(snapshot).execute(&mut **tx).await?;
            ready += 1;
        }
    }
    let reason = if failures.is_empty() {
        format!("已关联 {ready}/{total} 项，手机现场条件自动检查")
    } else {
        format!("已关联 {ready}/{total} 项；{}", failures.join("；"))
    };
    sqlx::query("INSERT INTO mobile_usage_jobs(usage_id,state,reason) VALUES($1,$2,$3) ON CONFLICT(usage_id) DO UPDATE SET state=$2,reason=$3,updated_at=clock_timestamp()").bind(id).bind(if failures.is_empty(){"ready"}else{"waiting"}).bind(reason).execute(&mut **tx).await?;
    Ok(())
}

impl Repository {
    pub async fn reconcile_usages(&self) -> Result<()> {
        // Persisted approved intents and enabled bindings ARE the durable desired queue.
        // Each intent/device commits independently; one bad row cannot roll back its neighbours.
        let ids:Vec<Uuid>=sqlx::query_scalar("SELECT u.id FROM mobile_usages u WHERE approval='approved' AND state='active' AND expires_at>clock_timestamp() AND NOT EXISTS(SELECT 1 FROM mobile_usages n WHERE n.school_id=u.school_id AND n.budget_root=u.budget_root AND n.approval='approved' AND (n.created_at,n.id)>(u.created_at,u.id)) ORDER BY u.id").fetch_all(&self.0).await?;
        for id in ids {
            let mut tx = self.0.begin().await?;
            sqlx::query("SET LOCAL statement_timeout='2s'")
                .execute(&mut *tx)
                .await?;
            match reconcile_selected(&mut tx, id).await {
                Ok(()) => tx.commit().await?,
                Err(e) => {
                    tx.rollback().await?;
                    tracing::warn!(error=%e, usage=%id, "one mobile usage remains pending reconciliation");
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn summary_depends_on_resolved_business_group_name_not_device_display_name() {
        let original = json!({"targets":[{"id":"group","name":"旧显示名","search_text":"默认群","revision":1}],"locators":[{"device_id":"phone","target_id":"group","target_index":0,"device_name":"手机","target_name":"默认群","search_text":"默认群","binding_locator_revision":2,"target_locator_revision":null}]});
        let mut renamed = original.clone();
        renamed["locators"][0]["device_name"] = json!("手机新备注");
        assert!(same_resource_scope(&original, &renamed));
        renamed["locators"][0]["search_text"] = json!("另一群");
        assert!(!same_resource_scope(&original, &renamed));
        renamed = original.clone();
        renamed["locators"][0]["binding_locator_revision"] = json!(3);
        assert!(!same_resource_scope(&original, &renamed));
    }
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
