use super::Repository;
use crate::{
    business::{Actor, Error, Page, Result},
    message_orchestration::domain::{
        MessageCounts, MessageView,
        progress::{
            DeliveryView, Diagnostic, Observation, Progress, message_state_with_channel_failures,
        },
    },
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProgressFilter {
    pub channel: Option<String>,
    pub stage: Option<String>,
    pub attention: Option<String>,
}
impl ProgressFilter {
    pub fn validate(&self) -> Result<()> {
        crate::business::require(
            self.channel
                .as_deref()
                .is_none_or(|v| matches!(v, "mobile" | "wecom")),
            "不支持的发送通道",
        )?;
        crate::business::require(
            self.stage.as_deref().is_none_or(|v| {
                matches!(
                    v,
                    "handoff"
                        | "checking"
                        | "creating_tasks"
                        | "scheduling"
                        | "device_receipt"
                        | "device_execution"
                        | "official_queue"
                        | "official_call"
                        | "official_result"
                        | "sent"
                        | "failed"
                        | "unknown"
                        | "missing_evidence"
                )
            }),
            "不支持的消息阶段",
        )?;
        crate::business::require(
            self.attention.as_deref().is_none_or(|v| {
                matches!(
                    v,
                    "normal"
                        | "retrying"
                        | "attention"
                        | "terminal"
                        | "unobserved"
                        | "projection_lag"
                )
            }),
            "不支持的关注类型",
        )
    }
}
fn stall_seconds() -> i64 {
    std::env::var("WEKEEPER_MESSAGE_PROGRESS_STALL_SECONDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(60)
        .clamp(60, 86400)
}
async fn observation_threshold(tx: &mut Transaction<'_, Postgres>) -> Result<()> {
    sqlx::query("SELECT set_config('wekeeper.progress_stall_seconds',$1,true)")
        .bind(stall_seconds().to_string())
        .execute(&mut **tx)
        .await?;
    Ok(())
}
async fn authorize(tx: &mut Transaction<'_, Postgres>, actor: &Actor) -> Result<()> {
    actor.platform()?;
    let valid: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM accounts WHERE id=$1 AND active AND platform_admin)",
    )
    .bind(actor.id)
    .fetch_one(&mut **tx)
    .await?;
    if !valid {
        return Err(Error::Forbidden);
    }
    Ok(())
}
async fn groups(
    tx: &mut Transaction<'_, Postgres>,
    ids: &[Uuid],
) -> Result<Vec<(Uuid, Observation)>> {
    let rows:Vec<(Uuid,Value)>=sqlx::query_as(r#"
      SELECT message_id,jsonb_build_object('channel',channel,'stage',stage,'severity',severity,'outcome',outcome,'count',count(*),
      'reason',reason_code,
      'since',min(last_progress_at),'retry_at',min(CASE WHEN channel='mobile' THEN next_retry_at ELSE next_sync_at END),
      'deadline_at',min(CASE WHEN grant_state='active' THEN result_deadline_at END),'historical_risk',bool_or(historical_risk))
      FROM orchestration_delivery_progress WHERE message_id=ANY($1)
      GROUP BY message_id,channel,stage,severity,outcome,reason_code
      ORDER BY message_id,CASE severity WHEN 'attention' THEN 0 WHEN 'unobserved' THEN 1 WHEN 'retrying' THEN 2 WHEN 'normal' THEN 3 ELSE 4 END,channel,stage
    "#).bind(ids).fetch_all(&mut **tx).await?;
    rows.into_iter()
        .map(|(id, value)| {
            Ok((
                id,
                serde_json::from_value(value).map_err(|_| Error::Unavailable)?,
            ))
        })
        .collect()
}
async fn versions(tx: &mut Transaction<'_, Postgres>, ids: &[Uuid]) -> Result<Vec<(Uuid, String)>> {
    Ok(sqlx::query_as("SELECT message_id,md5(string_agg(concat_ws('|',channel,target_id,stage,outcome,last_progress_at,attempt_id,reason_code,grant_state,accepted_at,result_deadline_at,next_retry_at,next_sync_at,historical_risk,severity),';' ORDER BY channel,target_id)) FROM orchestration_delivery_progress WHERE message_id=ANY($1) GROUP BY message_id").bind(ids).fetch_all(&mut **tx).await?)
}
impl Repository {
    pub async fn list_progress(
        &self,
        actor: &Actor,
        route: Option<String>,
        receipt: Option<Uuid>,
        page: Page,
        filter: ProgressFilter,
    ) -> Result<Value> {
        filter.validate()?;
        let (limit, offset) = page.fetch_bounds()?;
        let mut tx = self.0.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .execute(&mut *tx)
            .await?;
        authorize(&mut tx, actor).await?;
        observation_threshold(&mut tx).await?;
        let rows=sqlx::query(r#"
          SELECT jsonb_build_object('id',r.receipt_id,'message_id',m.id,'platform_id',r.source_id,'business_unit_id',r.business_unit_id,
           'receipt_id',r.receipt_id,'event_id',r.event_id,'created_at',r.received_at,'state','handoff','reason_code',d.reason_code,
           'content',coalesce(nullif(d.content,''),m.original->>'content',convert_from(inbox.request_body,'UTF8')::jsonb #>> '{data,attributes,content}'),'decision',NULLIF(d.decision,'{}'::jsonb),
           'sending',NULL,'delegates',coalesce((SELECT jsonb_agg(jsonb_build_object('id',x.id,'channel',x.channel,'task_id',x.task_id,'configuration_id',x.configuration_id,'state',x.state,'reason_code',x.reason_code,'target_count',jsonb_array_length(x.plan->'targets')) ORDER BY x.channel,x.id) FROM orchestration_delegates x WHERE x.message_id=m.id),'[]')) AS value,
           coalesce(d.state,CASE WHEN m.id IS NULL THEN 'handoff' ELSE 'pending' END) AS dispatch,
           coalesce(d.stage_entered_at,h.created_at,r.received_at) AS since,coalesce(d.next_attempt_at,h.claim_until,h.next_attempt_at) AS retry_at,
           coalesce(d.last_error_code,h.last_error) AS last_error,r.summary_updated_at,r.sending AS cached,
           h.completed_at,d.updated_at AS dispatch_updated_at
          FROM source_receipts r LEFT JOIN source_inbox inbox ON inbox.receipt_id=r.receipt_id LEFT JOIN orchestration_messages m ON m.evidence_ref=r.receipt_id
          LEFT JOIN source_message_handoffs h ON h.receipt_id=r.receipt_id LEFT JOIN orchestration_dispatches d ON d.message_id=m.id
          WHERE ($1::text IS NULL OR r.route_id=$1) AND ($2::uuid IS NULL OR r.receipt_id=$2)
          AND ($3::text IS NULL OR EXISTS(SELECT 1 FROM orchestration_delivery_progress v WHERE v.message_id=m.id AND v.channel=$3))
          AND ($4::text IS NULL OR ($4='handoff' AND m.id IS NULL) OR ($4='checking' AND m.id IS NOT NULL AND coalesce(d.state,'pending')='pending') OR ($4='creating_tasks' AND d.state IN ('preparing','ready')) OR EXISTS(SELECT 1 FROM orchestration_delivery_progress v WHERE v.message_id=m.id AND v.stage=$4))
          AND ($5::text IS NULL OR EXISTS(SELECT 1 FROM orchestration_delivery_progress v WHERE v.message_id=m.id AND v.severity=$5)
            OR ($5='projection_lag' AND EXISTS(SELECT 1 FROM orchestration_delivery_progress v WHERE v.message_id=m.id)
              AND r.sending IS DISTINCT FROM (SELECT jsonb_build_object('sent',count(*) FILTER(WHERE outcome='sent'),'failed',count(*) FILTER(WHERE outcome='failed'),'unknown',count(*) FILTER(WHERE outcome='unknown'),'pending',count(*) FILTER(WHERE outcome='pending')) FROM orchestration_delivery_progress v WHERE v.message_id=m.id))
            OR (NOT EXISTS(SELECT 1 FROM orchestration_delegates z WHERE z.message_id=m.id) AND
              $5=CASE WHEN d.state='failed' THEN 'terminal' WHEN coalesce(d.last_error_code,h.last_error) IS NOT NULL THEN 'retrying'
              WHEN coalesce(d.next_attempt_at,h.claim_until,h.next_attempt_at,r.received_at)+make_interval(secs=>coalesce(nullif(current_setting('wekeeper.progress_stall_seconds',true),'')::integer,60))<transaction_timestamp() THEN 'attention' ELSE 'normal' END))
          ORDER BY r.received_at DESC,r.receipt_id DESC LIMIT $6 OFFSET $7
        "#).bind(route).bind(receipt).bind(filter.channel).bind(filter.stage).bind(filter.attention).bind(limit).bind(offset).fetch_all(&mut *tx).await?;
        let more = rows.len() >= limit as usize;
        let rows: Vec<_> = rows.into_iter().take(limit as usize - 1).collect();
        let ids: Vec<Uuid> = rows
            .iter()
            .filter_map(|r| {
                r.get::<Value, _>("value")["message_id"]
                    .as_str()
                    .and_then(|v| v.parse().ok())
            })
            .collect();
        let observations = groups(&mut tx, &ids).await?;
        let versions = versions(&mut tx, &ids).await?;
        let now: DateTime<Utc> = sqlx::query_scalar("SELECT transaction_timestamp()")
            .fetch_one(&mut *tx)
            .await?;
        let mut items = Vec::new();
        for row in rows {
            let mut value: Value = row.get("value");
            let id = value["message_id"]
                .as_str()
                .and_then(|v| v.parse::<Uuid>().ok());
            let facts: Vec<_> = observations
                .iter()
                .filter(|(mid, _)| Some(*mid) == id)
                .map(|(_, o)| o)
                .collect();
            let mut counts = MessageCounts {
                pending: 0,
                sent: 0,
                failed: 0,
                unknown: 0,
            };
            let mut total = 0;
            let mut complete = true;
            for o in &facts {
                total += o.count;
                match o.outcome.as_str() {
                    "sent" => counts.sent += o.count,
                    "failed" => counts.failed += o.count,
                    "unknown" => counts.unknown += o.count,
                    _ => counts.pending += o.count,
                };
                if o.stage == "missing_evidence" {
                    complete = false;
                }
            }
            let dispatch: String = row.get("dispatch");
            let channel_failures: Vec<_> = value["delegates"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|d| d["state"] == "failed")
                .collect();
            let mut state = message_state_with_channel_failures(
                &dispatch,
                total,
                &counts,
                complete,
                !channel_failures.is_empty(),
            )
            .to_string();
            let mut diagnostics: Vec<Diagnostic> = facts.iter().map(|o| o.diagnose()).collect();
            for failed in channel_failures {
                diagnostics.push(
                    Observation {
                        channel: failed["channel"].as_str().map(str::to_owned),
                        stage: "not_dispatched".into(),
                        severity: "terminal".into(),
                        outcome: "failed".into(),
                        count: 0,
                        reason: failed["reason_code"].as_str().map(str::to_owned),
                        since: None,
                        retry_at: None,
                        deadline_at: None,
                        historical_risk: false,
                    }
                    .diagnose(),
                );
            }
            if total == 0 {
                let stage = match dispatch.as_str() {
                    "handoff" => "handoff",
                    "pending" => "checking",
                    "preparing" | "ready" => "creating_tasks",
                    "failed" => "not_dispatched",
                    _ => "missing_evidence",
                };
                let since: DateTime<Utc> = row.get("since");
                let retry: Option<DateTime<Utc>> = row.get("retry_at");
                let fault: Option<String> = row.get("last_error");
                let severity = if stage == "not_dispatched" {
                    "terminal"
                } else if fault.is_some() {
                    "retrying"
                } else if retry.unwrap_or(since) + chrono::Duration::seconds(stall_seconds()) < now
                {
                    "attention"
                } else if stage == "missing_evidence" {
                    "unobserved"
                } else {
                    "normal"
                };
                diagnostics.push(
                    Observation {
                        channel: None,
                        stage: stage.into(),
                        severity: severity.into(),
                        outcome: if dispatch == "failed" {
                            "failed"
                        } else {
                            "pending"
                        }
                        .into(),
                        count: 0,
                        reason: value["reason_code"].as_str().map(str::to_owned).or(fault),
                        since: Some(since),
                        retry_at: retry,
                        deadline_at: None,
                        historical_risk: false,
                    }
                    .diagnose(),
                );
            } else if state == "sending"
                && facts.iter().all(|o| {
                    matches!(
                        o.stage.as_str(),
                        "scheduling" | "device_receipt" | "official_queue"
                    )
                })
            {
                state = "dispatching".into();
            }
            let live = serde_json::to_value(&counts).map_err(|_| Error::Unavailable)?;
            let cached: Option<Value> = row.get("cached");
            let results_as_of: Option<DateTime<Utc>> = row.get("summary_updated_at");
            let projection_lag = total > 0 && cached.as_ref() != Some(&live);
            value["state"] = json!(state);
            value["sending"] = if total > 0 { live } else { Value::Null };
            let progress = Progress {
                version: versions
                    .iter()
                    .find(|(mid, _)| Some(*mid) == id)
                    .map(|(_, v)| v.clone())
                    .unwrap_or_else(|| "empty".into()),
                stage: state,
                counts_complete: complete,
                total: if total > 0 || dispatch == "failed" {
                    Some(total)
                } else {
                    None
                },
                diagnostics,
                observed_at: now,
                results_as_of,
                projection_lag,
            };
            value["progress"] = serde_json::to_value(progress).map_err(|_| Error::Unavailable)?;
            value["milestones"] = json!({"received_at":value["created_at"],"handoff_completed_at":row.get::<Option<DateTime<Utc>>,_>("completed_at"),"delegated_at":if dispatch=="delegated"{row.get::<Option<DateTime<Utc>>,_>("dispatch_updated_at")}else{None}});
            items.push(
                serde_json::from_value::<MessageView>(value).map_err(|_| Error::Unavailable)?,
            );
        }
        tx.commit().await?;
        Ok(json!({"items":items,"has_more":more}))
    }
    pub async fn deliveries(&self, actor: &Actor, receipt: Uuid, page: Page) -> Result<Value> {
        let (limit, offset) = page.fetch_bounds()?;
        let mut tx = self.0.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .execute(&mut *tx)
            .await?;
        authorize(&mut tx, actor).await?;
        observation_threshold(&mut tx).await?;
        let row=sqlx::query("SELECT m.id FROM source_receipts r LEFT JOIN orchestration_messages m ON m.evidence_ref=r.receipt_id WHERE r.receipt_id=$1").bind(receipt).fetch_optional(&mut *tx).await?.ok_or(Error::NotFound)?;
        let mid: Option<Uuid> = row.get("id");
        let rows=sqlx::query(r#"SELECT v.*,e.name AS device_name FROM orchestration_delivery_progress v LEFT JOIN execution_devices e ON e.id=v.device_id WHERE message_id=$1 ORDER BY channel,target_name,target_id LIMIT $2 OFFSET $3"#).bind(mid).bind(limit).bind(offset).fetch_all(&mut *tx).await?;
        let more = rows.len() >= limit as usize;
        let mut items = Vec::new();
        for row in rows.into_iter().take(limit as usize - 1) {
            let channel: String = row.get("channel");
            let target: Uuid = row.get("target_id");
            let stage: String = row.get("stage");
            let outcome: String = row.get("outcome");
            let diagnostic = Observation {
                channel: Some(channel.clone()),
                stage,
                severity: row.get("severity"),
                outcome: outcome.clone(),
                count: 1,
                reason: row.get("reason_code"),
                since: row.get("last_progress_at"),
                retry_at: if channel == "mobile" {
                    row.get("next_retry_at")
                } else {
                    row.get("next_sync_at")
                },
                deadline_at: if row.get::<Option<String>, _>("grant_state").as_deref()
                    == Some("active")
                {
                    row.get("result_deadline_at")
                } else {
                    None
                },
                historical_risk: row.get("historical_risk"),
            }
            .diagnose();
            items.push(DeliveryView {
                id: format!("{channel}:{target}"),
                channel,
                target_id: target,
                target_name: row.get("target_name"),
                task_id: row.get("task_id"),
                attempt_id: row.get("attempt_id"),
                attempt_no: row.get("attempt_no"),
                device_id: row.get("device_id"),
                device_name: row.get("device_name"),
                sender: row.get("sender"),
                outcome,
                diagnostic,
            });
        }
        let observed_at: DateTime<Utc> = sqlx::query_scalar("SELECT transaction_timestamp()")
            .fetch_one(&mut *tx)
            .await?;
        let version = versions(&mut tx, &mid.into_iter().collect::<Vec<_>>())
            .await?
            .into_iter()
            .next()
            .map(|(_, v)| v)
            .unwrap_or_else(|| "empty".into());
        tx.commit().await?;
        Ok(json!({"items":items,"has_more":more,"observed_at":observed_at,"version":version}))
    }
}
