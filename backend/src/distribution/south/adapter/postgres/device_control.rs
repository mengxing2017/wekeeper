use crate::{
    business::{Error, Result, hash},
    distribution::{domain::device_control::*, south::port::device_control::Store},
};
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;
#[derive(Clone)]
pub struct Repository(pub PgPool);
async fn admin(tx: &mut Transaction<'_, Postgres>, actor: Uuid) -> Result<()> {
    sqlx::query("SELECT id FROM accounts WHERE id=$1 AND active AND platform_admin FOR SHARE")
        .bind(actor)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(Error::Forbidden)?;
    Ok(())
}
async fn value_command(tx: &mut Transaction<'_, Postgres>, id: Uuid) -> Result<Value> {
    sqlx::query_scalar("SELECT to_jsonb(c) FROM device_control_commands c WHERE id=$1")
        .bind(id)
        .fetch_one(&mut **tx)
        .await
        .map_err(Into::into)
}
impl Store for Repository {
    async fn authenticate(&self, token: &str) -> Result<Credential> {
        if !token.starts_with("wk_device_") || token.len() > 256 {
            return Err(Error::Unauthorized);
        }
        let token_hash = hash(token);
        let (device_id,generation): (Uuid,i64)=sqlx::query_as("SELECT c.device_id,c.generation FROM device_credentials c JOIN execution_devices d ON d.id=c.device_id WHERE c.token_hash=$1 AND c.revoked_at IS NULL AND c.expires_at>clock_timestamp() AND d.enabled AND d.credential_generation=c.generation")
            .bind(&token_hash).fetch_optional(&self.0).await?.ok_or(Error::Unauthorized)?;
        Ok(Credential {
            device_id,
            generation,
            token_hash,
        })
    }

