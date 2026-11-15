//! The only automatic sending worker. Decisions and delegates survive restart.
use super::postgres::Repository;
use crate::{
    advertising::{
        domain::{decision::DecisionInput, targeting::SourceIdentity},
        north::local::AdDecisionService,
    },
    business::{Error, Result},
    distribution::{
        domain::receiving::{Channel, DeliveryPlan},
        south::adapter::postgres::{receiving, routing, shared_send_v2},
    },
};
use serde_json::{Value, json};
use sqlx::{Acquire, Postgres, Row, Transaction};
use uuid::Uuid;

impl Repository {
    pub async fn run_once(&self) -> Result<()> {
        let ids:Vec<Uuid>=sqlx::query_scalar("SELECT m.id FROM orchestration_messages m LEFT JOIN orchestration_dispatches d ON d.message_id=m.id WHERE (d.message_id IS NULL OR (d.state IN ('preparing','ready') AND d.next_attempt_at<=clock_timestamp())) ORDER BY m.accepted_at,m.id LIMIT 50")
            .fetch_all(&self.0).await?;
        for id in ids {
            if let Err(error) = self.process(id).await {
                tracing::warn!(message_id=%id,error=?error,"message delegation will retry");
                if matches!(
                    error,
                    Error::Invalid(_)
                        | Error::Policy(_)
                        | Error::Conflict
                        | Error::Forbidden
                        | Error::NotFound
                ) {
                    let mut tx = self.0.begin().await?;
                    sqlx::query("UPDATE orchestration_dispatches SET state='failed',reason_code='channel_resources_missing',updated_at=clock_timestamp() WHERE message_id=$1 AND state IN ('preparing','ready')").bind(id).execute(&mut *tx).await?;
                    let receipt: Uuid = sqlx::query_scalar(
                        "SELECT evidence_ref FROM orchestration_messages WHERE id=$1",
                    )
                    .bind(id)
                    .fetch_one(&mut *tx)
                    .await?;
                    mirror(
                        &mut tx,
                        receipt,
                        "not_dispatched",
                        Some("channel_resources_missing"),
                    )
                    .await?;
                    tx.commit().await?;
                } else {
                    sqlx::query("UPDATE orchestration_dispatches SET failures=failures+1,next_attempt_at=clock_timestamp()+make_interval(secs=>LEAST(300,5*power(2,LEAST(failures,6)))),updated_at=clock_timestamp() WHERE message_id=$1 AND state IN ('preparing','ready')").bind(id).execute(&self.0).await?;
                }
            }
        }
        self.aggregate().await
    }
    async fn process(&self, id: Uuid) -> Result<()> {
        let row=sqlx::query("SELECT platform_id,unit_id,evidence_ref,original FROM orchestration_messages WHERE id=$1").bind(id).fetch_one(&self.0).await?;
        let receipt: Uuid = row.get("evidence_ref");
        let original: Value = row.get("original");
        let text = crate::source_access::domain::keyword_filter::compose(
            original["title"].as_str(),
            original["content"].as_str(),
            original["detail_url"].as_str().unwrap_or(""),
        );
        let mut tx = self.0.begin().await?;
        // A transaction lock, not an in-memory per-message waiting thread.
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,178))")
            .bind(id.to_string())
            .execute(&mut *tx)
            .await?;
        let existing: Option<Value> = sqlx::query_scalar(
            "SELECT to_jsonb(d) FROM orchestration_dispatches d WHERE message_id=$1 FOR UPDATE",
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
        if existing.is_none() {
            let plans = receiving::plans(&mut tx, row.get("unit_id")).await?;
            // Evaluate current global keyword restrictions once. Configuration changes
            // never reinterpret an already frozen message or a delivered attempt.
            let (_, rules) =
                crate::source_access::south::adapter::postgres::keyword_filter::context_for_scope(
                    &mut tx,
                    row.get("platform_id"),
                    String::new(),
                    Some(row.get("unit_id")),
                )
                .await
                .map_err(|_| Error::Unavailable)?;
            let checked = crate::source_access::domain::keyword_filter::evaluate(
                &rules,
                &[
                    ("title", original["title"].as_str().unwrap_or("")),
                    ("content", original["content"].as_str().unwrap_or("")),
                ],
            )
            .map_err(|_| Error::Unavailable)?;
            let reason = if !checked.passed() {
                Some("keyword_blocked")
            } else if plans.is_empty() {
                Some("route_no_targets")
            } else if plans
                .iter()
                .all(|p| p.targets.iter().all(|t| !t.resource_available))
            {
                Some("channel_resources_missing")
            } else if !plans
                .iter()
                .any(|p| crate::advertising::domain::measure(&text, p.channel.as_str()).valid)
            {
                Some("channel_content_invalid")
            } else {
                None
            };
            if let Some(reason) = reason {
                sqlx::query("INSERT INTO orchestration_dispatches(message_id,content,decision,plans,state,reason_code) VALUES($1,$2,'{}',$3,'failed',$4)").bind(id).bind(&text).bind(json!(plans)).bind(reason).execute(&mut *tx).await?;
                mirror(&mut tx, receipt, "not_dispatched", Some(reason)).await?;
                tx.commit().await?;
                return Ok(());
            }
            sqlx::query("INSERT INTO orchestration_dispatches(message_id,content,decision,plans,state) VALUES($1,$2,'{}',$3,'preparing')").bind(id).bind(&text).bind(json!(plans)).execute(&mut *tx).await?;
            mirror(&mut tx, receipt, "delegation_pending", None).await?;
        }
        tx.commit().await?;
        // Persist the target/candidate input before advertising. A crash at any
        // boundary resumes these same facts, even if administrators edit configuration.
        let frozen = sqlx::query(
            "SELECT content,plans,state FROM orchestration_dispatches WHERE message_id=$1",
        )
        .bind(id)
        .fetch_one(&self.0)
        .await?;
        if frozen.get::<String, _>("state") == "preparing" {
            let plans: Vec<DeliveryPlan> =
                serde_json::from_value(frozen.get("plans")).map_err(|_| Error::Unavailable)?;
            let source: String = frozen.get("content");
            let channel = if plans.iter().any(|p| p.channel == Channel::Wecom)
                && crate::advertising::domain::measure(&source, "wecom").valid
            {
                "wecom"
            } else {
                "mobile"
            };
            let decision = AdDecisionService(
                crate::advertising::south::adapter::postgres::Repository(self.0.clone()),
            )
            .decide(DecisionInput {
                source: SourceIdentity {
                    platform_id: row.get("platform_id"),
                    unit_id: row.get("unit_id"),
                },
                message_id: receipt,
                original: source,
                channel: channel.into(),
            })
            .await
            .map_err(|_| Error::Unavailable)?;
            sqlx::query("UPDATE orchestration_dispatches SET content=$2,decision=$3,state='ready' WHERE message_id=$1 AND state='preparing'").bind(id).bind(&decision.content).bind(json!(decision)).execute(&self.0).await?;
        }
        tx = self.0.begin().await?;
        let dispatch=sqlx::query("SELECT content,plans,state FROM orchestration_dispatches WHERE message_id=$1 FOR UPDATE").bind(id).fetch_one(&mut *tx).await?;
        if dispatch.get::<String, _>("state") != "ready" {
            tx.commit().await?;
            return Ok(());
        }
        let plans: Vec<DeliveryPlan> =
            serde_json::from_value(dispatch.get("plans")).map_err(|_| Error::Unavailable)?;
        let content: String = dispatch.get("content");
        for mut plan in plans {
            if !crate::advertising::domain::measure(&content, plan.channel.as_str()).valid {
                failed_delegate(&mut tx, id, &plan, "channel_content_invalid").await?;
                continue;
            }
            let unavailable: Vec<_> = plan
                .targets
                .iter()
                .filter(|t| !t.resource_available)
                .cloned()
                .collect();
            plan.targets.retain(|t| t.resource_available);
            if !unavailable.is_empty() {
                let mut failed = plan.clone();
                failed.id = Uuid::now_v7();
                failed.targets = unavailable;
                failed_delegate(&mut tx, id, &failed, "channel_resources_missing").await?;
            }
            if plan.targets.is_empty() {
                continue;
            }
            // A target's business failure must not roll back other legal targets.
            for target in plan.targets.clone() {
                let mut individual = plan.clone();
                individual.id = Uuid::now_v7();
                individual.targets = vec![target];
                let mut child = tx.begin().await?;
                match delegate(&mut child, id, &individual, &content).await {
                    Ok(()) => child.commit().await?,
                    Err(
                        Error::Invalid(_)
                        | Error::Policy(_)
                        | Error::Forbidden
                        | Error::Conflict
                        | Error::NotFound,
                    ) => {
                        child.rollback().await?;
                        failed_delegate(&mut tx, id, &individual, "channel_resources_missing")
                            .await?;
                    }
                    Err(error) => return Err(error),
                }
            }
        }
        let accepted: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM orchestration_delegates WHERE message_id=$1 AND state='delegated')").bind(id).fetch_one(&mut *tx).await?;
        sqlx::query("UPDATE orchestration_dispatches SET state=$2,reason_code=$3,updated_at=clock_timestamp() WHERE message_id=$1").bind(id).bind(if accepted {"delegated"} else {"failed"}).bind(if accepted {None} else {Some("channel_resources_missing")}).execute(&mut *tx).await?;
        mirror(
            &mut tx,
            receipt,
            if accepted {
                "task_created"
            } else {
                "not_dispatched"
            },
            if accepted {
                None
            } else {
                Some("channel_resources_missing")
            },
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn aggregate(&self) -> Result<()> {
        // Results remain owned by channels; the message list reads their durable facts.
        let mut tx = self.0.begin().await?;
        let ids:Vec<Uuid>=sqlx::query_scalar("SELECT message_id FROM orchestration_result_queue ORDER BY message_id LIMIT 100 FOR UPDATE SKIP LOCKED").fetch_all(&mut *tx).await?;
        sqlx::query("WITH facts AS (SELECT d.message_id,i.id,CASE WHEN EXISTS(SELECT 1 FROM mobile_v9_attempts a WHERE a.item_id=i.id AND a.state IN ('observed_sent','manually_confirmed_sent')) THEN 'sent' WHEN EXISTS(SELECT 1 FROM mobile_v9_attempts a WHERE a.item_id=i.id AND a.state='unknown') THEN 'unknown' WHEN ds.state IN ('dispatch_failed','stopped','confirmation_expired') OR t.stopped THEN 'failed' ELSE 'pending' END AS state FROM orchestration_delegates d JOIN mobile_v9_tasks t ON t.id=d.task_id JOIN mobile_v9_task_items i ON i.task_id=t.id JOIN mobile_v9_dispatch_state ds ON ds.item_id=i.id WHERE d.channel='mobile' AND d.message_id=ANY($1) UNION ALL SELECT d.message_id,x.id,CASE WHEN x.official_status=1 THEN 'sent' WHEN b.state='unknown' OR b.stop_state='unknown' THEN 'unknown' WHEN b.state IN ('rejected','cancelled') OR x.rejected OR x.official_status IN (2,3) THEN 'failed' ELSE 'pending' END FROM orchestration_delegates d JOIN broadcast_targets x ON x.task_id=d.task_id JOIN broadcast_batches b ON b.id=x.batch_id WHERE d.channel='wecom' AND d.message_id=ANY($1) UNION ALL SELECT d.message_id,(target->>'id')::uuid,'failed' FROM orchestration_delegates d CROSS JOIN LATERAL jsonb_array_elements(d.plan->'targets') target WHERE d.state='failed' AND d.message_id=ANY($1)), totals AS (SELECT message_id,count(*) AS total,jsonb_build_object('sent',count(*) FILTER(WHERE state='sent'),'failed',count(*) FILTER(WHERE state='failed'),'unknown',count(*) FILTER(WHERE state='unknown'),'pending',count(*) FILTER(WHERE state='pending')) AS summary FROM facts GROUP BY message_id) UPDATE source_receipts r SET sending=t.summary,summary_total=t.total,summary_version='targets/1',summary_revision=r.summary_revision+1,summary_updated_at=clock_timestamp(),updated_at=clock_timestamp() FROM totals t JOIN orchestration_messages m ON m.id=t.message_id WHERE r.receipt_id=m.evidence_ref AND r.sending IS DISTINCT FROM t.summary").bind(&ids).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM orchestration_result_queue WHERE message_id=ANY($1)")
            .bind(&ids)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }
}
async fn mirror(
    tx: &mut Transaction<'_, Postgres>,
    receipt: Uuid,
    stage: &str,
    reason: Option<&str>,
) -> Result<()> {
    sqlx::query("UPDATE source_receipts SET status='applied',processing_stage=$2,reason_code=$3,updated_at=clock_timestamp() WHERE receipt_id=$1").bind(receipt).bind(stage).bind(reason).execute(&mut **tx).await?;
    sqlx::query("UPDATE source_inbox SET state=$2 WHERE receipt_id=$1")
        .bind(receipt)
        .bind(if stage == "not_dispatched" {
            "stopped"
        } else {
            "task_created"
        })
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn failed_delegate(
    tx: &mut Transaction<'_, Postgres>,
    message: Uuid,
    plan: &DeliveryPlan,
    reason: &str,
) -> Result<()> {
    sqlx::query("INSERT INTO orchestration_delegates(id,message_id,configuration_id,channel,plan,state,reason_code) VALUES($1,$2,$3,$4,$5,'failed',$6)")
        .bind(plan.id).bind(message).bind(plan.configuration_id).bind(plan.channel.as_str()).bind(json!(plan)).bind(reason).execute(&mut **tx).await?;
    Ok(())
}
async fn delegate(
    tx: &mut Transaction<'_, Postgres>,
    message: Uuid,
    plan: &DeliveryPlan,
    content: &str,
) -> Result<()> {
    let snapshot = Uuid::now_v7();
    sqlx::query("INSERT INTO distribution_message_snapshots(id,channel,content,content_hash) VALUES($1,$2,$3,$4)").bind(snapshot).bind(plan.channel.as_str()).bind(content).bind(crate::business::hash(content)).execute(&mut **tx).await?;
    let task = match plan.channel {
        Channel::Mobile => {
            shared_send_v2::source_routing::create(tx, plan, content, plan.authorized_by, snapshot)
                .await
        }
        Channel::Wecom => {
            routing::create_official_task(tx, plan, content, plan.authorized_by, snapshot).await
        }
    }?;
    sqlx::query("INSERT INTO orchestration_delegates(id,message_id,configuration_id,channel,task_id,snapshot_id,plan) VALUES($1,$2,$3,$4,$5,$6,$7)").bind(plan.id).bind(message).bind(plan.configuration_id).bind(plan.channel.as_str()).bind(task).bind(snapshot).bind(json!(plan)).execute(&mut **tx).await?;
    Ok(())
}
