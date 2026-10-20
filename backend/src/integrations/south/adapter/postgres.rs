use crate::business::{Error, Result, hash, secret};
use sqlx::PgPool;
use uuid::Uuid;
#[derive(Clone)]
pub struct Repository(pub PgPool);
impl Repository {
    pub async fn oauth_state(&self, return_path: &str) -> Result<String> {
        let state = secret();
        sqlx::query("DELETE FROM oauth_states WHERE expires_at<now()")
            .execute(&self.0)
            .await?;
        sqlx::query("INSERT INTO oauth_states(state_hash,session_hash,return_path,expires_at) VALUES($1,$2,$3,now()+interval '5 minutes')").bind(hash(&state)).bind(hash(&state)).bind(return_path).execute(&self.0).await?;
        Ok(state)
    }
    pub async fn consume(&self, state: &str) -> Result<String> {
        let row=sqlx::query_scalar("DELETE FROM oauth_states WHERE state_hash=$1 AND expires_at>now() RETURNING return_path").bind(hash(state)).fetch_optional(&self.0).await?;
        row.ok_or(Error::Forbidden)
    }
    pub async fn member(&self, userid: &str) -> Result<Uuid> {
        sqlx::query_scalar("SELECT id FROM accounts WHERE wecom_userid=$1 AND active")
            .bind(userid)
            .fetch_optional(&self.0)
            .await?
            .ok_or(Error::Forbidden)
    }
    pub async fn userid(&self, id: Uuid) -> Result<String> {
        sqlx::query_scalar::<_, Option<String>>(
            "SELECT wecom_userid FROM accounts WHERE id=$1 AND active",
        )
        .bind(id)
        .fetch_optional(&self.0)
        .await?
        .flatten()
        .ok_or(Error::Forbidden)
    }
    pub async fn wx_account(&self, openid: &str) -> Result<Uuid> {
        let id = Uuid::now_v7();
        Ok(sqlx::query_scalar("INSERT INTO accounts(id,username,display_name,wx_openid) VALUES($1,$2,'微信用户',$3) ON CONFLICT(wx_openid) DO UPDATE SET wx_openid=excluded.wx_openid RETURNING id").bind(id).bind(format!("wx_{}",hash(openid))).bind(openid).fetch_one(&self.0).await?)
    }
    pub async fn cached_or_fetch<F, Fut>(&self, key: &str, fetch: F) -> Result<String>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<(String, i64)>>,
    {
        let mut tx = self.0.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
            .bind(key)
            .execute(&mut *tx)
            .await?;
        if let Some(value) =
            sqlx::query_scalar("SELECT value FROM platform_cache WHERE key=$1 AND expires_at>now()")
                .bind(key)
                .fetch_optional(&mut *tx)
                .await?
        {
            return Ok(value);
        }
        let (value, ttl) = fetch().await?;
        sqlx::query("INSERT INTO platform_cache(key,value,expires_at) VALUES($1,$2,now()+$3*interval '1 second') ON CONFLICT(key) DO UPDATE SET value=excluded.value,expires_at=excluded.expires_at").bind(key).bind(&value).bind(ttl.clamp(30,7000)).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(value)
    }
}
