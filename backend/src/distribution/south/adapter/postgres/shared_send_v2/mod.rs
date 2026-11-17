//! All shared writes serialize with field checks. No UI/network work occurs in a transaction.
use crate::{
    business::{Actor, Error, Result, hash},
    distribution::{
        domain::{device_control::Credential, shared_mobile::shared_send_v2::*},
        south::port::shared_send_v2::*,
    },
};
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;
type Tx<'a> = Transaction<'a, Postgres>;
#[derive(Clone)]
pub struct Repository(pub PgPool);
mod business_scheme;
pub mod configuration;
pub mod delivery;
mod device;
mod diagnostics;
mod dispatch;
pub(crate) mod execution;
mod execution_status;
mod inbox;
mod inspections;
mod platform;
mod push;
mod readiness;
pub mod records;
mod secrets;
mod task_status;
mod tasks;
mod usage;
mod usage_budgets;
mod usage_locators;
mod wait_states;
async fn guard(tx: &mut Tx<'_>) -> Result<()> {
    sqlx::query("SELECT pg_advisory_xact_lock(78234610243)")
        .execute(&mut **tx)
        .await?;
    Ok(())
}
async fn authorize(tx: &mut Tx<'_>, actor: Uuid, _admin: bool) -> Result<()> {
    crate::distribution::south::adapter::postgres::authorization::authorize(tx, actor).await
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
    let old:Option<(String,Value)>=sqlx::query_as("SELECT request_hash,mobile_business_snapshot(response) FROM mobile_v9_operations WHERE principal_id=$1 AND generation=$2 AND scope=$3 AND idempotency_key=$4").bind(principal).bind(generation).bind(scope).bind(key).fetch_optional(&mut **tx).await?;
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
    sqlx::query("INSERT INTO mobile_v9_operations(principal_id,generation,scope,idempotency_key,request_hash,response) VALUES($1,$2,$3,$4,$5,$6)").bind(principal).bind(generation).bind(scope).bind(key).bind(digest(body)?).bind(result).execute(&mut **tx).await?;
    Ok(())
}
async fn policy(tx: &mut Tx<'_>) -> Result<Option<Value>> {
    Ok(sqlx::query_scalar(
        "SELECT to_jsonb(p) FROM mobile_v9_send_policy_versions p ORDER BY revision DESC LIMIT 1",
    )
    .fetch_optional(&mut **tx)
    .await?
    .or_else(|| Some(default_policy())))
}
fn default_policy() -> Value {
    let limits =
        json!({"interval_seconds":30,"window_seconds":3600,"max_calls":30,"daily_max_calls":150});
    json!({"id":null,"revision":0,"mode":"enabled","limits":{"global":limits,"device":limits,"target":limits},"timezone":"Asia/Shanghai","allowed_windows":[{"weekdays":[1,2,3,4,5,6,7],"start_minute":0,"end_minute":1440}],"confirmation_seconds":86400,"inspection_seconds":3600,"retry_seconds":300,"retry_limit":3})
}
fn policy_id(p: &Value) -> Result<Option<Uuid>> {
    p["id"]
        .as_str()
        .map(|v| v.parse().map_err(|_| Error::Conflict))
        .transpose()
}
fn uid(v: &Value, k: &str) -> Result<Uuid> {
    serde_json::from_value(v[k].clone()).map_err(|_| Error::Conflict)
}
async fn task(tx: &mut Tx<'_>, id: Uuid) -> Result<Value> {
    let mut value: Value = sqlx::query_scalar("SELECT to_jsonb(t)||jsonb_build_object('items',coalesce((SELECT jsonb_agg(to_jsonb(i)||jsonb_build_object('attempts',(SELECT jsonb_agg(to_jsonb(a)||jsonb_build_object('plan',mobile_business_snapshot(p.plan),'plan_digest',p.digest,'evidence',(SELECT to_jsonb(e) FROM channel_execution_observations e WHERE e.attempt_id=a.id ORDER BY e.id DESC LIMIT 1),'result_reason',coalesce((SELECT g.result_body->>'reason' FROM mobile_execution_grants g WHERE g.attempt_id=a.id),(SELECT e.body->>'reason' FROM mobile_v9_send_events e WHERE e.attempt_id=a.id AND e.receipt->>'disposition'<>'archived' ORDER BY e.created_at DESC,e.id DESC LIMIT 1))) ORDER BY a.attempt_no) FROM mobile_v9_attempts a JOIN mobile_v9_plans p ON p.id=a.plan_id WHERE a.item_id=i.id)) ORDER BY i.position) FROM mobile_v9_task_items i WHERE i.task_id=t.id),'[]'::jsonb)) FROM mobile_v9_tasks t WHERE t.id=$1").bind(id).fetch_optional(&mut **tx).await?.ok_or(Error::NotFound)?;
    let receipts:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('attempt_id',r.attempt_id,'received_at',r.received_at) FROM mobile_v9_task_receipts r JOIN mobile_v9_attempts a ON a.id=r.attempt_id JOIN mobile_v9_task_items i ON i.id=a.item_id WHERE i.task_id=$1").bind(id).fetch_all(&mut **tx).await?;
    if let Some(items) = value["items"].as_array_mut() {
        for item in items {
            if let Some(attempts) = item["attempts"].as_array_mut() {
                for attempt in attempts {
                    attempt["plan_digest"] = json!(digest(&attempt["plan"])?);
                    attempt["device_received_at"] = receipts
                        .iter()
                        .find(|r| r["attempt_id"] == attempt["id"])
                        .map(|r| r["received_at"].clone())
                        .unwrap_or(Value::Null);
                }
            }
        }
    }
    let dispatches:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('item_id',ds.item_id,'state',ds.state,'reason',ds.reason,'revision',ds.revision,'max_rounds',ds.max_rounds,'completed_rounds',ds.completed_rounds,'finished_at',ds.finished_at,'next_retry_at',ds.next_retry_at,'updated_at',ds.updated_at,'last_failed_attempt_id',ds.last_failed_attempt_id,'candidates',mobile_business_snapshot(sc.candidates),'assignments',coalesce((SELECT jsonb_agg(to_jsonb(r)||jsonb_build_object('from_device_name',op.plan->>'device_name','to_device_name',np.plan->>'device_name') ORDER BY r.created_at,r.id) FROM mobile_v9_reassignments r JOIN mobile_v9_attempts a ON a.id=r.new_attempt_id JOIN mobile_v9_plans np ON np.id=a.plan_id JOIN mobile_v9_attempts oa ON oa.id=r.old_attempt_id JOIN mobile_v9_plans op ON op.id=oa.plan_id WHERE a.item_id=ds.item_id),'[]'::jsonb)) FROM mobile_v9_dispatch_state ds JOIN mobile_v9_dispatch_scopes sc ON sc.item_id=ds.item_id JOIN mobile_v9_task_items i ON i.id=ds.item_id WHERE i.task_id=$1").bind(id).fetch_all(&mut **tx).await?;
    if let Some(items) = value["items"].as_array_mut() {
        for item in items {
            if let Some(dispatch) = dispatches.iter().find(|d| d["item_id"] == item["id"]) {
                item["dispatch"] = serde_json::to_value(
                    serde_json::from_value::<DispatchSnapshot>(dispatch.clone())
                        .map_err(|_| Error::Conflict)?,
                )
                .map_err(|_| Error::Conflict)?;
            }
        }
    }
    execution_status::task(tx, &mut value).await?;
    task_status::attach(tx, std::slice::from_mut(&mut value)).await?;
    Ok(value)
}
impl Store for Repository {
    async fn pending_deliveries(
        &self,
        c: &Credential,
        connection: Uuid,
        epoch: i64,
    ) -> Result<Vec<crate::distribution::domain::shared_mobile::execution::ExecutionDelivery>> {
        Repository::pending_deliveries(self, c, connection, epoch).await
    }
    async fn delivery_written(&self, id: Uuid, completed: bool) -> Result<()> {
        Repository::delivery_written(self, id, completed).await
    }
    async fn inspection_execution(
        &self,
        c: &Credential,
        key: &str,
        v: crate::distribution::domain::shared_mobile::execution::InspectionRequest,
    ) -> Result<Value> {
        device::write(self, c, key, DeviceCommand::Inspection(v)).await
    }

