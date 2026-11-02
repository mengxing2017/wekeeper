//! Shared persistence for short-lived, actor-bound business confirmations.
use crate::business::{Actor, Error, Result, hash, require};
use serde_json::{Value, json};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;
pub async fn platform(tx: &mut Transaction<'_, Postgres>, a: &Actor) -> Result<()> {
    sqlx::query("SELECT id FROM accounts WHERE id=$1 AND active AND platform_admin FOR SHARE")
        .bind(a.id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(Error::Forbidden)?;
    Ok(())
}
pub async fn authorize(
    tx: &mut Transaction<'_, Postgres>,
    a: &Actor,
    s: Option<Uuid>,
    role: &str,
) -> Result<()> {
    if a.platform_admin {
        platform(tx, a).await?;
        if let Some(s) = s {
            sqlx::query("SELECT id FROM schools WHERE id=$1 AND active FOR SHARE")
                .bind(s)
                .fetch_optional(&mut **tx)
                .await?
                .ok_or(Error::NotFound)?;
        }
        return Ok(());
    }
    let s = s.ok_or(Error::Forbidden)?;
    sqlx::query("SELECT a.id FROM accounts a JOIN school_memberships m ON m.account_id=a.id JOIN schools s ON s.id=m.school_id WHERE a.id=$1 AND a.active AND s.active AND s.id=$2 AND (m.role='admin' OR ($3='operator' AND m.role='operator') OR ($3='member' AND m.role<>'revoked')) FOR SHARE OF a,m,s").bind(a.id).bind(s).bind(role).fetch_optional(&mut **tx).await?.ok_or(Error::Forbidden)?;
    Ok(())
}
pub async fn create(
    tx: &mut Transaction<'_, Postgres>,
    a: &Actor,
    s: Option<Uuid>,
    kind: &str,
    payload: Value,
    dependencies: Value,
) -> Result<Value> {
    let id = Uuid::now_v7();
    let digest=hash(&json!({"id":id,"actor":a.id,"school":s,"kind":kind,"payload":payload,"dependencies":dependencies}).to_string());
    let expiry:chrono::DateTime<chrono::Utc>=sqlx::query_scalar("INSERT INTO product_previews(id,actor_id,school_id,kind,payload,dependencies,digest) VALUES($1,$2,$3,$4,$5,$6,$7) RETURNING expires_at").bind(id).bind(a.id).bind(s).bind(kind).bind(&payload).bind(&dependencies).bind(&digest).fetch_one(&mut **tx).await?;
    Ok(json!({"id":id,"digest":digest,"expires_at":expiry,"summary":payload}))
}
pub async fn load(
    tx: &mut Transaction<'_, Postgres>,
    a: &Actor,
    s: Option<Uuid>,
    kind: &str,
    id: Uuid,
    digest: &str,
) -> Result<(Value, Value, Option<Value>)> {
    let row=sqlx::query("SELECT payload,dependencies,result,expires_at>now() AS valid,digest FROM product_previews WHERE id=$1 AND actor_id=$2 AND school_id IS NOT DISTINCT FROM $3 AND kind=$4 FOR UPDATE").bind(id).bind(a.id).bind(s).bind(kind).fetch_optional(&mut **tx).await?.ok_or(Error::NotFound)?;
    require(row.get::<String, _>("digest") == digest, "核对摘要不匹配")?;
    let result = row.get::<Option<Value>, _>("result");
    require(
        result.is_some() || row.get::<bool, _>("valid"),
        "预览已过期，请重新核对",
    )?;
    Ok((row.get("payload"), row.get("dependencies"), result))
}
pub async fn finish(tx: &mut Transaction<'_, Postgres>, id: Uuid, result: &Value) -> Result<()> {
    sqlx::query(
        "UPDATE product_previews SET result=$2,confirmed_at=now() WHERE id=$1 AND result IS NULL",
    )
    .bind(id)
    .bind(result)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
