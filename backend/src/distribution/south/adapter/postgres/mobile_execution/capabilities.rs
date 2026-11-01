use super::*;
use crate::distribution::domain::{
    device_control::Credential,
    mobile_execution::shared_capability::{self, DecisionInput},
};

impl Repository {
    pub(super) async fn capability_context(&self, c: &Credential) -> Result<Vec<Value>> {
        let mut tx = self.0.begin().await?;
        super::consent::authenticate(&mut tx, c).await?;
        let rows: Vec<(Uuid,i64,String,String,Option<Value>)>=sqlx::query_as("SELECT k.id,k.revision,k.configuration_digest,k.name,to_jsonb(c) FROM mobile_device_capabilities k LEFT JOIN mobile_device_capability_consents c ON c.capability_id=k.id WHERE k.device_id=$1 AND EXISTS(SELECT 1 FROM mobile_capability_openings o JOIN mobile_device_environments e ON e.id=o.environment_snapshot_id JOIN mobile_device_environment_observations obs ON obs.device_id=o.device_id AND obs.snapshot_id=e.id WHERE o.capability_id=k.id AND o.configuration_revision=k.revision AND o.state='awaiting_consent' AND e.credential_generation=$2 AND obs.observed_at>clock_timestamp()-interval '5 minutes' AND NOT EXISTS(SELECT 1 FROM mobile_adapter_withdrawals w WHERE w.release_id=o.release_id)) ORDER BY k.id FOR SHARE OF k")
            .bind(c.device_id).bind(c.generation).fetch_all(&mut *tx).await?;
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
        let (revision,digest,name):(i64,String,String)=sqlx::query_as("SELECT revision,configuration_digest,name FROM mobile_device_capabilities WHERE id=$1 AND device_id=$2 FOR UPDATE")
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
        let candidate: bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_capability_openings o JOIN mobile_device_environments e ON e.id=o.environment_snapshot_id JOIN mobile_device_environment_observations obs ON obs.device_id=o.device_id AND obs.snapshot_id=e.id WHERE o.capability_id=$1 AND o.configuration_revision=$2 AND o.state='awaiting_consent' AND e.credential_generation=$3 AND obs.observed_at>clock_timestamp()-interval '5 minutes' AND NOT EXISTS(SELECT 1 FROM mobile_adapter_withdrawals w WHERE w.release_id=o.release_id))")
            .bind(input.capability_id).bind(revision).bind(c.generation).fetch_one(&mut *tx).await?;
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
            || (input.accepted && !candidate)
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
