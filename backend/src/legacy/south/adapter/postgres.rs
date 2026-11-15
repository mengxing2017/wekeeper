use crate::business::{Error, Result, hash};
use sqlx::PgPool;
#[derive(Clone)]
pub struct Repository(pub PgPool);
impl Repository {
    pub async fn media(&self, token: &str) -> Result<(String, Vec<u8>)> {
        sqlx::query_as("SELECT a.mime_type,a.content FROM media_access_tokens t JOIN media_assets a ON a.id=t.asset_id  WHERE t.token_hash=$1 AND t.expires_at>now()").bind(hash(token)).fetch_optional(&self.0).await?.ok_or(Error::NotFound)
    }
}
