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
async fn usage_snapshot(tx: &mut Tx<'_>, _a: &Actor, v: &UsageInput) -> Result<Value> {
    let corp = generation(tx, v.generation).await?;
    let (member, account) = crate::integrations::south::adapter::binding::snapshot(
        tx,
        v.account_id,
        v.member_id,
        v.generation,
    )
    .await?;
    let qualification: Option<Value> = sqlx::query_scalar(
        "SELECT to_jsonb(q) FROM wecom_sender_qualifications q WHERE account_id=$1 FOR SHARE",
    )
    .bind(v.account_id)
    .fetch_optional(&mut **tx)
    .await?;
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

        groups.push(json!({"directory":g,"binding":bound}));
    }
    Ok(
        json!({"member":member,"account":account,"qualification":qualification,"groups":groups,"corp":corp}),
    )
}
async fn apply_usage(tx: &mut Tx<'_>, a: &Actor, v: &UsageInput, snapshot: &Value) -> Result<()> {
    crate::integrations::south::adapter::binding::write(
        tx,
        a.id,
        v.account_id,
        Some(v.member_id),
        snapshot["account"]["binding_revision"]
            .as_i64()
            .ok_or(Error::Conflict)?,
        Some(v.generation),
    )
    .await?;
    sqlx::query("INSERT INTO wecom_sender_qualifications(account_id,enabled,updated_by) VALUES($1,true,$2) ON CONFLICT(account_id) DO UPDATE SET enabled=true,revision=wecom_sender_qualifications.revision+1,updated_by=EXCLUDED.updated_by,updated_at=now() WHERE NOT wecom_sender_qualifications.enabled").bind(v.account_id).bind(a.id).execute(&mut **tx).await?;
    for g in snapshot["groups"].as_array().ok_or(Error::Conflict)? {
        let d = &g["directory"];
        let name = d["data"]["name"].as_str().unwrap_or("客户群");
        let chat = d["data"]["chat_id"]
            .as_str()
            .or(d["key"].as_str())
            .ok_or(Error::Conflict)?;
        sqlx::query("INSERT INTO wecom_groups(id,corp_id,chat_id,name,directory_group_id) VALUES($1,$2,$3,$4,$5) ON CONFLICT(corp_id,chat_id) DO UPDATE SET active=true,directory_group_id=EXCLUDED.directory_group_id,revision=wecom_groups.revision+1 WHERE (NOT wecom_groups.active OR wecom_groups.directory_group_id IS DISTINCT FROM EXCLUDED.directory_group_id)").bind(Uuid::now_v7()).bind(snapshot["corp"].as_str()).bind(chat).bind(name).bind(d["id"].as_str().and_then(|s|s.parse::<Uuid>().ok())).execute(&mut **tx).await?;
    }
    crate::audit::record(
        tx,
        a,
        "official.usage.authorized",
        v.account_id,
        json!({"member_id":v.member_id,"groups":v.group_ids}),
    )
    .await?;
    Ok(())
}
async fn request(
    tx: &mut Tx<'_>,
    a: &Actor,
    kind: &str,
    key: &str,
    payload: Value,
) -> Result<Value> {
    let id = Uuid::now_v7();
    let prior:Option<Value>=sqlx::query_scalar("SELECT to_jsonb(r) FROM product_requests r WHERE kind=$1 AND context_key=$2 AND state IN ('pending','investigating','waiting_user') FOR UPDATE").bind(kind).bind(key).fetch_optional(&mut **tx).await?;
    if let Some(prior) = prior {
        return Ok(sanitize_request(prior));
    }
    let row=sqlx::query_scalar("INSERT INTO product_requests(id,kind,actor_id,context_key,payload) VALUES($1,$2,$3,$4,$5) RETURNING to_jsonb(product_requests)").bind(id).bind(kind).bind(a.id).bind(key).bind(payload).fetch_one(&mut **tx).await?;
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
        sqlx::query("INSERT INTO audit_events(id,actor_id,action,object_id,detail,request_id) VALUES($1,$2,'official.channel.control',$3,$4,'api')").bind(Uuid::now_v7()).bind(a.id).bind(a.id).bind(json!({"paused":v.paused,"reason":v.reason,"revision":revision})).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(json!({"id":"current","paused":v.paused,"revision":revision}))
    }

    async fn command(&self, a: &Actor, command: Command) -> Result<Value> {
        let mut tx = self.0.begin().await?;
        super::broadcast::guard(&mut tx).await?;
        preview::authorize(&mut tx, a, "operator").await?;
        let result = match command {
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
                    "member_policy",
                    json!(v),
                    json!({"member":member,"corp":corp,"old":old}),
                )
                .await?
            }
            Command::PolicyConfirm(c) => {
                preview::platform(&mut tx, a).await?;
                let (payload, deps, prior) =
                    preview::load(&mut tx, a, "member_policy", c.preview_id, &c.digest).await?;
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
                sqlx::query("INSERT INTO audit_events(id,actor_id,action,object_id,detail,request_id) VALUES($1,$2,'official.member.policy',$3,$4,$5)").bind(Uuid::now_v7()).bind(a.id).bind(v.member_id).bind(json!({"enabled":v.enabled,"window_seconds":v.window_seconds,"max_calls":v.max_calls})).bind(&a.request_id).execute(&mut *tx).await?;
                let result = json!({"id":v.member_id,"updated":true});
                preview::finish(&mut tx, c.preview_id, &result).await?;
                result
            }
            Command::UsagePreview(v) => {
                preview::authorize(&mut tx, a, "admin").await?;
                let snapshot = usage_snapshot(&mut tx, a, &v).await?;
                {
                    let mut summary = json!(v);
                    summary["effects"] = json!({
                        "bind_identity": snapshot["account"]["wecom_member_id"].is_null(),
                        "enable_qualification": snapshot["qualification"].is_null() || snapshot["qualification"]["enabled"] != true,
                        "restore_group_ids": snapshot["groups"].as_array().ok_or(Error::Conflict)?.iter().filter(|g| !g["binding"].is_null() && g["binding"]["active"] == false).map(|g| g["directory"]["id"].clone()).collect::<Vec<_>>()
                    });
                    let mut result =
                        preview::create(&mut tx, a, "official_usage", json!(v), snapshot).await?;
                    result["summary"] = summary;
                    result
                }
            }
            Command::UsageConfirm(c) => {
                preview::authorize(&mut tx, a, "admin").await?;
                let (payload, deps, prior) =
                    preview::load(&mut tx, a, "official_usage", c.preview_id, &c.digest).await?;
                if let Some(prior) = prior {
                    return Ok(sanitize_request(prior));
                }
                let v: UsageInput =
                    serde_json::from_value(payload.clone()).map_err(|_| Error::Conflict)?;
                let current = usage_snapshot(&mut tx, a, &v).await?;
                require(current == deps, "身份、职责或群目录已变化，请重新核对")?;
                apply_usage(&mut tx, a, &v, &deps).await?;
                let result = json!({"id":c.preview_id,"state":"approved"});
                preview::finish(&mut tx, c.preview_id, &result).await?;
                result
            }
            Command::Request { kind, input } => {
                let mut request_payload = json!(input);
                let context = if let Some(task) = input.task_id {
                    let task_info:Value=sqlx::query_scalar(
                        "SELECT jsonb_build_object('content',left(content,500),'created_at',created_at) FROM broadcast_tasks WHERE id=$1 FOR SHARE",
                    )
                    .bind(task)
                    .fetch_optional(&mut *tx)
                    .await?
                    .ok_or(Error::NotFound)?;
                    if let Some(batch) = input.batch_id {
                        sqlx::query("SELECT id FROM broadcast_batches WHERE task_id=$1 AND id=$2")
                            .bind(task)
                            .bind(batch)
                            .fetch_optional(&mut *tx)
                            .await?
                            .ok_or(Error::NotFound)?;
                    }
                    let groups:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('name',group_name,'sender_name',a.display_name) FROM broadcast_targets x JOIN broadcast_batches b ON b.id=x.batch_id LEFT JOIN accounts a ON a.id=b.sender_account WHERE x.task_id=$1 AND ($2::uuid IS NULL OR x.batch_id=$2) ORDER BY x.group_name,x.id").bind(task).bind(input.batch_id).fetch_all(&mut *tx).await?;
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
                request(&mut tx, a, &kind, &context, request_payload).await?
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

                preview::authorize(&mut tx, a, "admin").await?;
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
                    let current = usage_snapshot(&mut tx, a, &v).await?;
                    require(
                        current == payload["snapshot"],
                        "申请范围已变化，请申请人重新核对",
                    )?;
                    apply_usage(&mut tx, a, &v, &current).await?;
                }
                require(
                    kind != "official_usage" || input.state != "revoked",
                    "请在成员分发工作空间资格中明确核对影响并停用，申请状态不能代替撤权",
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
    async fn requests(&self, a: &Actor, page: Page) -> Result<Vec<Value>> {
        let mut tx = self.0.begin().await?;
        preview::authorize(&mut tx, a, "operator").await?;
        let (limit, offset) = page.fetch_bounds()?;
        Ok(sqlx::query_scalar("SELECT (to_jsonb(r)-'payload')||jsonb_build_object('payload',jsonb_build_object('reason',r.payload->'reason','input',r.payload->'input','summary',r.payload->'summary'))||jsonb_build_object('owner_name',a.display_name,'events',coalesce((SELECT jsonb_agg(to_jsonb(e) ORDER BY created_at,id) FROM product_request_events e WHERE e.request_id=r.id),'[]'::jsonb)) FROM product_requests r  LEFT JOIN accounts a ON a.id=r.owner_id  ORDER BY r.updated_at DESC,r.id DESC LIMIT $1 OFFSET $2").bind(limit).bind(offset).fetch_all(&mut *tx).await?)
    }
    async fn options(&self, a: &Actor) -> Result<Value> {
        let mut tx = self.0.begin().await?;
        preview::authorize(&mut tx, a, "admin").await?;
        let members:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('id',m.id,'name',m.name,'verified',m.verification='verified' AND m.status=1,'bound_account_id',a.id,'account_name',a.display_name,'policy',CASE WHEN p.resource_key IS NOT NULL THEN to_jsonb(p)-'resource_key' ELSE NULL END,'used',CASE WHEN p.resource_key IS NULL THEN NULL ELSE (SELECT count(*) FROM distribution_resource_usage u WHERE u.resource_key=p.resource_key AND u.called_at>now()-make_interval(secs=>p.window_seconds)) END,'restores_at',(SELECT min(u.called_at)+make_interval(secs=>p.window_seconds) FROM distribution_resource_usage u WHERE u.resource_key=p.resource_key AND u.called_at>now()-make_interval(secs=>p.window_seconds))) FROM wecom_members m CROSS JOIN wecom_directory_state d LEFT JOIN accounts a ON a.wecom_member_id=m.id LEFT JOIN distribution_resource_policies p ON p.resource_key='wecom:'||d.corp_id||':'||m.userid_key WHERE ($1 OR a.id=$2) ORDER BY m.name,m.id").bind(a.platform_admin).bind(a.id).fetch_all(&mut *tx).await?;
        let accounts:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('id',a.id,'name',a.display_name,'member_id',a.wecom_member_id) FROM accounts a  WHERE a.active AND ($1 OR a.id=$2) ORDER BY a.display_name,a.id").bind(a.platform_admin).bind(a.id).fetch_all(&mut *tx).await?;
        let groups:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('id',o.id,'name',o.data->>'name','owner_member_id',m.id,'checked_at',o.checked_at) FROM wecom_asset_objects o JOIN wecom_directory_state d ON d.credential_generation=o.credential_generation JOIN wecom_members m ON m.userid_key=lower(o.data->>'owner' COLLATE \"C\") LEFT JOIN accounts a ON a.wecom_member_id=m.id WHERE o.kind='groups' AND o.verification='verified' AND o.visibility='visible' AND o.checked_at>now()-interval '24 hours' AND ($1 OR a.id=$2)  ORDER BY o.data->>'name',o.id").bind(a.platform_admin).bind(a.id).fetch_all(&mut *tx).await?;
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
    async fn channel(&self, a: &Actor, configured: bool, scope: &str) -> Result<Value> {
        let mut tx = self.0.begin().await?;
        super::broadcast::authorize(&mut tx, a.id, false).await?;
        let health = channel_status(&mut tx, scope).await?;
        let available = configured && health.0;
        let pending:Option<Value>=sqlx::query_scalar("SELECT to_jsonb(r)||jsonb_build_object('owner_name',a.display_name) FROM product_requests r LEFT JOIN accounts a ON a.id=r.owner_id WHERE kind='channel' ORDER BY r.created_at DESC,r.id DESC LIMIT 1").fetch_optional(&mut *tx).await?;
        let control:Option<Value>=sqlx::query_scalar("SELECT jsonb_build_object('paused',paused,'revision',revision) FROM official_channel_controls WHERE credential_scope=$1").bind(scope).fetch_optional(&mut *tx).await?;
        Ok(
            json!({"id":"current","broadcast_available":available,"management_available":configured,
            "reason":if !configured {Some("官方渠道服务端凭据未配置，可保存草稿")} else if health.1=="paused" {Some("官方渠道已明确暂停新发送")} else if !available {Some("官方接口暂不可用，系统将自动重新检查；草稿保留")} else {None},
            "reason_code":if available {"available"} else if !configured {"credentials_missing"} else if health.1=="paused" {"channel_paused"} else {"channel_unavailable"},
            "health_state":health.1,"control":control,"request":pending}),
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
