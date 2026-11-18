//! Optional advertising participates in the caller's final-content transaction.
//! This adapter never commits a message decision independently.
use crate::advertising::domain::{
    Content,
    decision::{Decision, DecisionInput, SelectedAd, select_one},
};
use sqlx::{Acquire, Postgres, Row, Transaction};
use std::time::Duration;
use tokio::time::{Instant, timeout};

const TOTAL_BUDGET: Duration = Duration::from_secs(1);

struct Budget(Instant);
impl Budget {
    async fn apply(&self, tx: &mut Transaction<'_, Postgres>) -> Result<(), sqlx::Error> {
        let remaining = self.0.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(sqlx::Error::Protocol(
                "advertisement deadline elapsed".into(),
            ));
        }
        let statement = remaining.as_millis().clamp(1, 200);
        sqlx::query(
            "SELECT set_config('statement_timeout',$1,true),set_config('lock_timeout',$2,true)",
        )
        .bind(format!("{statement}ms"))
        .bind(format!("{}ms", statement.min(100)))
        .execute(&mut **tx)
        .await?;
        Ok(())
    }
}

/// On a recoverable ad failure, rollback is acknowledged before returning the
/// original. Connection/rollback failures escape to the durable message retry.
pub async fn decide(
    tx: &mut Transaction<'_, Postgres>,
    input: &DecisionInput,
) -> Result<Decision, sqlx::Error> {
    let started = Instant::now();
    let budget = Budget(started + TOTAL_BUDGET);
    let mut ad = timeout(TOTAL_BUDGET, tx.begin())
        .await
        .map_err(|_| sqlx::Error::Protocol("advertisement savepoint timed out".into()))??;
    let mut observed_count = None;
    let attempt = timeout(
        TOTAL_BUDGET,
        attempt(&mut ad, input, &budget, &mut observed_count),
    )
    .await;
    let decision = match attempt {
        Ok(Ok(decision)) => decision,
        error => {
            let (reason, sqlstate) = match &error {
                Err(_) => ("ad_decision_timeout", None),
                Ok(Err(error)) => {
                    let code = error.as_database_error().and_then(|e| e.code());
                    let reason = match code.as_deref() {
                        Some("55P03") => "ad_lock_timeout",
                        Some("57014") => "ad_decision_timeout",
                        _ => "ad_dependency_unavailable",
                    };
                    (reason, code.map(|v| v.into_owned()))
                }
                Ok(Ok(_)) => unreachable!(),
            };
            // Cancellation alone does not establish rollback. If recovery cannot
            // be confirmed, the outer transaction must not freeze a fallback.
            timeout(TOTAL_BUDGET, ad.rollback())
                .await
                .map_err(|_| sqlx::Error::Protocol("advertisement rollback timed out".into()))??;
            tracing::warn!(message_source_platform=%input.source.platform_id,
                business_unit=%input.source.unit_id, reason, sqlstate,
                elapsed_ms=started.elapsed().as_millis(), error=?error,
                "advertisement skipped after rollback");
            let mut skipped = Decision::skipped(&input.original, reason);
            skipped.selection_count = observed_count;
            return Ok(skipped);
        }
    };
    if decision.selected_ad.is_some() {
        // RELEASE SAVEPOINT only; caller owns COMMIT. Recovery and release
        // have their own bounded acknowledgement window after the attempt.
        timeout(TOTAL_BUDGET, ad.commit()).await.map_err(|_| {
            sqlx::Error::Protocol("advertisement savepoint release timed out".into())
        })??;
    } else {
        // No selection needs no advertising writes or retained counter locks.
        timeout(TOTAL_BUDGET, ad.rollback())
            .await
            .map_err(|_| sqlx::Error::Protocol("advertisement rollback timed out".into()))??;
    }
    Ok(decision)
}

async fn attempt(
    tx: &mut Transaction<'_, Postgres>,
    input: &DecisionInput,
    budget: &Budget,
    observed_count: &mut Option<i64>,
) -> Result<Decision, sqlx::Error> {
    let (statement_timeout, lock_timeout): (String, String) = sqlx::query_as(
        "SELECT current_setting('statement_timeout'),current_setting('lock_timeout')",
    )
    .fetch_one(&mut **tx)
    .await?;
    budget.apply(tx).await?;
    let rows = sqlx::query(
        "WITH matching AS (SELECT p.id,p.material_id,v.name,v.body,v.link, \
            CASE p.scope_kind WHEN 'unit' THEN 2 WHEN 'platform' THEN 1 ELSE 0 END AS priority \
         FROM ad_placements p JOIN ad_materials m ON m.id=p.material_id \
         JOIN ad_material_versions v ON v.id=m.current_version_id AND v.material_id=m.id \
         WHERE p.enabled AND NOT m.archived AND p.valid_from<=clock_timestamp() \
           AND (p.valid_until IS NULL OR clock_timestamp()<p.valid_until) \
           AND (p.scope_kind='global' OR (p.scope_kind='platform' AND p.platform_id=$1) \
                OR (p.scope_kind='unit' AND p.platform_id=$1 AND p.unit_id=$2))) \
         SELECT id,material_id,name,body,link FROM matching WHERE priority=(SELECT max(priority) FROM matching) ORDER BY id"
    ).bind(input.source.platform_id).bind(input.source.unit_id).fetch_all(&mut **tx).await?;
    if rows.is_empty() {
        return Ok(Decision::skipped(&input.original, "no_available_ad"));
    }
    let candidates = rows
        .into_iter()
        .map(|row| {
            Ok(SelectedAd {
                placement_id: row.try_get("id")?,
                material_id: row.try_get("material_id")?,
                content: Content {
                    name: row.try_get("name")?,
                    body: row.try_get("body")?,
                    link: row.try_get("link")?,
                },
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;
    budget.apply(tx).await?;
    sqlx::query(
        "INSERT INTO ad_unit_counters(platform_id,unit_id) VALUES($1,$2) ON CONFLICT DO NOTHING",
    )
    .bind(input.source.platform_id)
    .bind(input.source.unit_id)
    .execute(&mut **tx)
    .await?;
    budget.apply(tx).await?;
    let count: i64 = sqlx::query_scalar("SELECT selected_count FROM ad_unit_counters WHERE platform_id=$1 AND unit_id=$2 FOR UPDATE")
        .bind(input.source.platform_id).bind(input.source.unit_id).fetch_one(&mut **tx).await?;
    *observed_count = Some(count);
    let decision = select_one(input, count, candidates);
    if decision.selected_ad.is_some() {
        budget.apply(tx).await?;
        let updated = sqlx::query(
            "UPDATE ad_unit_counters SET selected_count=$3 WHERE platform_id=$1 AND unit_id=$2",
        )
        .bind(input.source.platform_id)
        .bind(input.source.unit_id)
        .bind(decision.selection_count)
        .execute(&mut **tx)
        .await?;
        if updated.rows_affected() != 1 {
            return Err(sqlx::Error::Protocol(
                "advertisement counter update was not applied".into(),
            ));
        }
        // RELEASE keeps SET LOCAL values, so restore the caller's settings.
        sqlx::query(
            "SELECT set_config('statement_timeout',$1,true),set_config('lock_timeout',$2,true)",
        )
        .bind(statement_timeout)
        .bind(lock_timeout)
        .execute(&mut **tx)
        .await?;
    }
    Ok(decision)
}
