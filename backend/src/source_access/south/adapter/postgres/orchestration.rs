use super::*;
use crate::{
    business::Actor,
    source_access::{domain::orchestration::*, south::port::OrchestrationStore},
};
#[derive(Clone)]
pub struct Repository(pub PgPool);
// Filter by the frozen workspace before pagination; route edits do not transfer historical access.
const READ: &str = "EXISTS(SELECT 1 FROM accounts a WHERE a.id=$1 AND a.active AND (a.platform_admin OR (t.channel='mobile' AND EXISTS(SELECT 1 FROM mobile_workspace_members m WHERE m.workspace_id=t.workspace_id AND m.account_id=a.id AND m.role IN ('operator','admin'))) OR (t.channel='wecom' AND EXISTS(SELECT 1 FROM wecom_workspace_members m WHERE m.school_id=t.workspace_id AND m.account_id=a.id AND m.role IN ('operator','admin')))))";
const WRITE: &str = "EXISTS(SELECT 1 FROM accounts a WHERE a.id=$1 AND a.active AND (a.platform_admin OR (t.channel='mobile' AND EXISTS(SELECT 1 FROM mobile_workspace_members m WHERE m.workspace_id=t.workspace_id AND m.account_id=a.id AND m.role='admin')) OR (t.channel='wecom' AND EXISTS(SELECT 1 FROM wecom_workspace_members m WHERE m.school_id=t.workspace_id AND m.account_id=a.id AND m.role='admin'))))";
fn row_sql() -> String {
    format!(
        "SELECT jsonb_build_object('id',t.id,'decision_id',t.decision_id,'source_id',t.source_id,'route_id',t.route_id,'unit_id',t.unit_id,'channel',t.channel,'workspace_id',t.workspace_id,'route_revision',t.route_revision,'event_id',t.event_id,'external_content_id',t.external_content_id,'state',t.state,'counts',t.counts,'stopped',t.stopped,'stop_reason',t.stop_reason,'revision',t.revision,'created_at',t.created_at,'has_unknown',t.has_unknown,'can_stop',NOT t.stopped AND t.state IN ('queued','scheduled','running','unknown') AND {WRITE},'can_retry',NOT t.stopped AND t.state IN ('queued','scheduled','running') AND {WRITE}) FROM source_automation_summaries t"
    )
}
async fn access(
    tx: &mut Transaction<'_, Postgres>,
    actor: Uuid,
    id: Uuid,
    write: bool,
) -> Result<()> {
    let allowed: bool = sqlx::query_scalar(&format!(
        "SELECT EXISTS(SELECT 1 FROM source_automation_tasks t WHERE t.id=$2 AND {})",
        if write { WRITE } else { READ }
    ))
    .bind(actor)
    .bind(id)
    .fetch_one(&mut **tx)
    .await?;
    ensure(allowed, "forbidden")
}
impl OrchestrationStore for Repository {
    async fn execute(&self, actor: &Actor, command: Command) -> Result<Value> {
        let mut tx = self.0.begin().await?;
        let result = match command {
            Command::List(f) => {
                let query = format!(
                    "{} WHERE {READ} AND ($2::uuid IS NULL OR t.source_id=$2) AND ($3::uuid IS NULL OR t.unit_id=$3) AND ($4::text IS NULL OR t.route_id=$4) AND ($5::uuid IS NULL OR t.workspace_id=$5) AND ($6::text IS NULL OR t.channel=$6) AND ($7::text IS NULL OR t.state=$7) AND ($8::timestamptz IS NULL OR t.created_at>=$8) AND ($9::timestamptz IS NULL OR t.created_at<=$9) ORDER BY t.created_at DESC,t.id DESC LIMIT 31 OFFSET $10",
                    row_sql()
                );
                let rows: Vec<Value> = sqlx::query_scalar(&query)
                    .bind(actor.id)
                    .bind(f.source_id)
                    .bind(f.unit_id)
                    .bind(f.route_id)
                    .bind(f.workspace_id)
                    .bind(f.channel)
                    .bind(f.state)
                    .bind(f.from)
                    .bind(f.until)
                    .bind((f.page.unwrap_or(1) - 1) * 30)
                    .fetch_all(&mut *tx)
                    .await?;
                let items: Vec<TaskRow> = rows
                    .iter()
                    .take(30)
                    .cloned()
                    .map(serde_json::from_value)
                    .collect::<std::result::Result<_, _>>()?;
                json!({"items":items,"has_more":rows.len()>30})
            }
            Command::Policy(route, input) => {
                super::routing::lock_route_mode(&mut tx, &route, true).await?;
                let admin = super::routing::administrator(&mut tx, actor.id).await?;
                super::routing::access(&mut tx, actor.id, &route, admin).await?;
                sqlx::query("SELECT route_id FROM source_routes WHERE route_id=$1 FOR UPDATE")
                    .bind(&route)
                    .fetch_optional(&mut *tx)
                    .await?
                    .ok_or(Fault("not_found"))?;
                if let Some(p) = input {
                    let current:Option<i64>=sqlx::query_scalar("SELECT revision FROM source_route_version_policies WHERE route_id=$1 FOR UPDATE").bind(&route).fetch_optional(&mut *tx).await?;
                    ensure(current.unwrap_or(0) == p.revision, "revision_conflict")?;
                    let policy = serde_json::to_value(&p.policy)?;
                    sqlx::query("INSERT INTO source_route_version_policies(route_id,policy,authorized_by) VALUES($1,$2,$3) ON CONFLICT(route_id) DO UPDATE SET policy=$2,authorized_by=$3,revision=source_route_version_policies.revision+1,effective_at=clock_timestamp()")
      .bind(&route).bind(policy.as_str()).bind(actor.id).execute(&mut *tx).await?;
                    crate::audit::record(
                        &mut tx,
                        actor,
                        Uuid::nil(),
                        "source.version_policy.updated",
                        actor.id,
                        json!({"route_id":route,"policy":p.policy,"reason":p.reason}),
                    )
                    .await
                    .map_err(|_| Fault("temporarily_unavailable"))?;
                }
                sqlx::query_scalar::<_,Value>("SELECT jsonb_build_object('policy',policy,'revision',revision,'effective_at',effective_at) FROM source_route_version_policies WHERE route_id=$1").bind(route).fetch_optional(&mut *tx).await?.unwrap_or(json!({"policy":"replace_unsubmitted","revision":0,"effective_at":null}))
            }
            command => {
                let id = match &command {
                    Command::Detail(id)
                    | Command::Events(id, _)
                    | Command::Observations(id, _)
                    | Command::Targets(id)
                    | Command::Attempts(id, _)
                    | Command::Stop(id, _)
                    | Command::Retry(id, _) => *id,
                    _ => unreachable!(),
                };
                let write = matches!(command, Command::Stop(..) | Command::Retry(..));
                access(&mut tx, actor.id, id, write).await?;
                match command {
                    Command::Detail(_) => {
                        let row: Value =
                            sqlx::query_scalar(&format!("{} WHERE t.id=$2 AND {READ}", row_sql()))
                                .bind(actor.id)
                                .bind(id)
                                .fetch_one(&mut *tx)
                                .await?;
                        let task: TaskRow = serde_json::from_value(row)?;
                        let frozen: Value = sqlx::query_scalar(
                            "SELECT to_jsonb(t)-'id' FROM source_automation_tasks t WHERE id=$1",
                        )
                        .bind(id)
                        .fetch_one(&mut *tx)
                        .await?;
                        let composition:Option<Value>=sqlx::query_scalar("SELECT to_jsonb(c) FROM source_message_compositions c WHERE id=(SELECT composition_id FROM source_execution_tasks WHERE receipt_id=$1)").bind(id).fetch_optional(&mut *tx).await?;
                        let execution = if task.channel == "wecom" {
                            json!(
                                crate::distribution::domain::broadcast::execution_capability(true)
                            )
                        } else {
                            json!({"execution_mode":"device_owned_group_text","target_kind":"group_name","platform_interaction":"none","external_confirmation":"none","result_semantics":"device_observed_sent"})
                        };
                        json!({"task":task,"frozen":frozen,"composition":composition,"execution":execution})
                    }
                    Command::Events(_, after) => {
                        let rows:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('id',id,'kind',kind,'detail',detail,'actor_id',actor_id,'executed_by',executed_by,'created_at',created_at) FROM source_automation_task_events WHERE task_id=$1 AND id>$2 ORDER BY id LIMIT 51").bind(id).bind(after).fetch_all(&mut *tx).await?;
                        json!({"items":rows.iter().take(50).collect::<Vec<_>>(),"next_cursor":if rows.len()>50 {rows[49]["id"].clone()}else{Value::Null}})
                    }
                    Command::Observations(_, after) => {
                        let rows:Vec<Value> = sqlx::query_scalar("SELECT to_jsonb(o) FROM channel_execution_observations o JOIN source_execution_generations g ON g.task_id=o.task_id AND g.channel=o.channel JOIN source_automation_tasks t ON t.id=g.receipt_id AND t.workspace_id=o.workspace_id AND t.channel=o.channel WHERE g.receipt_id=$1 AND o.id>$2 ORDER BY o.id LIMIT 51").bind(id).bind(after).fetch_all(&mut *tx).await?;
                        let items = rows
                            .iter()
                            .take(50)
                            .cloned()
                            .map(
                                serde_json::from_value::<
                                    crate::distribution::domain::execution_evidence::Observation,
                                >,
                            )
                            .collect::<std::result::Result<Vec<_>, _>>()?;
                        json!({"items":items,"next_cursor":if rows.len()>50 {rows[49]["id"].clone()} else {Value::Null}})
                    }
                    Command::Targets(_) => {
                        let rows:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(f)-'identity'-'receipt_id' FROM source_automation_target_facts f WHERE receipt_id=$1 ORDER BY target_id").bind(id).fetch_all(&mut *tx).await?;
                        let excluded:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('target_id',v,'reason','target_unavailable_or_unauthorized') FROM source_automation_tasks t CROSS JOIN LATERAL jsonb_array_elements_text(t.snapshot #> '{execution,target_ids}') v WHERE t.id=$1 AND NOT EXISTS(SELECT 1 FROM jsonb_array_elements(t.snapshot->'targets') x WHERE x->>'id'=v)").bind(id).fetch_all(&mut *tx).await?;
                        json!({"items":rows,"excluded":excluded})
                    }
                    Command::Attempts(_, target) => {
                        let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM source_automation_target_facts WHERE receipt_id=$1 AND target_id=$2)").bind(id).bind(target).fetch_one(&mut *tx).await?;
                        ensure(valid, "target_unavailable")?;
                        let rows:Vec<Value>=sqlx::query_scalar("SELECT evidence FROM (SELECT jsonb_build_object('id',a.id,'channel','mobile','channel_task_id',g.task_id,'generation',g.generation,'attempt_no',a.attempt_no,'current',a.current,'state',a.state,'device_id',p.device_id,'next_attempt_at',a.not_before,'grant_id',x.id,'received_at',r.received_at,'result_received_at',x.result_received_at,'result_status',x.result_body->>'outcome','delivery_id',x.id,'device_started_at',x.device_started_at,'device_finished_at',x.device_finished_at,'device_clock_trusted',false) AS evidence,g.generation,a.attempt_no FROM source_execution_generations g JOIN mobile_v9_task_items i ON i.task_id=g.task_id JOIN mobile_v9_attempts a ON a.item_id=i.id JOIN mobile_v9_plans p ON p.id=a.plan_id LEFT JOIN mobile_execution_grants x ON x.attempt_id=a.id LEFT JOIN LATERAL (SELECT min(received_at) AS received_at FROM mobile_v9_task_receipts WHERE attempt_id=a.id) r ON true WHERE g.receipt_id=$1 AND i.target_id=$2 UNION ALL SELECT jsonb_build_object('id',b.id,'channel','wecom','channel_task_id',g.task_id,'generation',g.generation,'state',b.state,'invocation_id',b.invocation_id,'msgid',b.msgid,'stop_state',b.stop_state,'error_code',b.error_code,'sync_error',b.sync_error,'reconciliation',b.reconciliation,'member_status',b.member_status,'last_synced_at',b.last_synced_at,'sync_failures',b.sync_failures,'next_sync_at',b.next_sync_at),g.generation,0 FROM source_execution_generations g JOIN broadcast_targets x ON x.task_id=g.task_id JOIN broadcast_batches b ON b.id=x.batch_id WHERE g.receipt_id=$1 AND x.group_id=$2) a ORDER BY generation,attempt_no").bind(id).bind(target).fetch_all(&mut *tx).await?;
                        json!({"items":rows})
                    }
                    Command::Stop(_, input) => {
                        self.control(&mut tx, actor, id, input, true).await?
                    }
                    Command::Retry(_, input) => {
                        self.control(&mut tx, actor, id, input, false).await?
                    }
                    _ => unreachable!(),
                }
            }
        };
        tx.commit().await?;
        Ok(result)
    }
}
impl Repository {
    async fn control(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        actor: &Actor,
        id: Uuid,
        input: Control,
        stop: bool,
    ) -> Result<Value> {
        // Same source/route ordering as signing and intake. Never hold a transaction across network calls.
        crate::distribution::south::adapter::postgres::routing::source_guard(tx)
            .await
            .map_err(|_| Fault("temporarily_unavailable"))?;
        sqlx::query("SELECT s.source_id FROM source_callers s JOIN source_automation_tasks t ON t.source_id=s.source_id WHERE t.id=$1 FOR SHARE OF s").bind(id).fetch_one(&mut **tx).await?;
        sqlx::query("SELECT r.route_id FROM source_routes r JOIN source_automation_tasks t ON t.route_id=r.route_id WHERE t.id=$1 FOR UPDATE OF r").bind(id).fetch_optional(&mut **tx).await?;
        let scope =
            sqlx::query("SELECT channel,workspace_id FROM source_automation_tasks WHERE id=$1")
                .bind(id)
                .fetch_one(&mut **tx)
                .await?;
        crate::distribution::south::adapter::postgres::routing::authorize(
            tx,
            actor.id,
            &scope.get::<String, _>("channel"),
            scope.get("workspace_id"),
        )
        .await
        .map_err(|_| Fault("forbidden"))?;
        let row = sqlx::query("SELECT * FROM source_automation_tasks WHERE id=$1 FOR UPDATE")
            .bind(id)
            .fetch_one(&mut **tx)
            .await?;
        access(tx, actor.id, id, true).await?;
        let hash = digest(serde_json::to_vec(&json!({"stop":stop,"input":input}))?.as_slice());
        let prior:Option<(String,Value)>=sqlx::query_as("SELECT request_hash,response FROM source_automation_commands WHERE task_id=$1 AND actor_id=$2 AND idempotency_key=$3").bind(id).bind(actor.id).bind(input.idempotency_key).fetch_optional(&mut **tx).await?;
        if let Some((old, response)) = prior {
            ensure(old == hash, "event_conflict")?;
            return Ok(response);
        }
        ensure(
            row.get::<i64, _>("revision") == input.expected_revision,
            "revision_conflict",
        )?;
        ensure(!row.get::<bool, _>("stopped"), "invalid_transition")?;
        if stop {
            let state: String =
                sqlx::query_scalar("SELECT state FROM source_automation_summaries WHERE id=$1")
                    .bind(id)
                    .fetch_one(&mut **tx)
                    .await?;
            ensure(
                matches!(
                    state.as_str(),
                    "queued" | "scheduled" | "running" | "unknown"
                ),
                "invalid_transition",
            )?;
            ensure(input.target_ids.is_empty(), "invalid_request")?;
            sqlx::query("UPDATE source_automation_tasks SET stopped=true,stop_reason=$2,revision=revision+1 WHERE id=$1").bind(id).bind(&input.reason).execute(&mut **tx).await?;
            super::routing_worker::halt(tx, id, "operator_stopped").await?;
            sqlx::query("UPDATE source_inbox SET state='stopped' WHERE receipt_id=$1 AND state IN ('received','waiting')").bind(id).execute(&mut **tx).await?;
            sqlx::query("UPDATE source_routing_waits SET stopped=true WHERE receipt_id=$1")
                .bind(id)
                .execute(&mut **tx)
                .await?;
            // Reuse the official executor's durable stop operations, including late-created batches.
            sqlx::query("INSERT INTO broadcast_operations(id,school_id,task_id,actor_id,operation,idempotency_key,request_hash,state) SELECT gen_random_uuid(),g.workspace_id,g.task_id,$2,'stop','automation-stop:'||g.task_id::text,$3,'done' FROM source_execution_generations g WHERE g.receipt_id=$1 AND g.channel='wecom' ON CONFLICT DO NOTHING").bind(id).bind(actor.id).bind(&hash).execute(&mut **tx).await?;
        } else {
            let eligible:Vec<Uuid>=sqlx::query_scalar("SELECT target_id FROM source_automation_target_facts WHERE receipt_id=$1 AND state IN ('queued','scheduled','retry_wait') AND ($2::uuid[]='{}' OR target_id=ANY($2))").bind(id).bind(&input.target_ids).fetch_all(&mut **tx).await?;
            ensure(
                !eligible.is_empty() && input.target_ids.iter().all(|t| eligible.contains(t)),
                "invalid_transition",
            )?;
            sqlx::query("UPDATE mobile_v9_dispatch_state d SET next_retry_at=LEAST(next_retry_at,now()) FROM mobile_v9_task_items i JOIN source_execution_tasks e ON e.task_id=i.task_id WHERE e.receipt_id=$1 AND d.item_id=i.id AND i.target_id=ANY($2) AND d.state IN ('scheduling','waiting_resources') AND d.completed_rounds<3").bind(id).bind(&eligible).execute(&mut **tx).await?;
            sqlx::query("UPDATE broadcast_batches b SET next_sync_at=LEAST(next_sync_at,now()) FROM broadcast_targets x JOIN source_execution_tasks e ON e.task_id=x.task_id WHERE e.receipt_id=$1 AND b.id=x.batch_id AND x.group_id=ANY($2) AND b.state='queued'").bind(id).bind(&eligible).execute(&mut **tx).await?;
            sqlx::query("UPDATE source_automation_tasks SET revision=revision+1 WHERE id=$1")
                .bind(id)
                .execute(&mut **tx)
                .await?;
        }
        let kind = if stop {
            "task.stop_requested"
        } else {
            "task.retry_requested"
        };
        sqlx::query("INSERT INTO source_automation_task_events(task_id,kind,actor_id,executed_by,detail) VALUES($1,$2,$3,'user',$4)").bind(id).bind(kind).bind(actor.id).bind(json!({"reason":input.reason,"target_ids":input.target_ids})).execute(&mut **tx).await?;
        crate::audit::record(
            tx,
            actor,
            Uuid::nil(),
            kind,
            id,
            json!({"reason":input.reason,"target_ids":input.target_ids}),
        )
        .await
        .map_err(|_| Fault("temporarily_unavailable"))?;
        let response = json!({"id":id,"accepted":true});
        sqlx::query("INSERT INTO source_automation_commands VALUES($1,$2,$3,$4,$5)")
            .bind(id)
            .bind(actor.id)
            .bind(input.idempotency_key)
            .bind(hash)
            .bind(&response)
            .execute(&mut **tx)
            .await?;
        Ok(response)
    }
}

