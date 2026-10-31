use super::*;
use crate::distribution::domain::device_control::Credential;
type Scope = (Uuid, i64, Uuid, i64);
async fn scope(tx: &mut Tx<'_>, school: Uuid, device: Uuid) -> Result<Scope> {
    sqlx::query_as("SELECT g.id,g.revision,c.id,c.revision FROM mobile_device_school_grants g JOIN schools s ON s.id=g.school_id JOIN execution_devices d ON d.id=g.device_id JOIN mobile_device_consents c ON c.grant_id=g.id WHERE g.school_id=$1 AND g.device_id=$2 AND g.enabled AND s.active AND d.enabled AND c.accepted AND c.grant_revision=g.revision AND c.scope->>'purpose'=g.purpose FOR SHARE OF g,s,d,c")
        .bind(school).bind(device).fetch_optional(&mut **tx).await?.ok_or(Error::Policy("device_consent_required"))
}
pub(super) async fn write(
    tx: &mut Tx<'_>,
    school: Uuid,
    actor: Uuid,
    action: &str,
    id: Uuid,
    input: &Value,
) -> Result<Value> {
    match action {
        "flow" => {
            let v: FlowInput = serde_json::from_value(input.clone())
                .map_err(|_| Error::Invalid("流程字段无效，仅接受 schema v2".into()))?;
            validate_text(&v.name, 1, 128, 512)?;
            v.flow.validate()?;
            let flow = serde_json::to_value(v.flow).map_err(|_| Error::Conflict)?;
            sqlx::query("INSERT INTO mobile_execution_flow_versions(id,school_id,name,flow,content_digest,created_by) VALUES($1,$2,$3,$4,$5,$6)")
                .bind(id).bind(school).bind(v.name).bind(&flow).bind(hash(&flow.to_string())).bind(actor).execute(&mut **tx).await?;
            object(tx, "mobile_execution_flow_versions", id).await
        }
        "validation" => {
            let v: ValidationInput = serde_json::from_value(input.clone())
                .map_err(|_| Error::Invalid("请选择本校设备和流程版本".into()))?;
            validate_text(&v.search_text, 1, 256, 1024)?;
            let (grant, grant_revision, consent, consent_revision) =
                scope(tx, school, v.device_id).await?;
            sqlx::query("SELECT id FROM mobile_execution_flow_versions WHERE school_id=$1 AND id=$2 FOR SHARE").bind(school).bind(v.flow_version_id).fetch_optional(&mut **tx).await?.ok_or(Error::NotFound)?;
            sqlx::query("INSERT INTO mobile_execution_flow_validations(id,school_id,flow_version_id,device_id,grant_id,grant_revision,consent_id,consent_revision,challenge,created_by,search_text) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)")
                .bind(id).bind(school).bind(v.flow_version_id).bind(v.device_id).bind(grant).bind(grant_revision).bind(consent).bind(consent_revision).bind(crate::business::secret()).bind(actor).bind(v.search_text).execute(&mut **tx).await?;
            // The one-time challenge is only exposed to the authenticated device.
            object(tx, "mobile_execution_flow_validations", id)
                .await?
                .as_object()
                .map(|v| {
                    let mut v = v.clone();
                    v.remove("challenge");
                    Value::Object(v)
                })
                .ok_or(Error::Conflict)
        }
        "release" => {
            let v: ReleaseInput = serde_json::from_value(input.clone())
                .map_err(|_| Error::Invalid("发布字段无效".into()))?;
            validate_text(&v.name, 1, 128, 512)?;
            let (device,grant,gr,consent,cr):(Uuid,Uuid,i64,Uuid,i64)=sqlx::query_as("SELECT v.device_id,v.grant_id,v.grant_revision,v.consent_id,v.consent_revision FROM mobile_execution_flow_validations v JOIN execution_devices d ON d.id=v.device_id WHERE v.school_id=$1 AND v.id=$2 AND v.flow_version_id=$3 AND v.state='passed' AND v.credential_generation=d.credential_generation AND v.completed_at>clock_timestamp()-interval '24 hours' AND NOT EXISTS(SELECT 1 FROM mobile_execution_flow_validations newer WHERE newer.device_id=v.device_id AND newer.flow_version_id=v.flow_version_id AND newer.state<>'pending' AND newer.completed_at>v.completed_at) FOR SHARE OF v,d")
                .bind(school).bind(v.validation_id).bind(v.flow_version_id).fetch_optional(&mut **tx).await?.ok_or(Error::Policy("device_validation_required"))?;
            require_scope(scope(tx, school, device).await?, (grant, gr, consent, cr))?;
            let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_execution_flow_releases WHERE validation_id=$1)").bind(v.validation_id).fetch_one(&mut **tx).await?;
            if exists {
                return Err(Error::Conflict);
            }
            sqlx::query("INSERT INTO mobile_execution_flow_releases(id,school_id,flow_version_id,validation_id,name,created_by) VALUES($1,$2,$3,$4,$5,$6)")
                .bind(id).bind(school).bind(v.flow_version_id).bind(v.validation_id).bind(v.name).bind(actor).execute(&mut **tx).await?;
            object(tx, "mobile_execution_flow_releases", id).await
        }
        "revoke-release" => {
            let v: RevisionInput = serde_json::from_value(input.clone())
                .map_err(|_| Error::Invalid("撤销字段无效".into()))?;
            validate_text(&v.reason, 1, 2000, 8000)?;
            let n=sqlx::query("UPDATE mobile_execution_flow_releases SET enabled=false,revision=revision+1,revoked_by=$4,revoked_at=clock_timestamp(),revocation_reason=$5 WHERE school_id=$1 AND id=$2 AND revision=$3 AND enabled")
                .bind(school).bind(id).bind(v.expected_revision).bind(actor).bind(v.reason).execute(&mut **tx).await?.rows_affected();
            if n != 1 {
                return Err(Error::Policy("revision_conflict"));
            }
            object(tx, "mobile_execution_flow_releases", id).await
        }
        "assignment" | "update-assignment" => {
            let v: AssignmentInput = serde_json::from_value(input.clone())
                .map_err(|_| Error::Invalid("配置字段无效".into()))?;
            if v.expected_revision < 0 {
                return Err(Error::Policy("revision_conflict"));
            }
            let grant:Uuid=sqlx::query_scalar("SELECT g.id FROM mobile_device_school_grants g JOIN execution_devices d ON d.id=g.device_id WHERE g.school_id=$1 AND g.device_id=$2 AND (NOT $3 OR (g.enabled AND d.enabled)) FOR SHARE OF g,d")
                .bind(school).bind(v.device_id).bind(v.enabled).fetch_optional(&mut **tx).await?.ok_or(Error::Policy("school_device_grant_required"))?;
            sqlx::query("SELECT id FROM mobile_execution_flow_releases WHERE school_id=$1 AND id=$2 AND (enabled OR NOT $3) FOR SHARE").bind(school).bind(v.release_id).bind(v.enabled).fetch_optional(&mut **tx).await?.ok_or(Error::Policy("flow_release_required"))?;
            let current:Option<(Uuid,i64)>=sqlx::query_as("SELECT id,execution_revision FROM mobile_execution_assignments WHERE school_id=$1 AND device_id=$2 FOR UPDATE").bind(school).bind(v.device_id).fetch_optional(&mut **tx).await?;
            if action == "update-assignment" && current.map(|v| v.0) != Some(id) {
                return Err(Error::NotFound);
            }
            if current.map(|v| v.1).unwrap_or(0) != v.expected_revision {
                return Err(Error::Policy("revision_conflict"));
            }
            if let Some((current_id, _)) = current {
                sqlx::query("UPDATE mobile_execution_assignments SET release_id=$3,enabled=$4,execution_revision=execution_revision+CASE WHEN release_id<>$3 OR enabled<>$4 THEN 1 ELSE 0 END,metadata_revision=metadata_revision+1,updated_by=$5,updated_at=clock_timestamp() WHERE school_id=$1 AND id=$2")
                    .bind(school).bind(current_id).bind(v.release_id).bind(v.enabled).bind(actor).execute(&mut **tx).await?;
                object(tx, "mobile_execution_assignments", current_id).await
            } else {
                sqlx::query("INSERT INTO mobile_execution_assignments(id,school_id,device_id,grant_id,release_id,enabled,created_by,updated_by) VALUES($1,$2,$3,$4,$5,$6,$7,$7)")
                    .bind(id).bind(school).bind(v.device_id).bind(grant).bind(v.release_id).bind(v.enabled).bind(actor).execute(&mut **tx).await?;
                object(tx, "mobile_execution_assignments", id).await
            }
        }
        _ => Err(Error::NotFound),
    }
}
fn require_scope(current: Scope, expected: Scope) -> Result<()> {
    if current != expected {
        return Err(Error::Policy("revision_conflict"));
    }
    Ok(())
}
impl Repository {
    pub(super) async fn device_validations(&self, c: &Credential) -> Result<Vec<Value>> {
        let mut tx = self.0.begin().await?;
        consent::authenticate(&mut tx, c).await?;
        let jobs:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(v)||jsonb_build_object('flow',f.flow,'flow_digest',f.content_digest,'school_name',s.name) FROM mobile_execution_flow_validations v JOIN mobile_execution_flow_versions f ON f.id=v.flow_version_id AND f.school_id=v.school_id JOIN schools s ON s.id=v.school_id JOIN mobile_device_school_grants g ON g.id=v.grant_id JOIN mobile_device_consents c ON c.id=v.consent_id WHERE v.device_id=$1 AND v.state='pending' AND v.expires_at>clock_timestamp() AND s.active AND g.enabled AND g.revision=v.grant_revision AND c.accepted AND c.revision=v.consent_revision AND c.grant_revision=g.revision ORDER BY v.created_at,v.id LIMIT 31")
            .bind(c.device_id).fetch_all(&mut *tx).await?;
        tx.commit().await?;
        Ok(jobs)
    }
    pub(super) async fn finish_validation(
        &self,
        c: &Credential,
        id: Uuid,
        key: &str,
        input: ValidationResultInput,
    ) -> Result<Value> {
        input.validate()?;
        let mut tx = self.0.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(78234610243)")
            .execute(&mut *tx)
            .await?;
        consent::authenticate(&mut tx, c).await?;
        let payload = hash(&json!([id, input]).to_string());
        let previous:Option<(String,Value)>=sqlx::query_as("SELECT payload_hash,result FROM mobile_execution_validation_operations WHERE device_id=$1 AND idempotency_key=$2").bind(c.device_id).bind(key).fetch_optional(&mut *tx).await?;
        if let Some((digest, result)) = previous {
            return if digest == payload {
                Ok(result)
            } else {
                Err(Error::Conflict)
            };
        }
        type Job = (Uuid, Uuid, i64, Uuid, i64, String, String, Value);
        let (school,grant,gr,consent,cr,challenge,digest,flow):Job=sqlx::query_as("SELECT v.school_id,v.grant_id,v.grant_revision,v.consent_id,v.consent_revision,v.challenge,f.content_digest,f.flow FROM mobile_execution_flow_validations v JOIN mobile_execution_flow_versions f ON f.school_id=v.school_id AND f.id=v.flow_version_id WHERE v.id=$1 AND v.device_id=$2 AND v.state='pending' AND v.expires_at>clock_timestamp() FOR UPDATE OF v")
            .bind(id).bind(c.device_id).fetch_optional(&mut *tx).await?.ok_or(Error::Conflict)?;
        require_scope(
            scope(&mut tx, school, c.device_id).await?,
            (grant, gr, consent, cr),
        )?;
        if input.challenge != challenge
            || input.flow_digest != digest
            || flow["package_name"] != input.package_name
        {
            return Err(Error::Policy("revision_conflict"));
        }
        let passed = input.checks.values().all(|v| *v);
        let result = serde_json::to_value(&input).map_err(|_| Error::Conflict)?;
        sqlx::query("UPDATE mobile_execution_flow_validations SET state=$2,result=$3,credential_generation=$4,completed_at=clock_timestamp() WHERE id=$1")
            .bind(id).bind(if passed {"passed"} else {"failed"}).bind(&result).bind(c.generation).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO mobile_execution_validation_events(id,validation_id,device_id,credential_generation,detail) VALUES($1,$2,$3,$4,$5)")
            .bind(Uuid::now_v7()).bind(id).bind(c.device_id).bind(c.generation).bind(result).execute(&mut *tx).await?;
        let mut response = object(&mut tx, "mobile_execution_flow_validations", id).await?;
        response
            .as_object_mut()
            .ok_or(Error::Conflict)?
            .remove("challenge");
        // Do not expose the challenge through management result documents either.
        response["result"]
            .as_object_mut()
            .ok_or(Error::Conflict)?
            .remove("challenge");
        sqlx::query("INSERT INTO mobile_execution_validation_operations(device_id,idempotency_key,payload_hash,result) VALUES($1,$2,$3,$4)")
            .bind(c.device_id).bind(key).bind(payload).bind(&response).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(response)
    }
}
