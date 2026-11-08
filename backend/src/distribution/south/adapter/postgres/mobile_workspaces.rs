use crate::business::{Actor, Error, Result, text};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Clone)]
pub struct Repository(pub PgPool);
impl Repository {
    pub async fn list(&self, actor: &Actor) -> Result<Vec<Value>> {
        Ok(sqlx::query_scalar("SELECT jsonb_build_object('id',w.id,'name',w.name,'role',CASE WHEN a.platform_admin THEN 'admin' ELSE m.role END) FROM accounts a CROSS JOIN mobile_workspaces w LEFT JOIN mobile_workspace_members m ON m.workspace_id=w.id AND m.account_id=a.id WHERE a.id=$1 AND a.active AND w.active AND (a.platform_admin OR m.role IN ('admin','operator')) ORDER BY w.created_at,w.id")
            .bind(actor.id).fetch_all(&self.0).await?)
    }
    pub async fn create(&self, actor: &Actor, id: Uuid, name: &str) -> Result<Value> {
        let name = text(name, 1, 120)?;
        let mut tx = self.0.begin().await?;
        sqlx::query("SELECT id FROM accounts WHERE id=$1 AND active AND platform_admin FOR SHARE")
            .bind(actor.id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(Error::Forbidden)?;
        // Client-generated ID makes retries safe without creating duplicate workspaces.
        let result = sqlx::query_scalar("INSERT INTO mobile_workspaces(id,name) VALUES($1,$2) ON CONFLICT(id) DO UPDATE SET name=EXCLUDED.name RETURNING jsonb_build_object('id',id,'name',name,'role','admin')")
            .bind(id).bind(name).fetch_one(&mut *tx).await?;
        tx.commit().await?;
        Ok(result)
    }
    pub async fn members(&self, actor: &Actor, workspace: Uuid) -> Result<Vec<Value>> {
        let mut tx = self.0.begin().await?;
        sqlx::query("SELECT id FROM accounts WHERE id=$1 AND active AND platform_admin FOR SHARE")
            .bind(actor.id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(Error::Forbidden)?;
        let rows=sqlx::query_scalar("SELECT jsonb_build_object('id',a.id,'username',a.username,'display_name',a.display_name,'role',m.role) FROM mobile_workspace_members m JOIN accounts a ON a.id=m.account_id WHERE m.workspace_id=$1 ORDER BY a.username").bind(workspace).fetch_all(&mut *tx).await?;
        tx.commit().await?;
        Ok(rows)
    }
    pub async fn grant(
        &self,
        actor: &Actor,
        workspace: Uuid,
        username: &str,
        role: &str,
    ) -> Result<Value> {
        if !matches!(role, "admin" | "operator" | "revoked") {
            return Err(Error::Invalid("移动分发角色无效".into()));
        }
        let username = text(username, 1, 120)?;
        let mut tx = self.0.begin().await?;
        sqlx::query("SELECT id FROM accounts WHERE id=$1 AND active AND platform_admin FOR SHARE")
            .bind(actor.id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(Error::Forbidden)?;
        // Serializes with mobile resource/task authorization, including device dispatch.
        sqlx::query("SELECT id FROM mobile_workspaces WHERE id=$1 AND active FOR UPDATE")
            .bind(workspace)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(Error::NotFound)?;
        let account: Uuid =
            sqlx::query_scalar("SELECT id FROM accounts WHERE username=$1 AND active FOR SHARE")
                .bind(username)
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(Error::NotFound)?;
        if role == "revoked" {
            sqlx::query(
                "DELETE FROM mobile_workspace_members WHERE workspace_id=$1 AND account_id=$2",
            )
            .bind(workspace)
            .bind(account)
            .execute(&mut *tx)
            .await?;
        } else {
            sqlx::query("INSERT INTO mobile_workspace_members(workspace_id,account_id,role) VALUES($1,$2,$3) ON CONFLICT(workspace_id,account_id) DO UPDATE SET role=$3").bind(workspace).bind(account).bind(role).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(json!({"id":account,"role":role}))
    }
}

/// Lock the workspace before reading membership so concurrent revocation is observed
/// even if this transaction had to wait for a membership writer.
pub(crate) async fn authorize(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    actor: Uuid,
    workspace: Option<Uuid>,
    admin: bool,
) -> Result<()> {
    if let Some(id) = workspace {
        sqlx::query("SELECT id FROM mobile_workspaces WHERE id=$1 AND active FOR SHARE")
            .bind(id)
            .fetch_optional(&mut **tx)
            .await?
            .ok_or(Error::Forbidden)?;
        sqlx::query("SELECT a.id FROM accounts a WHERE a.id=$1 AND a.active AND (a.platform_admin OR EXISTS(SELECT 1 FROM mobile_workspace_members m WHERE m.account_id=a.id AND m.workspace_id=$2 AND (m.role='admin' OR (NOT $3 AND m.role='operator')))) FOR SHARE OF a")
            .bind(actor).bind(id).bind(admin).fetch_optional(&mut **tx).await?.ok_or(Error::Forbidden)?;
    } else {
        sqlx::query("SELECT id FROM accounts WHERE id=$1 AND active AND platform_admin FOR SHARE")
            .bind(actor)
            .fetch_optional(&mut **tx)
            .await?
            .ok_or(Error::Forbidden)?;
    }
    Ok(())
}
