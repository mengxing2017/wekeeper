//! Platform account selection and display projections; never included in device snapshots.
use super::*;

pub(super) async fn require_eligible(tx: &mut Tx<'_>, actor: Uuid) -> Result<()> {
    // Lock the account and qualifying grants/schools until registration commits, so a
    // concurrent disable/revocation cannot pass between validation and insertion.
    let account = sqlx::query("SELECT id FROM accounts WHERE id=$1 AND active FOR SHARE")
        .bind(actor)
        .fetch_optional(&mut **tx)
        .await?;
    require(account.is_some(), "责任人不存在或已停用，请重新选择")?;
    let grants = sqlx::query("SELECT m.school_id FROM school_memberships m JOIN schools s ON s.id=m.school_id WHERE m.account_id=$1 AND m.role='admin' AND s.active ORDER BY m.school_id FOR SHARE OF m,s")
        .bind(actor).fetch_all(&mut **tx).await?;
    require(
        !grants.is_empty(),
        "责任人需在至少一个启用学校拥有管理员授权，请重新选择",
    )
}

pub(super) async fn candidates(
    tx: &mut Tx<'_>,
    p: &Principal,
    page: &Page,
    limit: i64,
    offset: i64,
) -> Result<Value> {
    platform(tx, p).await?;
    let search = text(page.search.as_deref().unwrap_or(""), 0, 100)?;
    let rows = sqlx::query("SELECT a.id,a.username,a.display_name,a.active,coalesce((SELECT jsonb_agg(jsonb_build_object('id',s.id,'name',s.name) ORDER BY s.name,s.id) FROM school_memberships m JOIN schools s ON s.id=m.school_id WHERE m.account_id=a.id AND m.role='admin' AND s.active),'[]'::jsonb) AS schools FROM accounts a WHERE a.active AND EXISTS(SELECT 1 FROM school_memberships m JOIN schools s ON s.id=m.school_id WHERE m.account_id=a.id AND m.role='admin' AND s.active) AND (strpos(lower(a.username),lower($1))>0 OR strpos(lower(a.display_name),lower($1))>0) ORDER BY lower(a.display_name),a.username,a.id LIMIT $2 OFFSET $3")
        .bind(search).bind(limit).bind(offset).fetch_all(&mut **tx).await?;
    Ok(json!(
        rows.into_iter().map(summary).collect::<Result<Vec<_>>>()?
    ))
}

fn summary(row: sqlx::postgres::PgRow) -> Result<ResponsibleActor> {
    let schools: Vec<ResponsibleSchool> =
        serde_json::from_value(row.get("schools")).map_err(|_| Error::Conflict)?;
    let active: bool = row.get("active");
    Ok(ResponsibleActor {
        id: row.get("id"),
        username: row.get("username"),
        display_name: row.get("display_name"),
        active,
        eligible: active && !schools.is_empty(),
        schools,
    })
}

pub(super) async fn account_displays(
    tx: &mut Tx<'_>,
    mut accounts: Vec<Value>,
) -> Result<Vec<Value>> {
    use std::collections::HashMap;
    if accounts.is_empty() {
        return Ok(accounts);
    }
    let actors = accounts
        .iter()
        .map(|a| uid(a, "responsible_actor_id"))
        .collect::<Result<Vec<_>>>()?;
    let ids = accounts
        .iter()
        .map(|a| uid(a, "id").map(|id| id.to_string()))
        .collect::<Result<Vec<_>>>()?;
    let rows = sqlx::query("SELECT a.id,a.username,a.display_name,a.active,coalesce((SELECT jsonb_agg(jsonb_build_object('id',s.id,'name',s.name) ORDER BY s.name,s.id) FROM school_memberships m JOIN schools s ON s.id=m.school_id WHERE m.account_id=a.id AND m.role='admin' AND s.active),'[]'::jsonb) AS schools FROM accounts a WHERE a.id=ANY($1)")
        .bind(&actors).fetch_all(&mut **tx).await?;
    let mut summaries = HashMap::new();
    for row in rows {
        let actor = summary(row)?;
        summaries.insert(actor.id, actor);
    }
    // Query the page's immutable events together, not once per account. Old
    // creation events lack a scope ID; don't invent historical evidence.
    let events = sqlx::query("SELECT DISTINCT ON(e.detail->>'scope_id',e.action) e.detail->>'scope_id' AS account_id,e.action,e.detail,jsonb_build_object('actor_id',e.actor_id,'actor_name',a.display_name,'actor_username',a.username,'reviewed_at',e.created_at,'profile_revision',e.detail->'profile_revision','evidence_ref',e.detail->>'evidence_ref') AS review FROM mobile_delivery_events e LEFT JOIN accounts a ON a.id=e.actor_id WHERE e.detail->>'scope_id'=ANY($1) AND e.action IN ('create_account','review_account_profile') ORDER BY e.detail->>'scope_id',e.action,e.created_at DESC,e.id DESC")
        .bind(&ids).fetch_all(&mut **tx).await?;
    let mut registrations = HashMap::<String, Value>::new();
    let mut reviews = HashMap::<String, Value>::new();
    for row in events {
        let id: String = row.get("account_id");
        if row.get::<String, _>("action") == "create_account" {
            let detail: Value = row.get("detail");
            registrations.insert(id, detail["identity_evidence_ref"].clone());
        } else {
            let review: AccountProfileReview =
                serde_json::from_value(row.get("review")).map_err(|_| Error::Conflict)?;
            reviews.insert(id, json!(review));
        }
    }
    for account in &mut accounts {
        let actor = uid(account, "responsible_actor_id")?;
        account["responsible_actor"] = json!(summaries.get(&actor).ok_or(Error::Conflict)?);
        let id = uid(account, "id")?.to_string();
        account["registration_evidence_ref"] = registrations.remove(&id).unwrap_or_else(|| {
            if account["verification_state"] == "pending" && account["profile_revision"] == 1 {
                account["identity_evidence_ref"].clone()
            } else {
                Value::Null
            }
        });
        account["latest_profile_review"] = reviews.remove(&id).unwrap_or(Value::Null);
    }
    Ok(accounts)
}