    async fn submit_mobile(
        &self,
        message: Uuid,
        receipt: Uuid,
        plan: crate::distribution::domain::receiving::DeliveryPlan,
        content: String,
    ) -> Result<Uuid> {
        Repository::submit_mobile(self, message, receipt, plan, content).await
    }
    async fn records_read(
        &self,
        actor: &Actor,
        kind: &str,
        id: Option<Uuid>,
        query: crate::distribution::domain::shared_mobile::dispatch_records::RecordQuery,
    ) -> Result<Value> {
        self.read_records(actor, kind, id, query).await
    }

    async fn complete_execution(
        &self,
        v: crate::distribution::domain::shared_mobile::execution::Completion,
    ) -> Result<Value> {
        let mut tx = self.0.begin().await?;
        let value = execution::complete(&mut tx, v).await?;
        tx.commit().await?;
        Ok(value)
    }

    async fn inbox_read(&self, c: &Credential, q: InboxQuery) -> Result<Value> {
        inbox::read(self, c, q).await
    }
    async fn inbox_receipt(&self, c: &Credential, key: &str, v: InboxReceipt) -> Result<Value> {
        inbox::receipt(self, c, key, v).await
    }
    async fn credential(&self, token: &str) -> Result<Credential> {
        if !token.starts_with("wk_device_") || token.len() > 256 {
            return Err(Error::Unauthorized);
        }
        let token_hash = crate::business::hash(token);
        let (device_id,generation):(Uuid,i64)=sqlx::query_as("SELECT c.device_id,c.generation FROM device_credentials c JOIN execution_devices d ON d.id=c.device_id WHERE c.token_hash=$1 AND c.revoked_at IS NULL AND c.expires_at>clock_timestamp() AND d.credential_generation=c.generation").bind(&token_hash).fetch_optional(&self.0).await?.ok_or(Error::Unauthorized)?;
        Ok(Credential {
            device_id,
            generation,
            token_hash,
        })
    }

