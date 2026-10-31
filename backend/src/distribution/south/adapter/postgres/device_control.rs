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
    async fn device(&self, c: &Credential, op: Operation) -> Result<Frame> {
        let mut tx = self.0.begin().await?;
        let expires:chrono::DateTime<chrono::Utc>=sqlx::query_scalar("SELECT c.expires_at FROM device_credentials c JOIN execution_devices d ON d.id=c.device_id WHERE c.token_hash=$1 AND c.device_id=$2 AND c.generation=$3 AND c.revoked_at IS NULL AND c.expires_at>clock_timestamp() AND d.enabled AND d.credential_generation=c.generation FOR SHARE OF c,d")
   .bind(&c.token_hash).bind(c.device_id).bind(c.generation).fetch_optional(&mut *tx).await?.ok_or(Error::Unauthorized)?;
        if matches!(&op, Operation::Renew { .. } | Operation::Activate) {
            // Renewal is independent of whether a control session has ever opened.
            sqlx::query("INSERT INTO device_control_sessions(device_id,connection_id,transport,connected) VALUES($1,$2,'https',false) ON CONFLICT(device_id) DO NOTHING")
                .bind(c.device_id).bind(Uuid::new_v4()).execute(&mut *tx).await?;
        }
        let mut closing = false;
        let mut heartbeat_id = None;
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
                sqlx::query("INSERT INTO device_control_sessions(device_id,connection_id,transport) VALUES($1,$2,$3) ON CONFLICT(device_id) DO UPDATE SET connection_id=EXCLUDED.connection_id,epoch=device_control_sessions.epoch+1,transport=EXCLUDED.transport,connected=true,last_heartbeat_at=clock_timestamp()")
     .bind(c.device_id).bind(Uuid::new_v4()).bind(transport).execute(&mut *tx).await?;
            }
            Operation::Pulse(p) => {
                heartbeat_id = p.heartbeat_id;
                let active:Option<Uuid>=sqlx::query_scalar("UPDATE device_control_sessions SET last_heartbeat_at=clock_timestamp(),connected=true,readiness=$4,block_reason=$5 WHERE device_id=$1 AND connection_id=$2 AND epoch=$3 RETURNING device_id")
     .bind(c.device_id).bind(p.connection_id).bind(p.epoch).bind(&p.readiness).bind(&p.block_reason).fetch_optional(&mut *tx).await?;
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
            Operation::Poll {
                connection_id,
                epoch,
            } => {
                let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM device_control_sessions WHERE device_id=$1 AND connection_id=$2 AND epoch=$3 AND connected AND last_heartbeat_at>clock_timestamp()-interval '65 seconds')")
     .bind(c.device_id).bind(connection_id).bind(epoch).fetch_one(&mut *tx).await?;
                if !valid {
                    return Err(Error::Policy("control_session_changed"));
                }
            }
            Operation::Close {
                connection_id,
                epoch,
            } => {
                sqlx::query("UPDATE device_control_sessions SET connected=false WHERE device_id=$1 AND connection_id=$2 AND epoch=$3").bind(c.device_id).bind(connection_id).bind(epoch).execute(&mut *tx).await?;
                closing = true;
            }
        }
        sqlx::query("UPDATE device_control_commands SET state='expired' WHERE device_id=$1 AND state IN ('queued','delivered','accepted') AND expires_at<=clock_timestamp()")
   .bind(c.device_id).execute(&mut *tx).await?;
        let raw:Value=sqlx::query_scalar("SELECT jsonb_build_object('id',device_id,'connection_id',connection_id,'epoch',epoch,'protocol_version',1,'heartbeat_interval_seconds',20,'offline_after_seconds',65,'server_time',clock_timestamp(),'desired_paused',desired_paused,'desired_revision',desired_revision,'credential_expires_at',$2::timestamptz,'commands','[]'::jsonb) FROM device_control_sessions WHERE device_id=$1")
   .bind(c.device_id).bind(expires).fetch_one(&mut *tx).await?;
        let mut frame: Frame =
            serde_json::from_value(raw).map_err(|e| Error::Database(e.to_string()))?;
        frame.heartbeat_id = heartbeat_id;
        if !closing {
            let rows:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(c) FROM device_control_commands c WHERE device_id=$1 AND state IN ('queued','delivered','accepted') ORDER BY sequence LIMIT 100").bind(c.device_id).fetch_all(&mut *tx).await?;
            for row in rows {
                frame
                    .commands
                    .push(serde_json::from_value(row).map_err(|e| Error::Database(e.to_string()))?);
            }
            sqlx::query("UPDATE device_control_commands SET state='delivered' WHERE id=ANY($1) AND state='queued'").bind(frame.commands.iter().map(|x|x.id).collect::<Vec<_>>()).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(frame)
    }
    async fn presence(&self, actor: Uuid, device: Uuid) -> Result<Value> {
        let mut tx = self.0.begin().await?;
        admin(&mut tx, actor).await?;
        let mut presence:Value=sqlx::query_scalar("SELECT jsonb_build_object('id',d.id,'state',CASE WHEN NOT d.enabled THEN 'disabled' WHEN NOT EXISTS(SELECT 1 FROM device_credentials c WHERE c.device_id=d.id AND c.generation=d.credential_generation AND c.revoked_at IS NULL AND c.expires_at>clock_timestamp()) THEN 'unauthorized' WHEN s.device_id IS NULL OR NOT s.connected OR s.last_heartbeat_at<=clock_timestamp()-interval '65 seconds' THEN 'offline' WHEN s.last_heartbeat_at<=clock_timestamp()-interval '30 seconds' THEN 'suspect' ELSE 'online' END,'last_control_heartbeat_at',s.last_heartbeat_at,'transport',s.transport,'readiness',s.readiness,'block_reason',s.block_reason,'desired_paused',coalesce(s.desired_paused,false),'desired_revision',coalesce(s.desired_revision,0),'credential_expires_at',(SELECT max(expires_at) FROM device_credentials WHERE device_id=d.id AND generation=d.credential_generation AND revoked_at IS NULL),'commands','[]'::jsonb) FROM execution_devices d LEFT JOIN device_control_sessions s ON s.device_id=d.id WHERE d.id=$1")
   .bind(device).fetch_optional(&mut *tx).await?.ok_or(Error::NotFound)?;
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
        let paused = match input.kind {
            CommandKind::PauseExecution => Some(true),
            CommandKind::ResumeExecution => Some(false),
            _ => None,
        };
        let revision: Option<i64> = if let Some(paused) = paused {
            Some(sqlx::query_scalar("INSERT INTO device_control_sessions(device_id,connection_id,transport,connected,desired_paused,desired_revision) VALUES($1,$2,'https',false,$3,1) ON CONFLICT(device_id) DO UPDATE SET desired_paused=$3,desired_revision=device_control_sessions.desired_revision+1 RETURNING desired_revision").bind(device).bind(Uuid::new_v4()).bind(paused).fetch_one(&mut *tx).await?)
        } else {
            None
        };
        let kind = serde_json::to_value(input.kind).map_err(|e| Error::Database(e.to_string()))?;
        let id = Uuid::new_v4();
        sqlx::query("INSERT INTO device_control_commands(id,device_id,kind,desired_revision,request_key,request_digest,actor_id) VALUES($1,$2,$3,$4,$5,$6,$7)")
   .bind(id).bind(device).bind(kind.as_str().ok_or(Error::Conflict)?).bind(revision).bind(key).bind(digest).bind(actor).execute(&mut *tx).await?;
        let result = value_command(&mut tx, id).await?;
        tx.commit().await?;
        Ok(result)
    }
}
