//! Platform business settings never publish technical capability or reset execution ledgers.
use super::*;
use crate::distribution::domain::mobile_execution::usage::SendingStatus;
pub(super) async fn status(tx: &mut Tx<'_>) -> Result<SendingStatus> {
    let p = policy(tx).await?;
    Ok(SendingStatus::project(
        p.as_ref()
            .filter(|p| !p["id"].is_null())
            .and_then(|p| p["mode"].as_str()),
        p.as_ref().is_some_and(|p| {
            p["allowed_stages"]
                .as_array()
                .is_some_and(|s| s.contains(&json!("production_allowlist")))
        }),
    ))
}
pub(super) async fn save(tx: &mut Tx<'_>, a: &Actor, v: PolicyInput) -> Result<Value> {
    let valid: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_timezone_names WHERE name=$1)")
            .bind(&v.timezone)
            .fetch_one(&mut **tx)
            .await?;
    require(valid, "shared_send_policy_invalid")?;
    let revision = policy(tx)
        .await?
        .and_then(|p| p["revision"].as_i64())
        .unwrap_or(0);
    require(revision == v.expected_revision, "revision_conflict")?;
    let id = Uuid::now_v7();
    sqlx::query("INSERT INTO mobile_v9_send_policy_versions(id,revision,mode,limits,created_by,reason,timezone,allowed_windows,allowed_stages,confirmation_seconds,inspection_seconds,retry_seconds,retry_limit) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)").bind(id).bind(revision+1).bind(v.mode).bind(json!(v.limits)).bind(a.id).bind(v.reason).bind(v.timezone).bind(json!(v.allowed_windows)).bind(json!(v.allowed_stages)).bind(v.confirmation_seconds).bind(v.inspection_seconds).bind(v.retry_seconds).bind(v.retry_limit).execute(&mut **tx).await?;
    Ok(json!({"id":id,"revision":revision+1}))
}

pub(super) async fn ensure_open(tx: &mut Tx<'_>) -> Result<()> {
    let p = policy(tx)
        .await?
        .ok_or(Error::Policy("shared_send_policy_invalid"))?;
    require(
        p["mode"] == "enabled"
            && p["allowed_stages"]
                .as_array()
                .is_some_and(|s| s.contains(&json!("production_allowlist"))),
        "shared_send_platform_closed",
    )
}
