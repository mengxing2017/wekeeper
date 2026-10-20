use crate::business::{Error, Result};
use sqlx::{PgPool, Row};
use uuid::Uuid;
#[derive(Clone)]
pub struct Repository(pub PgPool);
impl Repository {
    pub async fn put(&self, s: Uuid, owner: Uuid, mime: &str, content: Vec<u8>) -> Result<Uuid> {
        let id = Uuid::now_v7();
        sqlx::query("INSERT INTO media_assets(id,school_id,owner_id,mime_type,size_bytes,content) VALUES($1,$2,$3,$4,$5,$6)").bind(id).bind(s).bind(owner).bind(mime).bind(content.len() as i64).bind(content).execute(&self.0).await?;
        Ok(id)
    }
    pub async fn get(&self, s: Uuid, id: Uuid) -> Result<(String, Vec<u8>)> {
        let r =
            sqlx::query("SELECT mime_type,content FROM media_assets WHERE school_id=$1 AND id=$2")
                .bind(s)
                .bind(id)
                .fetch_optional(&self.0)
                .await?
                .ok_or(Error::NotFound)?;
        Ok((r.get("mime_type"), r.get("content")))
    }
}

impl crate::media::south::port::Store for Repository {
    async fn put(&self, s: Uuid, owner: Uuid, mime: &str, content: Vec<u8>) -> Result<Uuid> {
        Repository::put(self, s, owner, mime, content).await
    }
    async fn get(&self, s: Uuid, id: Uuid) -> Result<(String, Vec<u8>)> {
        Repository::get(self, s, id).await
    }
}
