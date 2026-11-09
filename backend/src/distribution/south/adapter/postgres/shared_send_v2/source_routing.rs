use super::*;
use crate::source_access::domain::routing::RouteSnapshot;
/// Automatic source tasks use an explicit route authorization, never a fabricated
/// browser session, preview approval or Confirmation command.
pub async fn create(
    tx: &mut Tx<'_>,
    snapshot: &RouteSnapshot,
    message: &str,
    authorizer: Uuid,
) -> Result<Uuid> {
    let config = snapshot.execution.as_ref().ok_or(Error::Conflict)?;
    let s = config.workspace_id;
    authorize(tx, authorizer, Some(s), true).await?;
    let input = DraftInput {
        target_ids: snapshot.targets.iter().map(|t| t.id).collect(),
        message: message.into(),
        content_type: "text/plain".into(),
        expected_revision: None,
        between_targets_seconds: 30,
    }
    .normalized()?;
    let draft = Uuid::now_v7();
    let intent = Uuid::now_v7();
    let preview = Uuid::now_v7();
    sqlx::query("INSERT INTO mobile_v9_task_drafts(id,school_id,intent_id,actor_id,target_ids,message,content_digest,between_targets_seconds) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(draft).bind(s).bind(intent).bind(authorizer).bind(&input.target_ids).bind(&input.message).bind(hash(&input.message)).bind(input.between_targets_seconds).execute(&mut **tx).await?;
    let mut items = Vec::new();
    for (position, target) in snapshot.targets.iter().enumerate() {
        let mut candidates = target.identity["candidates"]
            .as_array()
            .ok_or(Error::Conflict)?
            .clone();
        require(!candidates.is_empty(), "mobile_candidates_missing")?;
        for c in &mut candidates {
            if let Some(scope) = tasks::grant(tx, s, c).await? {
                c["scope_id"] = scope["id"].clone();
            }
            tasks::validate_plan(tx, s, c).await?;
            c["confirmable"] = json!(true);
        }
        items.push(json!({"position":position,"target_id":target.id,"selected":candidates[0],"candidates":candidates}));
    }
    let plan = json!({"schema":SCHEMA,"dispatch_mode":"candidate_pool","school_id":s,"intent_id":intent,"draft_id":draft,"draft_revision":1,"between_targets_seconds":input.between_targets_seconds,"message":input.message,"content_digest":hash(&input.message),"items":items,"policy_id":null,"authorization_kind":"source_route","route_id":snapshot.route_id,"config_revision":snapshot.revision});
    sqlx::query("INSERT INTO mobile_v9_previews(id,school_id,draft_id,draft_revision,plan,digest) VALUES($1,$2,$3,1,$4,$5)").bind(preview).bind(s).bind(draft).bind(&plan).bind(digest(&plan)?).execute(&mut **tx).await?;
    let p = json!({"id":preview,"plan":plan,"policy_id":null});
    let task = tasks::create_authorized_task(tx, authorizer, s, &p).await?;
    let id = uid(&task, "id")?;
    sqlx::query("UPDATE mobile_v9_tasks SET authorization_kind='source_route' WHERE id=$1")
        .bind(id)
        .execute(&mut **tx)
        .await?;
    Ok(id)
}
