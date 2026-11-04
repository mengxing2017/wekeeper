//! Historical configuration archive. Never read by task admission or execution.
use super::*;
use crate::distribution::domain::shared_mobile::runtime_configuration::{
    RuntimeConfiguration, RuntimeConfigurationStatus,
};

pub async fn install(pool: &PgPool, configuration: &RuntimeConfiguration) -> Result<()> {
    configuration.validate()?;
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(78234610244)")
        .execute(&mut *tx)
        .await?;
    let old: Option<Value> = sqlx::query_scalar("SELECT jsonb_build_object('id',id,'capability',capability,'version',version,'flow',flow,'flow_digest',flow_digest) FROM mobile_runtime_configurations WHERE id=$1")
        .bind(configuration.id).fetch_optional(&mut *tx).await?;
    if let Some(old) = old {
        require(
            old == json!(configuration),
            "configuration_identity_conflict",
        )?;
    } else {
        sqlx::query("INSERT INTO mobile_runtime_configurations(id,capability,version,flow,flow_digest) VALUES($1,$2,$3,$4,$5)")
            .bind(configuration.id).bind(&configuration.capability).bind(&configuration.version).bind(&configuration.flow).bind(&configuration.flow_digest).execute(&mut *tx).await?;
    }
    sqlx::query("INSERT INTO mobile_runtime_configuration_current(capability,configuration_id) VALUES($1,$2) ON CONFLICT(capability) DO UPDATE SET configuration_id=$2,updated_at=clock_timestamp() WHERE mobile_runtime_configuration_current.configuration_id<>$2")
        .bind(&configuration.capability).bind(configuration.id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}
/// Deliberately excludes raw rules, artifacts and credentials.
pub async fn status(pool: &PgPool) -> Result<Value> {
    let mut tx = pool.begin().await?;
    let result = status_in(&mut tx).await?;
    tx.commit().await?;
    Ok(json!(result))
}
pub(super) async fn status_in(tx: &mut Tx<'_>) -> Result<RuntimeConfigurationStatus> {
    Ok(RuntimeConfigurationStatus::project(
        &current(tx).await?,
        chrono::Utc::now(),
    ))
}
pub async fn verify(pool: &PgPool, expected: &RuntimeConfiguration) -> Result<Value> {
    expected.validate()?;
    let mut tx = pool.begin().await?;
    let actual = status_in(&mut tx).await?;
    tx.commit().await?;
    require(
        actual.matches(expected),
        "configuration_deployment_mismatch",
    )?;
    Ok(json!(actual))
}
pub(super) async fn current(tx: &mut Tx<'_>) -> Result<Value> {
    Ok(sqlx::query_scalar("SELECT jsonb_build_object('id',c.id,'version',c.version,'flow',c.flow,'flow_digest',c.flow_digest) FROM mobile_runtime_configuration_current p JOIN mobile_runtime_configurations c ON c.id=p.configuration_id AND c.capability=p.capability WHERE p.capability='wecom.group_text'")
        .fetch_optional(&mut **tx).await?.unwrap_or(Value::Null))
}
