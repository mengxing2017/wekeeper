//! Platform business settings never publish technical capability or reset execution ledgers.
use super::*;
use crate::distribution::domain::mobile_execution::usage::SendingStatus;
pub(super) async fn status(tx: &mut Tx<'_>) -> Result<SendingStatus> {
    let delivery:String=sqlx::query_scalar("SELECT CASE WHEN EXISTS(SELECT 1 FROM mobile_v9_activations a JOIN mobile_capability_deliveries d ON d.release_id=a.release_id JOIN mobile_v9_acceptance_runs c ON c.id=d.acceptance_id AND c.release_id=a.release_id WHERE a.capability='wecom.group_text' AND c.stage='production_allowlist' AND NOT EXISTS(SELECT 1 FROM mobile_v9_adapter_withdrawals w WHERE w.release_id=a.release_id)) THEN 'available' WHEN EXISTS(SELECT 1 FROM mobile_v9_activations a JOIN mobile_v9_adapter_withdrawals w ON w.release_id=a.release_id WHERE a.capability='wecom.group_text') THEN 'withdrawn' WHEN EXISTS(SELECT 1 FROM mobile_capability_deliveries) THEN 'inactive' ELSE 'missing' END").fetch_one(&mut **tx).await?;
    let p = policy(tx).await?;
    Ok(SendingStatus::project(
        &delivery,
        p.as_ref().and_then(|p| p["mode"].as_str()),
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
    if v.mode == "enabled" && v.allowed_stages.iter().any(|s| s == "production_allowlist") {
        require(
            status(tx).await?.delivery == "available",
            "shared_send_delivery_required",
        )?;
    }
    let revision = policy(tx)
        .await?
        .and_then(|p| p["revision"].as_i64())
        .unwrap_or(0);
    require(revision == v.expected_revision, "revision_conflict")?;
    let id = Uuid::now_v7();
    sqlx::query("INSERT INTO mobile_v9_send_policy_versions(id,revision,mode,limits,created_by,reason,timezone,allowed_windows,allowed_stages,confirmation_seconds,inspection_seconds,retry_seconds,retry_limit) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)").bind(id).bind(revision+1).bind(v.mode).bind(json!(v.limits)).bind(a.id).bind(v.reason).bind(v.timezone).bind(json!(v.allowed_windows)).bind(json!(v.allowed_stages)).bind(v.confirmation_seconds).bind(v.inspection_seconds).bind(v.retry_seconds).bind(v.retry_limit).execute(&mut **tx).await?;
    usage::reconcile(tx).await?;
    Ok(json!({"id":id,"revision":revision+1}))
}

pub(super) async fn ensure_open(tx: &mut Tx<'_>) -> Result<()> {
    let s = status(tx).await?;
    let code = match s.delivery.as_str() {
        "missing" => "shared_send_delivery_missing",
        "inactive" => "shared_send_delivery_inactive",
        "withdrawn" => "shared_send_delivery_withdrawn",
        _ => match s.platform.as_str() {
            "unconfigured" => "shared_send_scheme_missing",
            "preview_only" | "draft_only" => "shared_send_preparation_only",
            "unsupported" => "shared_send_scheme_unsupported",
            _ => "shared_send_platform_closed",
        },
    };
    require(s.can_confirm, code)
}
