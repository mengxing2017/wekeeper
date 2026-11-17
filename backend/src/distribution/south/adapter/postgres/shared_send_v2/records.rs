//! Phone-owned durable intake, diagnostic operations and append-only evidence.
use super::*;
use crate::distribution::domain::{
    receiving::DeliveryPlan,
    shared_mobile::dispatch_records::{RecordQuery, reason_label},
};
pub fn failure(error: &Error) -> String {
    match error {
        Error::Policy(code) => code.to_string(),
        Error::Invalid(message) => message.clone(),
        Error::Forbidden | Error::Unauthorized => "authorization_revoked".into(),
        Error::NotFound => "dependency_not_found".into(),
        Error::Conflict => "plan_dependency_changed".into(),
        _ => "internal_processing_unavailable".into(),
    }
}
pub(super) async fn context(tx: &mut Tx<'_>, key: &str, id: Uuid) -> Result<()> {
    sqlx::query("SELECT set_config($1,$2,true)")
        .bind(key)
        .bind(id.to_string())
        .execute(&mut **tx)
        .await?;
    Ok(())
}
pub(super) async fn event(
    tx: &mut Tx<'_>,
    intake: Uuid,
    kind: &str,
    reason: &str,
    result: &str,
    evidence: Value,
) -> Result<()> {
    sqlx::query("INSERT INTO mobile_v9_dispatch_events(intake_id,item_id,kind,reason,result,evidence) SELECT id,item_id,$2,$3,$4,$5 FROM mobile_dispatch_intakes WHERE id=$1").bind(intake).bind(kind).bind(reason).bind(result).bind(evidence).execute(&mut **tx).await?;
    Ok(())
}
pub(super) async fn check(
    tx: &mut Tx<'_>,
    device: Uuid,
    kind: &str,
    reason: &str,
    evidence: Value,
) -> Result<()> {
    sqlx::query("INSERT INTO mobile_dispatch_checks(id,operation_id,delivery_request_id,intake_id,device_id,check_kind,decision,reason_code,evidence) SELECT $1,op,ex,coalesce((SELECT intake_id FROM mobile_dispatch_operations WHERE id=op),(SELECT intake_id FROM mobile_delivery_exchanges WHERE id=ex)),$2,$3,CASE WHEN $4='passed' THEN 'passed' ELSE 'skipped' END,$4,$5 FROM (SELECT nullif(current_setting('wekeeper.mobile_operation',true),'')::uuid AS op,nullif(current_setting('wekeeper.mobile_exchange',true),'')::uuid AS ex)c WHERE num_nonnulls(op,ex)=1")
 .bind(Uuid::now_v7()).bind(device).bind(kind).bind(reason).bind(evidence).execute(&mut **tx).await?;
    Ok(())
}
pub(super) async fn ensure_task(
    tx: &mut Tx<'_>,
    task: Uuid,
    item: Uuid,
    target: Uuid,
    snapshot: Uuid,
    plan: &Value,
) -> Result<()> {
    let id:Option<Uuid>=sqlx::query_scalar("UPDATE mobile_dispatch_intakes SET task_id=$1,item_id=$2 WHERE snapshot_id=$3 AND target_id=$4 RETURNING id").bind(task).bind(item).bind(snapshot).bind(target).fetch_optional(&mut **tx).await?;
    if id.is_none() {
        let intake = Uuid::now_v7();
        sqlx::query("INSERT INTO mobile_dispatch_intakes(id,source_kind,target_id,target_name,snapshot_id,plan,content,payload_hash,task_id,item_id,creation_state) VALUES($1,'manual',$2,$3,$4,$5,$6,$7,$8,$9,'created')").bind(intake).bind(target).bind(plan["target_name"].as_str().unwrap_or("")).bind(snapshot).bind(plan).bind(plan["message"].as_str().unwrap_or("")).bind(digest(plan)?).bind(task).bind(item).execute(&mut **tx).await?;
        event(
            tx,
            intake,
            "intake_registered",
            "intake_registered",
            "recorded",
            json!({"source_kind":"manual"}),
        )
        .await?;
        event(
            tx,
            intake,
            "task_created",
            "task_created",
            "created",
            json!({"task_id":task}),
        )
        .await?;
    }
    Ok(())
}
impl Repository {
    pub async fn submit_mobile(
        &self,
        message: Uuid,
        receipt: Uuid,
        mut plan: DeliveryPlan,
        content: String,
    ) -> Result<Uuid> {
        let target = plan.targets.first();
        let tid = target.map(|t| t.id);
        let name = target
            .map(|t| t.name.clone())
            .unwrap_or_else(|| "未提供目标群".into());
        let payload = json!({"configuration_id":plan.configuration_id,"targets":plan.targets,"authorized_by":plan.authorized_by,"content":content});
        let fingerprint = digest(&payload)?;
        let mut tx = self.0.begin().await?;
        let proposed = Uuid::now_v7();
        let delegate = Uuid::now_v7();
        let snapshot = Uuid::now_v7();
        plan.id = delegate;
        sqlx::query("INSERT INTO mobile_dispatch_intakes(id,source_kind,message_id,receipt_id,configuration_id,target_id,target_name,delegate_id,snapshot_id,plan,content,payload_hash) VALUES($1,'source_route',$2,$3,$4,$5,$6,$7,$8,$9,$10,$11) ON CONFLICT(message_id,configuration_id,target_id) WHERE source_kind='source_route' DO NOTHING")
 .bind(proposed).bind(message).bind(receipt).bind(plan.configuration_id).bind(tid).bind(name).bind(delegate).bind(snapshot).bind(json!(plan)).bind(&content).bind(&fingerprint).execute(&mut *tx).await?;
        let row=sqlx::query("SELECT id,payload_hash FROM mobile_dispatch_intakes WHERE message_id=$1 AND configuration_id=$2 AND target_id IS NOT DISTINCT FROM $3 FOR UPDATE").bind(message).bind(plan.configuration_id).bind(tid).fetch_one(&mut *tx).await?;
        let id: Uuid = row.get("id");
        if row.get::<String, _>("payload_hash") != fingerprint {
            event(
                &mut tx,
                id,
                "intake_conflict",
                "intake_payload_conflict",
                "rejected",
                json!({}),
            )
            .await?;
            tx.commit().await?;
            return Err(Error::Policy("intake_payload_conflict"));
        }
        if id == proposed {
            event(
                &mut tx,
                id,
                "intake_registered",
                "intake_registered",
                "recorded",
                json!({}),
            )
            .await?;
        }
        tx.commit().await?;
        self.process_mobile_intake(id).await?;
        let state: String =
            sqlx::query_scalar("SELECT creation_state FROM mobile_dispatch_intakes WHERE id=$1")
                .bind(id)
                .fetch_one(&self.0)
                .await?;
        if !matches!(state.as_str(), "created" | "rejected") {
            return Err(Error::Unavailable);
        }
        Ok(id)
    }
    pub async fn process_mobile_intake(&self, id: Uuid) -> Result<()> {
        let mut claim = self.0.begin().await?;
        let eligible:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_dispatch_intakes WHERE id=$1 AND creation_state IN ('registered','retrying','processing') AND next_retry_at<=clock_timestamp())").bind(id).fetch_one(&mut *claim).await?;
        if !eligible {
            return Ok(());
        }
        let row=sqlx::query("SELECT to_jsonb(i) AS value FROM mobile_dispatch_intakes i WHERE id=$1 AND creation_state IN ('registered','processing','retrying') AND next_retry_at<=clock_timestamp() FOR UPDATE SKIP LOCKED").bind(id).fetch_optional(&mut *claim).await?;
        let Some(row) = row else {
            return Ok(());
        };
        let v: Value = row.get("value");
        if !matches!(
            v["creation_state"].as_str(),
            Some("registered" | "retrying" | "processing")
        ) {
            return Ok(());
        }
        sqlx::query("UPDATE mobile_dispatch_operations SET result='interrupted',reason_code='processing_interrupted',finished_at=clock_timestamp() WHERE intake_id=$1 AND kind='create' AND result='started'").bind(id).execute(&mut *claim).await?;
        let op = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO mobile_dispatch_operations(id,intake_id,kind) VALUES($1,$2,'create')",
        )
        .bind(op)
        .bind(id)
        .execute(&mut *claim)
        .await?;
        context(&mut claim, "wekeeper.mobile_operation", op).await?;
        event(
            &mut claim,
            id,
            "creation_started",
            "creation_started",
            "started",
            json!({}),
        )
        .await?;
        sqlx::query("UPDATE mobile_dispatch_intakes SET creation_state='processing',next_retry_at=clock_timestamp()+interval '60 seconds' WHERE id=$1").bind(id).execute(&mut *claim).await?;
        claim.commit().await?;
        let mut tx = self.0.begin().await?;
        guard(&mut tx).await?;
        // Recheck ownership after acquiring the global business write lock.
        let owns:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_dispatch_operations o JOIN mobile_dispatch_intakes i ON i.id=o.intake_id WHERE o.id=$1 AND o.result='started' AND i.creation_state='processing' FOR UPDATE OF o,i)").bind(op).fetch_one(&mut *tx).await?;
        if !owns {
            return Ok(());
        }
        context(&mut tx, "wekeeper.mobile_operation", op).await?;
        let result=async {
 let plan:DeliveryPlan=serde_json::from_value(v["plan"].clone()).map_err(|_|Error::Policy("frozen_plan_invalid"))?;
 let text=v["content"].as_str().ok_or(Error::Policy("content_invalid"))?;
 require(crate::advertising::domain::measure(text,"mobile").valid,"channel_content_invalid")?;
 require(!plan.targets.is_empty(),"mobile_targets_missing")?;
 require(plan.targets.iter().all(|t|t.resource_available),"channel_resources_missing")?;
 let snap=uid(&v,"snapshot_id")?;
 sqlx::query("INSERT INTO distribution_message_snapshots(id,channel,content,content_hash) VALUES($1,'mobile',$2,$3)").bind(snap).bind(text).bind(hash(text)).execute(&mut *tx).await?;
 let task=super::source_routing::create(&mut tx,&plan,text,plan.authorized_by,snap).await?;
 sqlx::query("INSERT INTO orchestration_delegates(id,message_id,configuration_id,channel,task_id,snapshot_id,plan) VALUES($1,$2,$3,'mobile',$4,$5,$6)").bind(plan.id).bind(uid(&v,"message_id")?).bind(plan.configuration_id).bind(task).bind(snap).bind(json!(plan)).execute(&mut *tx).await?;
 sqlx::query("UPDATE mobile_dispatch_intakes SET creation_state='created',reason_code=NULL,updated_at=clock_timestamp() WHERE id=$1").bind(id).execute(&mut *tx).await?;
 sqlx::query("UPDATE mobile_dispatch_operations SET result='created',finished_at=clock_timestamp() WHERE id=$1").bind(op).execute(&mut *tx).await?;
 event(&mut tx,id,"task_created","task_created","created",json!({"task_id":task})).await?;
 Ok::<(),Error>(())
 }.await;
        if let Err(error) = result {
            tx.rollback().await?;
            let reason = failure(&error);
            let retry = matches!(
                error,
                Error::Database(_) | Error::Unavailable | Error::Upstream(_)
            );
            let mut failure_tx = self.0.begin().await?;
            context(&mut failure_tx, "wekeeper.mobile_operation", op).await?;
            let delay:f64=sqlx::query_scalar("SELECT least(300,5*power(2,least(greatest(count(*)-1,0),6)))::double precision FROM mobile_dispatch_operations WHERE intake_id=$1 AND kind='create'").bind(id).fetch_one(&mut *failure_tx).await?;
            let changed=sqlx::query("UPDATE mobile_dispatch_operations SET result=$2,reason_code=$3,finished_at=clock_timestamp(),retry_at=CASE WHEN $2='retrying' THEN clock_timestamp()+make_interval(secs=>$4::double precision) END WHERE id=$1 AND result='started'").bind(op).bind(if retry{"retrying"}else{"rejected"}).bind(&reason).bind(delay).execute(&mut *failure_tx).await?.rows_affected();
            if changed > 0 {
                sqlx::query("UPDATE mobile_dispatch_intakes SET creation_state=$2,reason_code=$3,next_retry_at=clock_timestamp()+make_interval(secs=>$4::double precision),updated_at=clock_timestamp() WHERE id=$1").bind(id).bind(if retry{"retrying"}else{"rejected"}).bind(&reason).bind(delay).execute(&mut *failure_tx).await?;
                event(
                    &mut failure_tx,
                    id,
                    "creation_failed",
                    &reason,
                    if retry { "retrying" } else { "rejected" },
                    json!({"step":"task_creation"}),
                )
                .await?;
                if !retry {
                    sqlx::query("INSERT INTO orchestration_delegates(id,message_id,configuration_id,channel,plan,state,reason_code) SELECT delegate_id,message_id,configuration_id,'mobile',plan,'failed',$2 FROM mobile_dispatch_intakes WHERE id=$1 ON CONFLICT DO NOTHING").bind(id).bind(&reason).execute(&mut *failure_tx).await?;
                }
            }
            failure_tx.commit().await?;
            if retry {
                tracing::warn!(intake_id=%id,operation_id=%op,"mobile creation failed; durable retry scheduled");
                return Err(error);
            }
            return Ok(());
        }
        tx.commit().await?;
        Ok(())
    }
    pub async fn recover_mobile_intakes(&self) -> Result<()> {
        let ids:Vec<Uuid>=sqlx::query_scalar("SELECT id FROM mobile_dispatch_intakes WHERE source_kind='source_route' AND creation_state IN ('registered','processing','retrying') AND next_retry_at<=clock_timestamp() ORDER BY next_retry_at,id LIMIT 50").fetch_all(&self.0).await?;
        for id in ids {
            if let Err(error) = self.process_mobile_intake(id).await {
                tracing::warn!(intake_id=%id,%error,"mobile intake recovery deferred");
            }
        }
        sqlx::query("UPDATE mobile_delivery_exchanges SET response_state='response_observation_unknown',response_at=clock_timestamp() WHERE response_state IN ('not_started','response_started') AND created_at<clock_timestamp()-interval '2 minutes'").execute(&self.0).await?;
        Ok(())
    }
}
pub(super) async fn begin_evaluation(pool: &PgPool, item: Uuid) -> Result<Option<Uuid>> {
    let mut tx = pool.begin().await?;
    let intake: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM mobile_dispatch_intakes WHERE item_id=$1")
            .bind(item)
            .fetch_optional(&mut *tx)
            .await?;
    let Some(intake) = intake else {
        return Ok(None);
    };
    sqlx::query("UPDATE mobile_dispatch_operations SET result='interrupted',reason_code='processing_interrupted',finished_at=clock_timestamp() WHERE intake_id=$1 AND kind='evaluate' AND result='started'").bind(intake).execute(&mut *tx).await?;
    let id = Uuid::now_v7();
    sqlx::query("INSERT INTO mobile_dispatch_operations(id,intake_id,kind,round_no,attempt_id) SELECT $1,$2,'evaluate',least(d.completed_rounds+1,3),a.id FROM mobile_v9_dispatch_state d JOIN mobile_v9_attempts a ON a.item_id=d.item_id AND a.current WHERE d.item_id=$3").bind(id).bind(intake).bind(item).execute(&mut *tx).await?;
    context(&mut tx, "wekeeper.mobile_operation", id).await?;
    event(
        &mut tx,
        intake,
        "evaluation_started",
        "evaluation_started",
        "started",
        json!({}),
    )
    .await?;
    tx.commit().await?;
    Ok(Some(id))
}
impl Repository {
    pub async fn read_records(
        &self,
        actor: &Actor,
        kind: &str,
        id: Option<Uuid>,
        q: RecordQuery,
    ) -> Result<Value> {
        actor.platform()?;
        let limit = q.limit()?;
        let mut tx = self.0.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .execute(&mut *tx)
            .await?;
        let authorized: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM accounts WHERE id=$1 AND active AND platform_admin)",
        )
        .bind(actor.id)
        .fetch_one(&mut *tx)
        .await?;
        if !authorized {
            return Err(Error::Forbidden);
        }
        let mut qb = sqlx::QueryBuilder::<Postgres>::new(match kind {
            "intakes" => {
                "SELECT mobile_business_snapshot(to_jsonb(v)) AS value FROM mobile_dispatch_record_view v WHERE true"
            }
            "detail" => {
                "SELECT mobile_business_snapshot(to_jsonb(v))||jsonb_build_object('content',i.content) AS value FROM mobile_dispatch_record_view v JOIN mobile_dispatch_intakes i ON i.id=v.id WHERE true"
            }
            "operations" => {
                "SELECT mobile_business_snapshot(to_jsonb(v)) AS value FROM mobile_dispatch_operations v WHERE true"
            }
            "checks" | "exchange-checks" => {
                "SELECT mobile_business_snapshot(to_jsonb(v)) AS value FROM mobile_dispatch_checks v WHERE true"
            }
            "events" => {
                "SELECT mobile_business_snapshot(to_jsonb(v)) AS value FROM mobile_v9_dispatch_events v WHERE true"
            }
            "exchanges" | "exchange" => {
                "SELECT mobile_business_snapshot(to_jsonb(v)) AS value FROM mobile_delivery_exchange_view v WHERE true"
            }
            _ => return Err(Error::NotFound),
        });
        let listing = matches!(kind, "intakes" | "detail");
        let child = matches!(kind, "operations" | "events" | "checks" | "exchange-checks");
        if matches!(kind, "detail" | "exchange") {
            qb.push(" AND v.id=").push_bind(id.ok_or(Error::NotFound)?);
        }
        if child {
            let column = match kind {
                "checks" => "operation_id",
                "exchange-checks" => "delivery_request_id",
                _ => "intake_id",
            };
            let parent = match kind {
                "checks" => "mobile_dispatch_operations",
                "exchange-checks" => "mobile_delivery_exchanges",
                _ => "mobile_dispatch_intakes",
            };
            let exists: bool = sqlx::query_scalar(&format!(
                "SELECT EXISTS(SELECT 1 FROM {parent} WHERE id=$1)"
            ))
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
            if !exists {
                return Err(Error::NotFound);
            }
            qb.push(format!(" AND v.{column}="))
                .push_bind(id.ok_or(Error::NotFound)?);
        }
        if listing {
            let source = q.source_kind.as_deref().unwrap_or(if kind == "detail" {
                "all"
            } else {
                "source_route"
            });
            if source != "all" {
                qb.push(" AND v.source_kind=").push_bind(source);
            }
            for (column, value) in [
                ("message_id", q.message_id),
                ("receipt_id", q.receipt_id),
                ("task_id", q.task_id),
                ("configuration_id", q.configuration_id),
                ("id", q.intake_id),
            ] {
                if let Some(value) = value {
                    qb.push(format!(" AND v.{column}=")).push_bind(value);
                }
            }
            for (column, value) in [
                ("stage", q.stage.as_deref()),
                ("outcome", q.outcome.as_deref()),
            ] {
                if let Some(value) = value {
                    qb.push(format!(" AND v.{column}=")).push_bind(value);
                }
            }
            if let Some(target) = q.target {
                qb.push(" AND (v.target_name ILIKE ")
                    .push_bind(format!("%{target}%"))
                    .push(" OR v.target_id::text=")
                    .push_bind(target)
                    .push(")");
            }
            if let Some(device) = q.device_id {
                qb.push(" AND (v.device_id=").push_bind(device).push(" OR EXISTS(SELECT 1 FROM mobile_dispatch_checks c WHERE c.intake_id=v.id AND c.device_id=").push_bind(device).push(") OR EXISTS(SELECT 1 FROM mobile_v9_dispatch_events e WHERE e.intake_id=v.id AND e.device_id=").push_bind(device).push("))");
            }
            if let Some(reason) = q.reason.as_deref() {
                if q.reason_scope.as_deref() == Some("history") {
                    qb.push(" AND (EXISTS(SELECT 1 FROM mobile_v9_dispatch_events e WHERE e.intake_id=v.id AND e.reason=").push_bind(reason).push(") OR EXISTS(SELECT 1 FROM mobile_dispatch_checks c WHERE c.intake_id=v.id AND c.reason_code=").push_bind(reason).push("))");
                } else {
                    qb.push(" AND (v.reason_code=").push_bind(reason).push(" OR EXISTS(SELECT 1 FROM jsonb_array_elements(v.candidate_reasons) c WHERE c->>'reason_code'=").push_bind(reason).push("))");
                }
            }
        } else {
            if matches!(kind, "exchanges" | "exchange") {
                for (column, value) in [
                    ("device_id", q.device_id),
                    ("intake_id", q.intake_id),
                    ("execution_id", q.execution_id),
                ] {
                    if let Some(value) = value {
                        qb.push(format!(" AND v.{column}=")).push_bind(value);
                    }
                }
            }
            if let Some(reason) = q.reason.as_deref() {
                qb.push(if kind == "events" {
                    " AND v.reason="
                } else {
                    " AND v.reason_code="
                })
                .push_bind(reason);
            }
            if let Some(result) = q.result.as_deref() {
                qb.push(if matches!(kind, "checks" | "exchange-checks") {
                    " AND v.decision="
                } else {
                    " AND v.result="
                })
                .push_bind(result);
            }
            if matches!(kind, "operations" | "events") {
                if let Some(k) = q.kind.as_deref() {
                    qb.push(" AND v.kind=").push_bind(k);
                }
                if let Some(round) = q.round_no {
                    qb.push(" AND v.round_no=").push_bind(round);
                }
            }
        }
        let time = if kind == "operations" {
            "started_at"
        } else {
            "created_at"
        };
        if let Some(from) = q.from {
            qb.push(format!(" AND v.{time}>=")).push_bind(from);
        }
        if let Some(until) = q.until {
            qb.push(format!(" AND v.{time}<=")).push_bind(until);
        }
        // Cursor identifies an immutable row; tuple comparison remains stable as current state changes.
        if let Some(cursor) = q.cursor {
            if kind == "events" {
                let n = cursor
                    .parse::<i64>()
                    .map_err(|_| Error::Invalid("游标不合法".into()))?;
                qb.push(" AND (v.created_at,v.id)<(SELECT created_at,id FROM mobile_v9_dispatch_events WHERE id=").push_bind(n).push(")");
            } else {
                let c = cursor
                    .parse::<Uuid>()
                    .map_err(|_| Error::Invalid("游标不合法".into()))?;
                let table = match kind {
                    "intakes" | "detail" => "mobile_dispatch_intakes",
                    "operations" => "mobile_dispatch_operations",
                    "checks" | "exchange-checks" => "mobile_dispatch_checks",
                    _ => "mobile_delivery_exchanges",
                };
                qb.push(format!(
                    " AND (v.{time},v.id)<(SELECT {time},id FROM {table} WHERE id="
                ))
                .push_bind(c)
                .push(")");
            }
        }
        qb.push(format!(" ORDER BY v.{time} DESC,v.id DESC LIMIT "))
            .push_bind(limit + 1);
        let rows = qb.build().fetch_all(&mut *tx).await?;
        let has_more = rows.len() > limit as usize;
        let mut items: Vec<Value> = rows
            .into_iter()
            .take(limit as usize)
            .map(|r| r.get("value"))
            .collect();
        for item in &mut items {
            let reason = item["reason_code"]
                .as_str()
                .or(item["reason"].as_str())
                .unwrap_or("");
            item["reason_label"] = json!(reason_label(reason));
            if listing {
                if let Some(reasons) = item["candidate_reasons"].as_array_mut() {
                    for reason in reasons {
                        reason["reason_label"] =
                            json!(reason_label(reason["reason_code"].as_str().unwrap_or("")));
                    }
                }
                let wait = item["stage"] == "round_wait";
                item["owner"] = json!(if item["creation_state"] == "rejected" {
                    "admin"
                } else if matches!(
                    item["stage"].as_str(),
                    Some("executing" | "awaiting_receipt")
                ) {
                    "device"
                } else {
                    "system"
                });
                item["next_action"] = json!(if wait {
                    "系统将在指定时间重新评估候选；查看本轮逐候选原因"
                } else if item["stage"] == "awaiting_dispatch" {
                    "等待平台签发并直接推送；系统按授权候选扫描并改派"
                } else if item["stage"] == "executing" {
                    "等待设备最终回报；超时按既有授权候选换机"
                } else if item["creation_state"] == "rejected" {
                    "查看具体校验原因；不自动重放旧消息"
                } else {
                    "查看处理轨迹及下次处理时间"
                });
            }
        }
        let now: chrono::DateTime<chrono::Utc> =
            sqlx::query_scalar("SELECT transaction_timestamp()")
                .fetch_one(&mut *tx)
                .await?;
        let version = hash(&serde_json::to_string(&items).map_err(|_| Error::Unavailable)?);
        let next = if has_more {
            items.last().map(|v| {
                if let Some(s) = v["id"].as_str() {
                    s.to_owned()
                } else {
                    v["id"].to_string()
                }
            })
        } else {
            None
        };
        tx.commit().await?;
        if matches!(kind, "detail" | "exchange") {
            let mut value = items.pop().ok_or(Error::NotFound)?;
            value["observed_at"] = json!(now);
            return Ok(value);
        }
        Ok(
            json!({"id":"mobile-dispatch-records","items":items,"has_more":has_more,"next_cursor":next,"observed_at":now,"version":version}),
        )
    }
}
