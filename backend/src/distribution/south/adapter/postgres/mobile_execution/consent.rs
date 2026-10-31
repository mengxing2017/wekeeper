use super::*;
use crate::distribution::domain::device_control::Credential;

pub(super) async fn authenticate(tx: &mut Tx<'_>, c: &Credential) -> Result<()> {
    sqlx::query("SELECT c.device_id FROM device_credentials c JOIN execution_devices d ON d.id=c.device_id WHERE c.token_hash=$1 AND c.device_id=$2 AND c.generation=$3 AND c.revoked_at IS NULL AND c.expires_at>clock_timestamp() AND d.enabled AND d.credential_generation=c.generation FOR SHARE OF c,d")
        .bind(&c.token_hash).bind(c.device_id).bind(c.generation).fetch_optional(&mut **tx).await?.ok_or(Error::Unauthorized)?;
    Ok(())
}
impl Repository {
    pub(super) async fn device_consent_context(&self, c: &Credential) -> Result<Vec<Value>> {
        let mut tx = self.0.begin().await?;
        authenticate(&mut tx, c).await?;
        let rows:Vec<(Uuid,Uuid,i64,String,String,Option<Value>)>=sqlx::query_as("SELECT g.id,g.school_id,g.revision,g.purpose,s.name,to_jsonb(c) FROM mobile_device_school_grants g JOIN schools s ON s.id=g.school_id LEFT JOIN mobile_device_consents c ON c.grant_id=g.id WHERE g.device_id=$1 AND g.enabled AND s.active ORDER BY g.school_id")
            .bind(c.device_id).fetch_all(&mut *tx).await?;
        let result=rows.into_iter().map(|(grant,school,revision,purpose,name,consent)| {
            let scope=consent_scope(c.device_id,school,grant,revision,&purpose);
            let digest=hash(&scope.to_string());
            let effective=consent.as_ref().is_some_and(|v|v["accepted"]==true && v["scope_digest"]==digest && v["grant_revision"]==revision);
            json!({"id":grant,"school_name":name,"scope":scope,"scope_digest":digest,"consent":consent,"accepted":effective})
        }).collect();
        tx.commit().await?;
        Ok(result)
    }
    pub(super) async fn save_device_consent(
        &self,
        c: &Credential,
        key: &str,
        input: ConsentInput,
    ) -> Result<Value> {
        let mut tx = self.0.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(78234610243)")
            .execute(&mut *tx)
            .await?;
        authenticate(&mut tx, c).await?;
        // Ownership is checked even for an idempotent replay; a different device cannot read it.
        let (school,revision,purpose):(Uuid,i64,String)=sqlx::query_as("SELECT g.school_id,g.revision,g.purpose FROM mobile_device_school_grants g JOIN schools s ON s.id=g.school_id WHERE g.id=$1 AND g.device_id=$2 AND g.enabled AND s.active FOR SHARE OF g,s")
            .bind(input.grant_id).bind(c.device_id).fetch_optional(&mut *tx).await?.ok_or(Error::Forbidden)?;
        let payload = hash(
            &serde_json::to_string(&input).map_err(|_| Error::Invalid("授权字段无效".into()))?,
        );
        let previous:Option<(String,Value)>=sqlx::query_as("SELECT payload_hash,result FROM mobile_device_consent_operations WHERE device_id=$1 AND idempotency_key=$2")
            .bind(c.device_id).bind(key).fetch_optional(&mut *tx).await?;
        if let Some((old, result)) = previous {
            return if old == payload {
                Ok(result)
            } else {
                Err(Error::Conflict)
            };
        }
        let scope = consent_scope(c.device_id, school, input.grant_id, revision, &purpose);
        let digest = hash(&scope.to_string());
        if revision != input.grant_revision || digest != input.scope_digest {
            return Err(Error::Policy("revision_conflict"));
        }
        let current: Option<i64> = sqlx::query_scalar(
            "SELECT revision FROM mobile_device_consents WHERE grant_id=$1 FOR UPDATE",
        )
        .bind(input.grant_id)
        .fetch_optional(&mut *tx)
        .await?;
        if current.unwrap_or(0) != input.expected_revision {
            return Err(Error::Policy("revision_conflict"));
        }
        let result:Value=sqlx::query_scalar("INSERT INTO mobile_device_consents(id,school_id,device_id,grant_id,grant_revision,scope_digest,scope,revision,accepted,credential_generation) VALUES($1,$2,$3,$4,$5,$6,$7,1,$8,$9) ON CONFLICT(grant_id) DO UPDATE SET grant_revision=$5,scope_digest=$6,scope=$7,revision=mobile_device_consents.revision+1,accepted=$8,credential_generation=$9,updated_at=clock_timestamp() RETURNING to_jsonb(mobile_device_consents)")
            .bind(Uuid::now_v7()).bind(school).bind(c.device_id).bind(input.grant_id).bind(revision).bind(digest).bind(scope).bind(input.accepted).bind(c.generation).fetch_one(&mut *tx).await?;
        sqlx::query("INSERT INTO mobile_device_consent_events(id,device_id,grant_id,credential_generation,detail) VALUES($1,$2,$3,$4,$5)")
            .bind(Uuid::now_v7()).bind(c.device_id).bind(input.grant_id).bind(c.generation).bind(&result).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO mobile_device_consent_operations(device_id,idempotency_key,payload_hash,result) VALUES($1,$2,$3,$4)")
            .bind(c.device_id).bind(key).bind(payload).bind(&result).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(result)
    }
}
