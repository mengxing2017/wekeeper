//! A single transaction advisory guard deliberately serializes delivery writes in v1.
//! It is shared with official/legacy repositories; never hold it across a network call.
use crate::{
    business::{Error, Page, Result, hash, require, text},
    distribution::{domain::mobile_delivery::*, south::port::mobile_delivery::Store},
};
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;
#[derive(Clone)]
pub struct Repository(pub PgPool);
type Tx<'a> = Transaction<'a, Postgres>;
pub async fn guard(tx: &mut Tx<'_>) -> Result<()> {
    sqlx::query("SELECT pg_advisory_xact_lock(78234610241)")
        .execute(&mut **tx)
        .await?;
    Ok(())
}
async fn school_auth(tx: &mut Tx<'_>, actor: Uuid, school: Uuid, admin: bool) -> Result<()> {
    sqlx::query("SELECT a.id FROM accounts a JOIN school_memberships m ON m.account_id=a.id JOIN schools s ON s.id=m.school_id WHERE a.id=$1 AND m.school_id=$2 AND a.active AND s.active AND (m.role='admin' OR (NOT $3 AND m.role='operator')) FOR SHARE OF a,m,s").bind(actor).bind(school).bind(admin).fetch_optional(&mut **tx).await?.ok_or(Error::Forbidden)?;
    Ok(())
}
async fn platform(tx: &mut Tx<'_>, p: &Principal) -> Result<Uuid> {
    let id = p.actor()?;
    sqlx::query("SELECT id FROM accounts WHERE id=$1 AND active AND platform_admin FOR SHARE")
        .bind(id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(Error::Forbidden)?;
    Ok(id)
}
fn id(scope: &Scope) -> Result<Uuid> {
    scope.id.ok_or(Error::NotFound)
}
fn school(scope: &Scope) -> Result<Uuid> {
    scope.school.ok_or(Error::NotFound)
}
async fn object(tx: &mut Tx<'_>, table: &str, id: Uuid) -> Result<Value> {
    sqlx::query_scalar::<_, Value>(&format!("SELECT to_jsonb(t) FROM {table} t WHERE id=$1"))
        .bind(id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(Error::NotFound)
}

fn revision(v: &Value, expected: i64) -> Result<()> {
    if v["revision"] == expected {
        Ok(())
    } else {
        Err(Error::Policy("revision_conflict"))
    }
}
fn uid(v: &Value, key: &str) -> Result<Uuid> {
    v[key]
        .as_str()
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or(Error::Conflict)
}
fn strv<'a>(v: &'a Value, key: &str) -> Result<&'a str> {
    v[key].as_str().ok_or(Error::Conflict)
}
async fn device(tx: &mut Tx<'_>, p: &Principal) -> Result<(Uuid, i64)> {
    let Principal::Device { id, generation } = p else {
        return Err(Error::Unauthorized);
    };
    sqlx::query("SELECT d.id FROM execution_devices d WHERE d.id=$1 AND d.enabled AND d.credential_generation=$2 AND EXISTS(SELECT 1 FROM device_credentials c WHERE c.device_id=d.id AND c.generation=$2 AND c.revoked_at IS NULL AND c.expires_at>clock_timestamp()) FOR SHARE OF d").bind(id).bind(generation).fetch_optional(&mut **tx).await?.ok_or(Error::Policy("device_revoked"))?;
    Ok((*id, *generation))
}
async fn binding(tx: &mut Tx<'_>, d: Uuid) -> Result<Value> {
    sqlx::query_scalar("SELECT to_jsonb(b) FROM device_account_bindings b JOIN sending_accounts a ON a.id=b.sender_account_id WHERE b.device_id=$1 AND a.enabled").bind(d).fetch_optional(&mut **tx).await?.ok_or(Error::Policy("binding_changed"))
}
async fn event(
    tx: &mut Tx<'_>,
    p: &Principal,
    s: &Scope,
    task: Option<Uuid>,
    action: &str,
    detail: Value,
) -> Result<()> {
    let (actor, dev) = match p {
        Principal::Actor(id) => (Some(*id), None),
        Principal::Device { id, .. } => (None, Some(*id)),
        _ => (None, None),
    };
    sqlx::query("INSERT INTO mobile_delivery_events(id,school_id,task_id,actor_id,device_id,action,detail) VALUES($1,$2,$3,$4,$5,$6,$7)").bind(Uuid::now_v7()).bind(s.school).bind(task).bind(actor).bind(dev).bind(action).bind(detail).execute(&mut **tx).await?;
    Ok(())
}
async fn invalidate(tx: &mut Tx<'_>, d: Uuid) -> Result<()> {
    sqlx::query("UPDATE mobile_delivery_items i SET state=CASE WHEN c.state='permit_issued' THEN 'unknown' ELSE 'queued' END,revision=i.revision+1 FROM mobile_execution_claims c WHERE c.item_id=i.id AND c.device_id=$1 AND c.state IN ('claimed','prepared','permit_issued')").bind(d).execute(&mut **tx).await?;
    sqlx::query("UPDATE delivery_usage u SET state='unknown' FROM mobile_execution_claims c WHERE u.source_channel='mobile' AND u.source_id=c.item_id AND c.device_id=$1 AND c.state='permit_issued'").bind(d).execute(&mut **tx).await?;
    sqlx::query("UPDATE mobile_send_permits p SET state='unknown' FROM mobile_execution_claims c WHERE p.claim_id=c.id AND c.device_id=$1 AND p.state='issued'").bind(d).execute(&mut **tx).await?;
    sqlx::query("UPDATE mobile_execution_claims SET state='closed' WHERE device_id=$1 AND state IN ('claimed','prepared','permit_issued')").bind(d).execute(&mut **tx).await?;
    Ok(())
}
async fn invalidate_permit_claim(tx: &mut Tx<'_>, claim: Uuid) -> Result<()> {
    sqlx::query("UPDATE mobile_delivery_items i SET state='unknown',revision=i.revision+1 FROM mobile_execution_claims c WHERE c.item_id=i.id AND c.id=$1 AND c.state='permit_issued'").bind(claim).execute(&mut **tx).await?;
    sqlx::query("UPDATE delivery_usage u SET state='unknown' FROM mobile_execution_claims c WHERE u.source_channel='mobile' AND u.source_id=c.item_id AND c.id=$1 AND c.state='permit_issued'").bind(claim).execute(&mut **tx).await?;
    sqlx::query("UPDATE mobile_send_permits p SET state='unknown' FROM mobile_execution_claims c WHERE p.claim_id=c.id AND c.id=$1 AND c.state='permit_issued' AND p.state='issued'").bind(claim).execute(&mut **tx).await?;
    sqlx::query(
        "UPDATE mobile_execution_claims SET state='closed' WHERE id=$1 AND state='permit_issued'",
    )
    .bind(claim)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
async fn recover_tx(tx: &mut Tx<'_>) -> Result<()> {
    sqlx::query("UPDATE mobile_delivery_items i SET state='queued',revision=i.revision+1 FROM mobile_execution_claims c WHERE c.item_id=i.id AND c.state IN ('claimed','prepared') AND c.lease_until<=clock_timestamp()").execute(&mut **tx).await?;
    sqlx::query("UPDATE mobile_execution_claims SET state='expired' WHERE state IN ('claimed','prepared') AND lease_until<=clock_timestamp()").execute(&mut **tx).await?;
    sqlx::query("UPDATE mobile_delivery_items i SET state='unknown',revision=i.revision+1 FROM mobile_send_permits p WHERE p.item_id=i.id AND p.state='issued' AND p.execute_before<=clock_timestamp()").execute(&mut **tx).await?;
    sqlx::query("UPDATE delivery_usage u SET state='unknown' FROM mobile_send_permits p WHERE u.source_channel='mobile' AND u.source_id=p.item_id AND p.state='issued' AND p.execute_before<=clock_timestamp()").execute(&mut **tx).await?;
    sqlx::query("UPDATE mobile_execution_claims c SET state='closed' FROM mobile_send_permits p WHERE p.claim_id=c.id AND p.state='issued' AND p.execute_before<=clock_timestamp()").execute(&mut **tx).await?;
    sqlx::query("UPDATE mobile_send_permits SET state='unknown' WHERE state='issued' AND execute_before<=clock_timestamp()").execute(&mut **tx).await?;
    Ok(())
}
async fn scoped_object(tx: &mut Tx<'_>, s: &Scope, table: &str) -> Result<Value> {
    let v = object(tx, table, id(s)?).await?;
    require(uid(&v, "school_id")? == school(s)?, "学校范围不匹配")?;
    Ok(v)
}
async fn credential(tx: &mut Tx<'_>, d: Uuid) -> Result<Value> {
    invalidate(tx, d).await?;
    let generation:i64=sqlx::query_scalar("UPDATE execution_devices SET credential_generation=credential_generation+1,revision=revision+1 WHERE id=$1 RETURNING credential_generation").bind(d).fetch_one(&mut **tx).await?;
    sqlx::query(
        "UPDATE device_credentials SET revoked_at=clock_timestamp() WHERE device_id=$1 AND revoked_at IS NULL",
    )
    .bind(d)
    .execute(&mut **tx)
    .await?;
    let token = format!(
        "wk_device_{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    );
    let expires:chrono::DateTime<chrono::Utc>=sqlx::query_scalar("INSERT INTO device_credentials(token_hash,device_id,generation,expires_at) VALUES($1,$2,$3,clock_timestamp()+interval '30 days') RETURNING expires_at").bind(hash(&token)).bind(d).bind(generation).fetch_one(&mut **tx).await?;
    Ok(
        json!({"id":d,"device_token":token,"credential_generation":generation,"expires_at":expires,"protocol_version":PROTOCOL_VERSION}),
    )
}
async fn snapshot(tx: &mut Tx<'_>, s: Uuid, item: &ItemInput, origin: &str) -> Result<Value> {
    let g = object(tx, "delivery_groups", item.group_id).await?;
    require(
        uid(&g, "school_id")? == s && g["enabled"] == true,
        "群不可用",
    )?;
    if let Some(official) = g["wecom_group_id"].as_str() {
        let valid:bool=sqlx::query("SELECT id FROM wecom_groups WHERE id=$1 AND school_id=$2 AND active AND corp_id=$3 AND chat_id=$4 FOR SHARE").bind(official.parse::<Uuid>().map_err(|_|Error::Conflict)?).bind(s).bind(strv(&g,"identity_namespace")?).bind(strv(&g,"external_key")?).fetch_optional(&mut **tx).await?.is_some();
        require(valid, "官方群身份或状态已变化")?;
    }
    let a = object(tx, "sending_accounts", item.sender_account_id).await?;
    require(
        a["enabled"] == true && a["client_kind"] == g["client_kind"],
        "账号不可用或客户端不匹配",
    )?;
    school_auth(tx, uid(&a, "responsible_actor_id")?, s, true).await?;
    let b:Value=sqlx::query_scalar("SELECT to_jsonb(b) FROM sender_group_bindings b WHERE group_id=$1 AND sender_account_id=$2 AND school_id=$3").bind(item.group_id).bind(item.sender_account_id).bind(s).fetch_optional(&mut **tx).await?.ok_or(Error::Policy("binding_changed"))?;
    let (content, posts) = match &item.source {
        ContentSource::Text { text_content } => (normalize(text_content)?, Vec::<Value>::new()),
        ContentSource::Posts {
            template_version_id,
            post_ids,
        } => {
            let body: String = sqlx::query_scalar(
                "SELECT body FROM template_versions WHERE school_id=$1 AND id=$2 AND published FOR SHARE",
            )
            .bind(s)
            .bind(template_version_id)
            .fetch_optional(&mut **tx)
            .await?
            .ok_or(Error::Invalid("模板未发布".into()))?;
            let name: String = sqlx::query_scalar("SELECT name FROM schools WHERE id=$1")
                .bind(s)
                .fetch_one(&mut **tx)
                .await?;
            let mut rendered = Vec::new();
            let mut posts = Vec::new();
            for post in post_ids {
                let row=sqlx::query("SELECT title,content,revision FROM posts WHERE id=$1 AND school_id=$2 AND audit_status='approved' AND visibility='visible' FOR SHARE").bind(post).bind(s).fetch_optional(&mut **tx).await?.ok_or(Error::Invalid("帖子不可发送".into()))?;
                let values = std::collections::BTreeMap::from([
                    ("school.name", name.clone()),
                    ("post.title", row.get("title")),
                    (
                        "post.summary",
                        row.get::<String, _>("content").chars().take(200).collect(),
                    ),
                    (
                        "post.detail_url",
                        format!("{origin}/community/{s}/posts/{post}"),
                    ),
                ]);
                rendered.push(crate::distribution::domain::render(&body, &values)?);
                posts.push(json!({"id":post,"revision":row.get::<i64,_>("revision")}));
            }
            (normalize(&rendered.join("\n\n"))?, posts)
        }
    };
    Ok(
        json!({"group":g,"sender":a,"binding_revision":b["revision"],"content":content,"posts":posts,"source":item.source}),
    )
}
async fn validate_snapshot(tx: &mut Tx<'_>, i: &Value, origin: &str) -> Result<()> {
    let source: ContentSource =
        serde_json::from_value(i["source"].clone()).map_err(|_| Error::Conflict)?;
    let input = ItemInput {
        group_id: uid(i, "group_id")?,
        sender_account_id: uid(i, "sender_account_id")?,
        source,
    };
    let current = snapshot(tx, uid(i, "school_id")?, &input, origin).await?;
    // Policy changes are evaluated independently and must not invalidate identity snapshots.
    let mut old = i["snapshot"].clone();
    let mut new = current;
    for v in [&mut old, &mut new] {
        for k in ["group", "sender"] {
            if let Some(o) = v[k].as_object_mut() {
                o.remove("daily_limit");
                o.remove("revision");
            }
        }
    }
    if old != new {
        return Err(Error::Policy("binding_changed"));
    }
    Ok(())
}
async fn save_draft(
    tx: &mut Tx<'_>,
    p: &Principal,
    s: &Scope,
    draft: Draft,
    origin: &str,
) -> Result<Value> {
    draft.validate()?;
    let school = school(s)?;
    let actor = p.actor()?;
    school_auth(tx, actor, school, false).await?;
    let task = if let Some(task) = s.id {
        let old = scoped_object(tx, s, "mobile_delivery_tasks").await?;
        revision(
            &old,
            draft
                .expected_revision
                .ok_or(Error::Policy("revision_conflict"))?,
        )?;
        require(old["state"] == "draft", "仅可修改草稿")?;
        sqlx::query("DELETE FROM mobile_delivery_items WHERE task_id=$1")
            .bind(task)
            .execute(&mut **tx)
            .await?;
        sqlx::query(
            "UPDATE mobile_delivery_tasks SET revision=revision+1,activity_id=$2 WHERE id=$1",
        )
        .bind(task)
        .bind(&draft.activity_id)
        .execute(&mut **tx)
        .await?;
        task
    } else {
        let task = Uuid::now_v7();
        sqlx::query("INSERT INTO mobile_delivery_tasks(id,school_id,creator_id,activity_id) VALUES($1,$2,$3,$4)").bind(task).bind(school).bind(actor).bind(&draft.activity_id).execute(&mut **tx).await?;
        task
    };
    let mut contents = std::collections::BTreeSet::new();
    for item in &draft.items {
        let snap = snapshot(tx, school, item, origin).await?;
        let content = strv(&snap, "content")?;
        let digest = hash(content);
        require(
            contents.insert(digest.clone()),
            "同学校多群正文必须有实质差异；不能重复正文",
        )?;
        sqlx::query("INSERT INTO mobile_delivery_items(id,school_id,task_id,group_id,sender_account_id,content,content_digest,source,snapshot,snapshot_digest) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)").bind(Uuid::now_v7()).bind(school).bind(task).bind(item.group_id).bind(item.sender_account_id).bind(content).bind(digest).bind(json!(item.source)).bind(&snap).bind(hash(&snap.to_string())).execute(&mut **tx).await?;
    }
    object(tx, "mobile_delivery_tasks", task).await
}
async fn items(tx: &mut Tx<'_>, task: Uuid) -> Result<Vec<Value>> {
    Ok(sqlx::query_scalar(
        "SELECT to_jsonb(i) FROM mobile_delivery_items i WHERE task_id=$1 ORDER BY id",
    )
    .bind(task)
    .fetch_all(&mut **tx)
    .await?)
}
async fn settings(tx: &mut Tx<'_>, s: Uuid) -> Result<Value> {
    Ok(sqlx::query_scalar::<_,Value>("SELECT to_jsonb(t)||jsonb_build_object('id',school_id) FROM mobile_delivery_settings t WHERE school_id=$1").bind(s).fetch_optional(&mut **tx).await?.unwrap_or(json!({"id":s,"school_id":s,"enabled":false,"history_reviewed":false,"revision":0})))
}
async fn active_school(tx: &mut Tx<'_>, s: Uuid) -> Result<Value> {
    let v = settings(tx, s).await?;
    if v["enabled"] != true || v["history_reviewed"] != true {
        return Err(Error::Policy("official_delivery_pending"));
    }
    Ok(v)
}
/// Synchronous projection under the shared guard, also called by official write paths.
pub async fn project(tx: &mut Tx<'_>) -> Result<()> {
    let rows=sqlx::query("SELECT x.id,t.school_id,g.id AS group_id,t.content,b.sender_account AS actor_id,CASE WHEN x.official_status=1 THEN 'sent' WHEN (b.reconciliation='not_created' AND b.msgid IS NULL) OR (b.state='rejected' AND b.msgid IS NULL) OR (b.state='cancelled' AND b.invocation_id IS NULL) THEN 'not_sent' ELSE 'unknown' END AS state,CASE WHEN x.send_time>0 THEN to_timestamp(x.send_time) ELSE NULL END AS sent_at FROM broadcast_targets x JOIN broadcast_tasks t ON t.id=x.task_id JOIN broadcast_batches b ON b.id=x.batch_id JOIN delivery_groups g ON g.wecom_group_id=x.group_id WHERE t.state='confirmed'").fetch_all(&mut **tx).await?;
    for r in rows {
        project_row(tx, &r, "official").await?;
    }
    let rows=sqlx::query("SELECT a.id,a.school_id,g.id AS group_id,d.content,a.actor_id,CASE WHEN a.reconciliation='operator_confirmed' OR a.state='client_reported_success' THEN 'sent' WHEN a.reconciliation='operator_rejected' THEN 'not_sent' ELSE 'unknown' END AS state,a.reconciled_at AS sent_at FROM delivery_attempts a JOIN delivery_drafts d ON d.id=a.draft_id JOIN delivery_groups g ON g.wecom_group_id=d.group_id").fetch_all(&mut **tx).await?;
    for r in rows {
        project_row(tx, &r, "legacy").await?;
    }
    Ok(())
}
async fn project_row(tx: &mut Tx<'_>, r: &sqlx::postgres::PgRow, channel: &str) -> Result<()> {
    let actor: Option<Uuid> = r.get("actor_id");
    let sender: Option<Uuid> = if let Some(actor) = actor {
        sqlx::query_scalar(
            "SELECT min(id::text)::uuid FROM sending_accounts WHERE responsible_actor_id=$1 HAVING count(*)=1",
        )
        .bind(actor)
        .fetch_optional(&mut **tx)
        .await?
    } else {
        None
    };
    let content: String = r.get("content");
    let digest = digest(&content)?;
    sqlx::query("INSERT INTO delivery_usage(id,source_channel,source_id,school_id,group_id,sender_account_id,content_digest,state,observed_at,evidence_kind,responsible_actor_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,coalesce($9,clock_timestamp()),$2,$10) ON CONFLICT(source_channel,source_id) DO UPDATE SET state=EXCLUDED.state,sender_account_id=EXCLUDED.sender_account_id,observed_at=CASE WHEN delivery_usage.state<>EXCLUDED.state THEN EXCLUDED.observed_at ELSE delivery_usage.observed_at END,responsible_actor_id=EXCLUDED.responsible_actor_id")
      .bind(Uuid::now_v7()).bind(channel).bind(r.get::<Uuid,_>("id")).bind(r.get::<Uuid,_>("school_id")).bind(r.get::<Uuid,_>("group_id")).bind(sender).bind(digest).bind(r.get::<String,_>("state")).bind(r.get::<Option<chrono::DateTime<chrono::Utc>>,_>("sent_at")).bind(actor).execute(&mut **tx).await?;
    Ok(())
}
async fn restrictions(tx: &mut Tx<'_>, i: &Value) -> Result<()> {
    let account = uid(i, "sender_account_id")?;
    let group = uid(i, "group_id")?;
    let s = uid(i, "school_id")?;
    let item = uid(i, "id")?;
    let digest = strv(i, "content_digest")?;
    let account_limit: i32 =
        sqlx::query_scalar("SELECT daily_limit FROM sending_accounts WHERE id=$1")
            .bind(account)
            .fetch_one(&mut **tx)
            .await?;
    let group_limit: i32 =
        sqlx::query_scalar("SELECT daily_limit FROM delivery_groups WHERE id=$1")
            .bind(group)
            .fetch_one(&mut **tx)
            .await?;
    let duplicate:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM delivery_usage WHERE group_id=$1 AND content_digest=$2 AND state<>'not_sent')").bind(group).bind(digest).fetch_one(&mut **tx).await?;
    if duplicate {
        return Err(Error::Policy("duplicate_delivery"));
    }
    let allocation:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM delivery_dedup_reservations WHERE active AND item_id<>$1 AND content_digest=$2 AND ((school_id=$3 AND group_id<>$4) OR group_id=$4)) OR EXISTS(SELECT 1 FROM delivery_usage WHERE school_id=$3 AND group_id<>$4 AND content_digest=$2 AND state<>'not_sent')").bind(item).bind(digest).bind(s).bind(group).fetch_one(&mut **tx).await?;
    if allocation {
        return Err(Error::Policy("school_content_conflict"));
    }
    for post in i["snapshot"]["posts"].as_array().into_iter().flatten() {
        let post_id = uid(post, "id")?;
        let rev = post["revision"].as_i64().ok_or(Error::Conflict)?;
        let overlap:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_delivery_items m JOIN delivery_dedup_reservations r ON r.item_id=m.id, jsonb_array_elements(m.snapshot->'posts') p WHERE r.active AND m.id<>$1 AND (m.group_id=$2 OR m.school_id=$3) AND p->>'id'=$4 AND (p->>'revision')::bigint=$5) OR EXISTS(SELECT 1 FROM broadcast_task_posts p JOIN broadcast_targets x ON x.task_id=p.task_id JOIN delivery_groups g ON g.wecom_group_id=x.group_id JOIN delivery_usage u ON u.source_channel='official' AND u.source_id=x.id WHERE p.post_id=$4::uuid AND p.post_revision=$5 AND (g.id=$2 OR g.school_id=$3) AND u.state<>'not_sent') OR EXISTS(SELECT 1 FROM delivery_draft_posts p JOIN delivery_attempts a ON a.draft_id=p.draft_id JOIN delivery_usage u ON u.source_channel='legacy' AND u.source_id=a.id WHERE p.post_id=$4::uuid AND p.post_revision=$5 AND (u.group_id=$2 OR u.school_id=$3) AND u.state<>'not_sent')").bind(item).bind(group).bind(s).bind(post_id.to_string()).bind(rev).fetch_one(&mut **tx).await?;
        if overlap {
            return Err(Error::Policy("duplicate_delivery"));
        }
    }
    // Unknown/pending are never aged out. Known sends use observation time, not the device clock.
    let counts=sqlx::query("SELECT count(*) FILTER(WHERE (sender_account_id=$1 OR (sender_account_id IS NULL AND responsible_actor_id=(SELECT responsible_actor_id FROM sending_accounts WHERE id=$1)))) AS a,count(*) FILTER(WHERE group_id=$2) AS g FROM delivery_usage WHERE state IN ('pending','unknown') OR (state='sent' AND observed_at>clock_timestamp()-interval '24 hours')").bind(account).bind(group).fetch_one(&mut **tx).await?;
    if counts.get::<i64, _>("a") >= i64::from(account_limit)
        || counts.get::<i64, _>("g") >= i64::from(group_limit)
    {
        return Err(Error::Policy("quota_exhausted"));
    }
    let unknown:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM delivery_usage WHERE state IN ('pending','unknown') AND ((sender_account_id=$1 OR (sender_account_id IS NULL AND responsible_actor_id=(SELECT responsible_actor_id FROM sending_accounts WHERE id=$1))) OR group_id=$2 OR content_digest=$3))").bind(account).bind(group).bind(digest).fetch_one(&mut **tx).await?;
    if unknown {
        return Err(Error::Policy("unknown_requires_review"));
    }
    let cooldown:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM delivery_usage WHERE state='sent' AND (((sender_account_id=$1 OR (sender_account_id IS NULL AND responsible_actor_id=(SELECT responsible_actor_id FROM sending_accounts WHERE id=$1))) AND observed_at>clock_timestamp()-interval '30 minutes') OR (content_digest=$2 AND observed_at>clock_timestamp()-interval '10 minutes')))").bind(account).bind(digest).fetch_one(&mut **tx).await?;
    if cooldown {
        return Err(Error::Policy("cooldown_active"));
    }
    // Any unresolved official operation by this responsible actor also blocks un-mapped accounts.
    let pending:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM broadcast_batches b JOIN sending_accounts a ON a.responsible_actor_id=b.sender_account WHERE a.id=$1 AND b.state IN ('queued','submitting','created','unknown') AND (b.reconciliation IS NULL OR (b.reconciliation='not_created' AND b.msgid IS NOT NULL)) AND (b.stop_state<>'stopped' OR EXISTS(SELECT 1 FROM broadcast_targets x WHERE x.batch_id=b.id AND x.official_status IS DISTINCT FROM 1)))").bind(account).fetch_one(&mut **tx).await?;
    if pending {
        return Err(Error::Policy("official_delivery_pending"));
    }
    Ok(())
}
async fn preview(tx: &mut Tx<'_>, task: Uuid, origin: &str) -> Result<Value> {
    project(tx).await?;
    let list = items(tx, task).await?;
    let mut checks = Vec::new();
    for i in list {
        let result = match validate_snapshot(tx, &i, origin).await {
            Ok(()) => restrictions(tx, &i).await,
            Err(e) => Err(e),
        };
        let reason = match result {
            Ok(()) => None,
            Err(Error::Policy(c)) => Some(c),
            Err(Error::Invalid(_)) => Some("source_invalid"),
            Err(e) => return Err(e),
        };
        checks.push(json!({"id":i["id"],"group_id":i["group_id"],"content":i["content"],"snapshot_digest":i["snapshot_digest"],"reason":reason,"next_eligible_at":null}));
    }
    let task = object(tx, "mobile_delivery_tasks", task).await?;
    let digest = hash(&json!({"revision":task["revision"],"items":checks}).to_string());
    Ok(
        json!({"id":task["id"],"expected_revision":task["revision"],"preview_digest":digest,"items":checks,"requires_substantive_difference_review":true}),
    )
}
async fn claim_object(tx: &mut Tx<'_>, p: &Principal, id: Uuid) -> Result<(Value, Value)> {
    let (d, g) = device(tx, p).await?;
    let c = object(tx, "mobile_execution_claims", id).await?;
    if uid(&c, "device_id")? != d || c["generation"] != g {
        return Err(Error::Forbidden);
    }
    let i = object(tx, "mobile_delivery_items", uid(&c, "item_id")?).await?;
    Ok((c, i))
}
async fn claim_response(tx: &mut Tx<'_>, c: &Value, i: &Value) -> Result<Value> {
    let set = settings(tx, uid(i, "school_id")?).await?;
    let may_execute:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_send_permits p JOIN mobile_delivery_items i ON i.id=p.item_id JOIN mobile_delivery_tasks t ON t.id=i.task_id JOIN mobile_delivery_settings s ON s.school_id=i.school_id WHERE p.claim_id=$1 AND p.state='issued' AND p.execute_before>clock_timestamp() AND i.state='permit_issued' AND t.state='confirmed' AND s.enabled AND s.history_reviewed AND EXISTS(SELECT 1 FROM mobile_execution_claims c WHERE c.id=p.claim_id AND c.preparation_settings_revision=s.revision) AND EXISTS(SELECT 1 FROM accounts a JOIN school_memberships m ON m.account_id=a.id JOIN schools sc ON sc.id=m.school_id WHERE a.id=t.confirmed_by AND a.active AND sc.active AND m.school_id=i.school_id AND (m.role='admin' OR (i.source->>'kind'='posts' AND m.role='operator'))) AND EXISTS(SELECT 1 FROM sending_accounts a JOIN accounts u ON u.id=a.responsible_actor_id JOIN school_memberships m ON m.account_id=u.id WHERE a.id=i.sender_account_id AND a.enabled AND u.active AND m.school_id=i.school_id AND m.role='admin') AND EXISTS(SELECT 1 FROM delivery_groups g WHERE g.id=i.group_id AND g.enabled) AND NOT EXISTS(SELECT 1 FROM jsonb_array_elements(i.snapshot->'posts') sp LEFT JOIN posts post ON post.id=(sp->>'id')::uuid WHERE post.id IS NULL OR post.audit_status<>'approved' OR post.visibility<>'visible' OR post.revision<>(sp->>'revision')::bigint))").bind(uid(c,"id")?).fetch_one(&mut **tx).await? && std::env::var("WEKEEPER_MOBILE_EXECUTION_ENABLED").is_ok_and(|v|v=="true");
    let permit:Option<Value>=sqlx::query_scalar("SELECT jsonb_build_object('id',id,'invocation_id',invocation_id,'state',state,'execute_before',execute_before) FROM mobile_send_permits WHERE claim_id=$1").bind(uid(c,"id")?).fetch_optional(&mut **tx).await?;
    Ok(
        json!({"id":c["id"],"claim_id":c["id"],"item_id":i["id"],"school_id":i["school_id"],"state":c["state"],"item_state":i["state"],"fence":c["fence"],"binding_revision":c["binding_revision"],"lease_until":c["lease_until"],"group_snapshot":i["snapshot"]["group"],"sender_binding":i["snapshot"]["sender"],"content":i["content"],"content_digest":i["content_digest"],"snapshot_digest":i["snapshot_digest"],"permit":permit,"may_execute":may_execute,"requirements":{"protocol_version":PROTOCOL_VERSION,"client_version":set["approved_client_version"],"script_version":set["approved_script_version"],"baseline_version":set["approved_baseline_version"],"no_injection":true,"no_hook":true,"no_private_protocol":true,"no_client_data_modification":true,"local_invocation_consumption_required":true},"server_time":db_now(tx).await?}),
    )
}
async fn db_now(tx: &mut Tx<'_>) -> Result<chrono::DateTime<chrono::Utc>> {
    Ok(sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&mut **tx)
        .await?)
}
async fn ready_claim(
    tx: &mut Tx<'_>,
    p: &Principal,
    s: &Scope,
    fence: i64,
) -> Result<(Value, Value)> {
    let (c, i) = claim_object(tx, p, id(s)?).await?;
    if c["fence"] != fence || !["claimed", "prepared"].contains(&c["state"].as_str().unwrap_or(""))
    {
        return Err(Error::Policy("binding_changed"));
    }
    let live: bool = sqlx::query_scalar(
        "SELECT lease_until>clock_timestamp() FROM mobile_execution_claims WHERE id=$1",
    )
    .bind(id(s)?)
    .fetch_one(&mut **tx)
    .await?;
    if !live {
        return Err(Error::Policy("permit_expired"));
    }
    let b = binding(tx, uid(&c, "device_id")?).await?;
    if b["revision"] != c["binding_revision"] || b["sender_account_id"] != i["sender_account_id"] {
        return Err(Error::Policy("binding_changed"));
    }
    let t = object(tx, "mobile_delivery_tasks", uid(&i, "task_id")?).await?;
    if t["state"] != "confirmed" {
        return Err(Error::Policy("binding_changed"));
    }
    school_auth(
        tx,
        uid(&t, "confirmed_by")?,
        uid(&i, "school_id")?,
        matches!(
            serde_json::from_value::<ContentSource>(i["source"].clone()),
            Ok(ContentSource::Text { .. })
        ),
    )
    .await?;
    active_school(tx, uid(&i, "school_id")?).await?;
    Ok((c, i))
}
impl Repository {
    #[allow(clippy::too_many_arguments)]
    async fn apply(
        tx: &mut Tx<'_>,
        p: &Principal,
        s: &Scope,
        c: Command,
        enabled: bool,
        origin: &str,
    ) -> Result<Value> {
        match c {
            Command::CreateDevice { name } => {
                platform(tx, p).await?;
                let d = Uuid::now_v7();
                sqlx::query("INSERT INTO execution_devices(id,name) VALUES($1,$2)")
                    .bind(d)
                    .bind(text(&name, 1, 100)?)
                    .execute(&mut **tx)
                    .await?;
                object(tx, "execution_devices", d).await
            }
            Command::UpdateDevice {
                expected_revision,
                enabled,
                reason,
            } => {
                platform(tx, p).await?;
                text(&reason, 1, 1000)?;
                let d = id(s)?;
                revision(
                    &object(tx, "execution_devices", d).await?,
                    expected_revision,
                )?;
                invalidate(tx, d).await?;
                sqlx::query(
                    "UPDATE execution_devices SET enabled=$2,revision=revision+1 WHERE id=$1",
                )
                .bind(d)
                .bind(enabled)
                .execute(&mut **tx)
                .await?;
                object(tx, "execution_devices", d).await
            }
            Command::Enrollment { expected_revision } => {
                platform(tx, p).await?;
                let d = id(s)?;
                revision(
                    &object(tx, "execution_devices", d).await?,
                    expected_revision,
                )?;
                sqlx::query("UPDATE device_enrollments SET consumed_at=clock_timestamp() WHERE device_id=$1 AND consumed_at IS NULL").bind(d).execute(&mut **tx).await?;
                let code = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
                let enroll = Uuid::now_v7();
                let expires:chrono::DateTime<chrono::Utc>=sqlx::query_scalar("INSERT INTO device_enrollments(id,device_id,code_hash,expires_at) VALUES($1,$2,$3,clock_timestamp()+interval '10 minutes') RETURNING expires_at").bind(enroll).bind(d).bind(hash(&code)).fetch_one(&mut **tx).await?;
                Ok(json!({"id":enroll,"device_id":d,"code":code,"expires_at":expires}))
            }
            Command::Revoke {
                expected_revision,
                reason,
            } => {
                platform(tx, p).await?;
                text(&reason, 1, 1000)?;
                let d = id(s)?;
                revision(
                    &object(tx, "execution_devices", d).await?,
                    expected_revision,
                )?;
                invalidate(tx, d).await?;
                sqlx::query(
                    "UPDATE device_credentials SET revoked_at=clock_timestamp() WHERE device_id=$1",
                )
                .bind(d)
                .execute(&mut **tx)
                .await?;
                sqlx::query("UPDATE execution_devices SET enabled=false,credential_generation=credential_generation+1,revision=revision+1 WHERE id=$1").bind(d).execute(&mut **tx).await?;
                object(tx, "execution_devices", d).await
            }
            Command::CreateAccount {
                client_kind,
                identity_namespace,
                external_key,
                responsible_actor_id,
                identity_evidence_ref,
            } => {
                platform(tx, p).await?;
                require(
                    ["wechat", "wecom"].contains(&client_kind.as_str()),
                    "客户端类型无效",
                )?;
                let a = Uuid::now_v7();
                sqlx::query("INSERT INTO sending_accounts(id,client_kind,identity_namespace,external_key,responsible_actor_id,identity_evidence_ref) VALUES($1,$2,$3,$4,$5,$6)").bind(a).bind(client_kind).bind(text(&identity_namespace,1,200)?).bind(text(&external_key,1,200)?).bind(responsible_actor_id).bind(text(&identity_evidence_ref,1,500)?).execute(&mut **tx).await?;
                object(tx, "sending_accounts", a).await
            }
            Command::BindDevice {
                expected_revision,
                sender_account_id,
                identity_evidence_ref,
            } => {
                platform(tx, p).await?;
                let d = id(s)?;
                revision(
                    &object(tx, "execution_devices", d).await?,
                    expected_revision,
                )?;
                let authorized:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sender_group_bindings b JOIN sending_accounts a ON a.id=b.sender_account_id JOIN school_memberships m ON m.school_id=b.school_id AND m.account_id=a.responsible_actor_id JOIN schools s ON s.id=b.school_id JOIN accounts u ON u.id=a.responsible_actor_id WHERE b.sender_account_id=$1 AND m.role='admin' AND u.active AND s.active)").bind(sender_account_id).fetch_one(&mut **tx).await?;
                if !authorized {
                    return Err(Error::Forbidden);
                }
                invalidate(tx, d).await?;
                let old:Option<Uuid>=sqlx::query_scalar("SELECT device_id FROM device_account_bindings WHERE sender_account_id=$1 AND device_id<>$2").bind(sender_account_id).bind(d).fetch_optional(&mut **tx).await?;
                if let Some(old) = old {
                    invalidate(tx, old).await?;
                    sqlx::query("DELETE FROM device_account_bindings WHERE device_id=$1")
                        .bind(old)
                        .execute(&mut **tx)
                        .await?;
                    sqlx::query("UPDATE execution_devices SET revision=revision+1 WHERE id=$1")
                        .bind(old)
                        .execute(&mut **tx)
                        .await?;
                }
                sqlx::query("INSERT INTO device_account_bindings(device_id,sender_account_id,revision,identity_evidence_ref) VALUES($1,$2,$3,$4) ON CONFLICT(device_id) DO UPDATE SET sender_account_id=EXCLUDED.sender_account_id,revision=EXCLUDED.revision,identity_evidence_ref=EXCLUDED.identity_evidence_ref").bind(d).bind(sender_account_id).bind(expected_revision+1).bind(text(&identity_evidence_ref,1,500)?).execute(&mut **tx).await?;
                sqlx::query("UPDATE execution_devices SET revision=revision+1 WHERE id=$1")
                    .bind(d)
                    .execute(&mut **tx)
                    .await?;
                binding(tx, d).await.map(|mut v| {
                    v["id"] = json!(d);
                    v
                })
            }
            Command::CreateGroup {
                name,
                client_kind,
                identity_namespace,
                external_key,
                identity_evidence_ref,
                wecom_group_id,
            } => {
                school_auth(tx, p.actor()?, school(s)?, true).await?;
                require(
                    ["wechat", "wecom"].contains(&client_kind.as_str())
                        && (client_kind == "wecom") == wecom_group_id.is_some(),
                    "企业微信群必须映射现有官方群；普通微信群不可伪造官方身份",
                )?;
                if let Some(g) = wecom_group_id {
                    let matches:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM wecom_groups WHERE id=$1 AND school_id=$2 AND corp_id=$3 AND chat_id=$4)").bind(g).bind(school(s)?).bind(&identity_namespace).bind(&external_key).fetch_one(&mut **tx).await?;
                    require(matches, "官方群身份不匹配")?;
                }
                let g = Uuid::now_v7();
                sqlx::query("INSERT INTO delivery_groups(id,school_id,name,client_kind,identity_namespace,external_key,identity_evidence_ref,wecom_group_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(g).bind(school(s)?).bind(text(&name,1,100)?).bind(client_kind).bind(text(&identity_namespace,1,200)?).bind(text(&external_key,1,200)?).bind(text(&identity_evidence_ref,1,500)?).bind(wecom_group_id).execute(&mut **tx).await?;
                project(tx).await?;
                object(tx, "delivery_groups", g).await
            }
            Command::BindGroup {
                sender_account_id,
                mapping_revision,
                reason,
            } => {
                school_auth(tx, p.actor()?, school(s)?, true).await?;
                let g = scoped_object(tx, s, "delivery_groups").await?;
                revision(&g, mapping_revision)?;
                let a = object(tx, "sending_accounts", sender_account_id).await?;
                school_auth(tx, uid(&a, "responsible_actor_id")?, school(s)?, true).await?;
                require(a["client_kind"] == g["client_kind"], "客户端不匹配")?;
                // Updating identity during an invocation cannot retract the remote click.
                let devices: Vec<Uuid> = sqlx::query_scalar(
                    "SELECT device_id FROM device_account_bindings WHERE sender_account_id=$1",
                )
                .bind(sender_account_id)
                .fetch_all(&mut **tx)
                .await?;
                for d in devices {
                    invalidate(tx, d).await?;
                }
                sqlx::query("INSERT INTO sender_group_bindings(school_id,group_id,sender_account_id,evidence_ref) VALUES($1,$2,$3,$4) ON CONFLICT(group_id,sender_account_id) DO UPDATE SET revision=sender_group_bindings.revision+1,evidence_ref=EXCLUDED.evidence_ref").bind(school(s)?).bind(id(s)?).bind(sender_account_id).bind(text(&reason,1,1000)?).execute(&mut **tx).await?;
                sqlx::query("UPDATE delivery_groups SET revision=revision+1 WHERE id=$1")
                    .bind(id(s)?)
                    .execute(&mut **tx)
                    .await?;
                object(tx, "delivery_groups", id(s)?).await
            }
            Command::AccountPolicy {
                expected_revision,
                daily_limit,
                reason,
            } => {
                platform(tx, p).await?;
                policy(daily_limit, true)?;
                text(&reason, 1, 1000)?;
                let a = object(tx, "sending_accounts", id(s)?).await?;
                revision(&a, expected_revision)?;
                sqlx::query(
                    "UPDATE sending_accounts SET daily_limit=$2,revision=revision+1 WHERE id=$1",
                )
                .bind(id(s)?)
                .bind(daily_limit)
                .execute(&mut **tx)
                .await?;
                object(tx, "sending_accounts", id(s)?).await
            }
            Command::GroupPolicy {
                expected_revision,
                daily_limit,
                reason,
            } => {
                school_auth(tx, p.actor()?, school(s)?, true).await?;
                policy(daily_limit, false)?;
                text(&reason, 1, 1000)?;
                revision(
                    &scoped_object(tx, s, "delivery_groups").await?,
                    expected_revision,
                )?;
                sqlx::query(
                    "UPDATE delivery_groups SET daily_limit=$2,revision=revision+1 WHERE id=$1",
                )
                .bind(id(s)?)
                .bind(daily_limit)
                .execute(&mut **tx)
                .await?;
                object(tx, "delivery_groups", id(s)?).await
            }
            Command::Settings {
                expected_revision,
                enabled,
                history_reviewed,
                approved_client_version,
                approved_script_version,
                approved_baseline_version,
                evidence_ref,
            } => {
                school_auth(tx, p.actor()?, school(s)?, true).await?;
                revision(&settings(tx, school(s)?).await?, expected_revision)?;
                if enabled {
                    require(history_reviewed, "需人工核对历史及群映射")?;
                    let pending:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM broadcast_batches WHERE school_id=$1 AND state IN ('queued','submitting','created','unknown') AND (reconciliation IS NULL OR (reconciliation='not_created' AND msgid IS NOT NULL)) AND (stop_state<>'stopped' OR EXISTS(SELECT 1 FROM broadcast_targets x WHERE x.batch_id=broadcast_batches.id AND x.official_status IS DISTINCT FROM 1))) OR EXISTS(SELECT 1 FROM delivery_attempts WHERE school_id=$1 AND reconciliation IS NULL AND state<>'client_reported_success') OR EXISTS(SELECT 1 FROM wecom_groups g WHERE school_id=$1 AND NOT EXISTS(SELECT 1 FROM delivery_groups d WHERE d.wecom_group_id=g.id))").bind(school(s)?).fetch_one(&mut **tx).await?;
                    if pending {
                        return Err(Error::Policy("official_delivery_pending"));
                    }
                }
                sqlx::query("INSERT INTO mobile_delivery_settings(school_id,enabled,history_reviewed,revision,approved_client_version,approved_script_version,approved_baseline_version,evidence_ref) VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(school_id) DO UPDATE SET enabled=EXCLUDED.enabled,history_reviewed=EXCLUDED.history_reviewed,revision=EXCLUDED.revision,approved_client_version=EXCLUDED.approved_client_version,approved_script_version=EXCLUDED.approved_script_version,approved_baseline_version=EXCLUDED.approved_baseline_version,evidence_ref=EXCLUDED.evidence_ref").bind(school(s)?).bind(enabled).bind(history_reviewed).bind(expected_revision+1).bind(text(&approved_client_version,1,100)?).bind(text(&approved_script_version,1,100)?).bind(text(&approved_baseline_version,1,100)?).bind(text(&evidence_ref,1,500)?).execute(&mut **tx).await?;
                if !enabled {
                    let devices:Vec<Uuid>=sqlx::query_scalar("SELECT DISTINCT c.device_id FROM mobile_execution_claims c JOIN mobile_delivery_items i ON i.id=c.item_id WHERE i.school_id=$1 AND c.state IN ('claimed','prepared','permit_issued')").bind(school(s)?).fetch_all(&mut **tx).await?;
                    for d in devices {
                        invalidate(tx, d).await?;
                    }
                }
                project(tx).await?;
                settings(tx, school(s)?).await
            }
            Command::Draft { draft } => save_draft(tx, p, s, draft, origin).await,
            Command::Preview { expected_revision } => {
                school_auth(tx, p.actor()?, school(s)?, false).await?;
                revision(
                    &scoped_object(tx, s, "mobile_delivery_tasks").await?,
                    expected_revision,
                )?;
                preview(tx, id(s)?, origin).await
            }
            Command::Confirm {
                expected_revision,
                preview_digest,
                content_review,
                scope_acceptance,
                substantive_difference_reviewed,
                evidence_ref,
            } => {
                let task = scoped_object(tx, s, "mobile_delivery_tasks").await?;
                let list = items(tx, id(s)?).await?;
                let text = list.iter().any(|i| i["source"]["kind"] == "text");
                school_auth(tx, p.actor()?, school(s)?, text).await?;
                revision(&task, expected_revision)?;
                require(
                    task["state"] == "draft"
                        && content_review
                        && scope_acceptance
                        && substantive_difference_reviewed,
                    "需要逐群人工确认内容、范围和实质差异",
                )?;
                let check = preview(tx, id(s)?, origin).await?;
                if check["preview_digest"] != preview_digest {
                    return Err(Error::Policy("revision_conflict"));
                }
                for i in list {
                    validate_snapshot(tx, &i, origin).await?;
                    // Temporary quota/cooldown holds may be queued, permanent conflicts may not.
                    if let Err(e) = restrictions(tx, &i).await {
                        match e {
                            Error::Policy(
                                "quota_exhausted"
                                | "cooldown_active"
                                | "unknown_requires_review"
                                | "official_delivery_pending",
                            ) => {}
                            other => return Err(other),
                        }
                    }
                    sqlx::query("INSERT INTO delivery_dedup_reservations(item_id,school_id,group_id,content_digest) VALUES($1,$2,$3,$4)").bind(uid(&i,"id")?).bind(school(s)?).bind(uid(&i,"group_id")?).bind(strv(&i,"content_digest")?).execute(&mut **tx).await?;
                }
                sqlx::query("UPDATE mobile_delivery_tasks SET state='confirmed',confirmed_by=$2,revision=revision+1,review_evidence_ref=$3 WHERE id=$1").bind(id(s)?).bind(p.actor()?).bind(crate::business::text(&evidence_ref,1,500)?).execute(&mut **tx).await?;
                object(tx, "mobile_delivery_tasks", id(s)?).await
            }
            Command::Stop {
                expected_revision,
                reason,
            } => {
                school_auth(tx, p.actor()?, school(s)?, true).await?;
                text(&reason, 1, 1000)?;
                revision(
                    &scoped_object(tx, s, "mobile_delivery_tasks").await?,
                    expected_revision,
                )?;
                let devices:Vec<Uuid>=sqlx::query_scalar("SELECT DISTINCT c.device_id FROM mobile_execution_claims c JOIN mobile_delivery_items i ON i.id=c.item_id WHERE i.task_id=$1 AND c.state IN ('claimed','prepared','permit_issued')").bind(id(s)?).fetch_all(&mut **tx).await?;
                for d in devices {
                    invalidate(tx, d).await?;
                }
                sqlx::query("UPDATE mobile_delivery_items SET state='cancelled',revision=revision+1 WHERE task_id=$1 AND state IN ('queued','claimed','prepared')").bind(id(s)?).execute(&mut **tx).await?;
                sqlx::query("UPDATE delivery_dedup_reservations r SET active=false FROM mobile_delivery_items i WHERE r.item_id=i.id AND i.task_id=$1 AND i.state='cancelled'").bind(id(s)?).execute(&mut **tx).await?;
                sqlx::query("UPDATE mobile_delivery_tasks SET state='stopped',revision=revision+1 WHERE id=$1").bind(id(s)?).execute(&mut **tx).await?;
                object(tx, "mobile_delivery_tasks", id(s)?).await
            }
            Command::Reconcile {
                expected_revision,
                decision,
                evidence_ref,
                reason,
            } => {
                school_auth(tx, p.actor()?, school(s)?, true).await?;
                let i = scoped_object(tx, s, "mobile_delivery_items").await?;
                revision(&i, expected_revision)?;
                require(i["state"] == "unknown", "只允许核对不确定结果")?;
                require(
                    ["manually_sent", "verified_not_sent"].contains(&decision.as_str()),
                    "核对结果无效",
                )?;
                text(&reason, 1, 1000)?;
                text(&evidence_ref, 1, 500)?;
                let closed: bool = sqlx::query_scalar(
                    "SELECT execute_before<=clock_timestamp() FROM mobile_send_permits WHERE item_id=$1",
                )
                .bind(id(s)?)
                .fetch_one(&mut **tx)
                .await?;
                if !closed {
                    return Err(Error::Policy("permit_expired"));
                }
                sqlx::query("UPDATE mobile_delivery_items SET state=$2,revision=revision+1,observed_at=clock_timestamp() WHERE id=$1").bind(id(s)?).bind(&decision).execute(&mut **tx).await?;
                sqlx::query("UPDATE mobile_send_permits SET state=$2 WHERE item_id=$1")
                    .bind(id(s)?)
                    .bind(&decision)
                    .execute(&mut **tx)
                    .await?;
                sqlx::query("UPDATE mobile_execution_claims SET state='closed' WHERE item_id=$1")
                    .bind(id(s)?)
                    .execute(&mut **tx)
                    .await?;
                sqlx::query("UPDATE delivery_usage SET state=$2,evidence_kind='manual',observed_at=clock_timestamp() WHERE source_channel='mobile' AND source_id=$1").bind(id(s)?).bind(if decision=="manually_sent"{"sent"}else{"not_sent"}).execute(&mut **tx).await?;
                if decision == "verified_not_sent" {
                    sqlx::query(
                        "UPDATE delivery_dedup_reservations SET active=false WHERE item_id=$1",
                    )
                    .bind(id(s)?)
                    .execute(&mut **tx)
                    .await?;
                }
                sqlx::query("INSERT INTO mobile_delivery_events(id,school_id,task_id,actor_id,action,detail) VALUES($1,$2,$3,$4,'reconciliation',$5)").bind(Uuid::now_v7()).bind(school(s)?).bind(uid(&i,"task_id")?).bind(p.actor()?).bind(json!({"item_id":id(s)?,"decision":decision,"reason":reason,"evidence_ref":evidence_ref})).execute(&mut **tx).await?;
                object(tx, "mobile_delivery_items", id(s)?).await
            }
            Command::Resend {
                expected_revision,
                reason,
            } => {
                school_auth(tx, p.actor()?, school(s)?, true).await?;
                text(&reason, 1, 1000)?;
                let i = scoped_object(tx, s, "mobile_delivery_items").await?;
                revision(&i, expected_revision)?;
                require(
                    i["state"] == "verified_not_sent",
                    "仅核实未发送可建立重发草稿",
                )?;
                let t = object(tx, "mobile_delivery_tasks", uid(&i, "task_id")?).await?;
                let source: ContentSource =
                    serde_json::from_value(i["source"].clone()).map_err(|_| Error::Conflict)?;
                save_draft(
                    tx,
                    p,
                    &Scope {
                        school: s.school,
                        id: None,
                    },
                    Draft {
                        expected_revision: None,
                        activity_id: strv(&t, "activity_id")?.into(),
                        items: vec![ItemInput {
                            group_id: uid(&i, "group_id")?,
                            sender_account_id: uid(&i, "sender_account_id")?,
                            source,
                        }],
                    },
                    origin,
                )
                .await
            }
            Command::Enroll {
                device_id,
                code,
                client_instance_id,
                protocol_version,
            } => {
                require(matches!(p, Principal::Enrollment), "配对主体无效")?;
                if protocol_version != PROTOCOL_VERSION {
                    return Err(Error::Policy("protocol_unsupported"));
                }
                sqlx::query("UPDATE device_enrollments SET consumed_at=clock_timestamp() WHERE device_id=$1 AND code_hash=$2 AND consumed_at IS NULL AND expires_at>clock_timestamp() RETURNING id").bind(device_id).bind(hash(&code)).fetch_optional(&mut **tx).await?.ok_or(Error::Unauthorized)?;
                sqlx::query("UPDATE execution_devices SET client_instance_id=$2 WHERE id=$1")
                    .bind(device_id)
                    .bind(text(&client_instance_id, 1, 200)?)
                    .execute(&mut **tx)
                    .await?;
                credential(tx, device_id).await
            }
            Command::Rotate {} => {
                let (d, _) = device(tx, p).await?;
                credential(tx, d).await
            }
            Command::Heartbeat {
                binding_revision,
                sender_account_id,
                client_version,
                script_version,
                baseline_version,
                permissions_ready,
                current_claim_id,
            } => {
                let (d, _) = device(tx, p).await?;
                let b = binding(tx, d).await?;
                if b["revision"] != binding_revision
                    || b["sender_account_id"] != sender_account_id.to_string()
                {
                    return Err(Error::Policy("binding_changed"));
                }
                for v in [&client_version, &script_version, &baseline_version] {
                    text(v, 1, 100)?;
                }
                if let Some(c) = current_claim_id {
                    claim_object(tx, p, c).await?;
                }
                sqlx::query(
                    "UPDATE execution_devices SET last_seen_at=clock_timestamp() WHERE id=$1",
                )
                .bind(d)
                .execute(&mut **tx)
                .await?;
                Ok(
                    json!({"id":d,"binding":b,"permissions_ready_declared":permissions_ready,"server_time":db_now(tx).await?,"heartbeat_interval_seconds":30}),
                )
            }
            Command::Claim { binding_revision } => {
                if !enabled {
                    return Err(Error::Unavailable);
                }
                let (d, generation) = device(tx, p).await?;
                let b = binding(tx, d).await?;
                if b["revision"] != binding_revision {
                    return Err(Error::Policy("binding_changed"));
                }
                if let Some(c)=sqlx::query_scalar::<_,Value>("SELECT to_jsonb(c) FROM mobile_execution_claims c WHERE device_id=$1 AND state IN ('claimed','prepared','permit_issued')").bind(d).fetch_optional(&mut **tx).await? {let i=object(tx,"mobile_delivery_items",uid(&c,"item_id")?).await?;return claim_response(tx,&c,&i).await}
                project(tx).await?;
                let candidates:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(i) FROM (SELECT DISTINCT ON (i.group_id) i.* FROM mobile_delivery_items i JOIN mobile_delivery_tasks t ON t.id=i.task_id JOIN mobile_delivery_settings s ON s.school_id=i.school_id WHERE i.sender_account_id=$1 AND i.state='queued' AND t.state='confirmed' AND s.enabled AND s.history_reviewed ORDER BY i.group_id,i.created_at,i.id) i LEFT JOIN delivery_scheduler_cursors c ON c.school_id=i.school_id ORDER BY i.school_id,CASE WHEN i.group_id>c.last_group_id OR c.last_group_id IS NULL THEN 0 ELSE 1 END,i.group_id LIMIT 100").bind(uid(&b,"sender_account_id")?).fetch_all(&mut **tx).await?;
                let mut reason = "no_ready_task";
                for i in candidates {
                    // Persist skipped scan position too: >100 blocked groups must not starve later groups.
                    sqlx::query("INSERT INTO delivery_scheduler_cursors(school_id,last_group_id) VALUES($1,$2) ON CONFLICT(school_id) DO UPDATE SET last_group_id=EXCLUDED.last_group_id").bind(uid(&i,"school_id")?).bind(uid(&i,"group_id")?).execute(&mut **tx).await?;
                    let task = object(tx, "mobile_delivery_tasks", uid(&i, "task_id")?).await?;
                    if school_auth(
                        tx,
                        uid(&task, "confirmed_by")?,
                        uid(&i, "school_id")?,
                        i["source"]["kind"] == "text",
                    )
                    .await
                    .is_err()
                    {
                        reason = "school_authorization_revoked";
                        continue;
                    }
                    let checked = match validate_snapshot(tx, &i, origin).await {
                        Ok(()) => restrictions(tx, &i).await,
                        Err(e) => Err(e),
                    };
                    match checked {
                        Ok(()) => {}
                        Err(Error::Policy(c)) => {
                            reason = c;
                            continue;
                        }
                        Err(Error::Forbidden | Error::Invalid(_)) => {
                            reason = "binding_changed";
                            continue;
                        }
                        Err(e) => return Err(e),
                    }
                    let fence:i64=sqlx::query_scalar("UPDATE mobile_delivery_items SET state='claimed',fence=fence+1,revision=revision+1 WHERE id=$1 RETURNING fence").bind(uid(&i,"id")?).fetch_one(&mut **tx).await?;
                    let c = Uuid::now_v7();
                    sqlx::query("INSERT INTO mobile_execution_claims(id,item_id,device_id,generation,binding_revision,fence,lease_until,state) VALUES($1,$2,$3,$4,$5,$6,clock_timestamp()+interval '120 seconds','claimed')").bind(c).bind(uid(&i,"id")?).bind(d).bind(generation).bind(binding_revision).bind(fence).execute(&mut **tx).await?;
                    sqlx::query("INSERT INTO delivery_scheduler_cursors(school_id,last_group_id) VALUES($1,$2) ON CONFLICT(school_id) DO UPDATE SET last_group_id=EXCLUDED.last_group_id").bind(uid(&i,"school_id")?).bind(uid(&i,"group_id")?).execute(&mut **tx).await?;
                    let claim = object(tx, "mobile_execution_claims", c).await?;
                    let item = object(tx, "mobile_delivery_items", uid(&i, "id")?).await?;
                    return claim_response(tx, &claim, &item).await;
                }
                Ok(
                    json!({"waiting":true,"server_time":db_now(tx).await?,"retry_after_seconds":30,"next_eligible_at":null,"reason":reason}),
                )
            }
            Command::Renew {
                fence,
                binding_revision,
            } => {
                let (c, i) = ready_claim(tx, p, s, fence).await?;
                if c["binding_revision"] != binding_revision {
                    return Err(Error::Policy("binding_changed"));
                }
                sqlx::query("UPDATE mobile_execution_claims SET lease_until=clock_timestamp()+interval '120 seconds' WHERE id=$1").bind(id(s)?).execute(&mut **tx).await?;
                {
                    let claim = object(tx, "mobile_execution_claims", id(s)?).await?;
                    claim_response(tx, &claim, &i).await
                }
            }
            Command::Prepare {
                fence,
                snapshot_digest,
                observed_account_id,
                observed_group_id,
                client_version,
                script_version,
                baseline_version,
                checks_passed,
            } => {
                let (_, i) = ready_claim(tx, p, s, fence).await?;
                validate_snapshot(tx, &i, origin).await?;
                require(
                    checks_passed
                        && i["snapshot_digest"] == snapshot_digest
                        && uid(&i, "sender_account_id")? == observed_account_id
                        && uid(&i, "group_id")? == observed_group_id,
                    "准备检查与冻结身份不匹配",
                )?;
                let set = active_school(tx, uid(&i, "school_id")?).await?;
                if set["approved_client_version"] != client_version
                    || set["approved_script_version"] != script_version
                    || set["approved_baseline_version"] != baseline_version
                {
                    return Err(Error::Policy("protocol_unsupported"));
                }
                let prep = Uuid::now_v7();
                sqlx::query("UPDATE mobile_execution_claims SET state='prepared',preparation_id=$2,preparation_settings_revision=$3 WHERE id=$1").bind(id(s)?).bind(prep).bind(set["revision"].as_i64().ok_or(Error::Conflict)?).execute(&mut **tx).await?;
                sqlx::query("UPDATE mobile_delivery_items SET state='prepared',revision=revision+1 WHERE id=$1").bind(uid(&i,"id")?).execute(&mut **tx).await?;
                Ok(
                    json!({"id":prep,"claim_id":id(s)?,"fence":fence,"snapshot_digest":snapshot_digest}),
                )
            }
            Command::Permit {
                fence,
                snapshot_digest,
                preparation_id,
            } => {
                if !enabled {
                    return Err(Error::Unavailable);
                }
                let (c, i) = ready_claim(tx, p, s, fence).await?;
                let approved = active_school(tx, uid(&i, "school_id")?).await?;
                if c["preparation_settings_revision"] != approved["revision"] {
                    return Err(Error::Policy("protocol_unsupported"));
                }
                if c["state"] != "prepared"
                    || c["preparation_id"] != preparation_id.to_string()
                    || i["snapshot_digest"] != snapshot_digest
                {
                    return Err(Error::Policy("binding_changed"));
                }
                validate_snapshot(tx, &i, origin).await?;
                project(tx).await?;
                restrictions(tx, &i).await?;
                let permit = Uuid::now_v7();
                let invocation = Uuid::now_v7();
                let token = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
                let row=sqlx::query("INSERT INTO mobile_send_permits(id,claim_id,item_id,invocation_id,token_hash,execute_before) VALUES($1,$2,$3,$4,$5,clock_timestamp()+interval '60 seconds') RETURNING not_before,execute_before").bind(permit).bind(id(s)?).bind(uid(&i,"id")?).bind(invocation).bind(hash(&token)).fetch_one(&mut **tx).await?;
                sqlx::query("INSERT INTO delivery_usage(id,source_channel,source_id,school_id,group_id,sender_account_id,content_digest,state,evidence_kind) VALUES($1,'mobile',$2,$3,$4,$5,$6,'pending','permit')").bind(Uuid::now_v7()).bind(uid(&i,"id")?).bind(uid(&i,"school_id")?).bind(uid(&i,"group_id")?).bind(uid(&i,"sender_account_id")?).bind(strv(&i,"content_digest")?).execute(&mut **tx).await?;
                sqlx::query("UPDATE mobile_execution_claims SET state='permit_issued' WHERE id=$1")
                    .bind(id(s)?)
                    .execute(&mut **tx)
                    .await?;
                sqlx::query("UPDATE mobile_delivery_items SET state='permit_issued',revision=revision+1 WHERE id=$1").bind(uid(&i,"id")?).execute(&mut **tx).await?;
                Ok(json!(SendPermit {
                    id: permit,
                    claim_id: id(s)?,
                    invocation_id: invocation,
                    fence,
                    snapshot_digest,
                    permit_token: Some(token),
                    not_before: row.get("not_before"),
                    execute_before: row.get("execute_before"),
                    server_time: db_now(tx).await?
                }))
            }
            Command::Result {
                invocation_id,
                permit_token,
                event_id,
                result,
                evidence_ref,
                observed_account_id,
                observed_group_id,
                content_digest,
                local_state,
                reported_clicked_at,
            } => {
                let (c, i) = claim_object(tx, p, id(s)?).await?;
                let event_digest=hash(&json!({"invocation_id":invocation_id,"permit_token_hash":hash(&permit_token),"result":result,"evidence_ref":evidence_ref,"observed_account_id":observed_account_id,"observed_group_id":observed_group_id,"content_digest":content_digest,"local_state":local_state,"reported_clicked_at":reported_clicked_at}).to_string());
                let old:Option<Value>=sqlx::query_scalar("SELECT detail FROM mobile_delivery_events WHERE action='device_result' AND detail->>'event_id'=$1").bind(event_id.to_string()).fetch_optional(&mut **tx).await?;
                if let Some(old) = old {
                    if old["event_digest"] != event_digest {
                        return Err(Error::Policy("revision_conflict"));
                    }
                    return Ok(i);
                }
                let permit:Value=sqlx::query_scalar("SELECT to_jsonb(p) FROM mobile_send_permits p WHERE claim_id=$1 AND invocation_id=$2 AND token_hash=$3").bind(id(s)?).bind(invocation_id).bind(hash(&permit_token)).fetch_optional(&mut **tx).await?.ok_or(Error::Forbidden)?;
                require(
                    ["ui_observed_sent", "not_observed", "safety_blocked"]
                        .contains(&result.as_str()),
                    "结果无效",
                )?;
                text(&evidence_ref, 1, 500)?;
                require(
                    ["issued", "unknown"].contains(&permit["state"].as_str().unwrap_or("")),
                    "结果已经终结，不能覆盖",
                )?;
                let now = db_now(tx).await?;
                let before: chrono::DateTime<chrono::Utc> =
                    serde_json::from_value(permit["execute_before"].clone())
                        .map_err(|_| Error::Conflict)?;
                let start: chrono::DateTime<chrono::Utc> =
                    serde_json::from_value(permit["not_before"].clone())
                        .map_err(|_| Error::Conflict)?;
                let sent = result == "ui_observed_sent"
                    && permit["state"] == "issued"
                    && now <= before
                    && reported_clicked_at.is_some_and(|clicked| {
                        clicked >= start && clicked <= now && clicked <= before
                    })
                    && observed_account_id == uid(&i, "sender_account_id")?
                    && observed_group_id == uid(&i, "group_id")?
                    && i["content_digest"] == content_digest
                    && local_state == "sent_bubble_observed";
                let state = if sent { "ui_observed_sent" } else { "unknown" };
                sqlx::query("UPDATE mobile_send_permits SET state=$2 WHERE claim_id=$1")
                    .bind(id(s)?)
                    .bind(state)
                    .execute(&mut **tx)
                    .await?;
                sqlx::query("UPDATE mobile_execution_claims SET state='closed' WHERE id=$1")
                    .bind(id(s)?)
                    .execute(&mut **tx)
                    .await?;
                sqlx::query("UPDATE mobile_delivery_items SET state=$2,observed_at=clock_timestamp(),revision=revision+1 WHERE id=$1").bind(uid(&i,"id")?).bind(state).execute(&mut **tx).await?;
                sqlx::query("UPDATE delivery_usage SET state=$2,observed_at=clock_timestamp(),evidence_kind='ui' WHERE source_channel='mobile' AND source_id=$1").bind(uid(&i,"id")?).bind(if sent{"sent"}else{"unknown"}).execute(&mut **tx).await?;
                sqlx::query("INSERT INTO mobile_delivery_events(id,school_id,task_id,device_id,action,detail) VALUES($1,$2,$3,$4,'device_result',$5)").bind(Uuid::now_v7()).bind(uid(&i,"school_id")?).bind(uid(&i,"task_id")?).bind(uid(&c,"device_id")?).bind(json!({"event_id":event_id,"event_digest":event_digest,"invocation_id":invocation_id,"item_id":i["id"],"result":state,"evidence_ref":evidence_ref,"reported_clicked_at":reported_clicked_at})).execute(&mut **tx).await?;
                object(tx, "mobile_delivery_items", uid(&i, "id")?).await
            }
            Command::Abort { fence, reason } => {
                text(&reason, 1, 1000)?;
                let (c, i) = claim_object(tx, p, id(s)?).await?;
                if c["fence"] != fence
                    || !["claimed", "prepared", "permit_issued"]
                        .contains(&c["state"].as_str().unwrap_or(""))
                {
                    return Err(Error::Policy("binding_changed"));
                }
                invalidate(tx, uid(&c, "device_id")?).await?;
                object(tx, "mobile_delivery_items", uid(&i, "id")?).await
            }
        }
    }
}
impl Store for Repository {
    async fn authenticate(&self, token: &str) -> Result<Principal> {
        require(token.len() <= 256, "凭据过长")?;
        let r=sqlx::query("SELECT c.device_id,c.generation FROM device_credentials c JOIN execution_devices d ON d.id=c.device_id WHERE c.token_hash=$1 AND c.revoked_at IS NULL AND c.expires_at>clock_timestamp() AND d.enabled AND d.credential_generation=c.generation").bind(hash(token)).fetch_optional(&self.0).await?.ok_or(Error::Unauthorized)?;
        let requests:i32=sqlx::query_scalar("INSERT INTO device_request_windows(device_id,requests,reset_at) VALUES($1,1,clock_timestamp()+interval '1 minute') ON CONFLICT(device_id) DO UPDATE SET requests=CASE WHEN device_request_windows.reset_at<=clock_timestamp() THEN 1 ELSE device_request_windows.requests+1 END,reset_at=CASE WHEN device_request_windows.reset_at<=clock_timestamp() THEN clock_timestamp()+interval '1 minute' ELSE device_request_windows.reset_at END RETURNING requests").bind(r.get::<Uuid,_>("device_id")).fetch_one(&self.0).await?;
        if requests > 120 {
            return Err(Error::Policy("device_request_limit"));
        }
        Ok(Principal::Device {
            id: r.get("device_id"),
            generation: r.get("generation"),
        })
    }
    async fn recover(&self) -> Result<()> {
        let mut tx = self.0.begin().await?;
        guard(&mut tx).await?;
        recover_tx(&mut tx).await?;
        project(&mut tx).await?;
        sqlx::query("UPDATE mobile_delivery_tasks t SET state='completed',revision=revision+1 WHERE state='confirmed' AND NOT EXISTS(SELECT 1 FROM mobile_delivery_items i WHERE i.task_id=t.id AND i.state IN ('queued','claimed','prepared','permit_issued'))").execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
    async fn execute(
        &self,
        p: Principal,
        s: Scope,
        key: &str,
        c: Command,
        enabled: bool,
        origin: &str,
    ) -> Result<Value> {
        if let Command::Enroll { device_id, .. } = &c {
            let attempts:Option<i32>=sqlx::query_scalar("INSERT INTO device_pairing_windows(device_id,requests,reset_at) SELECT id,1,clock_timestamp()+interval '10 minutes' FROM execution_devices WHERE id=$1 ON CONFLICT(device_id) DO UPDATE SET requests=CASE WHEN device_pairing_windows.reset_at<=clock_timestamp() THEN 1 ELSE device_pairing_windows.requests+1 END,reset_at=CASE WHEN device_pairing_windows.reset_at<=clock_timestamp() THEN clock_timestamp()+interval '10 minutes' ELSE device_pairing_windows.reset_at END RETURNING requests").bind(device_id).fetch_optional(&self.0).await?;
            if attempts.is_none() {
                return Err(Error::Unauthorized);
            }
            if attempts.is_some_and(|n| n > 20) {
                return Err(Error::Policy("device_request_limit"));
            }
        }
        let mut tx = self.0.begin().await?;
        guard(&mut tx).await?;
        recover_tx(&mut tx).await?;
        match &p {
            Principal::Actor(actor) => {
                if let Some(school) = s.school {
                    school_auth(
                        &mut tx,
                        *actor,
                        school,
                        matches!(
                            c,
                            Command::CreateGroup { .. }
                                | Command::BindGroup { .. }
                                | Command::GroupPolicy { .. }
                                | Command::Settings { .. }
                                | Command::Stop { .. }
                                | Command::Reconcile { .. }
                                | Command::Resend { .. }
                        ),
                    )
                    .await?;
                } else {
                    platform(&mut tx, &p).await?;
                }
            }
            Principal::Device { .. } => {
                device(&mut tx, &p).await?;
                if let Some(id) = s.id {
                    claim_object(&mut tx, &p, id).await?;
                }
            }
            Principal::Enrollment => {
                if !matches!(c, Command::Enroll { .. }) {
                    return Err(Error::Unauthorized);
                }
            }
        }
        let value = json!(c);
        let operation = value["operation"]
            .as_str()
            .ok_or(Error::Conflict)?
            .to_owned();
        let operation = format!("{operation}:{:?}:{:?}", s.school, s.id);
        let request_hash = hash(&value.to_string());
        let principal = if let Command::Enroll { device_id, .. } = &c {
            format!("enrollment:{device_id}")
        } else {
            p.key()
        };
        let existing=sqlx::query("SELECT request_hash,response FROM mobile_delivery_operations WHERE principal=$1 AND operation=$2 AND idempotency_key=$3").bind(&principal).bind(&operation).bind(key).fetch_optional(&mut *tx).await?;
        if let Some(row) = existing {
            if row.get::<String, _>("request_hash") != request_hash {
                return Err(Error::Policy("revision_conflict"));
            }
            let mut response: Value = row.get("response");
            if matches!(c, Command::Permit { .. }) {
                invalidate_permit_claim(&mut tx, id(&s)?).await?;
                let state: String =
                    sqlx::query_scalar("SELECT state FROM mobile_send_permits WHERE claim_id=$1")
                        .bind(id(&s)?)
                        .fetch_one(&mut *tx)
                        .await?;
                response["state"] = json!(state);
                response["reason"] = json!(if state == "unknown" {
                    "unknown_requires_review"
                } else {
                    "permit_already_issued"
                });
            }
            tx.commit().await?;
            return Ok(response);
        }
        let response = Self::apply(&mut tx, &p, &s, c, enabled, origin).await?;
        let mut stored = response.clone();
        if let Some(obj) = stored.as_object_mut() {
            let mut secret = false;
            for field in ["device_token", "permit_token", "code"] {
                secret |= obj.remove(field).is_some();
            }
            if secret {
                obj.insert("secret_replay_unavailable".into(), json!(true));
            }
        }
        sqlx::query("INSERT INTO mobile_delivery_operations(principal,operation,idempotency_key,request_hash,response) VALUES($1,$2,$3,$4,$5)").bind(principal).bind(&operation).bind(key).bind(request_hash).bind(stored).execute(&mut *tx).await?;
        let action = value["operation"].as_str().ok_or(Error::Conflict)?;
        let task = if matches!(action, "draft" | "confirm" | "stop" | "preview" | "resend") {
            Some(uid(&response, "id")?)
        } else if action == "reconcile" {
            Some(uid(&response, "task_id")?)
        } else if matches!(p, Principal::Device { .. }) && s.id.is_some() {
            let (_, item) = claim_object(&mut tx, &p, id(&s)?).await?;
            Some(uid(&item, "task_id")?)
        } else {
            None
        };
        let mut audit_scope = s.clone();
        if let Some(task) = task {
            let t = object(&mut tx, "mobile_delivery_tasks", task).await?;
            audit_scope.school = Some(uid(&t, "school_id")?);
        }

        let mut detail = serde_json::Map::new();
        for field in [
            "reason",
            "evidence_ref",
            "identity_evidence_ref",
            "expected_revision",
            "daily_limit",
            "mapping_revision",
            "enabled",
            "sender_account_id",
        ] {
            if let Some(v) = value.get(field) {
                detail.insert(field.into(), v.clone());
            }
        }
        detail.insert("scope_id".into(), json!(s.id));
        detail.insert("request_digest".into(), json!(hash(&value.to_string())));
        event(
            &mut tx,
            &p,
            &audit_scope,
            task,
            action,
            Value::Object(detail),
        )
        .await?;
        sqlx::query("UPDATE mobile_delivery_tasks t SET state='completed',revision=revision+1 WHERE state='confirmed' AND NOT EXISTS(SELECT 1 FROM mobile_delivery_items i WHERE i.task_id=t.id AND i.state IN ('queued','claimed','prepared','permit_issued'))").execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(response)
    }
    async fn query(&self, p: Principal, s: Scope, q: Query, page: Page) -> Result<Value> {
        let (limit, offset) = page.fetch_bounds()?;
        let mut tx = self.0.begin().await?;
        match &p {
            Principal::Actor(a) => {
                if let Some(s) = s.school {
                    school_auth(&mut tx, *a, s, false).await?;
                } else {
                    platform(&mut tx, &p).await?;
                }
            }
            Principal::Device { .. } => {
                device(&mut tx, &p).await?;
            }
            _ => return Err(Error::Unauthorized),
        }
        let result = match q {
            Query::Devices | Query::Accounts => {
                platform(&mut tx, &p).await?;
                let table = if matches!(q, Query::Devices) {
                    "execution_devices"
                } else {
                    "sending_accounts"
                };
                json!(
                    sqlx::query_scalar::<_, Value>(&format!(
                        "SELECT to_jsonb(t) FROM {table} t ORDER BY id LIMIT $1 OFFSET $2"
                    ))
                    .bind(limit)
                    .bind(offset)
                    .fetch_all(&mut *tx)
                    .await?
                )
            }
            Query::Device | Query::AccountPolicy => {
                platform(&mut tx, &p).await?;
                object(
                    &mut tx,
                    if matches!(q, Query::Device) {
                        "execution_devices"
                    } else {
                        "sending_accounts"
                    },
                    id(&s)?,
                )
                .await?
            }
            Query::Groups | Query::Tasks => {
                p.actor()?;
                let table = if matches!(q, Query::Groups) {
                    "delivery_groups"
                } else {
                    "mobile_delivery_tasks"
                };
                json!(sqlx::query_scalar::<_,Value>(&format!("SELECT to_jsonb(t) FROM {table} t WHERE school_id=$1 ORDER BY id LIMIT $2 OFFSET $3")).bind(school(&s)?).bind(limit).bind(offset).fetch_all(&mut *tx).await?)
            }
            Query::GroupPolicy | Query::Task => {
                p.actor()?;
                scoped_object(
                    &mut tx,
                    &s,
                    if matches!(q, Query::Task) {
                        "mobile_delivery_tasks"
                    } else {
                        "delivery_groups"
                    },
                )
                .await?
            }
            Query::Settings => {
                school_auth(&mut tx, p.actor()?, school(&s)?, true).await?;
                settings(&mut tx, school(&s)?).await?
            }
            Query::Capabilities => {
                p.actor()?;
                let setting = settings(&mut tx, school(&s)?).await?;
                let server =
                    std::env::var("WEKEEPER_MOBILE_EXECUTION_ENABLED").is_ok_and(|s| s == "true");
                let reason = if !server {
                    Some("server_disabled")
                } else if setting["enabled"] != true {
                    Some("school_disabled")
                } else if setting["history_reviewed"] != true {
                    Some("history_review_required")
                } else {
                    None
                };
                let quotas:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('group_id',g.id,'daily_limit',g.daily_limit,'used',(SELECT count(*) FROM delivery_usage u WHERE u.group_id=g.id AND (u.state IN ('pending','unknown') OR (u.state='sent' AND u.observed_at>clock_timestamp()-interval '24 hours'))),'unknown',(SELECT count(*) FROM delivery_usage u WHERE u.group_id=g.id AND u.state IN ('pending','unknown'))) FROM delivery_groups g WHERE school_id=$1 ORDER BY id LIMIT 100").bind(school(&s)?).fetch_all(&mut *tx).await?;
                json!({"id":school(&s)?,"create":true,"confirm":true,"claim_enabled":reason.is_none(),"reason":reason,"quota_summary":quotas,"quota_summary_limit":100,"settings":setting,"minimum_protocol_version":PROTOCOL_VERSION,"account_default_limit":30,"account_max_limit":50,"group_default_limit":10,"group_max_limit":30})
            }
            Query::Items | Query::Events => {
                p.actor()?;
                scoped_object(&mut tx, &s, "mobile_delivery_tasks").await?;
                let table = if matches!(q, Query::Items) {
                    "mobile_delivery_items"
                } else {
                    "mobile_delivery_events"
                };
                json!(sqlx::query_scalar::<_,Value>(&format!("SELECT to_jsonb(t) FROM {table} t WHERE school_id=$1 AND task_id=$2 ORDER BY id LIMIT $3 OFFSET $4")).bind(school(&s)?).bind(id(&s)?).bind(limit).bind(offset).fetch_all(&mut *tx).await?)
            }
            Query::Session => {
                let (d, g) = device(&mut tx, &p).await?;
                let binding = match binding(&mut tx, d).await {
                    Ok(v) => Some(v),
                    Err(Error::Policy("binding_changed")) => None,
                    Err(e) => return Err(e),
                };
                json!({"id":d,"credential_generation":g,"binding":binding,"server_time":db_now(&mut tx).await?,"minimum_protocol_version":PROTOCOL_VERSION})
            }
            Query::Claim => {
                let (c, i) = claim_object(&mut tx, &p, id(&s)?).await?;
                claim_response(&mut tx, &c, &i).await?
            }
        };
        tx.commit().await?;
        Ok(result)
    }
}

/// School switch is checked inside official/legacy write transactions, not only in HTTP.
pub async fn official_allowed(tx: &mut Tx<'_>, school: Uuid) -> Result<()> {
    let enabled: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM mobile_delivery_settings WHERE school_id=$1 AND enabled) OR EXISTS(SELECT 1 FROM school_memberships m JOIN sending_accounts a ON a.responsible_actor_id=m.account_id JOIN sender_group_bindings b ON b.sender_account_id=a.id JOIN mobile_delivery_settings s ON s.school_id=b.school_id WHERE m.school_id=$1 AND s.enabled) OR EXISTS(SELECT 1 FROM delivery_usage u JOIN sending_accounts a ON a.id=u.sender_account_id JOIN school_memberships m ON m.account_id=a.responsible_actor_id WHERE m.school_id=$1 AND u.source_channel='mobile' AND (u.state IN ('pending','unknown') OR (u.state='sent' AND u.observed_at>clock_timestamp()-interval '24 hours')))",
    )
    .bind(school)
    .fetch_one(&mut **tx)
    .await?;
    if enabled {
        Err(Error::Policy("mobile_channel_exclusive"))
    } else {
        Ok(())
    }
}

impl Repository {
    pub async fn official_available(&self, school: Uuid) -> Result<bool> {
        let mut tx = self.0.begin().await?;
        match official_allowed(&mut tx, school).await {
            Ok(()) => Ok(true),
            Err(Error::Policy(_)) => Ok(false),
            Err(e) => Err(e),
        }
    }
}
