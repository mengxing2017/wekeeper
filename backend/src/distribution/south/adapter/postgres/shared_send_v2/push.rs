//! Durable direct delivery. Transactions prepare work; connection owners send after commit.
use super::*;
use crate::distribution::domain::shared_mobile::execution::GrantRequest;

pub(super) async fn prepare(tx: &mut Tx<'_>, attempt: Uuid) -> Result<Option<&'static str>> {
    let device: Uuid = sqlx::query_scalar("SELECT p.device_id FROM mobile_v9_attempts a JOIN mobile_v9_plans p ON p.id=a.plan_id WHERE a.id=$1")
        .bind(attempt).fetch_one(&mut **tx).await?;
    // Same phone lock as consent changes; credentials and runtime belong to this connection.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,78234610244))")
        .bind(device.to_string())
        .execute(&mut **tx)
        .await?;
    let identity: Option<(String,i64,Uuid)> = sqlx::query_as("SELECT c.token_hash,c.generation,s.runtime_epoch FROM device_control_sessions s JOIN device_credentials c ON c.device_id=s.device_id AND c.generation=s.credential_generation JOIN execution_devices d ON d.id=s.device_id AND d.credential_generation=c.generation WHERE s.device_id=$1 AND s.connected AND s.transport='wss' AND s.runtime_epoch IS NOT NULL AND d.enabled AND c.revoked_at IS NULL AND c.expires_at>clock_timestamp()+interval '210 seconds' ORDER BY c.expires_at DESC LIMIT 1")
        .bind(device).fetch_optional(&mut **tx).await?;
    let Some((token_hash, generation, runtime_epoch)) = identity else {
        records::check(
            tx,
            device,
            "delivery_connection",
            "delivery_connection_unavailable",
            json!({"attempt_id":attempt}),
        )
        .await?;
        return Ok(Some("delivery_connection_unavailable"));
    };
    let c = Credential {
        device_id: device,
        generation,
        token_hash,
    };
    let value = match execution::grant(
        tx,
        &c,
        GrantRequest {
            runtime_epoch,
            eligible_attempt_ids: vec![attempt],
            inspection_id: None,
        },
    )
    .await
    {
        Ok(value) => value,
        Err(Error::Policy(
            reason @ ("execution_credential_renewal_required" | "shared_send_consent_required"),
        )) => return Ok(Some(reason)),
        Err(error) => return Err(error),
    };
    if value["state"] != "granted" {
        return Ok(Some(match value["reason"].as_str() {
            Some("device_busy") => "device_busy",
            Some("plan_dependency_changed") => "plan_dependency_changed",
            _ => "delivery_conditions_unmet",
        }));
    }
    let id = uid(&value, "id")?;
    sqlx::query("INSERT INTO mobile_execution_delivery_outbox(execution_id) VALUES($1)")
        .bind(id)
        .execute(&mut **tx)
        .await?;
    sqlx::query("SELECT pg_notify('mobile_execution_delivery',$1)")
        .bind(device.to_string())
        .execute(&mut **tx)
        .await?;
    Ok(None)
}

impl Repository {
    pub async fn pending_deliveries(
        &self,
        c: &Credential,
        connection: Uuid,
        epoch: i64,
    ) -> Result<Vec<crate::distribution::domain::shared_mobile::execution::ExecutionDelivery>> {
        let mut tx = self.0.begin().await?;
        authenticate(&mut tx, c, true).await?;
        let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM device_control_sessions WHERE device_id=$1 AND connection_id=$2 AND epoch=$3 AND credential_generation=$4 AND connected AND transport='wss')")
            .bind(c.device_id).bind(connection).bind(epoch).bind(c.generation).fetch_one(&mut *tx).await?;
        require(valid, "control_session_changed")?;
        sqlx::query("UPDATE mobile_execution_delivery_outbox o SET finished_at=clock_timestamp() FROM mobile_execution_grants g JOIN mobile_v9_attempts a ON a.id=g.attempt_id JOIN mobile_v9_task_items i ON i.id=a.item_id JOIN mobile_v9_tasks t ON t.id=i.task_id WHERE o.execution_id=g.id AND g.device_id=$1 AND o.finished_at IS NULL AND (g.state<>'active' OR g.session_closed OR g.accepted_at IS NOT NULL OR g.result_id IS NOT NULL OR g.receipt_deadline_at<=clock_timestamp() OR g.execute_before<=clock_timestamp() OR NOT a.current OR t.stopped OR EXISTS(SELECT 1 FROM mobile_v9_attempts done LEFT JOIN mobile_execution_grants result ON result.attempt_id=done.id WHERE done.item_id=a.item_id AND (done.state IN ('observed_sent','manually_confirmed_sent') OR result.result_body->>'outcome'='observed_sent')))").bind(c.device_id).execute(&mut *tx).await?;
        let ids:Vec<Uuid>=sqlx::query_scalar("SELECT o.execution_id FROM mobile_execution_delivery_outbox o JOIN mobile_execution_grants g ON g.id=o.execution_id WHERE g.device_id=$1 AND g.credential_generation=$2 AND o.finished_at IS NULL AND o.next_send_at<=clock_timestamp() ORDER BY o.next_send_at LIMIT 1 FOR UPDATE OF o SKIP LOCKED")
            .bind(c.device_id).bind(c.generation).fetch_all(&mut *tx).await?;
        let mut deliveries = vec![];
        for id in ids {
            if !execution::can_redeliver(&mut tx, id).await? {
                sqlx::query("UPDATE mobile_execution_delivery_outbox SET finished_at=clock_timestamp() WHERE execution_id=$1").bind(id).execute(&mut *tx).await?;
                continue;
            }
            let envelope: Vec<u8> =
                sqlx::query_scalar("SELECT envelope FROM mobile_execution_grants WHERE id=$1")
                    .bind(id)
                    .fetch_one(&mut *tx)
                    .await?;
            let transmission = Uuid::now_v7();
            sqlx::query("INSERT INTO mobile_delivery_exchanges(id,device_id,generation,key_digest,intake_id,attempt_id,execution_id,result,finished_at,connection_id,connection_epoch,replay_of) SELECT $1,g.device_id,g.credential_generation,$2,i.id,g.attempt_id,g.id,'granted',clock_timestamp(),$4,$5,(SELECT x.id FROM mobile_delivery_exchanges x WHERE x.execution_id=g.id ORDER BY x.created_at,x.id LIMIT 1) FROM mobile_execution_grants g JOIN mobile_v9_attempts a ON a.id=g.attempt_id LEFT JOIN mobile_dispatch_intakes i ON i.item_id=a.item_id WHERE g.id=$3")
                .bind(transmission).bind(hash(&id.to_string())).bind(id).bind(connection).bind(epoch).execute(&mut *tx).await?;
            sqlx::query("UPDATE mobile_execution_delivery_outbox SET next_send_at=clock_timestamp()+interval '5 seconds',attempts=attempts+1 WHERE execution_id=$1").bind(id).execute(&mut *tx).await?;
            deliveries.push(serde_json::from_value(json!({"schema":"mobile_execution_delivery/1","delivery_id":id,"transmission_id":transmission,"connection_id":connection,"epoch":epoch,"delivery":execution::delivery_response(secrets::open(&envelope)?)?})).map_err(|_|Error::Conflict)?);
        }
        tx.commit().await?;
        Ok(deliveries)
    }
    pub async fn delivery_written(&self, id: Uuid, completed: bool) -> Result<()> {
        let mut tx = self.0.begin().await?;
        sqlx::query("UPDATE mobile_delivery_exchanges SET response_state=$2,response_at=clock_timestamp(),reason_code=CASE WHEN $3 THEN reason_code ELSE 'delivery_write_failed' END WHERE id=$1")
            .bind(id).bind(if completed {"server_write_completed"} else {"server_write_interrupted"}).bind(completed).execute(&mut *tx).await?;
        if !completed {
            sqlx::query("UPDATE mobile_execution_grants SET state='superseded',superseded_reason='delivery_write_failed' WHERE id=(SELECT execution_id FROM mobile_delivery_exchanges WHERE id=$1) AND state='active' AND accepted_at IS NULL AND result_id IS NULL").bind(id).execute(&mut *tx).await?;
            sqlx::query("UPDATE mobile_v9_dispatch_state ds SET next_retry_at=clock_timestamp() FROM mobile_v9_attempts a JOIN mobile_delivery_exchanges x ON x.attempt_id=a.id WHERE x.id=$1 AND ds.item_id=a.item_id AND ds.state NOT IN ('completed','stopped','confirmation_expired','dispatch_failed')").bind(id).execute(&mut *tx).await?;
            sqlx::query("INSERT INTO mobile_v9_dispatch_events(intake_id,item_id,attempt_id,device_id,execution_id,delivery_request_id,kind,reason) SELECT x.intake_id,a.item_id,x.attempt_id,x.device_id,x.execution_id,x.id,'delivery_failed','delivery_write_failed' FROM mobile_delivery_exchanges x JOIN mobile_v9_attempts a ON a.id=x.attempt_id WHERE x.id=$1").bind(id).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(())
    }
}

static DELIVERY_WAKES: std::sync::LazyLock<tokio::sync::broadcast::Sender<Uuid>> =
    std::sync::LazyLock::new(|| tokio::sync::broadcast::channel(256).0);
pub(super) fn subscribe() -> tokio::sync::broadcast::Receiver<Uuid> {
    DELIVERY_WAKES.subscribe()
}
impl Repository {
    /// One PostgreSQL listener per API process; durable polling covers reconnect and lag.
    pub async fn listen_delivery_notifications(&self) -> Result<()> {
        let mut listener = sqlx::postgres::PgListener::connect_with(&self.0).await?;
        listener.listen("mobile_execution_delivery").await?;
        loop {
            let event = listener.recv().await?;
            if let Ok(device) = event.payload().parse::<Uuid>() {
                let _ = DELIVERY_WAKES.send(device);
            }
        }
    }
    pub fn subscribe_delivery_notifications() -> tokio::sync::broadcast::Receiver<Uuid> {
        subscribe()
    }
}
