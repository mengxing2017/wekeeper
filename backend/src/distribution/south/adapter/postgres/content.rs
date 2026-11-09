//! Platform-owned content, qualification, preview and resource persistence.
use crate::{
    business::{Error, Result, hash, require},
    distribution::domain::{
        broadcast::Work,
        content::{Material, Preview, SourceReference},
    },
};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;
type Tx<'a> = Transaction<'a, Postgres>;
pub async fn forum_references(
    pool: &PgPool,
    school: Uuid,
    ids: &[Uuid],
) -> Result<Vec<SourceReference>> {
    let mut out = Vec::new();
    for id in ids {
        let version:String=sqlx::query_scalar("SELECT version FROM distribution_materials WHERE namespace='builtin.forum' AND school_id=$1 AND external_organization_id=$1::text AND external_content_id=$2 AND allowed")
            .bind(school).bind(id.to_string()).fetch_optional(pool).await?.ok_or(Error::Conflict)?;
        out.push(SourceReference {
            namespace: "builtin.forum".into(),
            external_organization_id: school.to_string(),
            external_content_id: id.to_string(),
            version,
        });
    }
    Ok(out)
}
pub async fn validate_materials(
    tx: &mut Tx<'_>,
    school: Uuid,
    materials: &[Material],
) -> Result<()> {
    for material in materials {
        let r = &material.reference;
        sqlx::query("SELECT m.version FROM distribution_materials m JOIN distribution_sources s USING(namespace) JOIN distribution_source_organizations o USING(namespace,external_organization_id,school_id) WHERE m.namespace=$1 AND m.external_organization_id=$2 AND m.external_content_id=$3 AND m.school_id=$4 AND m.version=$5 AND m.allowed AND m.title=$6 AND m.content=$7 AND s.enabled AND o.enabled FOR SHARE OF m,s,o")
            .bind(&r.namespace).bind(&r.external_organization_id).bind(&r.external_content_id).bind(school).bind(&r.version).bind(&material.title).bind(&material.content).fetch_optional(&mut **tx).await?.ok_or(Error::Conflict)?;
    }
    Ok(())
}
pub async fn save_snapshot(
    tx: &mut Tx<'_>,
    school: Uuid,
    task: Uuid,
    content: &str,
    materials: &[Material],
) -> Result<()> {
    let snapshot = Uuid::now_v7();
    sqlx::query("INSERT INTO distribution_message_snapshots(id,school_id,channel,content,content_hash,source_references) VALUES($1,$2,'wecom',$3,$4,$5)")
        .bind(snapshot).bind(school).bind(content).bind(hash(content)).bind(serde_json::json!(materials.iter().map(|m| &m.reference).collect::<Vec<_>>())).execute(&mut **tx).await?;
    sqlx::query("INSERT INTO broadcast_task_snapshots SELECT school_id,id,revision,$2 FROM broadcast_tasks WHERE id=$1").bind(task).bind(snapshot).execute(&mut **tx).await?;
    sqlx::query("DELETE FROM broadcast_content_references WHERE task_id=$1")
        .bind(task)
        .execute(&mut **tx)
        .await?;
    for (position, material) in materials.iter().enumerate() {
        let r = &material.reference;
        sqlx::query("INSERT INTO broadcast_content_references VALUES($1,$2,$3,$4,$5,$6,$7)")
            .bind(school)
            .bind(task)
            .bind(position as i32)
            .bind(&r.namespace)
            .bind(&r.external_organization_id)
            .bind(&r.external_content_id)
            .bind(&r.version)
            .execute(&mut **tx)
            .await?;
    }
    sqlx::query("DELETE FROM broadcast_previews WHERE task_id=$1")
        .bind(task)
        .execute(&mut **tx)
        .await?;
    Ok(())
}
pub async fn validate_task(tx: &mut Tx<'_>, task: Uuid) -> Result<()> {
    let row=sqlx::query("SELECT t.content_source,t.source_namespace,t.authorization_kind,t.school_id,t.content,s.enabled,s.max_pending FROM broadcast_tasks t JOIN distribution_sources s ON s.namespace=t.source_namespace WHERE t.id=$1 FOR SHARE OF s")
        .bind(task).fetch_one(&mut **tx).await?;
    require(row.get("enabled"), "内容来源已暂停")?;
    let refs=sqlx::query("SELECT r.*,m.version AS current_version,m.allowed,o.enabled AS organization_enabled FROM broadcast_content_references r LEFT JOIN distribution_materials m USING(namespace,external_organization_id,external_content_id,school_id) LEFT JOIN distribution_source_organizations o USING(namespace,external_organization_id,school_id) WHERE r.task_id=$1 ORDER BY r.position")
        .bind(task).fetch_all(&mut **tx).await?;
    let namespace: String = row.get("source_namespace");
    if row.get::<String, _>("content_source") == "text" {
        // Automatic messages use the frozen route generation, never a manual
        // confirmation or a namespace supplied by the caller.
        let source_route = namespace == "builtin.source-route"
            && row.get::<String, _>("authorization_kind") == "source_route"
            && sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM source_execution_generations WHERE task_id=$1 AND channel='wecom' AND workspace_id=$2)")
                .bind(task).bind(row.get::<Uuid,_>("school_id")).fetch_one(&mut **tx).await?;
        require(
            refs.is_empty() && (namespace == "builtin.direct" || source_route),
            "来源不匹配",
        )?;
    } else {
        require(!refs.is_empty(), "缺少来源资格证据")?;
        // Lock projection rows as the final local barrier. The transactional forum
        // producer cannot revoke these rows until this claim transaction ends.
        for r in &refs {
            sqlx::query("SELECT m.version FROM distribution_materials m JOIN distribution_source_organizations o USING(namespace,external_organization_id,school_id) WHERE m.namespace=$1 AND m.external_organization_id=$2 AND m.external_content_id=$3 AND m.school_id=$4 AND m.version=$5 AND m.allowed AND o.enabled FOR SHARE OF m,o")
                .bind(r.get::<String,_>("namespace")).bind(r.get::<String,_>("external_organization_id")).bind(r.get::<String,_>("external_content_id")).bind(row.get::<Uuid,_>("school_id")).bind(r.get::<String,_>("version")).fetch_optional(&mut **tx).await?.ok_or(Error::Policy("source_content_unavailable"))?;
            require(r.get::<String, _>("namespace") == namespace, "来源不匹配")?;
        }
    }
    let matches:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM broadcast_tasks t JOIN broadcast_task_snapshots l ON l.task_id=t.id AND l.task_revision=t.revision JOIN distribution_message_snapshots s ON s.id=l.snapshot_id WHERE t.id=$1 AND s.content=t.content AND s.content_hash=t.content_hash AND s.channel='wecom')").bind(task).fetch_one(&mut **tx).await?;
    require(matches, "消息快照不匹配")
}
async fn digest(tx: &mut Tx<'_>, task: Uuid) -> Result<(i64, String, String, i64)> {
    let row=sqlx::query("SELECT t.revision,t.content,(SELECT count(*) FROM broadcast_targets WHERE task_id=t.id) AS total,jsonb_build_object('task',t.id,'revision',t.revision,'content',t.content,'corp',t.corp_id,'generation',t.generation,'source',t.source_namespace,'source_policy',(SELECT to_jsonb(s) FROM distribution_sources s WHERE s.namespace=t.source_namespace),'resource_policies',(SELECT jsonb_agg(to_jsonb(p) ORDER BY p.resource_key) FROM distribution_resource_policies p),'references',(SELECT jsonb_agg(to_jsonb(r) ORDER BY r.position) FROM broadcast_content_references r WHERE r.task_id=t.id),'targets',(SELECT jsonb_agg(to_jsonb(x) ORDER BY x.id) FROM broadcast_targets x WHERE x.task_id=t.id))::text AS payload FROM broadcast_tasks t WHERE id=$1")
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
pub async fn consume_events(pool: &PgPool) -> Result<()> {
    let mut tx = pool.begin().await?;
    super::broadcast::guard(&mut tx).await?;
    let events=sqlx::query("SELECT * FROM distribution_source_events WHERE processed_at IS NULL ORDER BY created_at,id LIMIT 100 FOR UPDATE SKIP LOCKED").fetch_all(&mut *tx).await?;
    for event in events {
        // Read latest projection, never replay a stale 'allowed' event over it.
        sqlx::query("UPDATE broadcast_batches b SET state='cancelled',error_code='source_content_withdrawn' FROM broadcast_content_references r JOIN distribution_materials m USING(namespace,external_organization_id,external_content_id,school_id) WHERE b.task_id=r.task_id AND b.state='queued' AND r.namespace=$1 AND r.external_organization_id=$2 AND r.external_content_id=$3 AND (NOT m.allowed OR m.version<>r.version)")
            .bind(event.get::<String,_>("namespace")).bind(event.get::<String,_>("external_organization_id")).bind(event.get::<String,_>("external_content_id")).execute(&mut *tx).await?;
        sqlx::query("UPDATE distribution_source_events SET processed_at=now() WHERE id=$1")
            .bind(event.get::<Uuid, _>("id"))
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}
fn draft_namespace(input: &crate::distribution::domain::broadcast::DraftInput) -> &str {
    use crate::distribution::domain::broadcast::ContentSource;
    match input.content_source {
        ContentSource::Text => "builtin.direct",
        ContentSource::Posts => "builtin.forum",
        ContentSource::Materials => &input.materials[0].namespace,
    }
}
pub async fn find_draft_request(
    tx: &mut Tx<'_>,
    actor: &crate::business::Actor,
    school: Uuid,
    input: &crate::distribution::domain::broadcast::DraftInput,
) -> Result<Option<Uuid>> {
    let Some(key) = input.request_key.as_deref() else {
        return Ok(None);
    };
    require(!key.is_empty() && key.len() <= 128, "草稿幂等键无效")?;
    let row=sqlx::query("SELECT request_hash,task_id FROM broadcast_draft_requests WHERE source_namespace=$1 AND school_id=$2 AND actor_id=$3 AND request_key=$4")
        .bind(draft_namespace(input)).bind(school).bind(actor.id).bind(key).fetch_optional(&mut **tx).await?;
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
    school: Uuid,
    task: Uuid,
    input: &crate::distribution::domain::broadcast::DraftInput,
) -> Result<()> {
    if let Some(key) = input.request_key.as_deref() {
        sqlx::query("INSERT INTO broadcast_draft_requests VALUES($1,$2,$3,$4,$5,$6)")
            .bind(school)
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
