//! Persistent intake order and expiring, fenced worker ownership. No device lease.
use super::*;
pub(crate) async fn claim(db: &PgPool, receipt: Uuid) -> Result<Option<i64>> {
    let started = std::time::Instant::now();
    let mut tx = db.begin().await?;
    let route: String =
        sqlx::query_scalar("SELECT route_id FROM source_receipts WHERE receipt_id=$1")
            .bind(receipt)
            .fetch_one(&mut *tx)
            .await?;
    // Same short lock as intake: an allocated but uncommitted earlier receipt cannot be skipped.
    sqlx::query("SELECT route_id FROM source_route_receive_counters WHERE route_id=$1 FOR UPDATE")
        .bind(&route)
        .fetch_one(&mut *tx)
        .await?;
    let head:Option<Uuid>=sqlx::query_scalar("SELECT q.receipt_id FROM advertisement_pool_queue q JOIN source_inbox i USING(receipt_id) WHERE q.route_id=$1 AND i.state IN ('received','waiting') AND i.event_type<>'content.withdrawn' AND NOT EXISTS(SELECT 1 FROM source_execution_tasks t WHERE t.receipt_id=q.receipt_id) AND (q.state<>'deferred' OR i.next_attempt_at<=clock_timestamp()) ORDER BY q.receive_order,q.receipt_id LIMIT 1").bind(&route).fetch_optional(&mut *tx).await?;
    let existing: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM source_execution_tasks WHERE receipt_id=$1)",
    )
    .bind(receipt)
    .fetch_one(&mut *tx)
    .await?;
    // Recovery of an already-frozen task never allocates a new slot and must not
    // block fresh events behind historical delivery or asset-store failures.
    if !existing && head != Some(receipt) {
        return Ok(None);
    }
    let version:Option<i64>=sqlx::query_scalar("UPDATE advertisement_pool_queue SET reservation_version=reservation_version+1,reservation_expiry=clock_timestamp()+interval '2 minutes',state='pending',updated_at=clock_timestamp() WHERE receipt_id=$1 AND (reservation_expiry IS NULL OR reservation_expiry<=clock_timestamp()) RETURNING reservation_version").bind(receipt).fetch_optional(&mut *tx).await?;
    tx.commit().await?;
    tracing::debug!(receipt_id=%receipt,route_id=%route,claimed=version.is_some(),claim_millis=started.elapsed().as_millis(),"advertisement queue ownership checked");
    Ok(version)
}
pub(crate) async fn validate(tx: &mut Tx<'_>, receipt: Uuid, version: i64) -> Result<()> {
    let valid:bool=sqlx::query_scalar("SELECT coalesce(reservation_version=$2 AND reservation_expiry>clock_timestamp(),false) FROM advertisement_pool_queue WHERE receipt_id=$1 FOR UPDATE").bind(receipt).bind(version).fetch_one(&mut **tx).await?;
    ensure(valid, "ad_queue_ownership_lost")?;
    let head:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM source_execution_tasks WHERE receipt_id=$1) OR NOT EXISTS(SELECT 1 FROM advertisement_pool_queue earlier JOIN source_inbox i ON i.receipt_id=earlier.receipt_id JOIN advertisement_pool_queue cur ON cur.receipt_id=$1 WHERE earlier.route_id=cur.route_id AND (earlier.receive_order,earlier.receipt_id)<(cur.receive_order,cur.receipt_id) AND i.state IN ('received','waiting') AND i.event_type<>'content.withdrawn' AND NOT EXISTS(SELECT 1 FROM source_execution_tasks t WHERE t.receipt_id=earlier.receipt_id) AND (earlier.state<>'deferred' OR i.next_attempt_at<=clock_timestamp()))").bind(receipt).fetch_one(&mut **tx).await?;
    ensure(head, "ad_queue_head_changed")
}
pub(crate) async fn release(
    db: &PgPool,
    receipt: Uuid,
    version: i64,
    result: Result<()>,
) -> Result<()> {
    let mut tx = db.begin().await?;
    // Only explicit business deferral permits later messages to proceed. Technical retry stays head.
    let row = sqlx::query("SELECT state,last_error FROM source_inbox WHERE receipt_id=$1")
        .bind(receipt)
        .fetch_one(&mut *tx)
        .await?;
    let state: String = row.get("state");
    let reason: Option<String> = row.get("last_error");
    let deferred = result.is_ok() && state == "waiting";
    let next = if deferred {
        "deferred"
    } else if !matches!(state.as_str(), "received" | "waiting") {
        "done"
    } else {
        "pending"
    };
    sqlx::query("UPDATE advertisement_pool_queue SET state=$3,reason=$4,reservation_expiry=NULL,updated_at=clock_timestamp() WHERE receipt_id=$1 AND reservation_version=$2").bind(receipt).bind(version).bind(next).bind(reason).execute(&mut *tx).await?;
    tx.commit().await?;
    result
}
