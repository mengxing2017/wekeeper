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
    sqlx::query("UPDATE source_inbox SET state='waiting',next_attempt_at=now()+interval '30 seconds' WHERE receipt_id=$1 AND state IN ('received','waiting')").bind(receipt).execute(&mut *tx).await?;
    sqlx::query("UPDATE source_receipts SET reason_code=$2,processing_stage='configuration_wait',updated_at=now() WHERE receipt_id=$1 AND status='received'").bind(receipt).bind(reason).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}
async fn halt(tx: &mut Transaction<'_, Postgres>, old: Uuid) -> Result<bool> {
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
        let submitted:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_v9_task_items i JOIN mobile_v9_attempts a ON a.item_id=i.id LEFT JOIN mobile_execution_grants g ON g.attempt_id=a.id WHERE i.task_id=$1 AND (g.id IS NOT NULL OR a.state IN ('observed_sent','manually_confirmed_sent')))").bind(id).fetch_one(&mut **tx).await?;
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
        sqlx::query("UPDATE broadcast_batches SET state='cancelled',error_code='source_content_withdrawn' WHERE task_id=$1 AND state='queued'").bind(id).execute(&mut **tx).await?;
        submitted
    };
    Ok(submitted)
}
impl RoutingRepository {
    pub async fn run_once(&self) -> Result<()> {
        let ids:Vec<Uuid>=sqlx::query_scalar("SELECT i.receipt_id FROM source_inbox i JOIN source_receipts r USING(receipt_id) WHERE i.state IN ('received','waiting') AND i.next_attempt_at<=now() ORDER BY i.next_attempt_at,i.receipt_id LIMIT 50").fetch_all(&self.0).await?;
        for id in ids {
            if let Err(e) = self.process(id).await {
                if matches!(e.0, "channel_content_invalid" | "route_retired") {
                    let mut tx = self.0.begin().await?;
                    finish(&mut tx, id, "rejected", Some(e.0)).await?;
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
        let mut tx = self.0.begin().await?;
        execution::source_guard(&mut tx)
            .await
            .map_err(execution_error)?;
        sqlx::query("SELECT source_id FROM source_callers WHERE source_id=$1 FOR UPDATE")
            .bind(source)
            .fetch_one(&mut *tx)
            .await?;
        let state: String =
            sqlx::query_scalar("SELECT state FROM source_inbox WHERE receipt_id=$1 FOR UPDATE")
                .bind(receipt)
                .fetch_one(&mut *tx)
                .await?;
        if !matches!(state.as_str(), "received" | "waiting") {
            return Ok(());
        }
        let prior=sqlx::query("SELECT * FROM source_content_versions WHERE source_id=$1 AND route_id=$2 AND external_content_id=$3 FOR UPDATE").bind(source).bind(&route).bind(&event.external_content_id).fetch_optional(&mut *tx).await?;
        if let Some(prior) = prior {
            let sequence: i64 = prior.get("sequence");
            let previous: Uuid = prior.get("receipt_id");
            if previous != receipt && event.sequence <= sequence {
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
            if previous != receipt {
                let submitted = halt(&mut tx, previous).await?;
                sqlx::query("UPDATE source_routing_waits SET stopped=true WHERE receipt_id=$1")
                    .bind(previous)
                    .execute(&mut *tx)
                    .await?;
                if event.event_type == "content.upsert" && submitted {
                    sqlx::query("UPDATE source_content_versions SET sequence=$4,version=$5,withdrawn=false,receipt_id=$6,updated_at=now() WHERE source_id=$1 AND route_id=$2 AND external_content_id=$3").bind(source).bind(&route).bind(&event.external_content_id).bind(event.sequence).bind(&event.version).bind(receipt).execute(&mut *tx).await?;
                    finish(
                        &mut tx,
                        receipt,
                        "routed",
                        Some("content_updated_no_resend"),
                    )
                    .await?;
                    tx.commit().await?;
                    return Ok(());
                }
            }
        }
        sqlx::query("INSERT INTO source_content_versions(source_id,route_id,external_content_id,sequence,version,receipt_id,withdrawn) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT(source_id,route_id,external_content_id) DO UPDATE SET sequence=$4,version=$5,receipt_id=$6,withdrawn=$7,updated_at=now()").bind(source).bind(&route).bind(&event.external_content_id).bind(event.sequence).bind(&event.version).bind(receipt).bind(event.event_type=="content.withdrawn").execute(&mut *tx).await?;
        if event.event_type == "content.withdrawn" {
            finish(&mut tx, receipt, "withdrawn", None).await?;
            tx.commit().await?;
            return Ok(());
        }
        // Projection and latest sequence are durable before waiting for mutable route configuration.
        tx.commit().await?;
        let mut snapshot = match self.resolve(receipt).await? {
            Resolution::Waiting { reason_code } => {
                if reason_code == "route_retired" {
                    let mut tx = self.0.begin().await?;
                    finish(&mut tx, receipt, "rejected", Some("route_retired")).await?;
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
        finish(&mut tx, receipt, "routed", None).await?;
        tx.commit().await?;
        Ok(())
    }
    async fn aggregate(&self) -> Result<()> {
        // Results are read from the existing channel ledgers; receipt acceptance is never a send result.
        sqlx::query("WITH results AS (SELECT l.receipt_id, CASE WHEN EXISTS(SELECT 1 FROM mobile_v9_attempts a WHERE a.item_id=i.id AND a.state IN ('observed_sent','manually_confirmed_sent')) THEN 'sent' WHEN EXISTS(SELECT 1 FROM mobile_v9_attempts a WHERE a.item_id=i.id AND a.state='unknown') THEN 'unknown' WHEN ds.state IN ('dispatch_failed','stopped','confirmation_expired') THEN 'failed' ELSE 'pending' END AS result FROM source_execution_tasks l JOIN mobile_v9_task_items i ON i.task_id=l.task_id JOIN mobile_v9_dispatch_state ds ON ds.item_id=i.id WHERE l.channel='mobile' UNION ALL SELECT l.receipt_id, CASE WHEN x.official_status=1 THEN 'sent' WHEN b.state='unknown' THEN 'unknown' WHEN b.state IN ('rejected','cancelled') OR x.rejected OR x.official_status IN (2,3) THEN 'failed' ELSE 'pending' END FROM source_execution_tasks l JOIN broadcast_targets x ON x.task_id=l.task_id JOIN broadcast_batches b ON b.id=x.batch_id WHERE l.channel='wecom'), summaries AS (SELECT receipt_id,jsonb_build_object('pending',count(*) FILTER(WHERE result='pending'),'sent',count(*) FILTER(WHERE result='sent'),'failed',count(*) FILTER(WHERE result='failed'),'unknown',count(*) FILTER(WHERE result='unknown')) AS summary FROM results GROUP BY receipt_id) UPDATE source_receipts r SET sending=x.summary,updated_at=now() FROM summaries x WHERE r.receipt_id=x.receipt_id AND r.sending IS DISTINCT FROM x.summary").execute(&self.0).await?;
        Ok(())
    }
}
