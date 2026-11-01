use super::*;
use crate::distribution::domain::{
    device_control::Credential,
    mobile_execution::shared_capability::{self, ConfigurationInput, DecisionInput},
};

pub(super) async fn configuration(
    tx: &mut Tx<'_>,
    actor: Uuid,
    id: Uuid,
    input: &Value,
    creating: bool,
) -> Result<Value> {
    let v: ConfigurationInput = serde_json::from_value(input.clone())
        .map_err(|_| Error::Invalid("设备全局配置字段无效".into()))?;
    v.validate()?;
    sqlx::query("SELECT id FROM execution_devices WHERE id=$1 AND (enabled OR NOT $2) FOR SHARE")
        .bind(v.device_id)
        .bind(v.enabled)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(Error::NotFound)?;
    let flow = serde_json::to_value(&v.flow).map_err(|_| Error::Conflict)?;
    let digest = hash(&flow.to_string());
    if creating {
        if v.expected_revision != 0 {
            return Err(Error::Policy("revision_conflict"));
        }
        let exists: bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_device_capabilities WHERE device_id=$1 AND app_key='wecom' AND capability='group_text')")
            .bind(v.device_id).fetch_one(&mut **tx).await?;
        if exists {
            return Err(Error::Conflict);
        }
        sqlx::query("INSERT INTO mobile_device_capabilities(id,device_id,app_key,capability,name,revision,configuration_digest,enabled,created_by,updated_by) VALUES($1,$2,'wecom','group_text',$3,1,$4,$5,$6,$6)")
            .bind(id).bind(v.device_id).bind(&v.name).bind(&digest).bind(v.enabled).bind(actor).execute(&mut **tx).await?;
    } else {
        let changed=sqlx::query("UPDATE mobile_device_capabilities SET name=$4,enabled=$5,configuration_digest=$6,revision=revision+1,updated_by=$7,updated_at=clock_timestamp() WHERE id=$1 AND device_id=$2 AND revision=$3")
            .bind(id).bind(v.device_id).bind(v.expected_revision).bind(&v.name).bind(v.enabled).bind(&digest).bind(actor).execute(&mut **tx).await?.rows_affected();
        if changed != 1 {
            return Err(Error::Policy("revision_conflict"));
        }
    }
    sqlx::query("INSERT INTO mobile_device_capability_versions(capability_id,revision,name,enabled,flow,configuration_digest,created_by) SELECT id,revision,name,enabled,$2,configuration_digest,$3 FROM mobile_device_capabilities WHERE id=$1")
        .bind(id).bind(&flow).bind(actor).execute(&mut **tx).await?;
    let result = object(tx, "mobile_device_capabilities", id).await?;
    Ok(with_readiness(result))
}
fn with_readiness(mut value: Value) -> Value {
    value["execution_ready"] = json!(false);
    value["execution_block_reason"] = json!("shared_execution_upgrade_required");
    value
}
impl Repository {
    pub(super) async fn capability_context(&self, c: &Credential) -> Result<Vec<Value>> {
        let mut tx = self.0.begin().await?;
        super::consent::authenticate(&mut tx, c).await?;
        let rows: Vec<(Uuid,i64,String,String,Option<Value>)>=sqlx::query_as("SELECT k.id,k.revision,k.configuration_digest,k.name,to_jsonb(c) FROM mobile_device_capabilities k LEFT JOIN mobile_device_capability_consents c ON c.capability_id=k.id WHERE k.device_id=$1 AND k.enabled ORDER BY k.id FOR SHARE OF k")
            .bind(c.device_id).fetch_all(&mut *tx).await?;
        let offers=rows.into_iter().map(|(id,revision,digest,name,receipt)| {
            let scope=shared_capability::scope(c.device_id,c.generation,id,revision,&digest,&name);
            let scope_digest=hash(&scope.to_string());
            let accepted=receipt.as_ref().is_some_and(|v|v["accepted"]==true && v["scope_digest"]==scope_digest && v["credential_generation"]==c.generation && v["configuration_revision"]==revision);
            json!({"id":id,"scope":scope,"scope_digest":scope_digest,"consent":receipt,"accepted":accepted})
        }).collect();
        tx.commit().await?;
        Ok(offers)
    }
    pub(super) async fn capability_decision(
        &self,
        c: &Credential,
        key: &str,
        input: DecisionInput,
    ) -> Result<Value> {
        let mut tx = self.0.begin().await?;
        // Same transaction lock as platform configuration writes: read/decision cannot cross an update.
        sqlx::query("SELECT pg_advisory_xact_lock(78234610243)")
            .execute(&mut *tx)
            .await?;
        super::consent::authenticate(&mut tx, c).await?;
        let (revision,digest,name,enabled):(i64,String,String,bool)=sqlx::query_as("SELECT revision,configuration_digest,name,enabled FROM mobile_device_capabilities WHERE id=$1 AND device_id=$2 FOR UPDATE")
            .bind(input.capability_id).bind(c.device_id).fetch_optional(&mut *tx).await?.ok_or(Error::Forbidden)?;
        let payload = hash(&serde_json::to_string(&input).map_err(|_| Error::Conflict)?);
        let previous:Option<(String,Value)>=sqlx::query_as("SELECT payload_hash,result FROM mobile_device_capability_operations WHERE device_id=$1 AND credential_generation=$2 AND idempotency_key=$3")
            .bind(c.device_id).bind(c.generation).bind(key).fetch_optional(&mut *tx).await?;
        if let Some((old, result)) = previous {
            return if old == payload {
                Ok(result)
            } else {
                Err(Error::Conflict)
            };
        }
        let scope = shared_capability::scope(
            c.device_id,
            c.generation,
            input.capability_id,
            revision,
            &digest,
            &name,
        );
        let scope_digest = hash(&scope.to_string());
        if revision != input.configuration_revision
            || scope_digest != input.scope_digest
            || (input.accepted && !enabled)
        {
            return Err(Error::Policy("revision_conflict"));
        }
        let current:Option<i64>=sqlx::query_scalar("SELECT revision FROM mobile_device_capability_consents WHERE capability_id=$1 FOR UPDATE").bind(input.capability_id).fetch_optional(&mut *tx).await?;
        if current.unwrap_or(0) != input.expected_revision {
            return Err(Error::Policy("revision_conflict"));
        }
        let result:Value=sqlx::query_scalar("INSERT INTO mobile_device_capability_consents(id,device_id,capability_id,configuration_revision,credential_generation,scope_digest,scope,revision,accepted) VALUES($1,$2,$3,$4,$5,$6,$7,1,$8) ON CONFLICT(capability_id) DO UPDATE SET configuration_revision=$4,credential_generation=$5,scope_digest=$6,scope=$7,revision=mobile_device_capability_consents.revision+1,accepted=$8,updated_at=clock_timestamp() RETURNING to_jsonb(mobile_device_capability_consents)")
            .bind(Uuid::now_v7()).bind(c.device_id).bind(input.capability_id).bind(revision).bind(c.generation).bind(scope_digest).bind(scope).bind(input.accepted).fetch_one(&mut *tx).await?;
        sqlx::query("INSERT INTO mobile_device_capability_events(id,device_id,capability_id,credential_generation,detail) VALUES($1,$2,$3,$4,$5)")
            .bind(Uuid::now_v7()).bind(c.device_id).bind(input.capability_id).bind(c.generation).bind(&result).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO mobile_device_capability_operations(device_id,credential_generation,idempotency_key,payload_hash,result) VALUES($1,$2,$3,$4,$5)")
            .bind(c.device_id).bind(c.generation).bind(key).bind(payload).bind(&result).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(result)
    }
}
