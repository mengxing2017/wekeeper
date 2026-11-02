//! All shared writes serialize with field checks. No UI/network work occurs in a transaction.
use crate::{
    business::{Actor, Error, Result, hash},
    distribution::{
        domain::{device_control::Credential, mobile_execution::shared_send::*},
        south::port::shared_send::*,
    },
};
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;
type Tx<'a> = Transaction<'a, Postgres>;
#[derive(Clone)]
pub struct Repository(pub PgPool);
mod delegation;
mod device;
mod inspections;
mod readiness;
mod secrets;
mod tasks;
async fn expire(tx: &mut Tx<'_>) -> Result<()> {
    sqlx::query("UPDATE mobile_shared_attempts a SET state='failed_before_call',revision=revision+1 FROM mobile_shared_send_claims c WHERE a.id=c.attempt_id AND a.state IN ('leased','prepared') AND c.expires_at<clock_timestamp() AND NOT EXISTS(SELECT 1 FROM mobile_shared_invocations i WHERE i.claim_id=c.id)").execute(&mut **tx).await?;
    sqlx::query("UPDATE mobile_shared_quota_reservations q SET state='released' FROM mobile_shared_attempts a WHERE q.attempt_id=a.id AND q.state='reserved' AND a.state='failed_before_call'").execute(&mut **tx).await?;

    sqlx::query("UPDATE mobile_shared_attempts a SET state='unknown',revision=revision+1 FROM mobile_shared_invocations i JOIN mobile_shared_send_claims c ON c.id=i.claim_id LEFT JOIN mobile_shared_runtime r ON r.device_id=c.device_id WHERE a.id=i.attempt_id AND a.state='invocation_issued' AND i.state='risk_held' AND i.execute_before<clock_timestamp() AND (r.device_id IS NULL OR r.runtime_epoch<>c.runtime_epoch OR r.quiescent OR r.observed_at<clock_timestamp()-interval '90 seconds')").execute(&mut **tx).await?;
    Ok(())
}
async fn guard(tx: &mut Tx<'_>) -> Result<()> {
    sqlx::query("SELECT pg_advisory_xact_lock(78234610243)")
        .execute(&mut **tx)
        .await?;
    Ok(())
}
async fn authorize(tx: &mut Tx<'_>, actor: Uuid, school: Option<Uuid>, admin: bool) -> Result<()> {
    if let Some(s) = school {
        sqlx::query("SELECT a.id FROM accounts a JOIN school_memberships m ON m.account_id=a.id JOIN schools s ON s.id=m.school_id WHERE a.id=$1 AND a.active AND s.id=$2 AND s.active AND (m.role='admin' OR (NOT $3 AND m.role='operator')) FOR SHARE OF a,m,s").bind(actor).bind(s).bind(admin).fetch_optional(&mut **tx).await?.ok_or(Error::Forbidden)?;
    } else {
        sqlx::query("SELECT id FROM accounts WHERE id=$1 AND active AND platform_admin FOR SHARE")
            .bind(actor)
            .fetch_optional(&mut **tx)
            .await?
            .ok_or(Error::Forbidden)?;
    }
    Ok(())
}
async fn authenticate(tx: &mut Tx<'_>, c: &Credential, control: bool) -> Result<()> {
    sqlx::query("SELECT c.device_id FROM device_credentials c JOIN execution_devices d ON d.id=c.device_id WHERE c.token_hash=$1 AND c.device_id=$2 AND c.generation=$3 AND c.revoked_at IS NULL AND c.expires_at>clock_timestamp() AND (NOT $4 OR d.enabled) AND d.credential_generation=c.generation FOR SHARE OF c,d").bind(&c.token_hash).bind(c.device_id).bind(c.generation).bind(control).fetch_optional(&mut **tx).await?.ok_or(Error::Unauthorized)?;
    Ok(())
}
async fn replay(
    tx: &mut Tx<'_>,
    principal: Uuid,
    generation: i64,
    scope: &str,
    key: &str,
    body: &Value,
) -> Result<Option<Value>> {
    let old:Option<(String,Value)>=sqlx::query_as("SELECT request_hash,response FROM mobile_shared_operations WHERE principal_id=$1 AND generation=$2 AND scope=$3 AND idempotency_key=$4").bind(principal).bind(generation).bind(scope).bind(key).fetch_optional(&mut **tx).await?;
    if let Some((h, v)) = old {
        require(h == digest(body)?, "idempotency_payload_conflict")?;
        Ok(Some(v))
    } else {
        Ok(None)
    }
}
async fn remember(
    tx: &mut Tx<'_>,
    principal: Uuid,
    generation: i64,
    scope: &str,
    key: &str,
    body: &Value,
    result: &Value,
) -> Result<()> {
    sqlx::query("INSERT INTO mobile_shared_operations(principal_id,generation,scope,idempotency_key,request_hash,response) VALUES($1,$2,$3,$4,$5,$6)").bind(principal).bind(generation).bind(scope).bind(key).bind(digest(body)?).bind(result).execute(&mut **tx).await?;
    Ok(())
}
async fn policy(tx: &mut Tx<'_>) -> Result<Option<Value>> {
    Ok(sqlx::query_scalar("SELECT to_jsonb(p) FROM mobile_shared_send_policy_versions p ORDER BY revision DESC LIMIT 1").fetch_optional(&mut **tx).await?)
}
fn uid(v: &Value, k: &str) -> Result<Uuid> {
    serde_json::from_value(v[k].clone()).map_err(|_| Error::Conflict)
}
async fn task(tx: &mut Tx<'_>, s: Uuid, id: Uuid) -> Result<Value> {
    sqlx::query_scalar("SELECT to_jsonb(t)||jsonb_build_object('items',coalesce((SELECT jsonb_agg(to_jsonb(i)||jsonb_build_object('attempts',(SELECT jsonb_agg(to_jsonb(a)||jsonb_build_object('plan',p.plan,'plan_digest',p.digest) ORDER BY a.attempt_no) FROM mobile_shared_attempts a JOIN mobile_shared_plans p ON p.id=a.plan_id WHERE a.item_id=i.id)) ORDER BY i.position) FROM mobile_shared_task_items i WHERE i.task_id=t.id),'[]'::jsonb)) FROM mobile_shared_tasks t WHERE t.school_id=$1 AND t.id=$2").bind(s).bind(id).fetch_optional(&mut **tx).await?.ok_or(Error::NotFound)
}
impl Store for Repository {
    async fn recover_claim(&self, event: ClaimRecovery) -> Result<Value> {
        require(
            event.recovery_credential.len() == 64,
            "result_identity_mismatch",
        )?;
        let mut tx = self.0.begin().await?;
        guard(&mut tx).await?;
        let row:Value=sqlx::query_scalar("SELECT jsonb_build_object('id',c.id,'school_id',c.school_id,'attempt_id',c.attempt_id,'device_id',c.device_id) FROM mobile_shared_send_claims c WHERE c.id=$1 AND c.recovery_credential_hash=$2").bind(event.claim_id).bind(hash(&event.recovery_credential)).fetch_optional(&mut *tx).await?.ok_or(Error::Unauthorized)?;
        let encrypted: Option<Vec<u8>> = sqlx::query_scalar(
            "SELECT encrypted_response FROM mobile_shared_invocations WHERE claim_id=$1",
        )
        .bind(event.claim_id)
        .fetch_optional(&mut *tx)
        .await?;
        let permit = encrypted.as_deref().map(secrets::open).transpose()?;
        sqlx::query("UPDATE mobile_shared_send_claims SET state='closed' WHERE id=$1")
            .bind(event.claim_id)
            .execute(&mut *tx)
            .await?;
        if permit.is_some() {
            sqlx::query("UPDATE mobile_shared_attempts SET state='unknown',revision=revision+1 WHERE id=$1 AND state='invocation_issued'").bind(uid(&row,"attempt_id")?).execute(&mut *tx).await?;
        } else {
            sqlx::query("UPDATE mobile_shared_attempts SET state='failed_before_call',revision=revision+1 WHERE id=$1 AND state IN ('leased','prepared')").bind(uid(&row,"attempt_id")?).execute(&mut *tx).await?;
            sqlx::query("UPDATE mobile_shared_quota_reservations SET state='released' WHERE attempt_id=$1 AND state='reserved'").bind(uid(&row,"attempt_id")?).execute(&mut *tx).await?;
        }
        // Recovery can inhibit the original owner, never create/renew control or issue a call.
        tx.commit().await?;
        Ok(
            json!({"id":event.claim_id,"attempt_id":row["attempt_id"],"state":if permit.is_some(){"unknown"}else{"failed_before_call"},"permit":permit}),
        )
    }

