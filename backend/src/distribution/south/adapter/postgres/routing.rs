//! Transactional directory projection for routing. Never uses forum or school data.
use crate::business::{Error, Result};
use serde_json::{Value, json};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

pub async fn authorize(
    tx: &mut Transaction<'_, Postgres>,
    actor: Uuid,
    channel: &str,
    workspace: Uuid,
) -> Result<()> {
    match channel {
        "mobile" => super::mobile_workspaces::authorize(tx, actor, Some(workspace), true).await,
        "wecom" => super::wecom_workspaces::authorize(tx, actor, Some(workspace), true).await,
        _ => Err(Error::Invalid("通道无效".into())),
    }
}
pub async fn workspaces(
    tx: &mut Transaction<'_, Postgres>,
    actor: Uuid,
    channel: &str,
) -> Result<Vec<Value>> {
    let query = match channel {
        "mobile" => {
            "SELECT jsonb_build_object('id',w.id,'name',w.name) FROM mobile_workspaces w JOIN accounts a ON a.id=$1 WHERE w.active AND a.active AND (a.platform_admin OR EXISTS(SELECT 1 FROM mobile_workspace_members m WHERE m.workspace_id=w.id AND m.account_id=a.id AND m.role='admin')) ORDER BY w.name,w.id"
        }
        "wecom" => {
            "SELECT jsonb_build_object('id',w.id,'name',w.name) FROM wecom_workspaces w JOIN accounts a ON a.id=$1 WHERE w.active AND a.active AND (a.platform_admin OR EXISTS(SELECT 1 FROM wecom_workspace_members m WHERE m.school_id=w.id AND m.account_id=a.id AND m.role='admin')) ORDER BY w.name,w.id"
        }
        _ => return Err(Error::Invalid("通道无效".into())),
    };
    Ok(sqlx::query_scalar(query)
        .bind(actor)
        .fetch_all(&mut **tx)
        .await?)
}
/// Locks the workspace and current targets, serializing with membership changes and target edits.
pub async fn targets(
    tx: &mut Transaction<'_, Postgres>,
    channel: &str,
    workspace: Uuid,
) -> Result<Vec<Value>> {
    let table = match channel {
        "mobile" => "mobile_workspaces",
        "wecom" => "wecom_workspaces",
        _ => return Err(Error::Invalid("通道无效".into())),
    };
    sqlx::query(&format!(
        "SELECT id FROM {table} WHERE id=$1 AND active FOR SHARE"
    ))
    .bind(workspace)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(Error::Forbidden)?;
    let mut result = Vec::new();
    if channel == "mobile" {
        let rows=sqlx::query("SELECT id,search_text FROM mobile_targets WHERE school_id=$1 AND enabled AND archived_at IS NULL ORDER BY id FOR SHARE").bind(workspace).fetch_all(&mut **tx).await?;
        for row in rows {
            let id: Uuid = row.get("id");
            let candidates = super::shared_send_v2::routing_candidates(tx, workspace, id).await?;
            result.push(json!({"id":id,"name":row.get::<String,_>("search_text"),"identity":{"candidates":candidates},"resource_available":!candidates.is_empty()}));
        }
    } else {
        let rows=sqlx::query("SELECT id,name,corp_id,chat_id FROM wecom_groups WHERE school_id=$1 AND active ORDER BY id FOR SHARE").bind(workspace).fetch_all(&mut **tx).await?;
        for row in rows {
            let id: Uuid = row.get("id");
            let senders:Vec<Value>=sqlx::query_scalar(r#"SELECT jsonb_build_object('account_id',a.id,'userid',m.userid,'binding_revision',a.wecom_binding_revision)
    FROM wecom_groups g JOIN wecom_asset_objects o ON o.id=g.directory_group_id AND o.kind='groups'
    JOIN wecom_members m ON m.userid_key=lower(o.data->>'owner' COLLATE "C")
    JOIN accounts a ON a.wecom_member_id=m.id
    JOIN wecom_workspace_members sm ON sm.account_id=a.id AND sm.school_id=g.school_id
    JOIN wecom_directory_state ds ON ds.singleton
    WHERE g.id=$1 AND o.credential_generation=ds.credential_generation AND o.verification='verified' AND o.visibility='visible'
    AND o.checked_at>now()-interval '24 hours' AND m.verification='verified' AND m.status=1 AND a.active AND sm.role IN ('operator','admin')
    AND NOT EXISTS(SELECT 1 FROM wecom_sender_qualifications q WHERE q.school_id=g.school_id AND q.account_id=a.id AND NOT q.enabled)
    ORDER BY a.id FOR SHARE OF o,m,a,sm,ds"#).bind(id).fetch_all(&mut **tx).await?;
            result.push(json!({"id":id,"name":row.get::<String,_>("name"),"identity":{"corp_id":row.get::<String,_>("corp_id"),"chat_id":row.get::<String,_>("chat_id"),"senders":senders},"resource_available":!senders.is_empty()}));
        }
    }
    Ok(result)
}

pub async fn create_official_task(
    tx: &mut Transaction<'_, Postgres>,
    snapshot: &crate::source_access::domain::routing::RouteSnapshot,
    message: &str,
    authorizer: Uuid,
    snapshot_id: Uuid,
) -> Result<Uuid> {
    use crate::business::{hash, require};
    let config = snapshot.execution.as_ref().ok_or(Error::Conflict)?;
    authorize(tx, authorizer, "wecom", config.workspace_id).await?;
    let content = crate::distribution::domain::broadcast::normalize_text(message)?;
    require(sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM distribution_message_snapshots WHERE id=$1 AND school_id=$2 AND channel='wecom' AND content=$3 AND content_hash=$4)").bind(snapshot_id).bind(config.workspace_id).bind(&content).bind(hash(&content)).fetch_one(&mut **tx).await?, "source_snapshot_mismatch")?;
    let directory = sqlx::query(
        "SELECT corp_id,credential_generation FROM wecom_directory_state WHERE singleton FOR SHARE",
    )
    .fetch_one(&mut **tx)
    .await?;
    let corp: String = directory.get("corp_id");
    let generation: i64 = directory.get("credential_generation");
    let task = Uuid::now_v7();
    let input = json!({"content_source":"text","text_content":content,"group_ids":snapshot.targets.iter().map(|t|t.id).collect::<Vec<_>>()});
    sqlx::query("INSERT INTO broadcast_tasks(id,school_id,creator_id,content,content_hash,corp_id,generation,state,content_source,source_namespace,draft_input,authorization_kind,authorized_by) VALUES($1,$2,$3,$4,$5,$6,$7,'confirmed','text','builtin.source-route',$8,'source_route',$3)").bind(task).bind(config.workspace_id).bind(authorizer).bind(&content).bind(hash(&content)).bind(&corp).bind(generation).bind(input).execute(&mut **tx).await?;
    sqlx::query("INSERT INTO broadcast_task_snapshots SELECT school_id,id,revision,$2 FROM broadcast_tasks WHERE id=$1").bind(task).bind(snapshot_id).execute(&mut **tx).await?;
    for (position, target) in snapshot.targets.iter().enumerate() {
        require(
            target.identity["corp_id"] == corp,
            "source_channel_identity_changed",
        )?;
        let sender = target.identity["senders"]
            .as_array()
            .and_then(|s| s.first())
            .ok_or(Error::Policy("channel_account_missing"))?;
        let account: Uuid =
            serde_json::from_value(sender["account_id"].clone()).map_err(|_| Error::Conflict)?;
        let revision: i64 = sqlx::query_scalar(
            "SELECT revision FROM wecom_groups WHERE id=$1 AND school_id=$2 AND active FOR SHARE",
        )
        .bind(target.id)
        .bind(config.workspace_id)
        .fetch_one(&mut **tx)
        .await?;
        let batch = Uuid::now_v7();
        // One target per batch keeps stop/revocation decisions independent.
        sqlx::query("INSERT INTO broadcast_batches(id,school_id,task_id,ordinal,sender,sender_account,binding_revision,corp_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(batch).bind(config.workspace_id).bind(task).bind(position as i32).bind(sender["userid"].as_str()).bind(account).bind(sender["binding_revision"].as_i64()).bind(&corp).execute(&mut **tx).await?;
        sqlx::query("INSERT INTO broadcast_targets(id,school_id,task_id,batch_id,group_id,chat_id,group_name,group_revision) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(Uuid::now_v7()).bind(config.workspace_id).bind(task).bind(batch).bind(target.id).bind(target.identity["chat_id"].as_str()).bind(&target.name).bind(revision).execute(&mut **tx).await?;
    }
    sqlx::query("INSERT INTO broadcast_events(id,school_id,task_id,action,detail) VALUES($1,$2,$3,'source_route.authorized',$4)").bind(Uuid::now_v7()).bind(config.workspace_id).bind(task).bind(json!({"execution_actor":"source-routing","authorization_actor":authorizer,"route_id":snapshot.route_id,"config_revision":snapshot.revision})).execute(&mut **tx).await?;
    Ok(task)
}

pub async fn source_guard(tx: &mut Transaction<'_, Postgres>) -> Result<()> {
    // Source tasks mutate their own route/task/target rows. Shared barriers retain
    // compatibility with channel control writes without serializing unrelated pools.
    sqlx::query("SELECT pg_advisory_xact_lock_shared(78234610242)")
        .execute(&mut **tx)
        .await?;
    super::shared_send_v2::source_guard(tx).await
}
