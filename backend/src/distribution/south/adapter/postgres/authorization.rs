use crate::business::{Error, Result};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;
pub(crate) async fn authorize(tx: &mut Transaction<'_, Postgres>, actor: Uuid) -> Result<()> {
    sqlx::query("SELECT id FROM accounts WHERE id=$1 AND active AND platform_admin FOR SHARE")
        .bind(actor)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(Error::Forbidden)?;
    Ok(())
}
