//! Durable internal handoff from accepted source messages to existing channel tasks.
use super::routing::{RoutingRepository, guard_submission};
use super::*;
use crate::distribution::south::adapter::postgres::routing as execution;
use crate::source_access::{domain::routing::*, south::port::RoutingStore};

fn execution_error(error: crate::business::Error) -> Fault {
    match error {
        crate::business::Error::Invalid(_) => Fault("channel_content_invalid"),
        crate::business::Error::Forbidden => Fault("workspace_authorization_revoked"),
        crate::business::Error::Policy(code) => Fault(code),
        _ => Fault("temporarily_unavailable"),
    }
}
async fn finish(
    tx: &mut Transaction<'_, Postgres>,
    receipt: Uuid,
    state: &str,
    reason: Option<&str>,
) -> Result<()> {
    let status = if state == "ignored_stale" {
        "ignored_stale"
    } else if state == "rejected" {
        "rejected"
    } else {
        "applied"
    };
    sqlx::query("UPDATE source_receipts SET status=$2,reason_code=$3,processing_stage=$4,updated_at=now() WHERE receipt_id=$1").bind(receipt).bind(status).bind(reason).bind(state).execute(&mut **tx).await?;
    sqlx::query("UPDATE source_inbox SET state=$2 WHERE receipt_id=$1")
        .bind(receipt)
        .bind(state)
        .execute(&mut **tx)
        .await?;
    Ok(())
}
async fn wait(pool: &PgPool, receipt: Uuid, reason: &str) -> Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query("WITH waiting AS (UPDATE source_inbox SET state='waiting',failures=failures+1,last_error=$2,next_attempt_at=now()+interval '30 seconds' WHERE receipt_id=$1 AND state IN ('received','waiting') RETURNING receipt_id) UPDATE source_receipts SET reason_code=$2,processing_stage=CASE WHEN status='received' THEN 'processing_retry' ELSE 'configuration_wait' END,updated_at=clock_timestamp() WHERE receipt_id IN (SELECT receipt_id FROM waiting)").bind(receipt).bind(reason).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}
pub(super) async fn halt(
    tx: &mut Transaction<'_, Postgres>,
    old: Uuid,
    reason: &str,
) -> Result<bool> {
    // Return whether a submission may already exist; such content updates never auto-resend.
    let task =
        sqlx::query("SELECT task_id,channel FROM source_execution_tasks WHERE receipt_id=$1")
            .bind(old)
            .fetch_optional(&mut **tx)
            .await?;
    let Some(task) = task else { return Ok(false) };
    let id: Uuid = task.get("task_id");
    let channel: String = task.get("channel");
    let submitted = if channel == "mobile" {
        let submitted:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_v9_task_items i JOIN mobile_v9_attempts a ON a.item_id=i.id LEFT JOIN mobile_execution_grants g ON g.attempt_id=a.id WHERE i.task_id=$1 AND (g.id IS NOT NULL OR a.state IN ('unknown','observed_sent','manually_confirmed_sent')))").bind(id).fetch_one(&mut **tx).await?;
        sqlx::query("UPDATE mobile_v9_tasks SET stopped=true WHERE id=$1")
            .bind(id)
            .execute(&mut **tx)
            .await?;
        submitted
    } else {
        let submitted:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM broadcast_batches WHERE task_id=$1 AND state NOT IN ('queued','cancelled'))").bind(id).fetch_one(&mut **tx).await?;
        sqlx::query("UPDATE broadcast_tasks SET stopped=true WHERE id=$1")
            .bind(id)
            .execute(&mut **tx)
            .await?;
        sqlx::query("UPDATE broadcast_batches SET state='cancelled',error_code=$2 WHERE task_id=$1 AND state='queued'").bind(id).bind(reason).execute(&mut **tx).await?;
        submitted
    };
    Ok(submitted)
}
impl RoutingRepository {
    pub async fn run_once(&self) -> Result<()> {
        self.recover_checks().await?;
        let ids:Vec<Uuid>=sqlx::query_scalar("SELECT i.receipt_id FROM source_inbox i JOIN source_receipts r USING(receipt_id) WHERE i.state IN ('received','waiting') AND i.next_attempt_at<=now() ORDER BY i.next_attempt_at,i.receipt_id LIMIT 50").fetch_all(&self.0).await?;
        for id in ids {
            if let Err(e) = self.process(id).await {
                if matches!(e.0, "channel_content_invalid" | "route_retired") {
                    let mut tx = self.0.begin().await?;
                    super::projection::transition(
                        &mut tx,
                        id,
                        "applied",
                        "execution_stopped",
                        Some(e.0),
                        "stopped",
                    )
                    .await?;
                    tx.commit().await?;
                } else {
                    wait(&self.0, id, e.0).await?;
                }
                tracing::warn!(receipt=%id,code=e.0,"source execution processing deferred");
            }
        }
        self.retry_waiting().await?;
        self.aggregate().await
    }
    async fn process(&self, receipt: Uuid) -> Result<()> {
        let row=sqlx::query("SELECT r.source_id,r.route_id,i.request_body FROM source_receipts r JOIN source_inbox i USING(receipt_id) WHERE r.receipt_id=$1").bind(receipt).fetch_one(&self.0).await?;
        let source: Uuid = row.get("source_id");
        let route: String = row.get("route_id");
        let body: Vec<u8> = row.get("request_body");
        let doc: Document<Event> = serde_json::from_slice(&body)?;
        let event = doc.data.attributes;
        if !self.prepare(receipt, source, &event).await? {
            return Ok(());
        }
        let mut snapshot = match self.resolve(receipt).await? {
            Resolution::Waiting { reason_code } => {
                if reason_code == "route_retired" {
                    let mut tx = self.0.begin().await?;
                    super::projection::transition(
                        &mut tx,
                        receipt,
                        "applied",
                        "execution_stopped",
                        Some("route_retired"),
                        "stopped",
                    )
                    .await?;
                    tx.commit().await?;
                } else {
                    wait(&self.0, receipt, &reason_code).await?;
                }
                return Ok(());
            }
            Resolution::Ready { snapshot } => snapshot,
        };
        let mut tx = self.0.begin().await?;
        execution::source_guard(&mut tx)
            .await
            .map_err(execution_error)?;
        sqlx::query("SELECT source_id FROM source_callers WHERE source_id=$1 FOR UPDATE")
            .bind(source)
            .fetch_one(&mut *tx)
            .await?;
        let current:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM source_content_versions WHERE source_id=$1 AND route_id=$2 AND external_content_id=$3 AND receipt_id=$4 AND NOT withdrawn)").bind(source).bind(&route).bind(&event.external_content_id).bind(receipt).fetch_one(&mut *tx).await?;
        if !current {
            finish(
                &mut tx,
                receipt,
                "ignored_stale",
                Some("content_version_superseded"),
            )
            .await?;
            tx.commit().await?;
            return Ok(());
        }
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM source_execution_tasks WHERE receipt_id=$1)",
        )
        .bind(receipt)
        .fetch_one(&mut *tx)
        .await?;
        if exists {
            finish(&mut tx, receipt, "task_created", None).await?;
            tx.commit().await?;
            return Ok(());
        }
        for target in &mut snapshot.targets {
            *target = guard_submission(&mut tx, receipt, target.id).await?;
        }
        let authorizer:Uuid=sqlx::query_scalar("SELECT authorized_by FROM source_routes WHERE route_id=$1 AND authorized_by IS NOT NULL").bind(&route).fetch_one(&mut *tx).await?;
        let message = format!(
            "{}\n{}\n{}",
            event.title.unwrap_or_default(),
            event.content.unwrap_or_default(),
            event.detail_url
        );
        let config = snapshot
            .execution
            .as_ref()
            .ok_or(Fault("route_configuration_required"))?;
        let task=match config.channel {
   Channel::Mobile=>crate::distribution::south::adapter::postgres::shared_send_v2::source_routing::create(&mut tx,&snapshot,&message,authorizer).await.map_err(execution_error)?,
   Channel::Wecom=>execution::create_official_task(&mut tx,&snapshot,&message,authorizer).await.map_err(execution_error)?
  };
        sqlx::query("INSERT INTO source_execution_tasks(receipt_id,task_id,channel,workspace_id,authorization_actor_id,config_revision,content_digest,target_digest) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(receipt).bind(task).bind(config.channel.as_str()).bind(config.workspace_id).bind(authorizer).bind(snapshot.revision).bind(digest(message.as_bytes())).bind(digest(json!(snapshot.targets).to_string().as_bytes())).execute(&mut *tx).await?;
        audit(&mut tx,source,None,None,"source.task.created","source-routing",json!({"receipt_id":receipt,"task_id":task,"authorization_actor":authorizer,"config_revision":snapshot.revision})).await?;
        finish(&mut tx, receipt, "task_created", None).await?;
        tx.commit().await?;
        Ok(())
    }
    async fn aggregate(&self) -> Result<()> {
        let mut tx = self.0.begin().await?;
        execution::source_guard(&mut tx)
            .await
            .map_err(execution_error)?;
        sqlx::query("INSERT INTO source_summary_queue SELECT l.receipt_id FROM source_execution_tasks l JOIN source_receipts r USING(receipt_id) WHERE r.summary_version IS NULL ORDER BY l.created_at LIMIT 50 ON CONFLICT DO NOTHING").execute(&mut *tx).await?;
        let ids:Vec<Uuid>=sqlx::query_scalar("SELECT receipt_id FROM source_summary_queue ORDER BY receipt_id LIMIT 50 FOR UPDATE SKIP LOCKED").fetch_all(&mut *tx).await?;
        sqlx::query("WITH results AS (SELECT l.receipt_id, CASE WHEN EXISTS(SELECT 1 FROM mobile_v9_attempts a WHERE a.item_id=i.id AND a.state IN ('observed_sent','manually_confirmed_sent')) THEN 'sent' WHEN EXISTS(SELECT 1 FROM mobile_v9_attempts a LEFT JOIN mobile_execution_grants g ON g.attempt_id=a.id WHERE a.item_id=i.id AND (a.state='unknown' OR (g.id IS NOT NULL AND g.result_body IS NULL AND (t.stopped OR NOT a.current)))) THEN 'unknown' WHEN t.stopped OR ds.state IN ('dispatch_failed','stopped','confirmation_expired') THEN 'failed' ELSE 'pending' END AS result FROM source_execution_tasks l JOIN mobile_v9_tasks t ON t.id=l.task_id JOIN mobile_v9_task_items i ON i.task_id=l.task_id JOIN mobile_v9_dispatch_state ds ON ds.item_id=i.id WHERE l.channel='mobile' AND l.receipt_id=ANY($1) UNION ALL SELECT l.receipt_id, CASE WHEN x.official_status=1 THEN 'sent' WHEN b.state='unknown' OR b.stop_state='unknown' THEN 'unknown' WHEN b.stop_state='stopped' OR b.state IN ('rejected','cancelled') OR x.rejected OR x.official_status IN (2,3) THEN 'failed' ELSE 'pending' END FROM source_execution_tasks l JOIN broadcast_targets x ON x.task_id=l.task_id JOIN broadcast_batches b ON b.id=x.batch_id WHERE l.channel='wecom' AND l.receipt_id=ANY($1)), summaries AS (SELECT receipt_id,count(*) AS total,jsonb_build_object('pending',count(*) FILTER(WHERE result='pending'),'sent',count(*) FILTER(WHERE result='sent'),'failed',count(*) FILTER(WHERE result='failed'),'unknown',count(*) FILTER(WHERE result='unknown')) AS summary FROM results GROUP BY receipt_id) UPDATE source_receipts r SET sending=x.summary,summary_total=x.total,summary_version='targets/1',summary_revision=r.summary_revision+1,summary_updated_at=clock_timestamp(),updated_at=clock_timestamp() FROM summaries x WHERE r.receipt_id=x.receipt_id AND (r.sending IS DISTINCT FROM x.summary OR r.summary_version IS NULL)").bind(&ids).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM source_summary_queue WHERE receipt_id=ANY($1)")
            .bind(&ids)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }
}

impl crate::source_access::south::port::InboxWorker for RoutingRepository {
    async fn tick(&self) -> Result<()> {
        self.run_once().await
    }
}
