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
mod delegation;
pub mod delivery;
mod device;
mod diagnostics;
mod dispatch;
mod execution;
mod execution_status;
mod inbox;
mod inspections;
mod platform;
mod readiness;
mod screen;
mod secrets;
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
    let old:Option<(String,Value)>=sqlx::query_as("SELECT request_hash,response FROM mobile_v9_operations WHERE principal_id=$1 AND generation=$2 AND scope=$3 AND idempotency_key=$4").bind(principal).bind(generation).bind(scope).bind(key).fetch_optional(&mut **tx).await?;
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
    json!({"id":null,"revision":0,"mode":"enabled","limits":{"global":limits,"school":limits,"device":limits,"target":limits},"timezone":"Asia/Shanghai","allowed_windows":[{"weekdays":[1,2,3,4,5,6,7],"start_minute":0,"end_minute":1440}],"confirmation_seconds":86400,"inspection_seconds":3600,"retry_seconds":300,"retry_limit":3})
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
async fn task(tx: &mut Tx<'_>, s: Uuid, id: Uuid) -> Result<Value> {
    let mut value: Value = sqlx::query_scalar("SELECT to_jsonb(t)||jsonb_build_object('items',coalesce((SELECT jsonb_agg(to_jsonb(i)||jsonb_build_object('attempts',(SELECT jsonb_agg(to_jsonb(a)||jsonb_build_object('plan',p.plan,'plan_digest',p.digest,'result_reason',coalesce((SELECT g.result_body->>'reason' FROM mobile_execution_grants g WHERE g.attempt_id=a.id),(SELECT e.body->>'reason' FROM mobile_v9_send_events e WHERE e.attempt_id=a.id AND e.receipt->>'disposition'<>'archived' ORDER BY e.created_at DESC,e.id DESC LIMIT 1))) ORDER BY a.attempt_no) FROM mobile_v9_attempts a JOIN mobile_v9_plans p ON p.id=a.plan_id WHERE a.item_id=i.id)) ORDER BY i.position) FROM mobile_v9_task_items i WHERE i.task_id=t.id),'[]'::jsonb)) FROM mobile_v9_tasks t WHERE t.school_id=$1 AND t.id=$2").bind(s).bind(id).fetch_optional(&mut **tx).await?.ok_or(Error::NotFound)?;
    let receipts:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('attempt_id',r.attempt_id,'received_at',r.received_at) FROM mobile_v9_task_receipts r JOIN mobile_v9_attempts a ON a.id=r.attempt_id JOIN mobile_v9_task_items i ON i.id=a.item_id WHERE i.task_id=$1").bind(id).fetch_all(&mut **tx).await?;
    if let Some(items) = value["items"].as_array_mut() {
        for item in items {
            if let Some(attempts) = item["attempts"].as_array_mut() {
                for attempt in attempts {
                    attempt["device_received_at"] = receipts
                        .iter()
                        .find(|r| r["attempt_id"] == attempt["id"])
                        .map(|r| r["received_at"].clone())
                        .unwrap_or(Value::Null);
                }
            }
        }
    }
    let dispatches:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('item_id',ds.item_id,'state',ds.state,'reason',ds.reason,'revision',ds.revision,'max_rounds',ds.max_rounds,'completed_rounds',ds.completed_rounds,'finished_at',ds.finished_at,'next_retry_at',ds.next_retry_at,'updated_at',ds.updated_at,'last_failed_attempt_id',ds.last_failed_attempt_id,'candidates',sc.candidates,'assignments',coalesce((SELECT jsonb_agg(to_jsonb(r)||jsonb_build_object('from_device_name',op.plan->>'device_name','to_device_name',np.plan->>'device_name') ORDER BY r.created_at,r.id) FROM mobile_v9_reassignments r JOIN mobile_v9_attempts a ON a.id=r.new_attempt_id JOIN mobile_v9_plans np ON np.id=a.plan_id JOIN mobile_v9_attempts oa ON oa.id=r.old_attempt_id JOIN mobile_v9_plans op ON op.id=oa.plan_id WHERE a.item_id=ds.item_id),'[]'::jsonb)) FROM mobile_v9_dispatch_state ds JOIN mobile_v9_dispatch_scopes sc ON sc.item_id=ds.item_id JOIN mobile_v9_task_items i ON i.id=ds.item_id WHERE i.task_id=$1").bind(id).fetch_all(&mut **tx).await?;
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
    execution_status::task(tx, s, &mut value).await?;
    Ok(value)
}
impl Store for Repository {
    async fn complete_execution(
        &self,
        v: crate::distribution::domain::shared_mobile::execution::Completion,
    ) -> Result<Value> {
        let mut tx = self.0.begin().await?;
        guard(&mut tx).await?;
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

    async fn usage_read(
        &self,
        a: &Actor,
        s: Option<Uuid>,
        id: Option<Uuid>,
        get_options: bool,
    ) -> Result<Value> {
        Repository::usage_read(self, a, s, id, get_options).await
    }
    async fn usage_write(
        &self,
        a: &Actor,
        s: Uuid,
        key: &str,
        command: crate::distribution::domain::shared_mobile::usage::UsageCommand,
    ) -> Result<Value> {
        Repository::usage_write(self, a, s, key, command).await
    }
    async fn artifact_content(&self, a: &Actor, id: Uuid) -> Result<Vec<u8>> {
        let mut tx = self.0.begin().await?;
        authorize(&mut tx, a.id, None, true).await?;
        let data = sqlx::query_scalar("SELECT content FROM mobile_v9_artifacts WHERE id=$1")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(Error::NotFound)?;
        tx.commit().await?;
        Ok(data)
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
            SchoolCommand::Support(v) => (format!("{s}:support"), json!(v)),
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
        authorize(&mut tx, a.id, Some(s), false).await?;
        let v = if let Some(id) = id {
            task(&mut tx, s, id).await?
        } else {
            let tasks:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(t) FROM mobile_v9_tasks t WHERE school_id=$1 ORDER BY created_at DESC,id DESC LIMIT 100").bind(s).fetch_all(&mut *tx).await?;
            let drafts:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(d)||jsonb_build_object('targets',coalesce((SELECT jsonb_agg(jsonb_build_object('id',t.id,'name',t.search_text) ORDER BY ids.n) FROM unnest(d.target_ids) WITH ORDINALITY ids(id,n) JOIN mobile_targets t ON t.id=ids.id AND t.school_id=d.school_id),'[]'::jsonb)) FROM mobile_v9_task_drafts d WHERE school_id=$1 AND NOT EXISTS(SELECT 1 FROM mobile_v9_tasks t WHERE t.draft_id=d.id) ORDER BY created_at DESC LIMIT 100").bind(s).fetch_all(&mut *tx).await?;
            let inspections:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(i)-'challenge' FROM mobile_v9_send_inspections i WHERE school_id=$1 ORDER BY id DESC LIMIT 100").bind(s).fetch_all(&mut *tx).await?;
            json!({"id":s,"tasks":tasks,"drafts":drafts,"inspections":inspections,"readiness":readiness::school(&mut tx,s).await?,"pool":readiness::pool(&mut tx,s).await?,"sending_status":business_scheme::status(&mut tx).await?})
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
            PlatformCommand::Manage(v) => ("management", json!(v)),
            PlatformCommand::Artifact(v) => (
                "artifact",
                json!({"kind":v.kind,"name":v.name,"digest":hash_bytes(&v.content)}),
            ),
            PlatformCommand::Access(v) => ("school-access", json!(v)),
        };
        if let Some(v) = replay(&mut tx, a.id, 0, scope, key, &body).await? {
            return Ok(v);
        }
        let v = match command {
            PlatformCommand::Manage(v) => platform::write(&mut tx, a, v).await?,
            PlatformCommand::Artifact(v) => platform::artifact(&mut tx, a, v).await?,
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
                json!({"id":v.device_id,"school_id":v.school_id,"revision":revision,"enabled":v.enabled})
            }
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
        authorize(&mut tx, a.id, None, true).await?;
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
        v["schools"] = json!(sqlx::query_scalar::<_, Value>("SELECT jsonb_build_object('id',id,'name',name) FROM schools WHERE active ORDER BY name,id").fetch_all(&mut *tx).await?);
        v["targets"] = json!(sqlx::query_scalar::<_, Value>("SELECT to_jsonb(t)||jsonb_build_object('name',search_text) FROM mobile_targets t WHERE archived_at IS NULL ORDER BY school_id,id").fetch_all(&mut *tx).await?);
        v["access"] = json!(sqlx::query_scalar::<_, Value>("SELECT to_jsonb(a)||jsonb_build_object('id',a.device_id::text||':'||a.school_id::text,'device_name',d.name,'school_name',s.name) FROM mobile_shared_school_access a JOIN execution_devices d ON d.id=a.device_id JOIN schools s ON s.id=a.school_id ORDER BY s.name,d.name").fetch_all(&mut *tx).await?);
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
    async fn delegation_read(&self, c: &Credential) -> Result<Value> {
        let mut tx = self.0.begin().await?;
        authenticate(&mut tx, c, false).await?;
        let delegation = delegation::context(&mut tx, c).await?;
        let value = delegation_context_view(c, delegation, chrono::Utc::now());
        tx.commit().await?;
        Ok(value)
    }
    async fn device_read(&self, c: &Credential) -> Result<Value> {
        execution::context(self, c).await
    }
}

fn delegation_context_view(
    c: &Credential,
    delegation: Value,
    observed_at: chrono::DateTime<chrono::Utc>,
) -> Value {
    json!({"id":c.device_id,"device_id":c.device_id,"credential_generation":c.generation,"authorization_schema":SCOPE,"delegation":delegation,"observed_at":observed_at,"runtime_report_version":2})
}

fn hash_bytes(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod default_policy_tests {
    use super::*;
    #[test]
    fn delegation_context_advertises_additive_runtime_diagnostics_without_changing_consent() {
        let fixture: Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../api-contract/fixtures/shared-runtime-negotiation.json"
        )))
        .unwrap();
        let credential = Credential {
            device_id: serde_json::from_value(fixture["device_id"].clone()).unwrap(),
            generation: 1,
            token_hash: "not-exposed".into(),
        };
        let value = delegation_context_view(
            &credential,
            fixture["delegation"].clone(),
            serde_json::from_value(fixture["observed_at"].clone()).unwrap(),
        );
        assert_eq!(value, fixture);
        assert!(value.get("token_hash").is_none());
    }
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