    async fn recover_result(&self, key: &str, event: ResultEvent) -> Result<Value> {
        event.validate()?;
        let secret = event
            .result_credential
            .as_deref()
            .ok_or(Error::Unauthorized)?;
        require(secret.len() == 64, "result_identity_mismatch")?;
        let mut tx = self.0.begin().await?;
        guard(&mut tx).await?;
        let (device_id,generation):(Uuid,i64)=sqlx::query_as("SELECT device_id,credential_generation FROM mobile_shared_invocations WHERE id=$1 AND result_credential_hash=$2").bind(event.invocation_id.ok_or(Error::Unauthorized)?).bind(hash(secret)).fetch_optional(&mut *tx).await?.ok_or(Error::Unauthorized)?;
        let c = Credential {
            device_id,
            generation,
            token_hash: String::new(),
        };
        let response = device::result(&mut tx, &c, event).await?;
        let _ = key;
        tx.commit().await?;
        Ok(response)
    }

    async fn school_write(
        &self,
        a: &Actor,
        s: Uuid,
        key: &str,
        command: SchoolCommand,
    ) -> Result<Value> {
        let mut tx = self.0.begin().await?;
        guard(&mut tx).await?;
        authorize(
            &mut tx,
            a.id,
            Some(s),
            !matches!(
                &command,
                SchoolCommand::Draft(..) | SchoolCommand::Preview(_)
            ),
        )
        .await?;
        let (scope, body) = match &command {
            SchoolCommand::PoolBinding(v) => (format!("{s}:pool-binding"), json!(v)),
            SchoolCommand::Unbind(id, v) => (format!("{s}:unbind:{id}"), json!(v)),
            SchoolCommand::Draft(id, v) => (format!("{s}:draft:{id:?}"), json!(v)),
            SchoolCommand::Inspection(v) => (format!("{s}:inspection"), json!(v)),
            SchoolCommand::Preview(v) => (format!("{s}:preview"), json!(v)),
            SchoolCommand::Confirm(v) => (format!("{s}:confirm"), json!(v)),
            SchoolCommand::Stop(id, v) => (format!("{s}:stop:{id}"), json!(v)),
            SchoolCommand::Reassign(id, v) => (format!("{s}:reassign:{id}"), json!(v)),
            SchoolCommand::Review(id, v) => (format!("{s}:review:{id}"), json!(v)),
        };
        if let Some(v) = replay(&mut tx, a.id, 0, &scope, key, &body).await? {
            return Ok(v);
        }
        let v = tasks::write(&mut tx, a, s, command).await?;
        remember(&mut tx, a.id, 0, &scope, key, &body, &v).await?;
        tx.commit().await?;
        Ok(v)
    }
    async fn school_read(&self, a: &Actor, s: Uuid, id: Option<Uuid>) -> Result<Value> {
        let mut tx = self.0.begin().await?;
        guard(&mut tx).await?;
        authorize(&mut tx, a.id, Some(s), false).await?;
        expire(&mut tx).await?;
        let v = if let Some(id) = id {
            task(&mut tx, s, id).await?
        } else {
            let tasks:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(t) FROM mobile_shared_tasks t WHERE school_id=$1 ORDER BY created_at DESC,id DESC LIMIT 100").bind(s).fetch_all(&mut *tx).await?;
            let drafts:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(d)||jsonb_build_object('targets',coalesce((SELECT jsonb_agg(jsonb_build_object('id',t.id,'name',coalesce(nullif(t.alias,''),t.search_text)) ORDER BY ids.n) FROM unnest(d.target_ids) WITH ORDINALITY ids(id,n) JOIN mobile_targets t ON t.id=ids.id AND t.school_id=d.school_id),'[]'::jsonb)) FROM mobile_shared_task_drafts d WHERE school_id=$1 AND NOT EXISTS(SELECT 1 FROM mobile_shared_tasks t WHERE t.draft_id=d.id) ORDER BY created_at DESC LIMIT 100").bind(s).fetch_all(&mut *tx).await?;
            let inspections:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(i)-'challenge' FROM mobile_shared_send_inspections i WHERE school_id=$1 ORDER BY id DESC LIMIT 100").bind(s).fetch_all(&mut *tx).await?;
            json!({"id":s,"tasks":tasks,"drafts":drafts,"inspections":inspections,"readiness":readiness::school(&mut tx,s).await?,"pool":readiness::pool(&mut tx,s).await?})
        };
        tx.commit().await?;
        Ok(v)
    }
    async fn platform_write(
        &self,
        a: &Actor,
        key: &str,
        command: PlatformCommand,
    ) -> Result<Value> {
        let mut tx = self.0.begin().await?;
        guard(&mut tx).await?;
        authorize(&mut tx, a.id, None, true).await?;
        let (scope, body) = match &command {
            PlatformCommand::Policy(v) => ("policy", json!(v)),
            PlatformCommand::Access(v) => ("school-access", json!(v)),
        };
        if let Some(v) = replay(&mut tx, a.id, 0, scope, key, &body).await? {
            return Ok(v);
        }
        let v = match command {
            PlatformCommand::Access(v) => {
                let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM schools s,execution_devices d WHERE s.id=$1 AND d.id=$2 AND (NOT $3 OR (s.active AND d.enabled)))").bind(v.school_id).bind(v.device_id).bind(v.enabled).fetch_one(&mut *tx).await?;
                require(valid, "shared_access_invalid")?;
                let old:Option<(i64,bool)>=sqlx::query_as("SELECT revision,enabled FROM mobile_shared_school_access WHERE device_id=$1 AND school_id=$2").bind(v.device_id).bind(v.school_id).fetch_optional(&mut *tx).await?;
                require(
                    old.map(|r| r.0).unwrap_or(0) == v.expected_revision,
                    "revision_conflict",
                )?;
                let revision = if old.is_some_and(|r| r.1 == v.enabled) {
                    v.expected_revision
                } else {
                    let next = v.expected_revision + 1;
                    sqlx::query("INSERT INTO mobile_shared_school_access(device_id,school_id,enabled,revision,updated_by) VALUES($1,$2,$3,$4,$5) ON CONFLICT(device_id,school_id) DO UPDATE SET enabled=$3,revision=$4,updated_by=$5").bind(v.device_id).bind(v.school_id).bind(v.enabled).bind(next).bind(a.id).execute(&mut *tx).await?;
                    sqlx::query("INSERT INTO mobile_shared_access_events(id,device_id,school_id,revision,enabled,actor_id) VALUES($1,$2,$3,$4,$5,$6)").bind(Uuid::now_v7()).bind(v.device_id).bind(v.school_id).bind(next).bind(v.enabled).bind(a.id).execute(&mut *tx).await?;
                    next
                };
                inspections::reconcile(&mut tx, Some(v.device_id), None).await?;
                json!({"id":v.device_id,"school_id":v.school_id,"revision":revision,"enabled":v.enabled})
            }
            PlatformCommand::Policy(v) => {
                v.validate()?;
                if matches!(v.mode.as_str(), "test_send" | "production_allowlist") {
                    secrets::seal(&json!({}))?;
                }
                let valid_zone: bool = sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM pg_timezone_names WHERE name=$1)",
                )
                .bind(&v.timezone)
                .fetch_one(&mut *tx)
                .await?;
                require(valid_zone, "shared_send_policy_invalid")?;
                let rev = policy(&mut tx)
                    .await?
                    .and_then(|p| p["revision"].as_i64())
                    .unwrap_or(0);
                require(rev == v.expected_revision, "revision_conflict")?;
                let id = Uuid::now_v7();
                sqlx::query("INSERT INTO mobile_shared_send_policy_versions(id,revision,mode,limits,created_by,reason,timezone,allowed_windows) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(id).bind(rev+1).bind(v.mode).bind(json!(v.limits)).bind(a.id).bind(v.reason).bind(v.timezone).bind(json!(v.allowed_windows)).execute(&mut *tx).await?;
                json!({"id":id,"revision":rev+1})
            }
        };
        remember(&mut tx, a.id, 0, scope, key, &body, &v).await?;
        tx.commit().await?;
        Ok(v)
    }
    async fn platform_read(&self, a: &Actor) -> Result<Value> {
        let mut tx = self.0.begin().await?;
        guard(&mut tx).await?;
        authorize(&mut tx, a.id, None, true).await?;
        expire(&mut tx).await?;
        let releases:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(r)-'flow' FROM mobile_send_adapter_releases r WHERE NOT EXISTS(SELECT 1 FROM mobile_send_adapter_withdrawals w WHERE w.release_id=r.id)").fetch_all(&mut *tx).await?;
        let devices:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('device_id',d.id,'name',d.name,'delegation_accepted',coalesce((SELECT v.accepted FROM mobile_shared_delegations m JOIN mobile_shared_delegation_decisions v ON v.delegation_id=m.id WHERE m.device_id=d.id AND m.credential_generation=d.credential_generation ORDER BY v.revision DESC LIMIT 1),false),'runtime',to_jsonb(rt),'environment_snapshot_id',obs.snapshot_id,'environment_fresh',obs.observed_at>clock_timestamp()-interval '5 minutes','matching_releases',coalesce((SELECT jsonb_agg(jsonb_build_object('id',r.id,'name',r.name,'version',r.version)) FROM mobile_send_adapter_releases r WHERE r.environment=e.snapshot-'instance_ref' AND NOT EXISTS(SELECT 1 FROM mobile_send_adapter_withdrawals w WHERE w.release_id=r.id)),'[]'::jsonb),'risk_count',(SELECT count(*) FROM mobile_shared_invocations i WHERE i.device_id=d.id AND i.state='risk_held'),'acceptance_count',(SELECT count(*) FROM mobile_shared_release_scopes sc WHERE sc.device_id=d.id AND sc.environment_snapshot_id=obs.snapshot_id AND sc.expires_at>clock_timestamp())) FROM execution_devices d LEFT JOIN mobile_shared_runtime rt ON rt.device_id=d.id LEFT JOIN mobile_device_environment_observations obs ON obs.device_id=d.id LEFT JOIN mobile_device_environments e ON e.id=obs.snapshot_id AND e.device_id=d.id WHERE d.enabled ORDER BY d.name,d.id LIMIT 100").fetch_all(&mut *tx).await?;
        let schools:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('id',id,'name',name) FROM schools WHERE active ORDER BY name,id").fetch_all(&mut *tx).await?;
        let access:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(a)||jsonb_build_object('id',a.device_id::text||':'||a.school_id::text,'device_name',d.name,'school_name',s.name) FROM mobile_shared_school_access a JOIN execution_devices d ON d.id=a.device_id JOIN schools s ON s.id=a.school_id ORDER BY s.name,d.name").fetch_all(&mut *tx).await?;
        let v = json!({"id":"current","policy":policy(&mut tx).await?,"releases":releases,"devices":devices,"schools":schools,"access":access});
        tx.commit().await?;
        Ok(v)
    }
    async fn device_write(
        &self,
        c: &Credential,
        key: &str,
        command: DeviceCommand,
    ) -> Result<Value> {
        device::write(self, c, key, command).await
    }
    async fn device_read(&self, c: &Credential) -> Result<Value> {
        device::read(self, c).await
    }
}

/// Resource mutation callers already hold the shared transaction guard.
pub(super) async fn reconcile_resources(tx: &mut Transaction<'_, Postgres>) -> Result<()> {
    inspections::reconcile(tx, None, None).await
}
