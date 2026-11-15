//! Current phone resources: no queries into official customer groups or member identities.
use crate::{
    business::{Actor, Error, Page, Result, hash},
    distribution::{
        domain::shared_mobile::*,
        south::port::shared_mobile_resources::{Resource, Store},
    },
};
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;
#[derive(Clone)]
pub struct Repository(pub PgPool);
type Tx<'a> = Transaction<'a, Postgres>;
async fn authorize(tx: &mut Tx<'_>, actor: Uuid, _admin: bool) -> Result<()> {
    crate::distribution::south::adapter::postgres::authorization::authorize(tx, actor).await
}

async fn object(tx: &mut Tx<'_>, table: &str, id: Uuid) -> Result<Value> {
    sqlx::query_scalar(&format!("SELECT to_jsonb(t) FROM {table} t WHERE id=$1"))
        .bind(id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(Error::NotFound)
}
impl Store for Repository {
    async fn target(&self, actor: &Actor, id: Uuid) -> Result<Value> {
        let mut tx = self.0.begin().await?;
        authorize(&mut tx, actor.id, false).await?;
        let result = sqlx::query_scalar(
            "SELECT to_jsonb(t) FROM mobile_targets t WHERE id=$1 AND archived_at IS NULL",
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(Error::NotFound)?;
        tx.commit().await?;
        Ok(result)
    }

    async fn list(
        &self,
        actor: &Actor,
        resource: Resource,
        page: Page,
        search: String,
        device: Option<Uuid>,
    ) -> Result<Vec<Value>> {
        let mut tx = self.0.begin().await?;
        authorize(&mut tx, actor.id, false).await?;
        let (limit, offset) = page.fetch_bounds()?;
        let sql = match resource {
            Resource::Targets | Resource::PlatformTargets => {
                "SELECT to_jsonb(t) FROM mobile_targets t WHERE t.archived_at IS NULL AND strpos(lower(t.search_text),lower($1))>0 AND ($2::uuid IS NULL OR EXISTS(SELECT 1 FROM mobile_device_target_bindings b WHERE b.target_id=t.id AND b.device_id=$2 AND b.enabled)) ORDER BY t.created_at,t.id LIMIT $3 OFFSET $4"
            }
            Resource::Devices | Resource::PlatformDevices => {
                "SELECT jsonb_build_object('id',d.id,'name',d.name,'enabled',d.enabled,'last_seen_at',d.last_seen_at) FROM execution_devices d WHERE strpos(lower(d.name),lower($1))>0 AND ($2::uuid IS NULL OR d.id=$2) ORDER BY d.created_at,d.id LIMIT $3 OFFSET $4"
            }
            Resource::Bindings | Resource::PlatformBindings => {
                "SELECT to_jsonb(b)||jsonb_build_object('device_name',d.name,'search_text',t.search_text,'target_locator_revision',t.locator_revision,'available',b.enabled AND d.enabled AND t.enabled AND t.archived_at IS NULL) FROM mobile_device_target_bindings b JOIN execution_devices d ON d.id=b.device_id JOIN mobile_targets t ON t.id=b.target_id WHERE (strpos(lower(t.search_text),lower($1))>0 OR strpos(lower(d.name),lower($1))>0) AND ($2::uuid IS NULL OR b.device_id=$2) ORDER BY b.created_at,b.id LIMIT $3 OFFSET $4"
            }
        };
        let rows = sqlx::query_scalar(sql)
            .bind(search)
            .bind(device)
            .bind(limit)
            .bind(offset)
            .fetch_all(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(rows)
    }
    async fn write(
        &self,
        actor: &Actor,
        action: &str,
        id: Option<Uuid>,
        key: &str,
        input: Value,
    ) -> Result<Value> {
        let mut tx = self.0.begin().await?;
        // All current resource changes share one channel-local guard, including idempotency.
        sqlx::query("SELECT pg_advisory_xact_lock(78234610243)")
            .execute(&mut *tx)
            .await?;
        authorize(&mut tx, actor.id, true).await?;
        let scope = format!("{action}:{}", id.map(|s| s.to_string()).unwrap_or_default());
        let digest = hash(&input.to_string());
        let previous:Option<(String,Value)>=sqlx::query_as("SELECT payload_hash,result FROM shared_mobile_resource_operations WHERE actor_id=$1 AND scope=$2 AND idempotency_key=$3")
            .bind(actor.id).bind(&scope).bind(key).fetch_optional(&mut *tx).await?;
        if let Some((old, result)) = previous {
            return if old == digest {
                Ok(result)
            } else {
                Err(Error::Conflict)
            };
        }
        let resource = id.unwrap_or_else(Uuid::now_v7);
        let result = match action {
            "target" => {
                let v: TargetInput = serde_json::from_value(input.clone())
                    .map_err(|_| Error::Invalid("目标字段无效".into()))?;
                v.validate()?;
                let candidates:Vec<(Uuid,i64,i64)>=sqlx::query_as("SELECT id,metadata_revision,locator_revision FROM mobile_targets WHERE search_text=$1 AND archived_at IS NULL ORDER BY id").bind(&v.search_text).fetch_all(&mut *tx).await?;
                if !candidates.is_empty() {
                    // Confirmation binds exact input and duplicate candidates, never inferred identity.
                    let confirmation =
                        hash(&json!([v.search_text, v.remark, candidates]).to_string());
                    if v.duplicate_confirmation.as_deref() != Some(&confirmation) {
                        let response = json!({"id":resource,"confirmation_required":true,"duplicate_confirmation":confirmation});
                        sqlx::query("INSERT INTO shared_mobile_resource_operations(actor_id,scope,idempotency_key,payload_hash,result) VALUES($1,$2,$3,$4,$5)")
                            .bind(actor.id).bind(&scope).bind(key).bind(&digest).bind(&response).execute(&mut *tx).await?;
                        tx.commit().await?;
                        return Ok(response);
                    }
                }
                sqlx::query("INSERT INTO mobile_targets(id,search_text,remark,created_by,updated_by) VALUES($1,$2,$3,$4,$4)")
                    .bind(resource).bind(&v.search_text).bind(&v.remark).bind(actor.id).execute(&mut *tx).await?;
                object(&mut tx, "mobile_targets", resource).await?
            }
            "update-target" => {
                let v: TargetUpdate = serde_json::from_value(input.clone())
                    .map_err(|_| Error::Invalid("目标字段无效".into()))?;
                validate_text(&v.search_text, 1, 256, 1024)?;
                validate_text(&v.remark, 0, 2000, 8000)?;
                let n=sqlx::query("UPDATE mobile_targets SET search_text=$3,remark=$4,enabled=$5,metadata_revision=metadata_revision+1,locator_revision=locator_revision+CASE WHEN search_text<>$3 THEN 1 ELSE 0 END,updated_by=$6,updated_at=clock_timestamp() WHERE id=$1 AND metadata_revision=$2 AND archived_at IS NULL").bind(resource).bind(v.expected_revision).bind(v.search_text).bind(v.remark).bind(v.enabled).bind(actor.id).execute(&mut *tx).await?.rows_affected();
                if n != 1 {
                    return Err(Error::Policy("revision_conflict"));
                }
                object(&mut tx, "mobile_targets", resource).await?
            }
            "archive-target" => {
                let v: RevisionInput = serde_json::from_value(input.clone())
                    .map_err(|_| Error::Invalid("归档字段无效".into()))?;
                validate_text(&v.reason, 1, 2000, 8000)?;
                let active:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_v9_task_items i JOIN mobile_v9_attempts a ON a.item_id=i.id WHERE i.target_id=$1 AND a.current AND a.state IN ('queued','leased','prepared','invocation_issued','unknown'))").bind(resource).fetch_one(&mut *tx).await?;
                if active {
                    return Err(Error::Policy("mobile_target_unresolved"));
                }
                let n=sqlx::query("UPDATE mobile_targets SET enabled=false,archived_at=clock_timestamp(),metadata_revision=metadata_revision+1,locator_revision=locator_revision+1,updated_by=$3 WHERE id=$1 AND metadata_revision=$2 AND archived_at IS NULL").bind(resource).bind(v.expected_revision).bind(actor.id).execute(&mut *tx).await?.rows_affected();
                if n != 1 {
                    return Err(Error::Policy("revision_conflict"));
                }
                object(&mut tx, "mobile_targets", resource).await?
            }
            "bind" => {
                let v: SharedBindingInput = serde_json::from_value(input.clone())
                    .map_err(|_| Error::Invalid("绑定字段无效".into()))?;
                v.validate()?;
                sqlx::query("SELECT id FROM mobile_targets WHERE id=$1 AND enabled AND archived_at IS NULL FOR SHARE")
                    .bind(v.target_id).fetch_optional(&mut *tx).await?.ok_or(Error::NotFound)?;
                let mut bindings = Vec::new();
                for device in &v.device_ids {
                    sqlx::query(
                        "SELECT id FROM execution_devices WHERE id=$1 AND enabled FOR SHARE",
                    )
                    .bind(device)
                    .fetch_optional(&mut *tx)
                    .await?
                    .ok_or(Error::NotFound)?;
                    let binding: Uuid = sqlx::query_scalar("INSERT INTO mobile_device_target_bindings(id,device_id,target_id,created_by,updated_by) VALUES($1,$2,$3,$4,$4) ON CONFLICT(device_id,target_id) DO UPDATE SET enabled=true,revision=mobile_device_target_bindings.revision+CASE WHEN NOT mobile_device_target_bindings.enabled THEN 1 ELSE 0 END,locator_revision=mobile_device_target_bindings.locator_revision+CASE WHEN NOT mobile_device_target_bindings.enabled THEN 1 ELSE 0 END,updated_by=$4,updated_at=clock_timestamp() RETURNING id")
                        .bind(Uuid::now_v7()).bind(device).bind(v.target_id).bind(actor.id).fetch_one(&mut *tx).await?;
                    bindings.push(object(&mut tx, "mobile_device_target_bindings", binding).await?);
                }
                json!({"id":resource,"target_id":v.target_id,"bindings":bindings})
            }
            "update-binding" => {
                let v: SharedBindingUpdate = serde_json::from_value(input.clone())
                    .map_err(|_| Error::Invalid("绑定字段无效".into()))?;
                let n=sqlx::query("UPDATE mobile_device_target_bindings b SET enabled=$3,revision=revision+CASE WHEN enabled<>$3 THEN 1 ELSE 0 END,locator_revision=locator_revision+CASE WHEN enabled<>$3 THEN 1 ELSE 0 END,updated_by=$4,updated_at=clock_timestamp() WHERE id=$1 AND revision=$2 AND (NOT $3 OR EXISTS(SELECT 1 FROM mobile_targets t  JOIN execution_devices d ON d.id=b.device_id WHERE t.id=b.target_id AND t.enabled AND t.archived_at IS NULL AND d.enabled))")
                    .bind(resource).bind(v.expected_revision).bind(v.enabled).bind(actor.id).execute(&mut *tx).await?.rows_affected();
                if n != 1 {
                    return Err(Error::Policy("revision_conflict"));
                }
                object(&mut tx, "mobile_device_target_bindings", resource).await?
            }
            "revoke-binding" => {
                let v: RevisionInput = serde_json::from_value(input.clone())
                    .map_err(|_| Error::Invalid("解绑字段无效".into()))?;
                validate_text(&v.reason, 1, 2000, 8000)?;
                let n=sqlx::query("UPDATE mobile_device_target_bindings SET enabled=false,revision=revision+1,locator_revision=locator_revision+1,updated_by=$3,updated_at=clock_timestamp() WHERE id=$1 AND revision=$2")
                    .bind(resource).bind(v.expected_revision).bind(actor.id).execute(&mut *tx).await?.rows_affected();
                if n != 1 {
                    return Err(Error::Policy("revision_conflict"));
                }
                object(&mut tx, "mobile_device_target_bindings", resource).await?
            }
            _ => return Err(Error::NotFound),
        };
        sqlx::query("INSERT INTO shared_mobile_resource_events(id,resource_id,actor_id,action,detail) VALUES($1,$2,$3,$4,$5)")
            .bind(Uuid::now_v7())
            .bind(result["id"].as_str().and_then(|s|s.parse::<Uuid>().ok()).unwrap_or(resource)).bind(actor.id).bind(action).bind(json!({"input":input,"result":result})).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO shared_mobile_resource_operations(actor_id,scope,idempotency_key,payload_hash,result) VALUES($1,$2,$3,$4,$5)")
            .bind(actor.id).bind(scope).bind(key).bind(digest).bind(&result).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(result)
    }
}
