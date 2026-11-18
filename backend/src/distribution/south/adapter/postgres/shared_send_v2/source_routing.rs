use super::*;
use crate::distribution::domain::receiving::DeliveryPlan;
/// Automatic source tasks use an explicit route authorization, never a fabricated
/// browser session, preview approval or Confirmation command.
pub async fn create(
    tx: &mut Tx<'_>,
    snapshot: &DeliveryPlan,
    message: &str,
    authorizer: Uuid,
    snapshot_id: Uuid,
) -> Result<Uuid> {
    // Explicit source-route authorization was checked before this internal call.
    let client_kind =
        serde_json::from_value::<crate::distribution::domain::shared_mobile::MobileClient>(
            snapshot.targets.first().ok_or(Error::Conflict)?.identity["client_kind"].clone(),
        )
        .map_err(|_| Error::Conflict)?;
    require(
        snapshot
            .targets
            .iter()
            .all(|t| t.identity["client_kind"] == json!(client_kind)),
        "shared_send_client_mismatch",
    )?;
    let input = DraftInput {
        client_kind,
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
    sqlx::query("INSERT INTO mobile_v9_task_drafts(id,intent_id,actor_id,target_ids,message,content_digest,between_targets_seconds,client_kind) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(draft).bind(intent).bind(authorizer).bind(&input.target_ids).bind(&input.message).bind(hash(&input.message)).bind(input.between_targets_seconds).bind(client_kind.as_str()).execute(&mut **tx).await?;
    let mut items = Vec::new();
    for (position, target) in snapshot.targets.iter().enumerate() {
        let mut candidates = target.identity["candidates"]
            .as_array()
            .ok_or(Error::Conflict)?
            .clone();
        require(!candidates.is_empty(), "mobile_candidates_missing")?;
        let mut eligible = Vec::new();
        for mut c in candidates.drain(..) {
            match tasks::validate_plan(tx, &c).await {
                Ok(()) => {
                    c["confirmable"] = json!(true);
                    eligible.push(c);
                }
                Err(
                    error @ (Error::Policy(_)
                    | Error::Invalid(_)
                    | Error::Forbidden
                    | Error::NotFound),
                ) => {
                    records::check(
                        tx,
                        uid(&c, "device_id")?,
                        "plan_authorization",
                        &records::failure(&error),
                        json!({}),
                    )
                    .await?;
                }
                Err(error) => return Err(error),
            }
        }
        let candidates = eligible;
        require(!candidates.is_empty(), "mobile_candidates_missing")?;
        items.push(json!({"position":position,"target_id":target.id,"selected":candidates[0],"candidates":candidates}));
    }
    let unit: Uuid = sqlx::query_scalar(
        "SELECT business_unit_id FROM channel_receiving_configurations WHERE id=$1",
    )
    .bind(snapshot.configuration_id)
    .fetch_one(&mut **tx)
    .await?;
    let effective = business_scheme::resolve(tx, Some(unit)).await?;
    let plan = json!({"client_kind":client_kind,"schema":SCHEMA,"intent_id":intent,"draft_id":draft,"draft_revision":1,"between_targets_seconds":input.between_targets_seconds,"message":input.message,"content_digest":hash(&input.message),"items":items,"business_unit_id":unit,"scheme_snapshot":effective,"policy_id":effective["id"],"authorization_kind":"source_route","not_before":snapshot.not_before,"executed_by":"message-orchestration","delegate_id":snapshot.id,"configuration_id":snapshot.configuration_id});
    sqlx::query("INSERT INTO mobile_v9_previews(id,draft_id,draft_revision,plan,digest) VALUES($1,$2,1,$3,$4)").bind(preview).bind(draft).bind(&plan).bind(digest(&plan)?).execute(&mut **tx).await?;
    let p = json!({"id":preview,"plan":plan,"policy_id":effective["id"],"source_snapshot_id":snapshot_id});
    let task = tasks::create_authorized_task(tx, authorizer, &p).await?;
    let id = uid(&task, "id")?;
    Ok(id)
}
