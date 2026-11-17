use super::*;
pub(super) async fn diagnostic(tx: &mut Tx<'_>, c: &Credential, v: Diagnostic) -> Result<Value> {
    require(
        matches!(
            v.code.as_str(),
            "device_unavailable"
                | "execution_failed"
                | "target_not_found"
                | "target_ambiguous"
                | "execution_timeout"
                | "protocol_error"
        ) && matches!(v.stage.as_str(), "receive" | "execute" | "report")
            && v.details.len() <= 12
            && v.details.iter().all(|(k, s)| {
                [
                    "app_version",
                    "launcher_version",
                    "android_version",
                    "launcher_package",
                    "engine_version",
                ]
                .contains(&k.as_str())
                    && s.len() <= 512
            }),
        "shared_send_diagnostic_invalid",
    )?;
    let failed:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_execution_grants WHERE id=$1 AND device_id=$2 AND credential_generation=$3 AND result_body->>'outcome' IN ('not_called','failed','unknown','inspection_failed'))").bind(v.execution_id).bind(c.device_id).bind(c.generation).fetch_one(&mut **tx).await?;
    require(failed, "execution_diagnostic_without_failure")?;
    sqlx::query("INSERT INTO mobile_execution_diagnostics(id,execution_id,device_id,credential_generation,code,stage,details) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT(id) DO NOTHING").bind(v.event_id).bind(v.execution_id).bind(c.device_id).bind(c.generation).bind(v.code).bind(v.stage).bind(json!(v.details)).execute(&mut **tx).await?;
    Ok(json!({"id":v.event_id,"recorded":true}))
}
