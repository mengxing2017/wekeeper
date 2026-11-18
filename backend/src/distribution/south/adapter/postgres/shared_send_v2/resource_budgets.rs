//! Cumulative resource budgets are independent of a unit's periodic scheme limits.
use super::*;
pub(super) async fn check(tx: &mut Tx<'_>, binding: Uuid) -> Result<bool> {
    sqlx::query_scalar("SELECT b.enabled AND (b.valid_until IS NULL OR b.valid_until>clock_timestamp()+interval '5 seconds') AND (b.max_calls IS NULL OR (SELECT count(*) FROM mobile_execution_resource_charges c JOIN mobile_execution_grants g ON g.id=c.execution_id LEFT JOIN mobile_v9_attempts a ON a.id=g.attempt_id WHERE c.budget_root=b.budget_root AND CASE WHEN a.state IN ('observed_sent','manually_confirmed_sent') OR g.result_body->>'outcome'='observed_sent' THEN true WHEN a.state='verified_not_sent' THEN false ELSE coalesce(g.result_body->>'outcome','pending') NOT IN ('not_called','failed') END)<b.max_calls) FROM mobile_device_target_bindings b WHERE b.id=$1")
        .bind(binding).fetch_one(&mut **tx).await.map_err(Into::into)
}
pub(super) async fn record(tx: &mut Tx<'_>, execution: Uuid, binding: Uuid) -> Result<()> {
    sqlx::query("INSERT INTO mobile_execution_resource_charges(execution_id,binding_id,budget_root) SELECT $1,id,budget_root FROM mobile_device_target_bindings WHERE id=$2 ON CONFLICT DO NOTHING")
        .bind(execution).bind(binding).execute(&mut **tx).await?;
    Ok(())
}
pub(super) async fn expiry(
    tx: &mut Tx<'_>,
    device: Uuid,
    target: Uuid,
) -> Result<Option<chrono::DateTime<chrono::Utc>>> {
    sqlx::query_scalar(
        "SELECT valid_until FROM mobile_device_target_bindings WHERE device_id=$1 AND target_id=$2",
    )
    .bind(device)
    .bind(target)
    .fetch_one(&mut **tx)
    .await
    .map_err(Into::into)
}
