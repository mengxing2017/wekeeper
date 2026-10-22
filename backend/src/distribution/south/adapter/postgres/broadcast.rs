use crate::{
    business::{Actor, Error, Page, Result, hash, require},
    distribution::{
        domain::broadcast::*,
        south::port::broadcast::{Command, Store},
    },
};
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;
#[derive(Clone)]
pub struct Repository(pub PgPool);
type Tx<'a> = Transaction<'a, Postgres>;

async fn auth(tx: &mut Tx<'_>, actor: Uuid, school: Uuid, admin: bool) -> Result<()> {
    sqlx::query("SELECT a.id FROM accounts a JOIN school_memberships m ON m.account_id=a.id JOIN schools s ON s.id=m.school_id WHERE a.id=$1 AND m.school_id=$2 AND a.active AND s.active AND (m.role='admin' OR (NOT $3 AND m.role='operator')) FOR SHARE OF a,m,s")
        .bind(actor).bind(school).bind(admin).fetch_optional(&mut **tx).await?.ok_or(Error::Forbidden)?;
    Ok(())
}
async fn event(
    tx: &mut Tx<'_>,
    school: Uuid,
    task: Uuid,
    actor: Option<Uuid>,
    action: &str,
    detail: Value,
) -> Result<()> {
    sqlx::query("INSERT INTO broadcast_events(id,school_id,task_id,actor_id,action,detail) VALUES($1,$2,$3,$4,$5,$6)").bind(Uuid::now_v7()).bind(school).bind(task).bind(actor).bind(action).bind(detail).execute(&mut **tx).await?;
    Ok(())
}
const TASK: &str = "SELECT to_jsonb(t)||jsonb_build_object('total',(SELECT count(*) FROM broadcast_targets x WHERE x.task_id=t.id),'sent',(SELECT count(*) FROM broadcast_targets x WHERE x.task_id=t.id AND x.official_status=1),'rejected',(SELECT count(*) FROM broadcast_targets x JOIN broadcast_batches b ON b.id=x.batch_id WHERE x.task_id=t.id AND x.official_status IS DISTINCT FROM 1 AND (x.rejected OR x.official_status IN (2,3) OR b.state='rejected')),'uncertain',(SELECT count(*) FROM broadcast_batches b WHERE b.task_id=t.id AND (b.state='unknown' OR b.stop_state='unknown')),'pending',(SELECT count(*) FROM broadcast_targets x JOIN broadcast_batches b ON b.id=x.batch_id WHERE x.task_id=t.id AND (x.official_status IS NULL OR x.official_status=0) AND NOT x.rejected AND b.stop_state<>'stopped' AND b.state IN ('queued','submitting','created')),'batch_count',(SELECT count(*) FROM broadcast_batches b WHERE b.task_id=t.id),'post_ids',coalesce((SELECT jsonb_agg(p.post_id ORDER BY p.position) FROM broadcast_task_posts p WHERE p.task_id=t.id),'[]'::jsonb)) FROM broadcast_tasks t";
const WORK: &str = "SELECT to_jsonb(b)||jsonb_build_object('content',t.content,'generation',t.generation,'operation','create','operation_id',NULL,'chats',coalesce((SELECT jsonb_agg(x.chat_id ORDER BY x.chat_id) FROM broadcast_targets x WHERE x.batch_id=b.id),'[]'::jsonb)) FROM broadcast_batches b JOIN broadcast_tasks t ON t.id=b.task_id";
impl Repository {
    #[allow(clippy::too_many_arguments)] // Frozen draft plus credential scope are one transaction.
    async fn save(
        &self,
        a: &Actor,
        s: Uuid,
        id: Option<Uuid>,
        input: DraftInput,
        origin: String,
        corp: String,
        generation: i64,
    ) -> Result<Uuid> {
        input.validate()?;
        let mut tx = self.0.begin().await?;
        auth(&mut tx, a.id, s, input.source_task_id.is_some()).await?;
        let directory = sqlx::query("SELECT corp_id,credential_generation FROM wecom_directory_state WHERE singleton FOR SHARE").fetch_optional(&mut *tx).await?.ok_or(Error::Unavailable)?;
        require(
            directory.get::<String, _>("corp_id") == corp
                && directory.get::<i64, _>("credential_generation") == generation,
            "凭据已变化",
        )?;
        let task = id.unwrap_or_else(Uuid::now_v7);
        if id.is_some() {
            let old = sqlx::query("SELECT revision,state FROM broadcast_tasks WHERE school_id=$1 AND id=$2 FOR UPDATE").bind(s).bind(task).fetch_optional(&mut *tx).await?.ok_or(Error::NotFound)?;
            require(
                old.get::<String, _>("state") == "draft"
                    && input.revision == Some(old.get("revision")),
                "草稿版本冲突或已经提交",
            )?;
        }
        if let Some(source) = input.source_task_id {
            let prior = sqlx::query(
                "SELECT id FROM broadcast_tasks WHERE school_id=$1 AND id=$2 AND state='confirmed'",
            )
            .bind(s)
            .bind(source)
            .fetch_optional(&mut *tx)
            .await?;
            require(prior.is_some(), "再次发送来源无效")?;
            let allowed: Vec<Uuid> =
                sqlx::query_scalar("SELECT group_id FROM broadcast_targets WHERE task_id=$1")
                    .bind(source)
                    .fetch_all(&mut *tx)
                    .await?;
            require(
                input.group_ids.iter().all(|id| allowed.contains(id)),
                "再次发送仅允许原任务的目标群，已成功目标需明确选择并填写原因",
            )?;
        }
        if let Some(source) = input.source_draft_id {
            sqlx::query("SELECT id FROM delivery_drafts WHERE school_id=$1 AND id=$2")
                .bind(s)
                .bind(source)
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(Error::NotFound)?;
        }
        let mut revisions = Vec::new();
        let content = match input.content_source {
            ContentSource::Text => {
                normalize_text(input.text_content.as_deref().unwrap_or_default())?
            }
            ContentSource::Posts => {
                let name: String = sqlx::query_scalar("SELECT name FROM schools WHERE id=$1")
                    .bind(s)
                    .fetch_one(&mut *tx)
                    .await?;
                let body: String = sqlx::query_scalar("SELECT body FROM template_versions WHERE school_id=$1 AND id=$2 AND published FOR SHARE").bind(s).bind(input.template_version_id).fetch_optional(&mut *tx).await?.ok_or(Error::NotFound)?;
                let mut snippets = Vec::new();
                for post in &input.post_ids {
                    let p = sqlx::query("SELECT title,content,revision FROM posts WHERE school_id=$1 AND id=$2 AND audit_status='approved' AND visibility='visible' FOR SHARE").bind(s).bind(post).fetch_optional(&mut *tx).await?.ok_or(Error::Conflict)?;
                    let content: String = p.get("content");
                    snippets.push(crate::distribution::domain::render(
                        &body,
                        &std::collections::BTreeMap::from([
                            ("school.name", name.clone()),
                            ("post.title", p.get("title")),
                            ("post.summary", content.chars().take(200).collect()),
                            (
                                "post.detail_url",
                                format!("{origin}/community/{s}/posts/{post}"),
                            ),
                        ]),
                    )?);
                    revisions.push(p.get::<i64, _>("revision"));
                }
                snippets.join("\n\n")
            }
        };
        validate_content(&content)?;
        let mut groups = Vec::new();
        let mut sorted = input.group_ids.clone();
        sorted.sort();
        for group in sorted {
            let row = sqlx::query("SELECT g.id,g.chat_id,g.name,g.revision,m.userid,a.id AS account_id,a.wecom_binding_revision FROM wecom_groups g JOIN wecom_asset_objects o ON o.id=g.directory_group_id AND o.kind='groups' JOIN wecom_members m ON m.userid_key=lower(o.data->>'owner' COLLATE \"C\") JOIN accounts a ON a.wecom_member_id=m.id JOIN school_memberships sm ON sm.account_id=a.id AND sm.school_id=g.school_id WHERE g.id=$1 AND g.school_id=$2 AND g.active AND g.corp_id=$3 AND o.credential_generation=$4 AND o.verification='verified' AND o.visibility='visible' AND o.checked_at>now()-interval '24 hours' AND m.verification='verified' AND m.status=1 AND a.active AND sm.role IN ('operator','admin') FOR SHARE OF g,o,m,a,sm")
                .bind(group).bind(s).bind(&corp).bind(generation).fetch_optional(&mut *tx).await?.ok_or_else(|| Error::Invalid("群目录过期或群主未绑定本校有效运营账号，请同步并核对".into()))?;
            groups.push((row.get::<String, _>("userid"), row));
        }
        if id.is_some() {
            sqlx::query("DELETE FROM broadcast_targets WHERE task_id=$1")
                .bind(task)
                .execute(&mut *tx)
                .await?;
            sqlx::query("DELETE FROM broadcast_batches WHERE task_id=$1")
                .bind(task)
                .execute(&mut *tx)
                .await?;
            sqlx::query("DELETE FROM broadcast_task_posts WHERE task_id=$1")
                .bind(task)
                .execute(&mut *tx)
                .await?;
            sqlx::query("UPDATE broadcast_tasks SET revision=revision+1,template_version_id=$2,content=$3,content_hash=$4,corp_id=$5,generation=$6,source_task_id=$7,source_draft_id=$8,resend_reason=$9,content_source=$10 WHERE id=$1").bind(task).bind(input.template_version_id).bind(&content).bind(hash(&content)).bind(&corp).bind(generation).bind(input.source_task_id).bind(input.source_draft_id).bind(&input.reason).bind(input.content_source.as_str()).execute(&mut *tx).await?;
        } else {
            sqlx::query("INSERT INTO broadcast_tasks(id,school_id,creator_id,template_version_id,content,content_hash,corp_id,generation,source_task_id,source_draft_id,resend_reason,content_source) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)").bind(task).bind(s).bind(a.id).bind(input.template_version_id).bind(&content).bind(hash(&content)).bind(&corp).bind(generation).bind(input.source_task_id).bind(input.source_draft_id).bind(&input.reason).bind(input.content_source.as_str()).execute(&mut *tx).await?;
        }
        for (position, (post, revision)) in input.post_ids.iter().zip(revisions).enumerate() {
            sqlx::query("INSERT INTO broadcast_task_posts VALUES($1,$2,$3,$4,$5)")
                .bind(s)
                .bind(task)
                .bind(post)
                .bind(revision)
                .bind(position as i32)
                .execute(&mut *tx)
                .await?;
        }
        for (ordinal, (owner, rows)) in chunks(groups).into_iter().enumerate() {
            let batch = Uuid::now_v7();
            let first = &rows[0];
            sqlx::query("INSERT INTO broadcast_batches(id,school_id,task_id,ordinal,sender,sender_account,binding_revision,corp_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(batch).bind(s).bind(task).bind(ordinal as i32).bind(owner).bind(first.get::<Uuid,_>("account_id")).bind(first.get::<i64,_>("wecom_binding_revision")).bind(&corp).execute(&mut *tx).await?;
            for row in rows {
                sqlx::query("INSERT INTO broadcast_targets(id,school_id,task_id,batch_id,group_id,chat_id,group_name,group_revision) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(Uuid::now_v7()).bind(s).bind(task).bind(batch).bind(row.get::<Uuid,_>("id")).bind(row.get::<String,_>("chat_id")).bind(row.get::<String,_>("name")).bind(row.get::<i64,_>("revision")).execute(&mut *tx).await?;
            }
        }
        event(&mut tx, s, task, Some(a.id), "draft.saved", json!({})).await?;
        tx.commit().await?;
        Ok(task)
    }
    async fn validate(tx: &mut Tx<'_>, task: Uuid) -> Result<()> {
        let t = sqlx::query("SELECT * FROM broadcast_tasks WHERE id=$1")
            .bind(task)
            .fetch_one(&mut **tx)
            .await?;
        let s: Uuid = t.get("school_id");
        let actor: Uuid = t
            .get::<Option<Uuid>, _>("confirmed_by")
            .unwrap_or(t.get("creator_id"));
        auth(
            tx,
            actor,
            s,
            t.get::<String, _>("content_source") == "text"
                && t.get::<Option<Uuid>, _>("confirmed_by").is_some(),
        )
        .await?;
        sqlx::query("SELECT singleton FROM wecom_directory_state WHERE corp_id=$1 AND credential_generation=$2 FOR SHARE").bind(t.get::<String,_>("corp_id")).bind(t.get::<i64,_>("generation")).fetch_optional(&mut **tx).await?.ok_or(Error::Conflict)?;
        let posts=sqlx::query("SELECT p.id FROM posts p JOIN broadcast_task_posts x ON x.post_id=p.id WHERE x.task_id=$1 AND p.school_id=x.school_id AND p.revision=x.post_revision AND p.audit_status='approved' AND p.visibility='visible' FOR SHARE OF p").bind(task).fetch_all(&mut **tx).await?;
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM broadcast_task_posts WHERE task_id=$1")
                .bind(task)
                .fetch_one(&mut **tx)
                .await?;
        require(
            match t.get::<String, _>("content_source").as_str() {
                "text" => {
                    count == 0
                        && t.get::<Option<Uuid>, _>("template_version_id").is_none()
                        && normalize_text(&t.get::<String, _>("content"))
                            .is_ok_and(|body| body == t.get::<String, _>("content"))
                }
                "posts" => count > 0 && posts.len() as i64 == count,
                _ => false,
            },
            "内容已变化，请重新预览",
        )?;
        let groups=sqlx::query("SELECT g.id FROM broadcast_targets x JOIN broadcast_batches b ON b.id=x.batch_id JOIN wecom_groups g ON g.id=x.group_id JOIN wecom_asset_objects o ON o.id=g.directory_group_id JOIN accounts a ON a.id=b.sender_account JOIN wecom_members m ON m.id=a.wecom_member_id JOIN school_memberships sm ON sm.account_id=a.id AND sm.school_id=x.school_id WHERE x.task_id=$1 AND g.school_id=x.school_id AND g.active AND g.revision=x.group_revision AND g.chat_id=x.chat_id AND g.corp_id=b.corp_id AND o.credential_generation=$2 AND o.verification='verified' AND o.visibility='visible' AND o.checked_at>now()-interval '24 hours' AND lower(o.data->>'owner' COLLATE \"C\")=m.userid_key AND lower(b.sender COLLATE \"C\")=m.userid_key AND a.active AND a.wecom_binding_revision=b.binding_revision AND m.verification='verified' AND m.status=1 AND sm.role IN ('operator','admin') ORDER BY g.id FOR UPDATE OF g FOR SHARE OF a,m,sm,o")
            .bind(task).bind(t.get::<i64,_>("generation")).fetch_all(&mut **tx).await?;
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM broadcast_targets WHERE task_id=$1")
                .bind(task)
                .fetch_one(&mut **tx)
                .await?;
        require(
            count > 0 && groups.len() as i64 == count,
            "目标群、群主或授权已变化，请重新预览",
        )?;
        Ok(())
    }
    async fn duplicates(tx: &mut Tx<'_>, task: Uuid, actor: Uuid) -> Result<()> {
        let t = sqlx::query("SELECT * FROM broadcast_tasks WHERE id=$1")
            .bind(task)
            .fetch_one(&mut **tx)
            .await?;
        let s: Uuid = t.get("school_id");
        let prior=sqlx::query("SELECT DISTINCT p.id,b.state,b.stop_state,b.reconciliation FROM broadcast_tasks p JOIN broadcast_targets x ON x.task_id=p.id JOIN broadcast_targets target ON target.group_id=x.group_id AND target.task_id=$1 JOIN broadcast_batches b ON b.id=x.batch_id WHERE p.id<>$1 AND p.school_id=$2 AND p.state='confirmed' AND (p.content_hash=$3 OR EXISTS(SELECT 1 FROM broadcast_task_posts a JOIN broadcast_task_posts z ON z.post_id=a.post_id WHERE a.task_id=p.id AND z.task_id=$1))")
            .bind(task).bind(s).bind(t.get::<String,_>("content_hash")).fetch_all(&mut **tx).await?;
        let old=sqlx::query("SELECT DISTINCT a.id,a.state,a.reconciliation,d.id AS draft_id FROM delivery_attempts a JOIN delivery_drafts d ON d.id=a.draft_id JOIN broadcast_targets target ON target.group_id=d.group_id AND target.task_id=$1 WHERE d.school_id=$2 AND (d.content_hash=$3 OR EXISTS(SELECT 1 FROM delivery_draft_posts x JOIN broadcast_task_posts y ON y.post_id=x.post_id WHERE x.draft_id=d.id AND y.task_id=$1))")
            .bind(task).bind(s).bind(t.get::<String,_>("content_hash")).fetch_all(&mut **tx).await?;
        for p in &prior {
            let state: String = p.get("state");
            let stop: String = p.get("stop_state");
            let reconciliation: Option<String> = p.get("reconciliation");
            require(
                !matches!(state.as_str(), "queued" | "submitting")
                    && !(state == "unknown" && reconciliation.as_deref() != Some("not_created"))
                    && !(state == "created" && stop != "stopped")
                    && stop != "unknown",
                "相关内容仍有待执行或不确定任务，先停止并核对",
            )?;
        }
        for p in &old {
            let state: String = p.get("state");
            let decision: Option<String> = p.get("reconciliation");
            require(
                state != "prepared" && !(state == "unknown" && decision.is_none()),
                "历史人工分享仍待核对",
            )?;
        }
        if !prior.is_empty() || !old.is_empty() {
            auth(tx, actor, s, true).await?;
            require(
                !t.get::<String, _>("resend_reason").trim().is_empty(),
                "再次发送须填写原因",
            )?;
            let source: Option<Uuid> = t.get("source_task_id");
            let draft: Option<Uuid> = t.get("source_draft_id");
            require(
                prior.is_empty() || prior.iter().any(|p| Some(p.get::<Uuid, _>("id")) == source),
                "请从原群发任务创建再次发送草稿",
            )?;
            require(
                old.is_empty()
                    || old
                        .iter()
                        .any(|p| Some(p.get::<Uuid, _>("draft_id")) == draft),
                "请引用历史草稿并填写再次发送原因",
            )?;
        }
        Ok(())
    }
    async fn action(
        &self,
        a: &Actor,
        s: Uuid,
        id: Uuid,
        operation: String,
        key: String,
        input: Action,
    ) -> Result<()> {
        let mut tx = self.0.begin().await?;
        auth(
            &mut tx,
            a.id,
            s,
            matches!(operation.as_str(), "stop" | "reconcile"),
        )
        .await?;
        let t =
            sqlx::query("SELECT * FROM broadcast_tasks WHERE school_id=$1 AND id=$2 FOR UPDATE")
                .bind(s)
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(Error::NotFound)?;
        if operation == "confirm" && t.get::<String, _>("content_source") == "text" {
            auth(&mut tx, a.id, s, true).await?;
        }
        let request_hash = hash(&json!({"task":id,"input":input}).to_string());
        // Also serialize keys used on different tasks, not just double clicks on one task.
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
            .bind(format!("broadcast:{}:{s}:{operation}:{key}", a.id))
            .execute(&mut *tx)
            .await?;
        if let Some(prior)=sqlx::query("SELECT task_id,request_hash FROM broadcast_operations WHERE actor_id=$1 AND school_id=$2 AND operation=$3 AND idempotency_key=$4").bind(a.id).bind(s).bind(&operation).bind(&key).fetch_optional(&mut *tx).await? {
            if prior.get::<Uuid,_>("task_id")!=id || prior.get::<String,_>("request_hash")!=request_hash {return Err(Error::Conflict);}
            return Ok(());
        }
        match operation.as_str() {
            "confirm" => {
                require(
                    t.get::<String, _>("state") == "draft"
                        && input.revision == Some(t.get("revision")),
                    "草稿已提交或版本冲突",
                )?;
                Self::validate(&mut tx, id).await?;
                Self::duplicates(&mut tx, id, a.id).await?;
                sqlx::query("UPDATE broadcast_tasks SET state='confirmed',confirmed_by=$2,confirmed_at=now() WHERE id=$1").bind(id).bind(a.id).execute(&mut *tx).await?;
            }
            "abandon" => {
                require(t.get::<String, _>("state") == "draft", "只能放弃草稿")?;
                sqlx::query(
                    "UPDATE broadcast_tasks SET state='abandoned',revision=revision+1 WHERE id=$1",
                )
                .bind(id)
                .execute(&mut *tx)
                .await?;
            }
            "refresh" => {
                sqlx::query("UPDATE broadcast_batches SET next_sync_at=least(next_sync_at,now()) WHERE task_id=$1 AND msgid IS NOT NULL").bind(id).execute(&mut *tx).await?;
            }
            "remind" | "stop" => {
                require(t.get::<String, _>("state") == "confirmed", "任务尚未确认")?;
                if operation == "stop" {
                    sqlx::query("UPDATE broadcast_operations SET state='rejected',error_code='task_stopped' WHERE task_id=$1 AND operation='remind' AND state='pending'").bind(id).execute(&mut *tx).await?;
                    sqlx::query("UPDATE broadcast_tasks SET stopped=true WHERE id=$1")
                        .bind(id)
                        .execute(&mut *tx)
                        .await?;
                    sqlx::query("UPDATE broadcast_batches SET state='cancelled' WHERE task_id=$1 AND state='queued'").bind(id).execute(&mut *tx).await?;
                } else {
                    require(!t.get::<bool, _>("stopped"), "任务已停止")?;
                }
                let batches=sqlx::query("SELECT id,member_status,stop_state FROM broadcast_batches WHERE task_id=$1 AND state='created' AND ($2::uuid IS NULL OR id=$2) ORDER BY id FOR UPDATE").bind(id).bind(input.batch_id).fetch_all(&mut *tx).await?;
                let mut queued = 0;
                for b in batches {
                    let batch: Uuid = b.get("id");
                    let stop: String = b.get("stop_state");
                    if operation == "remind"
                        && (b.get::<Option<i32>, _>("member_status") == Some(2) || stop != "none")
                    {
                        continue;
                    }
                    if operation == "stop"
                        && matches!(stop.as_str(), "stopped" | "requested" | "unknown")
                    {
                        continue;
                    }
                    let pending:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM broadcast_operations WHERE batch_id=$1 AND operation=$2 AND state IN ('pending','calling','unknown'))").bind(batch).bind(&operation).fetch_one(&mut *tx).await?;
                    require(!pending, "已有执行中或不确定的操作，请先核对")?;
                    if operation == "remind" {
                        let count:i64=sqlx::query_scalar("SELECT count(*) FROM broadcast_operations WHERE batch_id=$1 AND operation='remind' AND created_at>now()-interval '24 hours'").bind(batch).fetch_one(&mut *tx).await?;
                        require(count < 3, "每个批次 24 小时最多提醒三次")?;
                    } else {
                        sqlx::query(
                            "UPDATE broadcast_batches SET stop_state='requested' WHERE id=$1",
                        )
                        .bind(batch)
                        .execute(&mut *tx)
                        .await?;
                    }
                    sqlx::query("INSERT INTO broadcast_operations(id,school_id,task_id,batch_id,actor_id,operation,idempotency_key,request_hash) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(Uuid::now_v7()).bind(s).bind(id).bind(batch).bind(a.id).bind(&operation).bind(format!("child:{key}:{batch}")).bind(&request_hash).execute(&mut *tx).await?;
                    queued += 1;
                }
                if operation == "remind" {
                    require(queued > 0, "没有可提醒的成员任务")?;
                }
            }
            "reconcile" => {
                require(
                    !input.reason.trim().is_empty() && input.reason.chars().count() <= 1000,
                    "请填写核对依据（最多 1000 字）",
                )?;
                let batch = input.batch_id.ok_or(Error::Conflict)?;
                let b=sqlx::query("SELECT state,stop_state FROM broadcast_batches WHERE task_id=$1 AND id=$2 FOR UPDATE").bind(id).bind(batch).fetch_optional(&mut *tx).await?.ok_or(Error::NotFound)?;
                let decision = input.decision.as_deref().unwrap_or("");
                if matches!(decision, "stop_confirmed" | "reminder_reviewed") {
                    let operation = if decision == "stop_confirmed" {
                        "stop"
                    } else {
                        "remind"
                    };
                    let n=sqlx::query("UPDATE broadcast_operations SET state='done' WHERE batch_id=$1 AND operation=$2 AND state='unknown'").bind(batch).bind(operation).execute(&mut *tx).await?.rows_affected();
                    require(n > 0, "没有待核对的提醒或停止操作")?;
                    if decision == "stop_confirmed" {
                        sqlx::query("UPDATE broadcast_batches SET stop_state='stopped' WHERE id=$1 AND stop_state='unknown'").bind(batch).execute(&mut *tx).await?;
                    }
                } else {
                    require(
                        b.get::<String, _>("state") == "unknown",
                        "只能核对创建结果不确定的批次",
                    )?;
                    require(
                        matches!(decision, "not_created" | "sent" | "linked"),
                        "核对结论无效",
                    )?;
                    if decision == "linked" {
                        let msgid = input
                            .msgid
                            .as_deref()
                            .filter(|s| !s.is_empty() && s.len() <= 512)
                            .ok_or(Error::Conflict)?;
                        sqlx::query("UPDATE broadcast_batches SET state='created',msgid=$2,next_sync_at=now() WHERE id=$1").bind(batch).bind(msgid).execute(&mut *tx).await?;
                    }
                    sqlx::query("UPDATE broadcast_batches SET reconciliation=$2,reconciliation_note=$3 WHERE id=$1").bind(batch).bind(decision).bind(&input.reason).execute(&mut *tx).await?;
                }
            }
            _ => return Err(Error::NotFound),
        }
        sqlx::query("INSERT INTO broadcast_operations(id,school_id,task_id,actor_id,operation,idempotency_key,request_hash,state) VALUES($1,$2,$3,$4,$5,$6,$7,'done')").bind(Uuid::now_v7()).bind(s).bind(id).bind(a.id).bind(&operation).bind(key).bind(request_hash).execute(&mut *tx).await?;
        event(&mut tx, s, id, Some(a.id), &operation, json!(input)).await?;
        crate::audit::record(
            &mut tx,
            a,
            s,
            &format!("broadcast.{operation}"),
            id,
            json!({}),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
}
impl Store for Repository {
    async fn authorize(&self, actor: Uuid, school: Uuid, admin: bool) -> Result<()> {
        let mut tx = self.0.begin().await?;
        auth(&mut tx, actor, school, admin).await
    }
    async fn execute(&self, a: &Actor, s: Uuid, command: Command) -> Result<Task> {
        let id = match command {
            Command::Save {
                id,
                input,
                origin,
                corp,
                generation,
            } => self.save(a, s, id, input, origin, corp, generation).await?,
            Command::Action {
                id,
                operation,
                key,
                input,
            } => {
                self.action(a, s, id, operation, key, input).await?;
                id
            }
        };
        self.task(s, id).await
    }
    async fn task(&self, s: Uuid, id: Uuid) -> Result<Task> {
        decode(
            sqlx::query_scalar(&format!("{TASK} WHERE t.school_id=$1 AND t.id=$2"))
                .bind(s)
                .bind(id)
                .fetch_optional(&self.0)
                .await?
                .ok_or(Error::NotFound)?,
        )
    }
    async fn list(&self, s: Uuid, page: Page) -> Result<Vec<Task>> {
        let (limit, offset) = page.fetch_bounds()?;
        let rows:Vec<Value>=sqlx::query_scalar(&format!("{TASK} WHERE t.school_id=$1 AND ($4::text IS NULL OR t.state=$4 OR ($4='pending' AND t.state='confirmed' AND NOT t.stopped AND EXISTS(SELECT 1 FROM broadcast_targets x JOIN broadcast_batches b ON b.id=x.batch_id WHERE x.task_id=t.id AND b.state IN ('queued','submitting','created') AND NOT x.rejected AND (x.official_status IS NULL OR x.official_status=0))) OR ($4='issues' AND t.state='confirmed' AND EXISTS(SELECT 1 FROM broadcast_targets x JOIN broadcast_batches b ON b.id=x.batch_id WHERE x.task_id=t.id AND (x.rejected OR x.official_status IN (2,3) OR b.state IN ('rejected','unknown') OR b.sync_error IS NOT NULL OR b.stop_state IN ('unknown','rejected'))))) ORDER BY t.created_at DESC,t.id DESC LIMIT $2 OFFSET $3")).bind(s).bind(limit).bind(offset).bind(page.status.filter(|x|!x.is_empty())).fetch_all(&self.0).await?;
        rows.into_iter().map(decode).collect()
    }
    async fn rows(&self, s: Uuid, id: Uuid, kind: &str, page: Page) -> Result<Vec<Value>> {
        self.task(s, id).await?;
        let (limit, offset) = page.fetch_bounds()?;
        let sql = match kind {
            "targets" => {
                "SELECT to_jsonb(x)||jsonb_build_object('sender',b.sender,'creation_state',b.state,'msgid',b.msgid,'member_status',b.member_status,'last_synced_at',b.last_synced_at,'sync_error',b.sync_error,'error_code',b.error_code,'stop_state',b.stop_state,'reconciliation',b.reconciliation,'operation_unknown',EXISTS(SELECT 1 FROM broadcast_operations o WHERE o.batch_id=b.id AND o.state='unknown')) FROM broadcast_targets x JOIN broadcast_batches b ON b.id=x.batch_id WHERE x.school_id=$1 AND x.task_id=$2 ORDER BY b.ordinal,x.group_name,x.id LIMIT $3 OFFSET $4"
            }
            "events" => {
                "SELECT to_jsonb(e) FROM broadcast_events e WHERE e.school_id=$1 AND e.task_id=$2 ORDER BY e.created_at DESC,e.id DESC LIMIT $3 OFFSET $4"
            }
            _ => return Err(Error::NotFound),
        };
        let rows: Vec<Value> = sqlx::query_scalar(sql)
            .bind(s)
            .bind(id)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.0)
            .await?;
        if kind == "targets" {
            rows.into_iter()
                .map(|v| Ok(json!(decode::<Target>(v)?)))
                .collect()
        } else {
            Ok(rows)
        }
    }
    async fn work(&self, task: Option<Uuid>) -> Result<Vec<Work>> {
        // Recovery never returns a previously claimed write to the queue.
        sqlx::query("UPDATE broadcast_batches SET state='unknown',error_code='interrupted' WHERE state='submitting' AND claimed_at<now()-interval '2 minutes'").execute(&self.0).await?;
        sqlx::query("UPDATE broadcast_operations SET state='unknown',error_code='interrupted' WHERE state='calling' AND claimed_at<now()-interval '2 minutes'").execute(&self.0).await?;
        sqlx::query("UPDATE broadcast_batches b SET stop_state='unknown' WHERE stop_state='requested' AND EXISTS(SELECT 1 FROM broadcast_operations o WHERE o.batch_id=b.id AND o.operation='stop' AND o.state='unknown')").execute(&self.0).await?;
        // A stop requested while creation was in flight must also cover its late msgid.
        sqlx::query("INSERT INTO broadcast_operations(id,school_id,task_id,batch_id,actor_id,operation,idempotency_key,request_hash) SELECT gen_random_uuid(),b.school_id,b.task_id,b.id,o.actor_id,'stop','auto-stop:'||b.id::text,'late-created-stop' FROM broadcast_batches b JOIN broadcast_tasks t ON t.id=b.task_id JOIN LATERAL (SELECT actor_id FROM broadcast_operations WHERE task_id=t.id AND operation='stop' AND batch_id IS NULL ORDER BY created_at DESC LIMIT 1) o ON true WHERE t.stopped AND b.state='created' AND b.stop_state='none' ON CONFLICT(actor_id,school_id,operation,idempotency_key) DO NOTHING").execute(&self.0).await?;
        sqlx::query("UPDATE broadcast_batches b SET stop_state='requested' WHERE b.stop_state='none' AND EXISTS(SELECT 1 FROM broadcast_operations o WHERE o.batch_id=b.id AND o.operation='stop' AND o.state='pending')").execute(&self.0).await?;
        let sql = if task.is_some() {
            format!("{WORK} WHERE t.id=$1 ORDER BY b.ordinal")
        } else {
            format!(
                "{WORK} WHERE t.state='confirmed' AND ((b.state='queued' AND NOT t.stopped) OR (b.msgid IS NOT NULL AND b.next_sync_at<=now())) ORDER BY CASE WHEN b.msgid IS NOT NULL THEN 0 ELSE 1 END,b.next_sync_at,b.id LIMIT 20"
            )
        };
        let rows: Vec<Value> = if let Some(task) = task {
            sqlx::query_scalar(&sql)
                .bind(task)
                .fetch_all(&self.0)
                .await?
        } else {
            sqlx::query_scalar(&sql).fetch_all(&self.0).await?
        };
        let mut out: Vec<Work> = rows.into_iter().map(decode).collect::<Result<_>>()?;
        if task.is_none() {
            for w in &mut out {
                if w.msgid.is_some() {
                    w.operation = "read".into();
                }
            }
            let rows:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(b)||jsonb_build_object('content',t.content,'generation',t.generation,'operation',o.operation,'operation_id',o.id,'chats',(SELECT jsonb_agg(x.chat_id) FROM broadcast_targets x WHERE x.batch_id=b.id)) FROM broadcast_operations o JOIN broadcast_batches b ON b.id=o.batch_id JOIN broadcast_tasks t ON t.id=b.task_id WHERE o.state='pending' ORDER BY o.created_at LIMIT 20").fetch_all(&self.0).await?;
            out.extend(rows.into_iter().map(decode).collect::<Result<Vec<_>>>()?);
        }
        Ok(out)
    }
    async fn claim(&self, w: &Work) -> Result<bool> {
        let mut tx = self.0.begin().await?;
        // Task lock serializes edits, confirmation, stopping, and all batch claims.
        let t = sqlx::query("SELECT * FROM broadcast_tasks WHERE id=$1 FOR UPDATE")
            .bind(w.task_id)
            .fetch_one(&mut *tx)
            .await?;
        if w.operation == "read" {
            let n=sqlx::query("UPDATE broadcast_batches SET next_sync_at=now()+interval '5 minutes' WHERE id=$1 AND next_sync_at<=now() AND msgid=$2").bind(w.id).bind(&w.msgid).execute(&mut *tx).await?.rows_affected();
            tx.commit().await?;
            return Ok(n == 1);
        }
        sqlx::query("SELECT singleton FROM wecom_directory_state WHERE corp_id=$1 AND credential_generation=$2 FOR SHARE").bind(&w.corp_id).bind(w.generation).fetch_optional(&mut *tx).await?.ok_or(Error::Conflict)?;
        if w.operation == "create" {
            if t.get::<bool, _>("stopped") || t.get::<String, _>("state") != "confirmed" {
                return Ok(false);
            }
            let state: String =
                sqlx::query_scalar("SELECT state FROM broadcast_batches WHERE id=$1")
                    .bind(w.id)
                    .fetch_one(&mut *tx)
                    .await?;
            if state != "queued" {
                return Ok(false);
            }
            Self::validate(&mut tx, w.task_id).await?;
            let n=sqlx::query("UPDATE broadcast_batches SET state='submitting',invocation_id=$2,claimed_at=now() WHERE id=$1 AND state='queued' AND invocation_id IS NULL").bind(w.id).bind(Uuid::now_v7()).execute(&mut *tx).await?.rows_affected();
            event(
                &mut tx,
                w.school_id,
                w.task_id,
                None,
                "create.claimed",
                json!({"batch_id":w.id}),
            )
            .await?;
            tx.commit().await?;
            return Ok(n == 1);
        }
        if w.operation == "remind" && t.get::<bool, _>("stopped") {
            return Ok(false);
        }
        let op = w.operation_id.ok_or(Error::Conflict)?;
        let o =
            sqlx::query("SELECT actor_id,state FROM broadcast_operations WHERE id=$1 FOR UPDATE")
                .bind(op)
                .fetch_one(&mut *tx)
                .await?;
        if o.get::<String, _>("state") != "pending" {
            return Ok(false);
        }
        auth(
            &mut tx,
            o.get("actor_id"),
            w.school_id,
            w.operation == "stop",
        )
        .await?;
        let n=sqlx::query("UPDATE broadcast_operations SET state='calling',claimed_at=now() WHERE id=$1 AND state='pending'").bind(op).execute(&mut *tx).await?.rows_affected();
        tx.commit().await?;
        Ok(n == 1)
    }
    async fn finish(&self, w: &Work, outcome: Outcome) -> Result<()> {
        let mut tx = self.0.begin().await?;
        sqlx::query("SELECT id FROM broadcast_tasks WHERE id=$1 FOR UPDATE")
            .bind(w.task_id)
            .fetch_one(&mut *tx)
            .await?;
        let (state, error) = match &outcome {
            Outcome::Created { .. } | Outcome::Done => ("created", None),
            Outcome::Rejected(code) => ("rejected", Some(code.clone())),
            Outcome::Unknown => ("unknown", Some("response_unknown".into())),
        };
        if matches!(w.operation.as_str(), "create" | "preflight") {
            // Rejected preflight is safe only while unclaimed; after claim all ambiguous failures stay unknown.
            let current = sqlx::query(
                "SELECT state,reconciliation FROM broadcast_batches WHERE id=$1 FOR UPDATE",
            )
            .bind(w.id)
            .fetch_one(&mut *tx)
            .await?;
            if w.operation == "preflight" && current.get::<String, _>("state") != "queued" {
                return Ok(());
            }
            if !matches!(
                current.get::<String, _>("state").as_str(),
                "queued" | "submitting" | "unknown"
            ) {
                return Ok(());
            }
            let msgid = if let Outcome::Created { msgid, .. } = &outcome {
                Some(msgid)
            } else {
                None
            };
            sqlx::query("UPDATE broadcast_batches SET state=$2,error_code=$3,msgid=$4,next_sync_at=now() WHERE id=$1").bind(w.id).bind(state).bind(&error).bind(msgid).execute(&mut *tx).await?;
            if let Outcome::Created { failed, .. } = &outcome {
                sqlx::query("UPDATE broadcast_targets SET rejected=true WHERE batch_id=$1 AND chat_id=ANY($2)").bind(w.id).bind(failed).execute(&mut *tx).await?;
            }
        } else {
            let op = w.operation_id.ok_or(Error::Conflict)?;
            let old: String =
                sqlx::query_scalar("SELECT state FROM broadcast_operations WHERE id=$1 FOR UPDATE")
                    .bind(op)
                    .fetch_one(&mut *tx)
                    .await?;
            if matches!(&outcome, Outcome::Rejected(code) if code=="preflight_failed" || code=="authorization_or_snapshot_changed")
                && old != "pending"
            {
                return Ok(());
            }
            sqlx::query("UPDATE broadcast_operations SET state=$2,error_code=$3 WHERE id=$1 AND state IN ('pending','calling','unknown')").bind(op).bind(if state=="created"{"done"}else{state}).bind(&error).execute(&mut *tx).await?;
            if w.operation == "stop" {
                sqlx::query(
                    "UPDATE broadcast_batches SET stop_state=$2,next_sync_at=now() WHERE id=$1",
                )
                .bind(w.id)
                .bind(if state == "created" { "stopped" } else { state })
                .execute(&mut *tx)
                .await?;
            }
        }
        event(
            &mut tx,
            w.school_id,
            w.task_id,
            None,
            &format!("{}.result", w.operation),
            json!({"batch_id":w.id,"state":state,"error_code":error}),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
    async fn observed(&self, w: &Work, results: Result<Results>) -> Result<()> {
        let mut tx = self.0.begin().await?;
        // A stale credential-scope read may not publish into a newer directory generation.
        if sqlx::query("SELECT singleton FROM wecom_directory_state WHERE corp_id=$1 AND credential_generation=$2 FOR SHARE").bind(&w.corp_id).bind(w.generation).fetch_optional(&mut *tx).await?.is_none() {
            sqlx::query("UPDATE broadcast_batches SET sync_error='credential_generation_changed' WHERE id=$1").bind(w.id).execute(&mut *tx).await?;
            tx.commit().await?; return Ok(());
        }
        match results {
            Ok(results) => {
                sqlx::query("UPDATE broadcast_batches SET member_status=$2,last_synced_at=now(),sync_error=NULL WHERE id=$1 AND msgid=$3").bind(w.id).bind(results.member_status).bind(&w.msgid).execute(&mut *tx).await?;
                for r in results.groups {
                    // A late/paginated observation cannot erase an existing official success.
                    sqlx::query("UPDATE broadcast_targets SET official_status=$3,send_time=$4 WHERE batch_id=$1 AND chat_id=$2 AND (official_status IS DISTINCT FROM 1 OR $3=1)").bind(w.id).bind(r.chat_id).bind(r.status).bind(r.send_time).execute(&mut *tx).await?;
                }
            }
            Err(_) => {
                sqlx::query(
                    "UPDATE broadcast_batches SET sync_error='sync_incomplete' WHERE id=$1",
                )
                .bind(w.id)
                .execute(&mut *tx)
                .await?;
            }
        }
        tx.commit().await?;
        Ok(())
    }
}
