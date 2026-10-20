use crate::business::{Actor, Result};
use serde_json::Value;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;
pub async fn record(
    tx: &mut PgConnection,
    a: &Actor,
    school: Uuid,
    action: &str,
    object: Uuid,
    detail: Value,
) -> Result<()> {
    sqlx::query("INSERT INTO audit_events(id,school_id,actor_id,action,object_id,detail,request_id) VALUES($1,$2,$3,$4,$5,$6,$7)").bind(Uuid::now_v7()).bind(if school.is_nil(){None}else{Some(school)}).bind(a.id).bind(action).bind(object).bind(detail).bind(&a.request_id).execute(tx).await?;
    Ok(())
}
pub async fn list(pool: &PgPool, school: Uuid, limit: i64, offset: i64) -> Result<Vec<Value>> {
    Ok(sqlx::query_scalar("SELECT to_jsonb(e) FROM audit_events e WHERE school_id=$1 ORDER BY created_at DESC LIMIT $2 OFFSET $3").bind(if school.is_nil(){None}else{Some(school)}).bind(limit).bind(offset).fetch_all(pool).await?)
}
