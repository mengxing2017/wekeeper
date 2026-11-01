use crate::{
    business::{Actor, Error, Result, require},
    distribution::domain::resource_policy::{ResourcePolicy, SourcePolicy},
};
use sqlx::PgPool;
#[derive(Clone)]
pub struct Repository(pub PgPool);
impl Repository {
    pub async fn policies(&self, actor: &Actor) -> Result<serde_json::Value> {
        actor.platform()?;
        let sources: Vec<serde_json::Value> =
            sqlx::query_scalar("SELECT to_jsonb(s) FROM distribution_sources s ORDER BY namespace")
                .fetch_all(&self.0)
                .await?;
        let resources: Vec<serde_json::Value> = sqlx::query_scalar(
            "SELECT to_jsonb(p) FROM distribution_resource_policies p ORDER BY resource_key",
        )
        .fetch_all(&self.0)
        .await?;
        Ok(serde_json::json!({"sources":sources,"resources":resources}))
    }
    pub async fn source(&self, actor: &Actor, input: SourcePolicy) -> Result<()> {
        actor.platform()?;
        require(
            (1..=100000).contains(&input.max_pending),
            "来源积压限制无效",
        )?;
        let mut tx = self.0.begin().await?;
        super::broadcast::guard(&mut tx).await?;
        let n=sqlx::query("UPDATE distribution_sources SET enabled=$2,max_pending=$3,revision=revision+1 WHERE namespace=$1 AND revision=$4").bind(&input.namespace).bind(input.enabled).bind(input.max_pending).bind(input.revision).execute(&mut *tx).await?.rows_affected();
        if n != 1 {
            return Err(Error::Conflict);
        }
        audit(
            &mut tx,
            actor,
            "distribution.source.policy",
            serde_json::to_value(input).map_err(|_| Error::Conflict)?,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn resource(&self, actor: &Actor, input: ResourcePolicy) -> Result<()> {
        actor.platform()?;
        input.validate()?;
        let mut tx = self.0.begin().await?;
        super::broadcast::guard(&mut tx).await?;
        // Keys must resolve to real configured official identities, not phone remarks.
        let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM wecom_members m CROSS JOIN wecom_directory_state d WHERE 'wecom:'||d.corp_id||':'||m.userid_key=$1)").bind(&input.resource_key).fetch_one(&mut *tx).await?;
        require(exists, "请先核实官方企业和成员资源")?;
        let n = if input.revision == 0 {
            sqlx::query("INSERT INTO distribution_resource_policies(resource_key,enabled,window_seconds,max_calls) VALUES($1,$2,$3,$4) ON CONFLICT DO NOTHING")
                .bind(&input.resource_key).bind(input.enabled).bind(input.window_seconds).bind(input.max_calls).execute(&mut *tx).await?.rows_affected()
        } else {
            sqlx::query("UPDATE distribution_resource_policies SET enabled=$2,window_seconds=$3,max_calls=$4,revision=revision+1 WHERE resource_key=$1 AND revision=$5")
                .bind(&input.resource_key).bind(input.enabled).bind(input.window_seconds).bind(input.max_calls).bind(input.revision).execute(&mut *tx).await?.rows_affected()
        };
        if n != 1 {
            return Err(Error::Conflict);
        }
        audit(
            &mut tx,
            actor,
            "distribution.resource.policy",
            serde_json::to_value(input).map_err(|_| Error::Conflict)?,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
}
async fn audit(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    actor: &Actor,
    action: &str,
    detail: serde_json::Value,
) -> Result<()> {
    // Public audit supports platform scope without inventing a school ownership.
    sqlx::query("INSERT INTO audit_events(id,school_id,actor_id,action,object_id,detail,request_id) VALUES($1,NULL,$2,$3,$1,$4,$5)")
        .bind(uuid::Uuid::now_v7()).bind(actor.id).bind(action).bind(detail).bind(&actor.request_id).execute(&mut **tx).await?;
    Ok(())
}
