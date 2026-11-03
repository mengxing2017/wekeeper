//! One explicit native delegation per enrollment; technical releases never create consent.
use super::*;
pub(super) async fn context(tx: &mut Tx<'_>, c: &Credential) -> Result<Value> {
    let existing: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM mobile_shared_delegations WHERE device_id=$1 AND credential_generation=$2",
    )
    .bind(c.device_id)
    .bind(c.generation)
    .fetch_optional(&mut **tx)
    .await?;
    let id = if let Some(id) = existing {
        id
    } else {
        let platform: Uuid =
            sqlx::query_scalar("SELECT id FROM mobile_shared_platform_identity WHERE singleton")
                .fetch_one(&mut **tx)
                .await?;
        let scope = delegation_scope(platform, c.device_id, c.generation);
        let id = Uuid::now_v7();
        sqlx::query("INSERT INTO mobile_shared_delegations(id,device_id,credential_generation,scope,scope_digest) VALUES($1,$2,$3,$4,$5) ON CONFLICT(device_id,credential_generation) DO NOTHING").bind(id).bind(c.device_id).bind(c.generation).bind(&scope).bind(digest(&scope)?).execute(&mut **tx).await?;
        sqlx::query_scalar("SELECT id FROM mobile_shared_delegations WHERE device_id=$1 AND credential_generation=$2").bind(c.device_id).bind(c.generation).fetch_one(&mut **tx).await?
    };
    sqlx::query_scalar("SELECT to_jsonb(d)||jsonb_build_object('decision',(SELECT to_jsonb(v) FROM mobile_shared_delegation_decisions v WHERE v.delegation_id=d.id ORDER BY revision DESC LIMIT 1)) FROM mobile_shared_delegations d WHERE d.id=$1").bind(id).fetch_one(&mut **tx).await.map_err(Into::into)
}
pub(super) async fn decide(tx: &mut Tx<'_>, c: &Credential, v: ConsentInput) -> Result<Value> {
    let d = context(tx, c).await?;
    require(
        d["id"] == json!(v.delegation_id) && d["scope_digest"] == v.scope_digest,
        "shared_delegation_changed",
    )?;
    let old = &d["decision"];
    let revision = old["revision"].as_i64().unwrap_or(0);
    let Some(next) = next_decision_revision(
        revision,
        v.expected_revision,
        old["accepted"].as_bool(),
        v.accepted,
    )?
    else {
        return Ok(old.clone());
    };
    let id = Uuid::now_v7();
    sqlx::query("INSERT INTO mobile_shared_delegation_decisions(id,delegation_id,accepted,revision) VALUES($1,$2,$3,$4)").bind(id).bind(v.delegation_id).bind(v.accepted).bind(revision+1).execute(&mut **tx).await?;
    Ok(json!({"id":id,"delegation_id":v.delegation_id,"revision":next,"accepted":v.accepted}))
}
