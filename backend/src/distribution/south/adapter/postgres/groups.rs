use crate::business::{Actor, Error, Result};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;
#[derive(Clone)]
pub struct Repository(pub PgPool);
impl Repository {
    pub async fn groups(&self) -> Result<Vec<Value>> {
        Ok(
            sqlx::query_scalar("SELECT to_jsonb(g) FROM wecom_groups g  ORDER BY name")
                .fetch_all(&self.0)
                .await?,
        )
    }
    pub async fn group(&self, a: &Actor, v: Value) -> Result<Value> {
        let id = v["id"]
            .as_str()
            .and_then(|s| s.parse().ok())
            .unwrap_or_else(Uuid::now_v7);
        let rev = v["revision"].as_i64().unwrap_or(0);
        let mut tx = self.0.begin().await?;
        crate::distribution::south::adapter::postgres::authorization::authorize(&mut tx, a.id)
            .await?;
        if let Some(generation) = v["verified_generation"].as_i64() {
            sqlx::query("SELECT singleton FROM wecom_directory_state WHERE credential_generation=$1 FOR UPDATE").bind(generation).fetch_optional(&mut *tx).await?.ok_or(Error::Conflict)?;
        }
        let directory_id = v["directory_group_id"]
            .as_str()
            .and_then(|v| v.parse::<Uuid>().ok());
        let item = if rev == 0 {
            sqlx::query_scalar("INSERT INTO wecom_groups(id,corp_id,chat_id,name,active,directory_group_id) VALUES($1,$2,$3,$4,$5,$6) RETURNING to_jsonb(wecom_groups)").bind(id).bind(v["corp_id"].as_str()).bind(v["chat_id"].as_str()).bind(v["name"].as_str()).bind(v["active"].as_bool().unwrap_or(true)).bind(directory_id).fetch_one(&mut *tx).await?
        } else {
            sqlx::query_scalar("UPDATE wecom_groups SET name=$2,active=$3,revision=revision+1,directory_group_id=coalesce($7,directory_group_id) WHERE id=$1  AND revision=$4 AND corp_id=$5 AND chat_id=$6 RETURNING to_jsonb(wecom_groups)").bind(id).bind(v["name"].as_str()).bind(v["active"].as_bool().unwrap_or(true)).bind(rev).bind(v["corp_id"].as_str()).bind(v["chat_id"].as_str()).bind(directory_id).fetch_optional(&mut *tx).await?.ok_or(Error::Conflict)?
        };
        crate::audit::record(&mut tx, a, "group.save", id, json!({})).await?;
        tx.commit().await?;
        Ok(item)
    }
}
