use crate::business::{Error, Result, hash, secret};
use sqlx::PgPool;
use uuid::Uuid;
#[derive(Clone)]
pub struct Repository(pub PgPool);
impl Repository {
    pub async fn signed_media(&self, s: Uuid, path: &str, origin: &str) -> Result<String> {
        let current = format!("/api/v1/wecom-distribution/workspaces/{s}/media/");
        let historical = format!("/api/v1/schools/{s}/media/");
        let Some(asset) = path
            .strip_prefix(&current)
            .or_else(|| path.strip_prefix(&historical))
        else {
            return Ok(path.into());
        };
        let id = asset.parse::<Uuid>().ok().ok_or(Error::NotFound)?;
        let token = secret();
        sqlx::query("DELETE FROM media_access_tokens WHERE expires_at<now()")
            .execute(&self.0)
            .await?;
        sqlx::query("INSERT INTO media_access_tokens SELECT $1,$2,id,now()+interval '15 minutes' FROM media_assets WHERE school_id=$2 AND id=$3").bind(hash(&token)).bind(s).bind(id).execute(&self.0).await?;
        Ok(format!("{origin}/api/legacy/media/{token}"))
    }
    pub async fn media(&self, token: &str) -> Result<(String, Vec<u8>)> {
        sqlx::query_as("SELECT a.mime_type,a.content FROM media_access_tokens t JOIN media_assets a ON a.id=t.asset_id AND a.school_id=t.school_id WHERE t.token_hash=$1 AND t.expires_at>now()").bind(hash(token)).fetch_optional(&self.0).await?.ok_or(Error::NotFound)
    }
}
