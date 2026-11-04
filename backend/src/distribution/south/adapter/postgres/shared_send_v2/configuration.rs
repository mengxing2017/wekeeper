//! Deployment-owned configuration installation; never a school/browser business step.
use super::*;
use crate::distribution::domain::shared_mobile::runtime_configuration::{
    RuntimeConfiguration, inspect_flow,
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
    let result = current(&mut tx).await?;
    tx.commit().await?;
    let issue = inspect_flow(&result["flow"], &result["flow_digest"]).err();
    Ok(
        json!({"capability":"wecom.group_text","available":issue.is_none(),"configuration_id":result["id"],"version":result["version"],"flow_digest":result["flow_digest"],"issue":issue}),
    )
}
pub(super) async fn current(tx: &mut Tx<'_>) -> Result<Value> {
    Ok(sqlx::query_scalar("SELECT jsonb_build_object('id',c.id,'version',c.version,'flow',c.flow,'flow_digest',c.flow_digest) FROM mobile_runtime_configuration_current p JOIN mobile_runtime_configurations c ON c.id=p.configuration_id AND c.capability=p.capability WHERE p.capability='wecom.group_text'")
        .fetch_optional(&mut **tx).await?.unwrap_or(Value::Null))
}
/// Installation happens before HTTP starts. Missing package keeps unrelated services usable.
pub async fn install_from_environment(pool: &PgPool) -> Result<()> {
    let packaged = std::path::Path::new("/opt/wekeeper/runtime-resources/wecom-group-text.json");
    let source = std::env::var_os("WEKEEPER_SHARED_SEND_CONFIGURATION")
        .filter(|p| !p.is_empty())
        .or_else(|| packaged.is_file().then(|| packaged.as_os_str().to_owned()));
    if let Some(path) = source {
        let raw =
            std::fs::read(path).map_err(|_| Error::Policy("configuration_package_unreadable"))?;
        let configuration = RuntimeConfiguration::parse(&raw)?;
        install(pool, &configuration).await?;
    }
    let state = status(pool).await?;
    if state["available"] != true {
        tracing::error!(
            issue_code = state["issue"]["code"].as_str(),
            "shared mobile execution configuration unavailable; other services remain running"
        );
    }
    Ok(())
}
