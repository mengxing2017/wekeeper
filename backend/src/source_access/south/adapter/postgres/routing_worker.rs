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
    let technical = matches!(
        reason,
        "temporarily_unavailable" | "ad_selection_stale" | "ad_configuration_unavailable"
    );
    sqlx::query("WITH waiting AS (UPDATE source_inbox SET state='waiting',job_state=$3,reason_class=$4,failures=failures+CASE WHEN $5 THEN 1 ELSE 0 END,last_error=$2,next_attempt_at=now()+make_interval(secs=>CASE WHEN $5 THEN LEAST(300,5*power(2,LEAST(failures,6)))+random()*2 ELSE 30 END) WHERE receipt_id=$1 AND state IN ('received','waiting') RETURNING receipt_id) UPDATE source_receipts SET reason_code=$2,processing_stage=CASE WHEN $5 THEN 'processing_retry' ELSE 'configuration_wait' END,updated_at=clock_timestamp() WHERE receipt_id IN (SELECT receipt_id FROM waiting)").bind(receipt).bind(reason).bind(if technical {"retry_wait"}else{"configuration_error"}).bind(if technical {"technical"}else{"configuration"}).bind(technical).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}
pub(super) async fn halt(
    tx: &mut Transaction<'_, Postgres>,
    old: Uuid,
    reason: &str,
) -> Result<bool> {
    // Return whether a submission may already exist; such content updates never auto-resend.
    let tasks=sqlx::query("SELECT task_id,channel FROM source_execution_generations WHERE receipt_id=$1 AND ($2<>'composition_superseded' OR task_id=(SELECT task_id FROM source_execution_tasks WHERE receipt_id=$1)) ORDER BY generation").bind(old).bind(reason).fetch_all(&mut **tx).await?;
    if tasks.is_empty() {
        return Ok(false);
    }
    if reason != "composition_superseded" {
        sqlx::query("UPDATE source_automation_tasks SET stopped=true,stop_reason=COALESCE(stop_reason,$2),revision=revision+1 WHERE id=$1 AND NOT stopped").bind(old).bind(reason).execute(&mut **tx).await?;
    }
    if reason == "composition_superseded" {
        sqlx::query("SELECT set_config('wekeeper.composition_replacement','on',true)")
            .execute(&mut **tx)
            .await?;
    }

    let mut any_submitted = false;
    for task in tasks {
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
            if reason != "operator_stopped" && reason != "composition_superseded" {
                sqlx::query("INSERT INTO broadcast_operations(id,school_id,task_id,actor_id,operation,idempotency_key,request_hash,state) SELECT gen_random_uuid(),workspace_id,$2,authorization_actor_id,'stop','source-stop:'||$2::text,$3,'done' FROM source_execution_tasks WHERE receipt_id=$1 ON CONFLICT DO NOTHING").bind(old).bind(id).bind(reason).execute(&mut **tx).await?;
            }
            submitted
        };
        any_submitted |= submitted;
    }
    Ok(any_submitted)
}
impl RoutingRepository {
    pub async fn run_once(&self) -> Result<()> {
        if let Err(error) = self.recover_orchestration().await {
            tracing::warn!(
                code = error.0,
                "source orchestration recovery deferred; intake and withdrawals continue"
            );
        }

        let withdrawals:Vec<Uuid>=sqlx::query_scalar("SELECT receipt_id FROM source_inbox WHERE event_type='content.withdrawn' AND state IN ('received','waiting') AND next_attempt_at<=now() ORDER BY next_attempt_at,receipt_id LIMIT 50").fetch_all(&self.0).await?;
        for id in withdrawals {
            if let Err(e) = self.process(id).await {
                wait(&self.0, id, e.0).await?;
            }
        }
        if let Err(error) = self.recover_checks().await {
            tracing::warn!(
                code = error.0,
                "keyword recovery will retry; withdrawals remain independent"
            );
        }
        if let Err(error) =
            crate::advertising::south::adapter::postgres::pipeline::recover(&self.0).await
        {
            tracing::warn!(code = error.0, "advertisement recovery will retry");
        }
        let rows:Vec<(String,Uuid)>=sqlx::query_as("WITH due AS (SELECT r.route_id,i.receipt_id,row_number() OVER(PARTITION BY r.route_id ORDER BY r.receive_order,i.receipt_id) AS route_position FROM source_inbox i JOIN source_receipts r USING(receipt_id) WHERE i.state IN ('received','waiting') AND i.event_type<>'content.withdrawn' AND i.next_attempt_at<=now()) SELECT route_id,receipt_id FROM due ORDER BY route_position,route_id LIMIT 50").fetch_all(&self.0).await?;
        let mut by_route = std::collections::BTreeMap::<String, Vec<Uuid>>::new();
        for (route, id) in rows {
            by_route.entry(route).or_default().push(id);
        }
        let mut groups = by_route.into_values();
        let mut jobs = tokio::task::JoinSet::new();
        loop {
            while jobs.len() < 4 {
                let Some(ids) = groups.next() else { break };
                let repo = self.clone();
                jobs.spawn(async move {
                for id in ids {
                    if let Err(e)=repo.process(id).await {
                        if matches!(e.0,"channel_content_invalid"|"route_retired") {
                            let mut tx=repo.0.begin().await?;
                            super::projection::transition(&mut tx,id,"applied","execution_stopped",Some(e.0),"stopped").await?;
                            tx.commit().await?;
                        } else {wait(&repo.0,id,e.0).await?;}
                        tracing::warn!(receipt=%id,code=e.0,"source execution processing deferred");
                    }
                }
                Ok::<(),Fault>(())
            });
            }
            let Some(result) = jobs.join_next().await else {
                break;
            };
            if !matches!(result, Ok(Ok(()))) {
                tracing::warn!("source route batch failed; independent routes continue");
            }
        }
        self.retry_waiting().await?;
        self.aggregate().await
    }
    async fn process(&self, receipt: Uuid) -> Result<()> {
        use crate::advertising::south::adapter::postgres::queue;
        let withdrawn: bool = sqlx::query_scalar(
            "SELECT event_type='content.withdrawn' FROM source_inbox WHERE receipt_id=$1",
        )
        .bind(receipt)
        .fetch_one(&self.0)
        .await?;
        if withdrawn {
            return self.process_claimed(receipt, None).await;
        }
        let Some(version) = queue::claim(&self.0, receipt).await? else {
            return Ok(());
        };
        let result = self.process_claimed(receipt, Some(version)).await;
        queue::release(&self.0, receipt, version, result).await
    }
    async fn process_claimed(&self, receipt: Uuid, reservation: Option<i64>) -> Result<()> {
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
        let Some(prepared) = crate::advertising::south::adapter::postgres::pipeline::prepare(
            &self.0, receipt, &event, &snapshot,
        )
        .await?
        else {
            return Ok(());
        };
        let message = &prepared.content;
        let mut tx = self.0.begin().await?;
        execution::source_guard(&mut tx)
            .await
            .map_err(execution_error)?;
        sqlx::query("SELECT source_id FROM source_callers WHERE source_id=$1 FOR SHARE")
            .bind(source)
            .fetch_one(&mut *tx)
            .await?;
        // Projection/withdrawal owns this same route row. Read content validity only
        // after locking it, otherwise a withdrawal could commit while we wait later.
        sqlx::query("SELECT route_id FROM source_routes WHERE route_id=$1 FOR UPDATE")
            .bind(&route)
            .fetch_optional(&mut *tx)
            .await?;
        if let Some(version) = reservation {
            crate::advertising::south::adapter::postgres::queue::validate(
                &mut tx, receipt, version,
            )
            .await?;
        }
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
        if !exists {
            let current_revision: Option<i64> =
                sqlx::query_scalar("SELECT config_revision FROM source_routes WHERE route_id=$1")
                    .bind(&route)
                    .fetch_optional(&mut *tx)
                    .await?;
            ensure(
                current_revision == Some(snapshot.revision),
                "ad_selection_stale",
            )?;
        }
        let stopped: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM source_automation_tasks WHERE id=$1 AND stopped)",
        )
        .bind(receipt)
        .fetch_one(&mut *tx)
        .await?;
        if stopped {
            finish(&mut tx, receipt, "stopped", Some("message_stopped")).await?;
            tx.commit().await?;
            return Ok(());
        }
        crate::advertising::south::adapter::postgres::pipeline::validate(
            &mut tx, receipt, &snapshot, &prepared,
        )
        .await?;
        if exists && !prepared.replacement {
            sqlx::query("DELETE FROM advertisement_target_blocks WHERE receipt_id=$1")
                .bind(receipt)
                .execute(&mut *tx)
                .await?;
            finish(&mut tx, receipt, "task_created", None).await?;
            tx.commit().await?;
            return Ok(());
        }
        let (composition, generation, snapshot_id) =
            crate::advertising::south::adapter::postgres::pipeline::save(
                &mut tx, receipt, &snapshot, &prepared,
            )
            .await?;
        if exists {
            ensure(
                !halt(&mut tx, receipt, "composition_superseded").await?,
                "ad_selection_stale",
            )?;
            sqlx::query("UPDATE source_execution_tasks SET composition_id=$2,generation=$3 WHERE receipt_id=$1").bind(receipt).bind(composition).bind(generation).execute(&mut *tx).await?;
        }
        for target in &mut snapshot.targets {
            *target = guard_submission(&mut tx, receipt, target.id).await?;
        }
        if !exists {
            sqlx::query("UPDATE source_routing_proposals SET snapshot=$2,target_digest=$3 WHERE receipt_id=$1").bind(receipt).bind(json!(snapshot)).bind(digest(json!(snapshot.targets).to_string().as_bytes())).execute(&mut *tx).await?;
        }
        let authorizer:Uuid=sqlx::query_scalar("SELECT authorized_by FROM source_routes WHERE route_id=$1 AND authorized_by IS NOT NULL").bind(&route).fetch_one(&mut *tx).await?;
        let config = snapshot
            .execution
            .as_ref()
            .ok_or(Fault("route_configuration_required"))?;
        let task=match config.channel {
   Channel::Mobile=>crate::distribution::south::adapter::postgres::shared_send_v2::source_routing::create(&mut tx,&snapshot,message,authorizer,snapshot_id).await.map_err(execution_error)?,
   Channel::Wecom=>execution::create_official_task(&mut tx,&snapshot,message,authorizer,snapshot_id).await.map_err(execution_error)?
  };
        if exists && config.channel == Channel::Mobile {
            // A new physical child must not renew the original business deadline or dispatch budget.
            sqlx::query("UPDATE mobile_v9_tasks n SET confirmed_until=o.confirmed_until,next_eligible_at=o.next_eligible_at FROM mobile_v9_tasks o JOIN source_execution_tasks l ON l.task_id=o.id WHERE n.id=$1 AND l.receipt_id=$2").bind(task).bind(receipt).execute(&mut *tx).await?;
            sqlx::query("UPDATE mobile_v9_dispatch_state n SET completed_rounds=o.completed_rounds,round_origin=o.round_origin,candidate_cursor=o.candidate_cursor,assigned_at=o.assigned_at,next_retry_at=o.next_retry_at,state=o.state,reason=o.reason,finished_at=o.finished_at,infrastructure_failures=o.infrastructure_failures FROM mobile_v9_task_items ni JOIN source_execution_tasks l ON l.receipt_id=$2 JOIN mobile_v9_task_items oi ON oi.task_id=l.task_id AND oi.target_id=ni.target_id JOIN mobile_v9_dispatch_state o ON o.item_id=oi.id WHERE ni.task_id=$1 AND n.item_id=ni.id").bind(task).bind(receipt).execute(&mut *tx).await?;
        }
        sqlx::query("INSERT INTO source_execution_tasks(receipt_id,task_id,channel,workspace_id,authorization_actor_id,config_revision,content_digest,target_digest,composition_id,generation) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) ON CONFLICT(receipt_id) DO UPDATE SET task_id=$2,content_digest=$7,composition_id=$9,generation=$10").bind(receipt).bind(task).bind(config.channel.as_str()).bind(config.workspace_id).bind(authorizer).bind(snapshot.revision).bind(digest(message.as_bytes())).bind(digest(json!(snapshot.targets).to_string().as_bytes())).bind(composition).bind(generation).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM advertisement_target_blocks WHERE receipt_id=$1")
            .bind(receipt)
            .execute(&mut *tx)
            .await?;
        audit(&mut tx,source,None,None,"source.task.created","source-routing",json!({"receipt_id":receipt,"task_id":task,"authorization_actor":authorizer,"config_revision":snapshot.revision})).await?;
        finish(&mut tx, receipt, "task_created", None).await?;
        sqlx::query("UPDATE advertisement_pool_queue SET state='done',reservation_expiry=NULL,updated_at=clock_timestamp() WHERE receipt_id=$1").bind(receipt).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn aggregate(&self) -> Result<()> {
        let mut tx = self.0.begin().await?;
        execution::source_guard(&mut tx)
            .await
            .map_err(execution_error)?;
        sqlx::query("INSERT INTO source_summary_queue SELECT l.receipt_id FROM source_execution_tasks l JOIN source_receipts r USING(receipt_id) WHERE r.summary_version IS NULL ORDER BY l.created_at LIMIT 50 ON CONFLICT DO NOTHING").execute(&mut *tx).await?;
        let ids:Vec<Uuid>=sqlx::query_scalar("SELECT receipt_id FROM source_summary_queue ORDER BY receipt_id LIMIT 50 FOR UPDATE SKIP LOCKED").fetch_all(&mut *tx).await?;
        sqlx::query("WITH results AS (SELECT l.receipt_id,i.target_id AS logical_target, CASE WHEN EXISTS(SELECT 1 FROM mobile_v9_attempts a WHERE a.item_id=i.id AND a.state IN ('observed_sent','manually_confirmed_sent')) THEN 'sent' WHEN EXISTS(SELECT 1 FROM mobile_v9_attempts a LEFT JOIN mobile_execution_grants g ON g.attempt_id=a.id WHERE a.item_id=i.id AND (a.state='unknown' OR (g.id IS NOT NULL AND g.result_body IS NULL AND (t.stopped OR NOT a.current)))) THEN 'unknown' WHEN EXISTS(SELECT 1 FROM keyword_target_blocks kb WHERE kb.receipt_id=l.receipt_id AND kb.target_id=i.target_id) OR EXISTS(SELECT 1 FROM advertisement_target_blocks ab WHERE ab.receipt_id=l.receipt_id AND ab.target_id=i.target_id) OR t.stopped OR ds.state IN ('dispatch_failed','stopped','confirmation_expired') THEN 'failed' ELSE 'pending' END AS result FROM source_execution_generations l JOIN mobile_v9_tasks t ON t.id=l.task_id JOIN mobile_v9_task_items i ON i.task_id=l.task_id JOIN mobile_v9_dispatch_state ds ON ds.item_id=i.id WHERE l.channel='mobile' AND l.receipt_id=ANY($1) AND (l.task_id=(SELECT cur.task_id FROM source_execution_tasks cur WHERE cur.receipt_id=l.receipt_id) OR EXISTS(SELECT 1 FROM mobile_v9_attempts a LEFT JOIN mobile_execution_grants g ON g.attempt_id=a.id WHERE a.item_id=i.id AND (g.id IS NOT NULL OR a.state IN ('observed_sent','manually_confirmed_sent','unknown')))) UNION ALL SELECT l.receipt_id,x.group_id AS logical_target, CASE WHEN x.official_status=1 THEN 'sent' WHEN b.state='unknown' OR b.stop_state='unknown' THEN 'unknown' WHEN EXISTS(SELECT 1 FROM keyword_target_blocks kb WHERE kb.receipt_id=l.receipt_id AND kb.target_id=x.group_id) OR EXISTS(SELECT 1 FROM advertisement_target_blocks ab WHERE ab.receipt_id=l.receipt_id AND ab.target_id=x.group_id) OR b.stop_state='stopped' OR b.state IN ('rejected','cancelled') OR x.rejected OR x.official_status IN (2,3) THEN 'failed' ELSE 'pending' END FROM source_execution_generations l JOIN broadcast_targets x ON x.task_id=l.task_id JOIN broadcast_batches b ON b.id=x.batch_id WHERE l.channel='wecom' AND l.receipt_id=ANY($1) AND (l.task_id=(SELECT cur.task_id FROM source_execution_tasks cur WHERE cur.receipt_id=l.receipt_id) OR b.msgid IS NOT NULL OR b.state NOT IN ('queued','cancelled') OR b.stop_state='unknown')), logical AS (SELECT receipt_id,logical_target,CASE WHEN bool_or(result='sent') THEN 'sent' WHEN bool_or(result='unknown') THEN 'unknown' WHEN bool_or(result='pending') THEN 'pending' ELSE 'failed' END AS result FROM results GROUP BY receipt_id,logical_target), summaries AS (SELECT receipt_id,count(*) AS total,jsonb_build_object('pending',count(*) FILTER(WHERE result='pending'),'sent',count(*) FILTER(WHERE result='sent'),'failed',count(*) FILTER(WHERE result='failed'),'unknown',count(*) FILTER(WHERE result='unknown')) AS summary FROM logical GROUP BY receipt_id) UPDATE source_receipts r SET sending=x.summary,summary_total=x.total,summary_version='targets/1',summary_revision=r.summary_revision+1,summary_updated_at=clock_timestamp(),updated_at=clock_timestamp() FROM summaries x WHERE r.receipt_id=x.receipt_id AND (r.sending IS DISTINCT FROM x.summary OR r.summary_version IS NULL)").bind(&ids).execute(&mut *tx).await?;
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