impl super::routing::RoutingRepository {
    pub async fn recover_orchestration(&self) -> Result<()> {
        let mut tx = self.0.begin().await?;
        // Backfill only actual channel tasks; preserve historical times, never invent execution events.
        let ids:Vec<Uuid>=sqlx::query_scalar("SELECT e.receipt_id FROM source_execution_tasks e WHERE NOT EXISTS(SELECT 1 FROM source_automation_tasks t WHERE t.id=e.receipt_id) ORDER BY e.created_at,e.receipt_id LIMIT 50 FOR UPDATE OF e SKIP LOCKED").fetch_all(&mut *tx).await?;
        sqlx::query("INSERT INTO source_automation_tasks(id,decision_id,source_id,route_id,unit_id,channel,workspace_id,route_revision,snapshot,content,composition_id,authorized_by,created_at,reconstructed,stopped,stop_reason) SELECT e.receipt_id,gen_random_uuid(),r.source_id,r.route_id,(d.snapshot->>'unit_id')::uuid,e.channel,e.workspace_id,e.config_revision,d.snapshot,COALESCE(c.content,b.content,ms.content),e.composition_id,e.authorization_actor_id,e.created_at,true,COALESCE(m.stopped,b.stopped,false),CASE WHEN COALESCE(m.stopped,b.stopped,false) THEN 'historical_channel_stop' END FROM source_execution_tasks e JOIN source_receipts r USING(receipt_id) JOIN source_routing_decisions d USING(receipt_id) LEFT JOIN source_message_compositions c ON c.id=e.composition_id LEFT JOIN broadcast_tasks b ON b.id=e.task_id LEFT JOIN mobile_v9_tasks m ON m.id=e.task_id LEFT JOIN LATERAL (SELECT s.content FROM mobile_v9_task_items i JOIN mobile_v9_plans p ON p.item_id=i.id JOIN distribution_message_snapshots s ON s.id=p.snapshot_id WHERE i.task_id=e.task_id LIMIT 1) ms ON true WHERE e.receipt_id=ANY($1) ON CONFLICT DO NOTHING").bind(&ids).execute(&mut *tx).await?;
        let job=sqlx::query("SELECT * FROM source_orchestration_recovery_jobs WHERE NOT complete ORDER BY id LIMIT 1 FOR UPDATE SKIP LOCKED").fetch_optional(&mut *tx).await?;
        if let Some(job) = job {
            let id: i64 = job.get("id");
            let route: Option<String> = job.get("route_id");
            let after: Option<Uuid> = job.get("after_receipt");
            let scope: Option<String> = job.get("scope_kind");
            let scope_id: Option<Uuid> = job.get("scope_id");
            let rows:Vec<Uuid>=sqlx::query_scalar("SELECT i.receipt_id FROM source_inbox i JOIN source_receipts r USING(receipt_id) WHERE i.state='waiting' AND ($1::text IS NULL OR r.route_id=$1) AND ($2::uuid IS NULL OR i.receipt_id>$2) AND ($3::text IS NULL OR EXISTS(SELECT 1 FROM source_routes sr WHERE sr.route_id=r.route_id AND CASE WHEN $3='unit' THEN sr.unit_id=$4 ELSE sr.group_id=$4 END)) AND NOT EXISTS(SELECT 1 FROM source_automation_tasks t WHERE t.id=i.receipt_id AND t.stopped) AND NOT EXISTS(SELECT 1 FROM source_routing_waits w WHERE w.receipt_id=i.receipt_id AND w.stopped) AND EXISTS(SELECT 1 FROM source_content_versions v WHERE v.receipt_id=i.receipt_id AND NOT v.withdrawn AND NOT v.no_resend) ORDER BY i.receipt_id LIMIT 50").bind(route).bind(after).bind(scope).bind(scope_id).fetch_all(&mut *tx).await?;
            sqlx::query("UPDATE source_inbox SET next_attempt_at=now() WHERE receipt_id=ANY($1)")
                .bind(&rows)
                .execute(&mut *tx)
                .await?;
            sqlx::query("UPDATE source_routing_waits SET next_attempt_at=now() WHERE receipt_id=ANY($1) AND NOT stopped").bind(&rows).execute(&mut *tx).await?;
            sqlx::query("UPDATE source_orchestration_recovery_jobs SET after_receipt=COALESCE($2,after_receipt),complete=$3 WHERE id=$1").bind(id).bind(rows.last()).bind(rows.len()<50).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(())
    }
}