    async fn usage_read(&self, a: &Actor, id: Option<Uuid>, get_options: bool) -> Result<Value> {
        Repository::usage_read(self, a, id, get_options).await
    }
    async fn usage_write(
        &self,
        a: &Actor,
        key: &str,
        command: crate::distribution::domain::shared_mobile::usage::UsageCommand,
    ) -> Result<Value> {
        Repository::usage_write(self, a, key, command).await
    }
    async fn artifact_content(&self, a: &Actor, id: Uuid) -> Result<Vec<u8>> {
        let mut tx = self.0.begin().await?;
        authorize(&mut tx, a.id, true).await?;
        let data = sqlx::query_scalar("SELECT content FROM mobile_v9_artifacts WHERE id=$1")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(Error::NotFound)?;
        tx.commit().await?;
        Ok(data)
    }
    async fn task_write(&self, a: &Actor, key: &str, command: TaskCommand) -> Result<Value> {
        let mut tx = self.0.begin().await?;
        guard(&mut tx).await?;
        authorize(
            &mut tx,
            a.id,
            !matches!(&command, TaskCommand::Draft(..) | TaskCommand::Preview(_)),
        )
        .await?;
        let (scope, body) = match &command {
            TaskCommand::Support(v) => ("support".to_string(), json!(v)),

            TaskCommand::Draft(id, v) => (format!("draft:{id:?}"), json!(v)),
            TaskCommand::Inspection(v) => ("inspection".to_string(), json!(v)),
            TaskCommand::Preview(v) => ("preview".to_string(), json!(v)),
            TaskCommand::Confirm(v) => ("confirm".to_string(), json!(v)),
            TaskCommand::Stop(id, v) => (format!("stop:{id}"), json!(v)),
            TaskCommand::Reassign(id, v) => (format!("reassign:{id}"), json!(v)),
            TaskCommand::Review(id, v) => (format!("review:{id}"), json!(v)),
        };
        if let Some(v) = replay(&mut tx, a.id, 0, &scope, key, &body).await? {
            return Ok(v);
        }
        let v = tasks::write(&mut tx, a, command).await?;
        remember(&mut tx, a.id, 0, &scope, key, &body, &v).await?;
        tx.commit().await?;
        Ok(v)
    }
    async fn task_read(&self, a: &Actor, id: Option<Uuid>) -> Result<Value> {
        let mut tx = self.0.begin().await?;
        authorize(&mut tx, a.id, false).await?;
        let v = if let Some(id) = id {
            task(&mut tx, id).await?
        } else {
            let mut tasks:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(t) FROM mobile_v9_tasks t  ORDER BY created_at DESC,id DESC LIMIT 100").fetch_all(&mut *tx).await?;
            task_status::attach(&mut tx, &mut tasks).await?;
            let drafts:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(d)||jsonb_build_object('targets',coalesce((SELECT jsonb_agg(jsonb_build_object('id',t.id,'name',t.search_text) ORDER BY ids.n) FROM unnest(d.target_ids) WITH ORDINALITY ids(id,n) JOIN mobile_targets t ON t.id=ids.id ),'[]'::jsonb)) FROM mobile_v9_task_drafts d WHERE NOT EXISTS(SELECT 1 FROM mobile_v9_tasks t WHERE t.draft_id=d.id) ORDER BY created_at DESC LIMIT 100").fetch_all(&mut *tx).await?;
            let inspections:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(i)-'challenge' FROM mobile_v9_send_inspections i  ORDER BY id DESC LIMIT 100").fetch_all(&mut *tx).await?;
            json!({"id":"tasks","tasks":tasks,"drafts":drafts,"inspections":inspections,"readiness":readiness::resources(&mut tx).await?,"pool":readiness::pool(&mut tx).await?,"sending_status":business_scheme::status(&mut tx).await?})
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
        authorize(&mut tx, a.id, true).await?;
        let (scope, body) = match &command {
            PlatformCommand::Policy(v) => ("policy", json!(v)),
            PlatformCommand::Manage(v) => ("management", json!(v)),
            PlatformCommand::Artifact(v) => (
                "artifact",
                json!({"kind":v.kind,"name":v.name,"digest":hash_bytes(&v.content)}),
            ),
        };
        if let Some(v) = replay(&mut tx, a.id, 0, scope, key, &body).await? {
            return Ok(v);
        }
        let v = match command {
            PlatformCommand::Manage(v) => platform::write(&mut tx, a, v).await?,
            PlatformCommand::Artifact(v) => platform::artifact(&mut tx, a, v).await?,
            PlatformCommand::Policy(v) => {
                v.validate()?;
                if matches!(v.mode.as_str(), "enabled") {
                    secrets::seal(&json!({}))?;
                }
                business_scheme::save(&mut tx, a, v).await?
            }
        };
        remember(&mut tx, a.id, 0, scope, key, &body, &v).await?;
        tx.commit().await?;
        Ok(v)
    }
    async fn platform_read(&self, a: &Actor, page: u32) -> Result<Value> {
        let mut tx = self.0.begin().await?;
        authorize(&mut tx, a.id, true).await?;
        let mut v = platform::read(&mut tx, page).await?;
        v["id"] = json!("current");
        v["policy"] = json!(policy(&mut tx).await?);
        v["devices"] = json!(
            sqlx::query_scalar::<_, Value>(
                "SELECT jsonb_build_object('id',d.id,'device_id',d.id,'name',d.name,'enabled',d.enabled) FROM execution_devices d ORDER BY name,id"
            )
            .fetch_all(&mut *tx)
            .await?
        );

        v["targets"] = json!(sqlx::query_scalar::<_, Value>("SELECT to_jsonb(t)||jsonb_build_object('name',search_text) FROM mobile_targets t WHERE archived_at IS NULL ORDER BY id").fetch_all(&mut *tx).await?);

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
        execution::context(self, c).await
    }
}

fn hash_bytes(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod default_policy_tests {
    use super::*;

    #[test]
    fn no_configuration_has_valid_bounded_defaults_without_a_fabricated_database_id() {
        let p = default_policy();
        assert_eq!(policy_id(&p).unwrap(), None);
        let limits: Limits = serde_json::from_value(p["limits"].clone()).unwrap();
        limits.validate().unwrap();
        assert_eq!(p["timezone"], "Asia/Shanghai");
        assert_eq!(
            p["allowed_windows"][0]["weekdays"],
            json!([1, 2, 3, 4, 5, 6, 7])
        );
        assert_eq!(p["mode"], "enabled");
        assert_eq!(p["revision"], 0);
        let mut input = p.clone();
        let o = input.as_object_mut().unwrap();
        o.remove("id");
        o.remove("revision");
        o.insert("expected_revision".into(), json!(0));
        o.insert("reason".into(), json!("系统默认设置"));
        serde_json::from_value::<PolicyInput>(input)
            .unwrap()
            .validate()
            .unwrap();
    }
}

/// Routing freezes only business-authorized candidates, never transient phone readiness.
pub(crate) async fn routing_candidates(tx: &mut Tx<'_>, target: Uuid) -> Result<Vec<Value>> {
    let mut result = Vec::new();
    for candidate in tasks::raw_candidates(tx, target).await? {
        if tasks::scope_control(tx, uid(&candidate, "device_id")?, target).await?
            == tasks::ScopeControl::Authorized
        {
            result.push(candidate);
        }
    }
    Ok(result)
}

pub mod source_routing;

pub(crate) async fn source_guard(tx: &mut Tx<'_>) -> Result<()> {
    sqlx::query("SELECT pg_advisory_xact_lock_shared(78234610243)")
        .execute(&mut **tx)
        .await?;
    Ok(())
}
