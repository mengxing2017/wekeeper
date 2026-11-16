//! Platform-owned content, qualification, preview and resource persistence.
use crate::{
    business::{Error, Result, hash, require},
    distribution::domain::{broadcast::Work, content::Preview},
};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;
type Tx<'a> = Transaction<'a, Postgres>;
pub async fn save_snapshot(tx: &mut Tx<'_>, task: Uuid, content: &str) -> Result<()> {
    let snapshot = Uuid::now_v7();
    sqlx::query("INSERT INTO distribution_message_snapshots(id,channel,content,content_hash,source_references) VALUES($1,'wecom',$2,$3,$4)")
        .bind(snapshot).bind(content).bind(hash(content)).bind(serde_json::json!([])).execute(&mut **tx).await?;
    sqlx::query("INSERT INTO broadcast_task_snapshots SELECT id,revision,$2 FROM broadcast_tasks WHERE id=$1").bind(task).bind(snapshot).execute(&mut **tx).await?;
    sqlx::query("DELETE FROM broadcast_previews WHERE task_id=$1")
        .bind(task)
        .execute(&mut **tx)
        .await?;
    Ok(())
}
pub async fn validate_task(tx: &mut Tx<'_>, task: Uuid) -> Result<()> {
    let row=sqlx::query("SELECT t.state,t.content_source,t.source_namespace,t.authorization_kind,t.content,s.enabled,s.max_pending FROM broadcast_tasks t JOIN distribution_sources s ON s.namespace=t.source_namespace WHERE t.id=$1 FOR SHARE OF s")
        .bind(task).fetch_one(&mut **tx).await?;
    let frozen = row.get::<String, _>("state") == "confirmed";
    if !frozen {
        require(row.get("enabled"), "内容来源已暂停")?;
    }
    let namespace: String = row.get("source_namespace");
    let source_route = namespace == "builtin.source-route"
        && row.get::<String, _>("authorization_kind") == "source_route"
        && sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM orchestration_delegates WHERE task_id=$1 AND channel='wecom')")
            .bind(task).fetch_one(&mut **tx).await?;
    require(
        row.get::<String, _>("content_source") == "text"
            && (namespace == "builtin.direct" || source_route),
        "来源不匹配",
    )?;
    let matches:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM broadcast_tasks t JOIN broadcast_task_snapshots l ON l.task_id=t.id AND l.task_revision=t.revision JOIN distribution_message_snapshots s ON s.id=l.snapshot_id WHERE t.id=$1 AND s.content=t.content AND s.content_hash=t.content_hash AND s.channel='wecom')").bind(task).fetch_one(&mut **tx).await?;
    require(matches, "消息快照不匹配")
}
async fn digest(tx: &mut Tx<'_>, task: Uuid) -> Result<(i64, String, String, i64)> {
    let row=sqlx::query("SELECT t.revision,t.content,(SELECT count(*) FROM broadcast_targets WHERE task_id=t.id) AS total,jsonb_build_object('task',t.id,'revision',t.revision,'content',t.content,'corp',t.corp_id,'generation',t.generation,'source',t.source_namespace,'source_policy',(SELECT to_jsonb(s) FROM distribution_sources s WHERE s.namespace=t.source_namespace),'resource_policies',(SELECT jsonb_agg(to_jsonb(p) ORDER BY p.resource_key) FROM distribution_resource_policies p),'targets',(SELECT jsonb_agg(to_jsonb(x) ORDER BY x.id) FROM broadcast_targets x WHERE x.task_id=t.id))::text AS payload FROM broadcast_tasks t WHERE id=$1")
        .bind(task).fetch_one(&mut **tx).await?;
    Ok((
        row.get("revision"),
        hash(&row.get::<String, _>("payload")),
        row.get("content"),
        row.get("total"),
    ))
}
pub async fn preview(tx: &mut Tx<'_>, task: Uuid) -> Result<Preview> {
    let (revision, digest, content, total) = digest(tx, task).await?;
    let expires:chrono::DateTime<chrono::Utc>=sqlx::query_scalar("INSERT INTO broadcast_previews VALUES($1,$2,$3,now()+interval '10 minutes') ON CONFLICT(task_id) DO UPDATE SET revision=excluded.revision,digest=excluded.digest,expires_at=excluded.expires_at RETURNING expires_at")
        .bind(task).bind(revision).bind(&digest).fetch_one(&mut **tx).await?;
    Ok(Preview {
        id: task,
        revision,
        digest,
        expires_at: expires.to_rfc3339(),
        content,
        target_count: total,
        channel: "wecom".into(),
    })
}
pub async fn check_preview(tx: &mut Tx<'_>, task: Uuid, expected: Option<&str>) -> Result<()> {
    let (revision, digest, _, _) = digest(tx, task).await?;
    require(expected == Some(digest.as_str()), "请先重新预览并确认摘要")?;
    sqlx::query("SELECT task_id FROM broadcast_previews WHERE task_id=$1 AND revision=$2 AND digest=$3 AND expires_at>clock_timestamp() FOR SHARE")
        .bind(task).bind(revision).bind(digest).fetch_optional(&mut **tx).await?.ok_or(Error::Policy("preview_expired"))?;
    let pending:i64=sqlx::query_scalar("SELECT count(*) FROM broadcast_batches b JOIN broadcast_tasks t ON t.id=b.task_id WHERE t.source_namespace=(SELECT source_namespace FROM broadcast_tasks WHERE id=$1) AND t.state='confirmed' AND b.state IN ('queued','submitting','unknown')").bind(task).fetch_one(&mut **tx).await?;
    let incoming: i64 =
        sqlx::query_scalar("SELECT count(*) FROM broadcast_batches WHERE task_id=$1")
            .bind(task)
            .fetch_one(&mut **tx)
            .await?;
    let limit:i32=sqlx::query_scalar("SELECT max_pending FROM distribution_sources WHERE namespace=(SELECT source_namespace FROM broadcast_tasks WHERE id=$1)").bind(task).fetch_one(&mut **tx).await?;
    require(
        pending + incoming <= i64::from(limit),
        "来源待执行量已达限制",
    )
}
pub async fn reserve_resource(tx: &mut Tx<'_>, work: &Work) -> Result<bool> {
    // Policy is platform-owned; absent policy means existing channel limits only,
    // not an invented promise about safe account sending frequency.
    let key = format!("wecom:{}:{}", work.corp_id, work.sender.to_lowercase());
    if let Some(policy)=sqlx::query("SELECT enabled,window_seconds,max_calls FROM distribution_resource_policies WHERE resource_key=$1 FOR SHARE").bind(&key).fetch_optional(&mut **tx).await? {
        let used:i64=sqlx::query_scalar("SELECT count(*) FROM distribution_resource_usage WHERE resource_key=$1 AND called_at>now()-make_interval(secs=>$2::double precision)").bind(&key).bind(policy.get::<i32,_>("window_seconds") as f64).fetch_one(&mut **tx).await?;
        if !policy.get::<bool,_>("enabled") || used>=i64::from(policy.get::<i32,_>("max_calls")) {
            sqlx::query("UPDATE broadcast_batches SET next_sync_at=now()+interval '30 seconds' WHERE id=$1").bind(work.id).execute(&mut **tx).await?;
            return Ok(false);
        }
    }
    sqlx::query("INSERT INTO distribution_resource_usage(batch_id,resource_key,source_namespace) SELECT $1,$2,source_namespace FROM broadcast_tasks WHERE id=$3 ON CONFLICT(batch_id) DO NOTHING").bind(work.id).bind(&key).bind(work.task_id).execute(&mut **tx).await?;
    sqlx::query("INSERT INTO distribution_source_turns SELECT source_namespace,now() FROM broadcast_tasks WHERE id=$1 ON CONFLICT(source_namespace) DO UPDATE SET last_claimed_at=excluded.last_claimed_at").bind(work.task_id).execute(&mut **tx).await?;
    Ok(true)
}
fn draft_namespace(input: &crate::distribution::domain::broadcast::DraftInput) -> &str {
    let _ = input;
    "builtin.direct"
}
pub async fn find_draft_request(
    tx: &mut Tx<'_>,
    actor: &crate::business::Actor,
    input: &crate::distribution::domain::broadcast::DraftInput,
) -> Result<Option<Uuid>> {
    let Some(key) = input.request_key.as_deref() else {
        return Ok(None);
    };
    require(!key.is_empty() && key.len() <= 128, "草稿幂等键无效")?;
    let row=sqlx::query("SELECT request_hash,task_id FROM broadcast_draft_requests WHERE source_namespace=$1  AND actor_id=$2 AND request_key=$3")
        .bind(draft_namespace(input)).bind(actor.id).bind(key).fetch_optional(&mut **tx).await?;
    if let Some(row) = row {
        require(
            row.get::<String, _>("request_hash") == hash(&serde_json::json!(input).to_string()),
            "幂等键已用于不同草稿内容",
        )?;
        Ok(Some(row.get("task_id")))
    } else {
        Ok(None)
    }
}
pub async fn record_draft_request(
    tx: &mut Tx<'_>,
    actor: &crate::business::Actor,
    task: Uuid,
    input: &crate::distribution::domain::broadcast::DraftInput,
) -> Result<()> {
    if let Some(key) = input.request_key.as_deref() {
        sqlx::query("INSERT INTO broadcast_draft_requests VALUES($1,$2,$3,$4,$5)")
            .bind(draft_namespace(input))
            .bind(actor.id)
            .bind(key)
            .bind(hash(&serde_json::json!(input).to_string()))
            .bind(task)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}