    async fn device(&self, c: &Credential, op: Operation) -> Result<Frame> {
        let mut tx = self.0.begin().await?;
        let expires:chrono::DateTime<chrono::Utc>=sqlx::query_scalar("SELECT c.expires_at FROM device_credentials c JOIN execution_devices d ON d.id=c.device_id WHERE c.token_hash=$1 AND c.device_id=$2 AND c.generation=$3 AND c.revoked_at IS NULL AND c.expires_at>clock_timestamp() AND d.enabled AND d.credential_generation=c.generation FOR SHARE OF c,d")
   .bind(&c.token_hash).bind(c.device_id).bind(c.generation).fetch_optional(&mut *tx).await?.ok_or(Error::Unauthorized)?;
        if matches!(&op, Operation::Renew { .. } | Operation::Activate) {
            // Renewal is independent of whether a control session has ever opened.
            sqlx::query("INSERT INTO device_control_sessions(device_id,connection_id,transport,connected) VALUES($1,$2,'https',false) ON CONFLICT(device_id) DO NOTHING")
                .bind(c.device_id).bind(Uuid::new_v4()).execute(&mut *tx).await?;
        }
        let event = match &op {
            Operation::Open { .. } => "control_session_opened",
            Operation::Close { .. } => "control_session_closed",
            Operation::Pulse(_) => "control_heartbeat_received",
            _ => "control_frame_polled",
        };
        let mut closing = false;
        let mut heartbeat_id = None;
        let mut heartbeat_fresh = false;
        match op {
            Operation::Renew { candidate_hash } => {
                if candidate_hash.len() != 64
                    || !candidate_hash.bytes().all(|b| b.is_ascii_hexdigit())
                {
                    return Err(Error::Invalid("候选凭据摘要无效".into()));
                }
                // Serialize renewal for this token without the delivery global guard.
                sqlx::query(
                    "SELECT token_hash FROM device_credentials WHERE token_hash=$1 FOR UPDATE",
                )
                .bind(&c.token_hash)
                .fetch_one(&mut *tx)
                .await?;
                let previous: Option<String> = sqlx::query_scalar(
                    "SELECT previous_hash FROM device_credential_renewals WHERE candidate_hash=$1",
                )
                .bind(&candidate_hash)
                .fetch_optional(&mut *tx)
                .await?;
                if let Some(previous) = previous {
                    if previous != c.token_hash {
                        return Err(Error::Conflict);
                    }
                } else {
                    let pending: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM device_credential_renewals WHERE previous_hash=$1 AND activated_at IS NULL)")
                        .bind(&c.token_hash).fetch_one(&mut *tx).await?;
                    if pending {
                        return Err(Error::Conflict);
                    }
                    sqlx::query("INSERT INTO device_credentials(token_hash,device_id,generation,expires_at) VALUES($1,$2,$3,clock_timestamp()+interval '30 days')").bind(&candidate_hash).bind(c.device_id).bind(c.generation).execute(&mut *tx).await?;
                    sqlx::query("INSERT INTO device_credential_renewals(candidate_hash,previous_hash) VALUES($1,$2)").bind(candidate_hash).bind(&c.token_hash).execute(&mut *tx).await?;
                }
            }
            Operation::Activate => {
                let previous:Option<String>=sqlx::query_scalar("UPDATE device_credential_renewals SET activated_at=coalesce(activated_at,clock_timestamp()) WHERE candidate_hash=$1 RETURNING previous_hash").bind(&c.token_hash).fetch_optional(&mut *tx).await?;
                if let Some(previous) = previous {
                    sqlx::query("UPDATE device_credentials SET expires_at=least(expires_at,(SELECT activated_at+interval '10 minutes' FROM device_credential_renewals WHERE candidate_hash=$2)) WHERE token_hash=$1").bind(previous).bind(&c.token_hash).execute(&mut *tx).await?;
                } else {
                    return Err(Error::Policy("control_renewal_missing"));
                }
            }
            Operation::Open { transport } => {
                if !matches!(transport.as_str(), "https" | "wss") {
                    return Err(Error::Invalid("无效控制传输".into()));
                }
                sqlx::query("INSERT INTO device_control_sessions(device_id,connection_id,transport,credential_generation) VALUES($1,$2,$3,$4) ON CONFLICT(device_id) DO UPDATE SET connection_id=EXCLUDED.connection_id,epoch=device_control_sessions.epoch+1,transport=EXCLUDED.transport,credential_generation=EXCLUDED.credential_generation,connected=true,connected_at=clock_timestamp(),presence_started_at=CASE WHEN device_control_sessions.credential_generation=EXCLUDED.credential_generation THEN device_control_sessions.presence_started_at ELSE clock_timestamp() END,last_heartbeat_at=CASE WHEN device_control_sessions.credential_generation=EXCLUDED.credential_generation THEN device_control_sessions.last_heartbeat_at ELSE NULL END,readiness='waiting',block_reason=NULL,disconnect_reason=NULL")
     .bind(c.device_id).bind(Uuid::new_v4()).bind(transport).bind(c.generation).execute(&mut *tx).await?;
                sqlx::query("DELETE FROM device_control_pulse_receipts WHERE device_id=$1")
                    .bind(c.device_id)
                    .execute(&mut *tx)
                    .await?;
            }
            Operation::Pulse(p) => {
                heartbeat_id = p.heartbeat_id;
                // Lock the current session before deduplicating or renewing presence.
                let transport: String = sqlx::query_scalar("SELECT transport FROM device_control_sessions WHERE device_id=$1 AND connection_id=$2 AND epoch=$3 AND connected FOR UPDATE")
                    .bind(c.device_id).bind(p.connection_id).bind(p.epoch)
                    .fetch_optional(&mut *tx).await?.ok_or(Error::Policy("control_session_changed"))?;
                let fresh = if p.heartbeat_sequence > 0 {
                    sqlx::query("INSERT INTO device_control_pulse_receipts(device_id,connection_id,heartbeat_id,heartbeat_sequence) VALUES($1,$2,$3,$4) ON CONFLICT(device_id) DO UPDATE SET connection_id=EXCLUDED.connection_id,heartbeat_id=EXCLUDED.heartbeat_id,heartbeat_sequence=EXCLUDED.heartbeat_sequence WHERE device_control_pulse_receipts.connection_id=EXCLUDED.connection_id AND device_control_pulse_receipts.heartbeat_sequence<EXCLUDED.heartbeat_sequence")
                        .bind(c.device_id).bind(p.connection_id).bind(p.heartbeat_id.ok_or_else(|| Error::Invalid("缺少心跳标识".into()))?).bind(p.heartbeat_sequence)
                        .execute(&mut *tx).await?.rows_affected() == 1
                } else {
                    false
                };
                heartbeat_fresh = fresh;
                let active:Option<Uuid>=sqlx::query_scalar("UPDATE device_control_sessions SET last_heartbeat_at=CASE WHEN $6 THEN clock_timestamp() ELSE last_heartbeat_at END,connected=true,readiness=$4,block_reason=$5 WHERE device_id=$1 AND connection_id=$2 AND epoch=$3 AND connected RETURNING device_id")
     .bind(c.device_id).bind(p.connection_id).bind(p.epoch).bind(&p.readiness).bind(&p.block_reason).bind(fresh && (transport == "https" || p.probe_id.is_some())).fetch_optional(&mut *tx).await?;
                active.ok_or(Error::Policy("control_session_changed"))?;
                for receipt in p.receipts {
                    let state = match receipt.state {
                        ReceiptState::Accepted => "accepted",
                        ReceiptState::Completed => "completed",
                        ReceiptState::Rejected => "rejected",
                    };
                    sqlx::query("UPDATE device_control_commands SET state=$3,accepted_at=coalesce(accepted_at,clock_timestamp()),completed_at=CASE WHEN $3 IN ('completed','rejected') THEN coalesce(completed_at,clock_timestamp()) ELSE completed_at END,result_code=$4 WHERE device_id=$1 AND id=$2 AND state IN ('queued','delivered','accepted') AND expires_at>clock_timestamp()")
      .bind(c.device_id).bind(receipt.id).bind(state).bind(receipt.result_code).execute(&mut *tx).await?;
                }
            }
            Operation::ConfirmHeartbeat {
                connection_id,
                epoch,
                heartbeat_id,
            } => {
                // The successful WSS reply completes the exchange. Never confirm an
                // old/replaced pulse or extend a replacement connection's evidence.
                sqlx::query("UPDATE device_control_sessions s SET last_heartbeat_at=clock_timestamp() FROM device_control_pulse_receipts r WHERE s.device_id=$1 AND s.connection_id=$2 AND s.epoch=$3 AND s.connected AND r.device_id=s.device_id AND r.connection_id=s.connection_id AND r.heartbeat_id=$4")
                    .bind(c.device_id).bind(connection_id).bind(epoch).bind(heartbeat_id).execute(&mut *tx).await?;
            }
            Operation::Poll {
                connection_id,
                epoch,
            } => {
                let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM device_control_sessions WHERE device_id=$1 AND connection_id=$2 AND epoch=$3 AND connected)")
     .bind(c.device_id).bind(connection_id).bind(epoch).fetch_one(&mut *tx).await?;
                if !valid {
                    return Err(Error::Policy("control_session_changed"));
                }
            }
            Operation::Close {
                connection_id,
                epoch,
            } => {
                sqlx::query("UPDATE device_control_sessions SET connected=false,last_disconnected_at=clock_timestamp(),disconnect_reason='transport_closed' WHERE device_id=$1 AND connection_id=$2 AND epoch=$3").bind(c.device_id).bind(connection_id).bind(epoch).execute(&mut *tx).await?;
                closing = true;
            }
        }
        sqlx::query("UPDATE device_control_commands SET state='expired' WHERE device_id=$1 AND state IN ('queued','delivered','accepted') AND expires_at<=clock_timestamp()")
   .bind(c.device_id).execute(&mut *tx).await?;
        let raw:Value=sqlx::query_scalar("SELECT jsonb_build_object('id',device_id,'connection_id',connection_id,'epoch',epoch,'protocol_version',1,'heartbeat_interval_seconds',p.heartbeat_interval_seconds,'offline_after_seconds',p.offline_after_seconds,'server_time',clock_timestamp(),'desired_paused',desired_paused,'desired_revision',desired_revision,'credential_expires_at',$2::timestamptz,'commands','[]'::jsonb) FROM device_control_sessions CROSS JOIN device_control_policy p WHERE device_id=$1")
   .bind(c.device_id).bind(expires).fetch_one(&mut *tx).await?;
        let mut frame: Frame =
            serde_json::from_value(raw).map_err(|e| Error::Database(e.to_string()))?;
        frame.heartbeat_id = heartbeat_id;
        frame.heartbeat_fresh = heartbeat_fresh;
        if !closing {
            let rows:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(c) FROM device_control_commands c WHERE device_id=$1 AND state IN ('queued','delivered','accepted') AND kind NOT IN ('pause_execution','resume_execution') ORDER BY sequence LIMIT 100").bind(c.device_id).fetch_all(&mut *tx).await?;
            for row in rows {
                frame
                    .commands
                    .push(serde_json::from_value(row).map_err(|e| Error::Database(e.to_string()))?);
            }
            sqlx::query("UPDATE device_control_commands SET state='delivered' WHERE id=ANY($1) AND state='queued'").bind(frame.commands.iter().map(|x|x.id).collect::<Vec<_>>()).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        if event != "control_heartbeat_received" && event != "control_frame_polled" {
            tracing::debug!(event, device_id=%c.device_id, connection_id=%frame.connection_id,
                epoch=frame.epoch, "device control committed");
        }
        Ok(frame)
    }
    async fn presence(&self, actor: Uuid, device: Uuid) -> Result<Value> {
        let mut tx = self.0.begin().await?;
        admin(&mut tx, actor).await?;
        let mut presence: Value =
            sqlx::query_scalar("SELECT to_jsonb(p) FROM device_control_presence p WHERE id=$1")
                .bind(device)
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(Error::NotFound)?;
        // Validate the public projection against the transport-neutral wire contract.
        let snapshot: Presence =
            serde_json::from_value(presence).map_err(|e| Error::Database(e.to_string()))?;
        if !snapshot.policy.valid() {
            return Err(Error::Policy("control_policy_invalid"));
        }
        presence = serde_json::to_value(snapshot).map_err(|e| Error::Database(e.to_string()))?;
        presence["commands"]=json!(sqlx::query_scalar::<_,Value>("SELECT to_jsonb(c)-'request_digest'-'request_key' FROM device_control_commands c WHERE device_id=$1 ORDER BY sequence DESC LIMIT 30").bind(device).fetch_all(&mut *tx).await?);
        tx.commit().await?;
        Ok(presence)
    }
    async fn enqueue(
        &self,
        actor: Uuid,
        device: Uuid,
        key: &str,
        input: CommandInput,
    ) -> Result<Value> {
        let mut tx = self.0.begin().await?;
        admin(&mut tx, actor).await?;
        if input.kind == CommandKind::CancelExecution {
            return Err(Error::Forbidden);
        }
        sqlx::query("SELECT id FROM execution_devices WHERE id=$1 FOR UPDATE")
            .bind(device)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(Error::NotFound)?;
        let digest =
            hash(&serde_json::to_string(&input).map_err(|e| Error::Database(e.to_string()))?);
        if let Some(old)=sqlx::query_scalar::<_,Value>("SELECT to_jsonb(c) FROM device_control_commands c WHERE device_id=$1 AND request_key=$2").bind(device).bind(key).fetch_optional(&mut *tx).await? {
   if old["request_digest"]!=digest{return Err(Error::Conflict)}
   tx.commit().await?;return Ok(old)
  }
        let kind = serde_json::to_value(input.kind).map_err(|e| Error::Database(e.to_string()))?;
        let id = Uuid::new_v4();
        sqlx::query("INSERT INTO device_control_commands(id,device_id,kind,desired_revision,request_key,request_digest,actor_id) VALUES($1,$2,$3,$4,$5,$6,$7)")
   .bind(id).bind(device).bind(kind.as_str().ok_or(Error::Conflict)?).bind(Option::<i64>::None).bind(key).bind(digest).bind(actor).execute(&mut *tx).await?;
        let result = value_command(&mut tx, id).await?;
        tx.commit().await?;
        Ok(result)
    }
}
