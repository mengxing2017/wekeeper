use super::product_preview as preview;
use crate::{
    business::{Actor, Error, Page, Result, require},
    distribution::{domain::workflows::*, south::port::workflows::Store},
};
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;
#[derive(Clone)]
pub struct Repository(pub PgPool);
type Tx<'a> = Transaction<'a, Postgres>;
async fn generation(tx: &mut Tx<'_>, g: i64) -> Result<String> {
    sqlx::query_scalar(
        "SELECT corp_id FROM wecom_directory_state WHERE credential_generation=$1 FOR SHARE",
    )
    .bind(g)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(Error::Conflict)
}
async fn usage_snapshot(tx: &mut Tx<'_>, a: &Actor, s: Uuid, v: &UsageInput) -> Result<Value> {
    let corp = generation(tx, v.generation).await?;
    let member:Value=sqlx::query_scalar("SELECT to_jsonb(m) FROM wecom_members m WHERE id=$1 AND verification='verified' AND status=1 AND visibility='visible' AND checked_at>now()-interval '24 hours' FOR SHARE").bind(v.member_id).fetch_optional(&mut **tx).await?.ok_or_else(||Error::Invalid("成员目录未准备，请由平台负责人处理".into()))?;
    if !a.platform_admin {
        let own: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM accounts WHERE id=$1 AND wecom_member_id=$2 AND active)",
        )
        .bind(a.id)
        .bind(v.member_id)
        .fetch_one(&mut **tx)
        .await?;
        require(own, "学校只能办理本人已核实的官方身份")?;
        require(v.account_id == a.id, "学校只能选择本人的登录账号")?;
    }
    let account:Value=sqlx::query_scalar("SELECT jsonb_build_object('id',id,'display_name',display_name,'wecom_member_id',wecom_member_id,'binding_revision',wecom_binding_revision) FROM accounts WHERE id=$1 AND active FOR UPDATE").bind(v.account_id).fetch_optional(&mut **tx).await?.ok_or(Error::NotFound)?;
    require(
        account["wecom_member_id"].is_null() || account["wecom_member_id"] == json!(v.member_id),
        "该账号已绑定其他官方成员",
    )?;
    let collision: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM accounts WHERE wecom_member_id=$1 AND id<>$2)",
    )
    .bind(v.member_id)
    .bind(v.account_id)
    .fetch_one(&mut **tx)
    .await?;
    require(!collision, "该官方成员已关联其他账号，请由负责人核对")?;
    let role: Option<String> = sqlx::query_scalar(
        "SELECT role FROM school_memberships WHERE school_id=$1 AND account_id=$2 FOR SHARE",
    )
    .bind(s)
    .bind(v.account_id)
    .fetch_optional(&mut **tx)
    .await?;
    let qualification:Option<Value>=sqlx::query_scalar("SELECT to_jsonb(q) FROM wecom_sender_qualifications q WHERE school_id=$1 AND account_id=$2 FOR SHARE").bind(s).bind(v.account_id).fetch_optional(&mut **tx).await?;
    let mut groups = Vec::new();
    let mut ids = v.group_ids.clone();
    ids.sort();
    for id in ids {
        let g:Value=sqlx::query_scalar("SELECT to_jsonb(o) FROM wecom_asset_objects o WHERE id=$1 AND kind='groups' AND credential_generation=$2 AND verification='verified' AND visibility='visible' AND checked_at>now()-interval '24 hours' AND lower(data->>'owner' COLLATE \"C\")=$3 FOR SHARE").bind(id).bind(v.generation).bind(member["userid_key"].as_str()).fetch_optional(&mut **tx).await?.ok_or_else(||Error::Invalid("群目录或群主资格已变化，请重新选择".into()))?;
        let bound: Option<Value> = sqlx::query_scalar(
            "SELECT to_jsonb(g) FROM wecom_groups g WHERE corp_id=$1 AND chat_id=$2 FOR UPDATE",
        )
        .bind(&corp)
        .bind(g["data"]["chat_id"].as_str().or(g["key"].as_str()))
        .fetch_optional(&mut **tx)
        .await?;
        require(
            bound.as_ref().is_none_or(|b| b["school_id"] == json!(s)),
            "群当前不能关联到本校，请由负责人核对",
        )?;
        groups.push(json!({"directory":g,"binding":bound}));
    }
    Ok(
        json!({"member":member,"account":account,"role":role,"qualification":qualification,"groups":groups,"corp":corp}),
    )
}
async fn apply_usage(
    tx: &mut Tx<'_>,
    a: &Actor,
    s: Uuid,
    v: &UsageInput,
    snapshot: &Value,
) -> Result<()> {
    // Platform-only identity/role changes. A school admin may reuse existing exact relations.
    if !a.platform_admin {
        require(
            snapshot["account"]["wecom_member_id"] == json!(v.member_id)
                && snapshot["role"] == v.role
                && snapshot["qualification"]
                    .as_object()
                    .is_none_or(|q| q.get("enabled") == Some(&json!(true))),
            "需要平台审批身份或职责变化",
        )?;
    }
    if snapshot["account"]["wecom_member_id"].is_null() {
        preview::platform(tx, a).await?;
        sqlx::query("UPDATE accounts SET wecom_member_id=$2,wecom_binding_revision=wecom_binding_revision+1 WHERE id=$1 AND wecom_member_id IS NULL").bind(v.account_id).bind(v.member_id).execute(&mut **tx).await?;
    }
    let old = snapshot["role"].as_str();
    if old != Some(&v.role) {
        preview::platform(tx, a).await?;
        require(
            old != Some("admin") || v.role == "admin",
            "本次不能降低已有管理员职责",
        )?;
        sqlx::query("INSERT INTO school_memberships(school_id,account_id,role) VALUES($1,$2,$3) ON CONFLICT(school_id,account_id) DO UPDATE SET role=EXCLUDED.role").bind(s).bind(v.account_id).bind(&v.role).execute(&mut **tx).await?;
    }
    sqlx::query("INSERT INTO wecom_sender_qualifications(school_id,account_id,enabled,updated_by) VALUES($1,$2,true,$3) ON CONFLICT(school_id,account_id) DO UPDATE SET enabled=true,revision=wecom_sender_qualifications.revision+1,updated_by=EXCLUDED.updated_by,updated_at=now() WHERE NOT wecom_sender_qualifications.enabled").bind(s).bind(v.account_id).bind(a.id).execute(&mut **tx).await?;
    for g in snapshot["groups"].as_array().ok_or(Error::Conflict)? {
        let d = &g["directory"];
        let name = d["data"]["name"].as_str().unwrap_or("客户群");
        let chat = d["data"]["chat_id"]
            .as_str()
            .or(d["key"].as_str())
            .ok_or(Error::Conflict)?;
        sqlx::query("INSERT INTO wecom_groups(id,school_id,corp_id,chat_id,name,directory_group_id) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT(corp_id,chat_id) DO UPDATE SET active=true,directory_group_id=EXCLUDED.directory_group_id,revision=wecom_groups.revision+1 WHERE wecom_groups.school_id=EXCLUDED.school_id AND (NOT wecom_groups.active OR wecom_groups.directory_group_id IS DISTINCT FROM EXCLUDED.directory_group_id)").bind(Uuid::now_v7()).bind(s).bind(snapshot["corp"].as_str()).bind(chat).bind(name).bind(d["id"].as_str().and_then(|s|s.parse::<Uuid>().ok())).execute(&mut **tx).await?;
    }
    crate::audit::record(
        tx,
        a,
        s,
        "official.usage.authorized",
        v.account_id,
        json!({"member_id":v.member_id,"role":v.role,"groups":v.group_ids}),
    )
    .await?;
    Ok(())
}
async fn request(
    tx: &mut Tx<'_>,
    a: &Actor,
    s: Uuid,
    kind: &str,
    key: &str,
    payload: Value,
) -> Result<Value> {
    let id = Uuid::now_v7();
    let prior:Option<Value>=sqlx::query_scalar("SELECT to_jsonb(r) FROM product_requests r WHERE school_id=$1 AND kind=$2 AND context_key=$3 AND state IN ('pending','investigating','waiting_user') FOR UPDATE").bind(s).bind(kind).bind(key).fetch_optional(&mut **tx).await?;
    if let Some(prior) = prior {
        return Ok(sanitize_request(prior));
    }
    let row=sqlx::query_scalar("INSERT INTO product_requests(id,school_id,kind,actor_id,context_key,payload) VALUES($1,$2,$3,$4,$5,$6) RETURNING to_jsonb(product_requests)").bind(id).bind(s).bind(kind).bind(a.id).bind(key).bind(payload).fetch_one(&mut **tx).await?;
    sqlx::query("INSERT INTO product_request_events(id,request_id,actor_id,state,response) VALUES($1,$2,$3,'pending','已提交，等待负责人处理')").bind(Uuid::now_v7()).bind(id).bind(a.id).execute(&mut **tx).await?;
    Ok(row)
}
impl Store for Repository {
    async fn claim_health_probe(&self, scope: &str) -> Result<bool> {
        Ok(sqlx::query("INSERT INTO official_channel_health(credential_scope,next_probe_at) VALUES($1,clock_timestamp()+interval '60 seconds') ON CONFLICT(credential_scope) DO UPDATE SET next_probe_at=clock_timestamp()+interval '60 seconds' WHERE official_channel_health.next_probe_at<=clock_timestamp() RETURNING credential_scope")
            .bind(scope).fetch_optional(&self.0).await?.is_some())
    }
    async fn record_health(&self, v: ChannelHealth) -> Result<()> {
        let mut tx = self.0.begin().await?;
        super::broadcast::guard(&mut tx).await?;
        if v.available {
            sqlx::query("SELECT singleton FROM wecom_directory_state WHERE corp_id=$1 AND credential_generation=$2 AND credential_scope=$3 FOR SHARE")
                .bind(&v.corp).bind(v.generation).bind(&v.scope).fetch_optional(&mut *tx).await?.ok_or(Error::Conflict)?;
        }
        sqlx::query("UPDATE official_channel_health SET corp_id=$2,generation=$3,available=$4,checked_at=clock_timestamp(),last_success_at=CASE WHEN $4 THEN clock_timestamp() ELSE last_success_at END,last_failure_at=CASE WHEN NOT $4 THEN clock_timestamp() ELSE last_failure_at END,error_code=$5 WHERE credential_scope=$1")
            .bind(&v.scope).bind(v.corp).bind(v.generation).bind(v.available).bind(v.error_code).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
    async fn channel_control_read(&self, a: &Actor, scope: &str) -> Result<Value> {
        let mut tx = self.0.begin().await?;
        sqlx::query("SELECT id FROM accounts WHERE id=$1 AND active AND platform_admin FOR SHARE")
            .bind(a.id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(Error::Forbidden)?;
        let control:Option<Value>=sqlx::query_scalar("SELECT jsonb_build_object('paused',paused,'revision',revision) FROM official_channel_controls WHERE credential_scope=$1").bind(scope).fetch_optional(&mut *tx).await?;
        let health:Option<Value>=sqlx::query_scalar("SELECT jsonb_build_object('checked_at',checked_at,'last_success_at',last_success_at,'last_failure_at',last_failure_at) FROM official_channel_health WHERE credential_scope=$1").bind(scope).fetch_optional(&mut *tx).await?;
        let status = channel_status(&mut tx, scope).await?;
        Ok(
            json!({"id":"current","paused":control.as_ref().map(|c|c["paused"].clone()).unwrap_or(json!(false)),"revision":control.as_ref().map(|c|c["revision"].clone()).unwrap_or(json!(0)),"health_state":status.1,"health":health}),
        )
    }
    async fn channel_control(&self, a: &Actor, scope: &str, v: ChannelControl) -> Result<Value> {
        let mut tx = self.0.begin().await?;
        super::broadcast::guard(&mut tx).await?;
        sqlx::query("SELECT id FROM accounts WHERE id=$1 AND active AND platform_admin FOR SHARE")
            .bind(a.id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(Error::Forbidden)?;
        let old: Option<i64> = sqlx::query_scalar(
            "SELECT revision FROM official_channel_controls WHERE credential_scope=$1",
        )
        .bind(scope)
        .fetch_optional(&mut *tx)
        .await?;
        require(
            old.unwrap_or(0) == v.expected_revision,
            "修订已变化，请刷新后重试",
        )?;
        let revision = v.expected_revision + 1;
        sqlx::query("INSERT INTO official_channel_controls(credential_scope,paused,revision,reason,updated_by) VALUES($1,$2,$3,$4,$5) ON CONFLICT(credential_scope) DO UPDATE SET paused=$2,revision=$3,reason=$4,updated_by=$5,updated_at=clock_timestamp()")
            .bind(scope).bind(v.paused).bind(revision).bind(&v.reason).bind(a.id).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO audit_events(id,school_id,actor_id,action,object_id,detail,request_id) VALUES($1,NULL,$2,'official.channel.control',$3,$4,'api')").bind(Uuid::now_v7()).bind(a.id).bind(a.id).bind(json!({"paused":v.paused,"reason":v.reason,"revision":revision})).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(json!({"id":"current","paused":v.paused,"revision":revision}))
    }

    async fn command(&self, a: &Actor, s: Option<Uuid>, command: Command) -> Result<Value> {
        let mut tx = self.0.begin().await?;
        super::broadcast::guard(&mut tx).await?;
        preview::authorize(&mut tx, a, s, "operator").await?;
        let result = match command {
            Command::TemplatePreview(v) => {
                let s = s.ok_or(Error::Forbidden)?;
                if let Some(id) = v.id {
                    sqlx::query("SELECT id FROM templates WHERE id=$1 AND school_id=$2 AND revision=$3 FOR SHARE").bind(id).bind(s).bind(v.revision).fetch_optional(&mut *tx).await?.ok_or(Error::Conflict)?;
                }
                let name: String = sqlx::query_scalar("SELECT name FROM schools WHERE id=$1")
                    .bind(s)
                    .fetch_one(&mut *tx)
                    .await?;
                let sample: Option<Value> = if let Some(id) = v.sample_post_id {
                    sqlx::query_scalar("SELECT jsonb_build_object('title',title,'summary',left(content,200),'id',id,'revision',revision) FROM distribution_forum_materials WHERE school_id=$1 AND id=$2 AND audit_status='approved' AND visibility='visible'").bind(s).bind(id).fetch_optional(&mut *tx).await?
                } else {
                    None
                };
                require(
                    v.sample_post_id.is_none() || sample.is_some(),
                    "预览内容不可用",
                )?;
                let sample_values = sample.clone().unwrap_or(
                    json!({"title":"示例通知","summary":"此处为示例内容","id":Uuid::nil()}),
                );
                let rendered = crate::distribution::domain::render(
                    &v.body,
                    &std::collections::BTreeMap::from([
                        ("school.name", name),
                        (
                            "post.title",
                            sample_values["title"].as_str().unwrap_or("").into(),
                        ),
                        (
                            "post.summary",
                            sample_values["summary"].as_str().unwrap_or("").into(),
                        ),
                        (
                            "post.detail_url",
                            format!(
                                "/community/{s}/posts/{}",
                                sample_values["id"].as_str().unwrap_or("")
                            ),
                        ),
                    ]),
                )?;
                let mut out = preview::create(
                    &mut tx,
                    a,
                    Some(s),
                    "template",
                    json!(v),
                    json!({"sample":sample}),
                )
                .await?;
                out["rendered"] = json!(rendered);
                out["sample"] = json!(v.sample_post_id.is_none());
                out
            }
            Command::TemplateConfirm(c) => {
                let s = s.ok_or(Error::Forbidden)?;
                let (payload, deps, prior) =
                    preview::load(&mut tx, a, Some(s), "template", c.preview_id, &c.digest).await?;
                if let Some(prior) = prior {
                    return Ok(sanitize_request(prior));
                }
                let v: TemplateInput =
                    serde_json::from_value(payload).map_err(|_| Error::Conflict)?;
                if let Some(id) = v.sample_post_id {
                    sqlx::query("SELECT id FROM distribution_forum_materials WHERE school_id=$1 AND id=$2 AND revision=$3 AND audit_status='approved' AND visibility='visible'").bind(s).bind(id).bind(deps["sample"]["revision"].as_i64()).fetch_optional(&mut *tx).await?.ok_or(Error::Conflict)?;
                }
                let id = v.id.unwrap_or_else(Uuid::now_v7);
                if v.id.is_none() {
                    sqlx::query(
                        "INSERT INTO templates(id,school_id,name,draft_body) VALUES($1,$2,$3,$4)",
                    )
                    .bind(id)
                    .bind(s)
                    .bind(&v.name)
                    .bind(&v.body)
                    .execute(&mut *tx)
                    .await?;
                } else {
                    let n=sqlx::query("UPDATE templates SET name=$3,draft_body=$4,revision=revision+1 WHERE id=$1 AND school_id=$2 AND revision=$5").bind(id).bind(s).bind(&v.name).bind(&v.body).bind(v.revision).execute(&mut *tx).await?.rows_affected();
                    require(n == 1, "模板已变化，请重新核对")?;
                }
                let version = if v.publish {
                    let version = Uuid::now_v7();
                    sqlx::query("INSERT INTO template_versions(id,school_id,template_id,body,published,published_by,published_at,change_note) VALUES($1,$2,$3,$4,true,$5,now(),$6)").bind(version).bind(s).bind(id).bind(&v.body).bind(a.id).bind(&v.note).execute(&mut *tx).await?;
                    sqlx::query("UPDATE templates SET current_version_id=$2 WHERE id=$1")
                        .bind(id)
                        .bind(version)
                        .execute(&mut *tx)
                        .await?;
                    Some(version)
                } else {
                    None
                };
                crate::audit::record(
                    &mut tx,
                    a,
                    s,
                    if v.publish {
                        "template.publish"
                    } else {
                        "template.draft"
                    },
                    id,
                    json!({"version":version}),
                )
                .await?;
                let result = json!({"id":id,"published":v.publish,"version_id":version});
                preview::finish(&mut tx, c.preview_id, &result).await?;
                result
            }
            Command::PolicyPreview(v) => {
                preview::platform(&mut tx, a).await?;
                let corp = generation(&mut tx, v.generation).await?;
                let member:Value=sqlx::query_scalar("SELECT jsonb_build_object('id',id,'name',name,'userid_key',userid_key,'verification',verification,'status',status) FROM wecom_members WHERE id=$1 FOR SHARE").bind(v.member_id).fetch_optional(&mut *tx).await?.ok_or(Error::NotFound)?;
                let key = format!(
                    "wecom:{corp}:{}",
                    member["userid_key"].as_str().ok_or(Error::Conflict)?
                );
                let old:Option<Value>=sqlx::query_scalar("SELECT to_jsonb(p) FROM distribution_resource_policies p WHERE resource_key=$1 FOR SHARE").bind(&key).fetch_optional(&mut *tx).await?;
                require(
                    old.as_ref()
                        .map(|v| v["revision"].as_i64().unwrap_or(0))
                        .unwrap_or(0)
                        == v.revision,
                    "限制已变化，请刷新",
                )?;
                let current = member["verification"] == "verified" && member["status"] == 1;
                require(
                    current
                        || old.as_ref().is_some_and(|o| {
                            !v.enabled
                                || v.window_seconds
                                    >= o["window_seconds"].as_i64().unwrap_or(0) as i32
                                    && v.max_calls <= o["max_calls"].as_i64().unwrap_or(0) as i32
                        }),
                    "历史成员只能暂停或收紧现有限制",
                )?;
                preview::create(
                    &mut tx,
                    a,
                    None,
                    "member_policy",
                    json!(v),
                    json!({"member":member,"corp":corp,"old":old}),
                )
                .await?
            }
            Command::PolicyConfirm(c) => {
                preview::platform(&mut tx, a).await?;
                let (payload, deps, prior) =
                    preview::load(&mut tx, a, None, "member_policy", c.preview_id, &c.digest)
                        .await?;
                if let Some(prior) = prior {
                    return Ok(sanitize_request(prior));
                }
                let v: MemberPolicyInput =
                    serde_json::from_value(payload).map_err(|_| Error::Conflict)?;
                let corp = generation(&mut tx, v.generation).await?;
                require(deps["corp"] == corp, "企业已变化")?;
                let key = format!(
                    "wecom:{corp}:{}",
                    deps["member"]["userid_key"]
                        .as_str()
                        .ok_or(Error::Conflict)?
                );
                let member:Value=sqlx::query_scalar("SELECT jsonb_build_object('id',id,'name',name,'userid_key',userid_key,'verification',verification,'status',status) FROM wecom_members WHERE id=$1 FOR SHARE").bind(v.member_id).fetch_optional(&mut *tx).await?.ok_or(Error::Conflict)?;
                require(member == deps["member"], "成员状态已变化，请重新核对")?;
                let n = if v.revision == 0 {
                    sqlx::query("INSERT INTO distribution_resource_policies(resource_key,enabled,window_seconds,max_calls) VALUES($1,$2,$3,$4) ON CONFLICT DO NOTHING").bind(&key).bind(v.enabled).bind(v.window_seconds).bind(v.max_calls).execute(&mut *tx).await?.rows_affected()
                } else {
                    sqlx::query("UPDATE distribution_resource_policies SET enabled=$2,window_seconds=$3,max_calls=$4,revision=revision+1 WHERE resource_key=$1 AND revision=$5").bind(&key).bind(v.enabled).bind(v.window_seconds).bind(v.max_calls).bind(v.revision).execute(&mut *tx).await?.rows_affected()
                };
                require(n == 1, "限制已变化，请重新核对")?;
                sqlx::query("INSERT INTO audit_events(id,school_id,actor_id,action,object_id,detail,request_id) VALUES($1,NULL,$2,'official.member.policy',$3,$4,$5)").bind(Uuid::now_v7()).bind(a.id).bind(v.member_id).bind(json!({"enabled":v.enabled,"window_seconds":v.window_seconds,"max_calls":v.max_calls})).bind(&a.request_id).execute(&mut *tx).await?;
                let result = json!({"id":v.member_id,"updated":true});
                preview::finish(&mut tx, c.preview_id, &result).await?;
                result
            }
            Command::UsagePreview(v) => {
                let s = s.ok_or(Error::Forbidden)?;
                preview::authorize(&mut tx, a, Some(s), "admin").await?;
                let snapshot = usage_snapshot(&mut tx, a, s, &v).await?;
                let mut out = preview::create(
                    &mut tx,
                    a,
                    Some(s),
                    "official_usage",
                    json!(v),
                    snapshot.clone(),
                )
                .await?;
                out["requires_approval"] = json!(
                    !a.platform_admin
                        && (snapshot["role"] != v.role
                            || snapshot["account"]["wecom_member_id"] != json!(v.member_id)
                            || snapshot["qualification"]["enabled"] == false)
                );
                out
            }
            Command::UsageConfirm(c) => {
                let s = s.ok_or(Error::Forbidden)?;
                preview::authorize(&mut tx, a, Some(s), "admin").await?;
                let (payload, deps, prior) = preview::load(
                    &mut tx,
                    a,
                    Some(s),
                    "official_usage",
                    c.preview_id,
                    &c.digest,
                )
                .await?;
                if let Some(prior) = prior {
                    return Ok(sanitize_request(prior));
                }
                let v: UsageInput =
                    serde_json::from_value(payload.clone()).map_err(|_| Error::Conflict)?;
                let current = usage_snapshot(&mut tx, a, s, &v).await?;
                require(current == deps, "身份、职责或群目录已变化，请重新核对")?;
                let result = if a.platform_admin
                    || (deps["role"] == v.role
                        && deps["account"]["wecom_member_id"] == json!(v.member_id)
                        && deps["qualification"]["enabled"] != false)
                {
                    apply_usage(&mut tx, a, s, &v, &deps).await?;
                    json!({"id":c.preview_id,"state":"approved"})
                } else {
                    request(
                        &mut tx,
                        a,
                        s,
                        "official_usage",
                        &crate::business::hash(&json!({"input":payload,"dependencies":deps}).to_string()),
                        json!({"input":v,"snapshot":deps,"summary":{"member_name":deps["member"]["name"],"account_name":deps["account"]["display_name"],"role":v.role,"group_names":deps["groups"].as_array().map(|gs|gs.iter().map(|g|json!(g["directory"]["data"]["name"].as_str().or(g["binding"]["name"].as_str()).unwrap_or("客户群"))).collect::<Vec<_>>())}}),
                    )
                    .await?
                };
                preview::finish(&mut tx, c.preview_id, &result).await?;
                result
            }
            Command::Request { kind, input } => {
                let s = s.ok_or(Error::Forbidden)?;
                let mut request_payload = json!(input);
                let context = if let Some(task) = input.task_id {
                    let task_info:Value=sqlx::query_scalar(
                        "SELECT jsonb_build_object('content',left(content,500),'created_at',created_at) FROM broadcast_tasks WHERE school_id=$1 AND id=$2 FOR SHARE",
                    )
                    .bind(s)
                    .bind(task)
                    .fetch_optional(&mut *tx)
                    .await?
                    .ok_or(Error::NotFound)?;
                    if let Some(batch) = input.batch_id {
                        sqlx::query("SELECT id FROM broadcast_batches WHERE school_id=$1 AND task_id=$2 AND id=$3").bind(s).bind(task).bind(batch).fetch_optional(&mut *tx).await?.ok_or(Error::NotFound)?;
                    }
                    let groups:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('name',group_name,'sender_name',a.display_name) FROM broadcast_targets x JOIN broadcast_batches b ON b.id=x.batch_id LEFT JOIN accounts a ON a.id=b.sender_account WHERE x.school_id=$1 AND x.task_id=$2 AND ($3::uuid IS NULL OR x.batch_id=$3) ORDER BY x.group_name,x.id").bind(s).bind(task).bind(input.batch_id).fetch_all(&mut *tx).await?;
                    request_payload["summary"] = json!({"content":task_info["content"],"created_at":task_info["created_at"],"target_count":groups.len(),"group_names":groups.iter().map(|g|g["name"].clone()).collect::<Vec<_>>()});
                    format!(
                        "{task}:{}",
                        input.batch_id.map(|v| v.to_string()).unwrap_or_default()
                    )
                } else {
                    let generation: Option<i64> = sqlx::query_scalar(
                        "SELECT credential_generation FROM wecom_directory_state WHERE singleton",
                    )
                    .fetch_optional(&mut *tx)
                    .await?;
                    format!("channel:{}", generation.unwrap_or(0))
                };
                request(&mut tx, a, s, &kind, &context, request_payload).await?
            }
            Command::Decision { id, input } => {
                preview::platform(&mut tx, a).await?;
                let input_hash = crate::business::hash(&json!(input).to_string());
                if let Some(prior)=sqlx::query("SELECT actor_id,input_hash,result FROM product_decision_replays WHERE request_id=$1 AND previous_revision=$2").bind(id).bind(input.revision).fetch_optional(&mut *tx).await?{require(prior.get::<Uuid,_>("actor_id")==a.id&&prior.get::<String,_>("input_hash")==input_hash,"该修订已由另一处理决定消费")?;return Ok(sanitize_request(prior.get("result")));}
                let row = sqlx::query(
                    "SELECT * FROM product_requests WHERE id=$1 AND revision=$2 FOR UPDATE",
                )
                .bind(id)
                .bind(input.revision)
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(Error::Conflict)?;
                let school: Uuid = row.get("school_id");
                preview::authorize(&mut tx, a, Some(school), "admin").await?;
                let kind: String = row.get("kind");
                let state: String = row.get("state");
                require(
                    matches!(state.as_str(), "pending" | "investigating" | "waiting_user")
                        || input.state == "revoked",
                    "此请求已处理",
                )?;
                if let Some(owner) = input.owner_id {
                    sqlx::query("SELECT id FROM accounts WHERE id=$1 AND active AND platform_admin FOR SHARE").bind(owner).fetch_optional(&mut *tx).await?.ok_or(Error::Forbidden)?;
                }
                if input.state == "approved" && kind == "official_usage" {
                    let payload: Value = row.get("payload");
                    let v: UsageInput = serde_json::from_value(payload["input"].clone())
                        .map_err(|_| Error::Conflict)?;
                    let current = usage_snapshot(&mut tx, a, school, &v).await?;
                    require(
                        current == payload["snapshot"],
                        "申请范围已变化，请申请人重新核对",
                    )?;
                    apply_usage(&mut tx, a, school, &v, &current).await?;
                }
                require(
                    kind != "official_usage" || input.state != "revoked",
                    "请在成员学校资格中明确核对影响并停用，申请状态不能代替撤权",
                )?;
                require(
                    kind != "official_usage" || input.state != "resolved",
                    "使用申请请明确批准或退回",
                )?;
                let result=sqlx::query_scalar("UPDATE product_requests SET state=$2,response=$3,owner_id=$4,revision=revision+1,updated_at=now() WHERE id=$1 RETURNING to_jsonb(product_requests)").bind(id).bind(&input.state).bind(&input.response).bind(input.owner_id.or(Some(a.id))).fetch_one(&mut *tx).await?;
                sqlx::query("INSERT INTO product_request_events(id,request_id,actor_id,state,response) VALUES($1,$2,$3,$4,$5)").bind(Uuid::now_v7()).bind(id).bind(a.id).bind(input.state).bind(input.response).execute(&mut *tx).await?;
                sqlx::query("INSERT INTO product_decision_replays(request_id,previous_revision,actor_id,input_hash,result) VALUES($1,$2,$3,$4,$5)").bind(id).bind(input.revision).bind(a.id).bind(input_hash).bind(&result).execute(&mut *tx).await?;
                result
            }
        };
        tx.commit().await?;
        Ok(sanitize_request(result))
    }
    async fn requests(&self, a: &Actor, s: Option<Uuid>, page: Page) -> Result<Vec<Value>> {
        let mut tx = self.0.begin().await?;
        preview::authorize(&mut tx, a, s, "operator").await?;
        let (limit, offset) = page.fetch_bounds()?;
        Ok(sqlx::query_scalar("SELECT (to_jsonb(r)-'payload')||jsonb_build_object('payload',jsonb_build_object('reason',r.payload->'reason','input',r.payload->'input','summary',r.payload->'summary'))||jsonb_build_object('school_name',s.name,'owner_name',a.display_name,'events',coalesce((SELECT jsonb_agg(to_jsonb(e) ORDER BY created_at,id) FROM product_request_events e WHERE e.request_id=r.id),'[]'::jsonb)) FROM product_requests r JOIN schools s ON s.id=r.school_id LEFT JOIN accounts a ON a.id=r.owner_id WHERE ($1::uuid IS NULL OR r.school_id=$1) ORDER BY r.updated_at DESC,r.id DESC LIMIT $2 OFFSET $3").bind(s).bind(limit).bind(offset).fetch_all(&mut *tx).await?)
    }
    async fn options(&self, a: &Actor, s: Option<Uuid>) -> Result<Value> {
        let mut tx = self.0.begin().await?;
        preview::authorize(&mut tx, a, s, "admin").await?;
        let members:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('id',m.id,'name',m.name,'verified',m.verification='verified' AND m.status=1,'bound_account_id',a.id,'account_name',a.display_name,'policy',CASE WHEN p.resource_key IS NOT NULL THEN to_jsonb(p)-'resource_key' ELSE NULL END,'used',CASE WHEN p.resource_key IS NULL THEN NULL ELSE (SELECT count(*) FROM distribution_resource_usage u WHERE u.resource_key=p.resource_key AND u.called_at>now()-make_interval(secs=>p.window_seconds)) END,'restores_at',(SELECT min(u.called_at)+make_interval(secs=>p.window_seconds) FROM distribution_resource_usage u WHERE u.resource_key=p.resource_key AND u.called_at>now()-make_interval(secs=>p.window_seconds))) FROM wecom_members m CROSS JOIN wecom_directory_state d LEFT JOIN accounts a ON a.wecom_member_id=m.id LEFT JOIN distribution_resource_policies p ON p.resource_key='wecom:'||d.corp_id||':'||m.userid_key WHERE ($1 OR a.id=$2) ORDER BY m.name,m.id").bind(a.platform_admin).bind(a.id).fetch_all(&mut *tx).await?;
        let accounts:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('id',a.id,'name',a.display_name,'member_id',a.wecom_member_id,'role',m.role) FROM accounts a LEFT JOIN school_memberships m ON m.account_id=a.id AND m.school_id=$1 WHERE a.active AND ($2 OR a.id=$3) ORDER BY a.display_name,a.id").bind(s).bind(a.platform_admin).bind(a.id).fetch_all(&mut *tx).await?;
        let groups:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('id',o.id,'name',o.data->>'name','owner_member_id',m.id,'checked_at',o.checked_at,'school_id',g.school_id) FROM wecom_asset_objects o JOIN wecom_directory_state d ON d.credential_generation=o.credential_generation JOIN wecom_members m ON m.userid_key=lower(o.data->>'owner' COLLATE \"C\") LEFT JOIN accounts a ON a.wecom_member_id=m.id LEFT JOIN wecom_groups g ON g.corp_id=d.corp_id AND g.chat_id=coalesce(o.data->>'chat_id',o.key) WHERE o.kind='groups' AND o.verification='verified' AND o.visibility='visible' AND o.checked_at>now()-interval '24 hours' AND ($1 OR a.id=$2) AND (g.school_id IS NULL OR g.school_id=$3) ORDER BY o.data->>'name',o.id").bind(a.platform_admin).bind(a.id).bind(s).fetch_all(&mut *tx).await?;
        let generation: Option<i64> = sqlx::query_scalar(
            "SELECT credential_generation FROM wecom_directory_state WHERE singleton",
        )
        .fetch_optional(&mut *tx)
        .await?;
        let owners: Vec<Value> = if a.platform_admin {
            sqlx::query_scalar("SELECT jsonb_build_object('id',id,'name',display_name) FROM accounts WHERE active AND platform_admin ORDER BY display_name,id").fetch_all(&mut *tx).await?
        } else {
            Vec::new()
        };
        Ok(
            json!({"id":"current","generation":generation,"owners":owners,"members":members,"accounts":accounts,"groups":groups,"reason":if generation.is_none(){Some("目录尚未准备，等待平台负责人完成接入")}else{None}}),
        )
    }
    async fn channel(
        &self,
        a: &Actor,
        s: Uuid,
        configured: bool,
        enabled: bool,
        scope: &str,
    ) -> Result<Value> {
        let mut tx = self.0.begin().await?;
        super::broadcast::authorize(&mut tx, a.id, s, false).await?;
        let health = channel_status(&mut tx, scope).await?;
        let available = configured && enabled && health.0;
        let pending:Option<Value>=sqlx::query_scalar("SELECT to_jsonb(r)||jsonb_build_object('owner_name',a.display_name) FROM product_requests r LEFT JOIN accounts a ON a.id=r.owner_id WHERE r.school_id=$1 AND kind='channel' ORDER BY r.created_at DESC,r.id DESC LIMIT 1").bind(s).fetch_optional(&mut *tx).await?;
        let control:Option<Value>=sqlx::query_scalar("SELECT jsonb_build_object('paused',paused,'revision',revision) FROM official_channel_controls WHERE credential_scope=$1").bind(scope).fetch_optional(&mut *tx).await?;
        Ok(
            json!({"id":"current","broadcast_available":available,"management_available":configured,
            "reason":if !configured {Some("官方渠道服务端凭据未配置，可保存草稿")} else if !enabled || health.1=="paused" {Some("官方渠道已明确暂停新发送")} else if !available {Some("官方接口暂不可用，系统将自动重新检查；草稿保留")} else {None},
            "reason_code":if available {"available"} else if !configured {"credentials_missing"} else if !enabled || health.1=="paused" {"channel_paused"} else {"channel_unavailable"},
            "health_state":health.1,"control":control,"can_edit_draft":true,"can_preview":true,"can_confirm":available,"can_refresh":configured,"can_stop":configured,"request":pending}),
        )
    }
}
pub(crate) async fn channel_status(tx: &mut Tx<'_>, scope: &str) -> Result<(bool, &'static str)> {
    let paused:bool=sqlx::query_scalar("SELECT coalesce((SELECT paused FROM official_channel_controls WHERE credential_scope=$1),false)").bind(scope).fetch_one(&mut **tx).await?;
    if paused {
        return Ok((false, "paused"));
    }
    let ready:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM official_channel_health h JOIN wecom_directory_state d ON d.corp_id=h.corp_id AND d.credential_generation=h.generation AND d.credential_scope=h.credential_scope WHERE h.credential_scope=$1 AND h.available AND h.checked_at>clock_timestamp()-interval '180 seconds')").bind(scope).fetch_one(&mut **tx).await?;
    Ok((ready, if ready { "available" } else { "checking" }))
}

fn sanitize_request(mut v: Value) -> Value {
    if let Some(payload) = v.get_mut("payload").and_then(Value::as_object_mut) {
        payload.remove("snapshot");
    }
    v
}
