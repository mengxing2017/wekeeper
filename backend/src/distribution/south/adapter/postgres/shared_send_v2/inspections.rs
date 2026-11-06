//! Inspections carry no message and cannot obtain a send invocation.
use super::*;
pub(super) async fn request(
    tx: &mut Tx<'_>,
    a: &Actor,
    s: Uuid,
    v: InspectionInput,
) -> Result<Value> {
    let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_device_target_bindings WHERE school_id=$1 AND target_id=$2 AND device_id=$3 AND enabled)").bind(s).bind(v.target_id).bind(v.device_id).fetch_one(&mut **tx).await?;
    require(exists, "shared_binding_missing")?;
    let _ = a;
    let status = readiness::school(tx, s)
        .await?
        .into_iter()
        .find(|r| r["device_id"] == json!(v.device_id) && r["target_id"] == json!(v.target_id))
        .ok_or(Error::NotFound)?;
    Ok(json!({"id":status["binding_id"],"state":status["state"],"readiness":status}))
}
pub(super) async fn busy(tx: &mut Tx<'_>, device: Uuid) -> Result<bool> {
    Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_execution_grants WHERE device_id=$1 AND NOT session_closed)").bind(device).fetch_one(&mut **tx).await?)
}
