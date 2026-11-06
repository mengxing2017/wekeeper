//! Business metering survives missing, stale or replaced execution projections.
use super::*;

pub(super) async fn check(
    tx: &mut Tx<'_>,
    school: Uuid,
    device: Uuid,
    target: Uuid,
) -> Result<Vec<Uuid>> {
    let usages = tasks::operative_usages(tx, school, device, target).await?;
    let mut ids = Vec::new();
    for usage in usages {
        // validate_plan has already enforced current controls in this transaction.
        if usage["state"] != "active" || usage["expired"] != false {
            continue;
        }
        let allowed: bool = sqlx::query_scalar(
            "SELECT (SELECT count(*) FROM mobile_usage_invocation_budgets b JOIN mobile_usages u ON u.id=b.usage_id WHERE u.school_id=$1 AND b.budget_root=$2)+(SELECT count(*) FROM mobile_execution_usage_budgets b JOIN mobile_usages u ON u.id=b.usage_id JOIN mobile_execution_grants g ON g.id=b.execution_id WHERE u.school_id=$1 AND b.budget_root=$2 AND coalesce(g.result_body->>'outcome','pending') NOT IN ('not_called','failed'))<$3",
        )
        .bind(school)
        .bind(uid(&usage, "budget_root")?)
        .bind(usage["body"]["max_calls"].as_i64().ok_or(Error::Conflict)?)
        .fetch_one(&mut **tx)
        .await?;
        require(allowed, "quota_wait")?;
        ids.push(uid(&usage, "id")?);
    }
    Ok(ids)
}

/// The caller holds the shared execution fence through check, invocation insert
/// and attribution insert. All roots applicable at issue time retain that charge.
pub(super) async fn record(tx: &mut Tx<'_>, invocation: Uuid, usages: &[Uuid]) -> Result<()> {
    sqlx::query("INSERT INTO mobile_execution_usage_budgets(execution_id,usage_id,budget_root) SELECT $1,u.id,u.budget_root FROM mobile_usages u WHERE u.id=ANY($2) ON CONFLICT DO NOTHING")
        .bind(invocation).bind(usages).execute(&mut **tx).await?;
    Ok(())
}
