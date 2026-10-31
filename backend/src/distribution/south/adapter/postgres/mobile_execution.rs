//! Current phone resources: no queries into official customer groups or member identities.
use crate::{
    business::{Actor, Error, Page, Result, hash},
    distribution::{
        domain::mobile_execution::*,
        south::port::mobile_execution::{Resource, Store},
    },
};
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;
#[derive(Clone)]
pub struct Repository(pub PgPool);
type Tx<'a> = Transaction<'a, Postgres>;
async fn authorize(tx: &mut Tx<'_>, actor: Uuid, school: Option<Uuid>, admin: bool) -> Result<()> {
    if let Some(school) = school {
        sqlx::query("SELECT a.id FROM accounts a JOIN school_memberships m ON m.account_id=a.id JOIN schools s ON s.id=m.school_id WHERE a.id=$1 AND a.active AND s.id=$2 AND s.active AND (m.role='admin' OR (NOT $3 AND m.role='operator')) FOR SHARE OF a,m,s")
            .bind(actor).bind(school).bind(admin).fetch_optional(&mut **tx).await?.ok_or(Error::Forbidden)?;
    } else {
        sqlx::query("SELECT id FROM accounts WHERE id=$1 AND active AND platform_admin FOR SHARE")
            .bind(actor)
            .fetch_optional(&mut **tx)
            .await?
            .ok_or(Error::Forbidden)?;
    }
    Ok(())
}
async fn object(tx: &mut Tx<'_>, table: &str, id: Uuid) -> Result<Value> {
    sqlx::query_scalar(&format!("SELECT to_jsonb(t) FROM {table} t WHERE id=$1"))
        .bind(id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(Error::NotFound)
}
async fn bind(
    tx: &mut Tx<'_>,
    school: Uuid,
    actor: Uuid,
    target: Uuid,
    devices: &[Uuid],
) -> Result<()> {
    for d in devices {
        let grant:Uuid=sqlx::query_scalar("SELECT g.id FROM mobile_device_school_grants g JOIN execution_devices d ON d.id=g.device_id JOIN mobile_targets t ON t.school_id=g.school_id WHERE g.school_id=$1 AND g.device_id=$2 AND g.enabled AND d.enabled AND t.id=$3 AND t.enabled AND t.archived_at IS NULL FOR UPDATE OF g,t,d")
            .bind(school).bind(d).bind(target).fetch_optional(&mut **tx).await?.ok_or(Error::Policy("school_device_grant_required"))?;
        sqlx::query("INSERT INTO mobile_device_target_bindings(id,school_id,device_id,target_id,grant_id,created_by,updated_by) VALUES($1,$2,$3,$4,$5,$6,$6) ON CONFLICT(school_id,device_id,target_id) DO UPDATE SET enabled=true,revision=mobile_device_target_bindings.revision+CASE WHEN mobile_device_target_bindings.enabled THEN 0 ELSE 1 END,updated_by=$6,updated_at=clock_timestamp()")
            .bind(Uuid::now_v7()).bind(school).bind(d).bind(target).bind(grant).bind(actor).execute(&mut **tx).await?;
    }
    Ok(())
}
impl Store for Repository {
    async fn validations(
        &self,
        c: &crate::distribution::domain::device_control::Credential,
    ) -> Result<Vec<Value>> {
        self.device_validations(c).await
    }
    async fn validation_result(
        &self,
        c: &crate::distribution::domain::device_control::Credential,
        id: Uuid,
        key: &str,
        input: ValidationResultInput,
    ) -> Result<Value> {
        self.finish_validation(c, id, key, input).await
    }

    async fn consent_context(
        &self,
        c: &crate::distribution::domain::device_control::Credential,
    ) -> Result<Vec<Value>> {
        self.device_consent_context(c).await
    }
    async fn consent(
        &self,
        c: &crate::distribution::domain::device_control::Credential,
        key: &str,
        input: ConsentInput,
    ) -> Result<Value> {
        self.save_device_consent(c, key, input).await
    }

    async fn target(&self, actor: &Actor, school: Uuid, id: Uuid) -> Result<Value> {
        let mut tx = self.0.begin().await?;
        authorize(&mut tx, actor.id, Some(school), false).await?;
        let result=sqlx::query_scalar("SELECT to_jsonb(t) FROM mobile_targets t WHERE school_id=$1 AND id=$2 AND archived_at IS NULL")
            .bind(school).bind(id).fetch_optional(&mut *tx).await?.ok_or(Error::NotFound)?;
        tx.commit().await?;
        Ok(result)
    }

    async fn list(
        &self,
        actor: &Actor,
        school: Option<Uuid>,
        resource: Resource,
        page: Page,
        search: String,
        device: Option<Uuid>,
    ) -> Result<Vec<Value>> {
        if matches!(resource, Resource::PlatformDevices | Resource::Grants) && school.is_some() {
            return Err(Error::Forbidden);
        }
        let mut tx = self.0.begin().await?;
        authorize(&mut tx, actor.id, school, false).await?;
        let (limit, offset) = page.fetch_bounds()?;
        let sql = match resource {
            Resource::Flows => {
                "SELECT to_jsonb(f) FROM mobile_execution_flow_versions f WHERE school_id=$1 AND strpos(lower(name),lower($2))>0 AND $3::uuid IS NULL ORDER BY created_at DESC,id DESC LIMIT $4 OFFSET $5"
            }
            Resource::Validations => {
                "SELECT (to_jsonb(v)-'challenge'-'result')||jsonb_build_object('result',v.result-'challenge','device_name',d.name,'flow_name',f.name,'expired',v.expires_at<=clock_timestamp()) FROM mobile_execution_flow_validations v JOIN execution_devices d ON d.id=v.device_id JOIN mobile_execution_flow_versions f ON f.id=v.flow_version_id WHERE v.school_id=$1 AND strpos(lower(f.name),lower($2))>0 AND ($3::uuid IS NULL OR v.device_id=$3) ORDER BY v.created_at DESC,v.id DESC LIMIT $4 OFFSET $5"
            }
            Resource::Releases => {
                "SELECT to_jsonb(r)||jsonb_build_object('flow_name',f.name,'flow_digest',f.content_digest) FROM mobile_execution_flow_releases r JOIN mobile_execution_flow_versions f ON f.id=r.flow_version_id WHERE r.school_id=$1 AND strpos(lower(r.name),lower($2))>0 AND $3::uuid IS NULL ORDER BY r.created_at DESC,r.id DESC LIMIT $4 OFFSET $5"
            }
            Resource::Assignments => {
                "SELECT to_jsonb(a)||jsonb_build_object('device_name',d.name,'release_name',r.name,'consent_required',NOT COALESCE(c.accepted AND c.grant_revision=g.revision,false),'available',a.enabled AND g.enabled AND d.enabled AND r.enabled AND COALESCE(c.accepted AND c.grant_revision=g.revision,false) AND EXISTS(SELECT 1 FROM mobile_execution_flow_validations v WHERE v.school_id=a.school_id AND v.device_id=a.device_id AND v.flow_version_id=r.flow_version_id AND v.state='passed' AND v.grant_revision=g.revision AND v.consent_id=c.id AND v.consent_revision=c.revision AND v.credential_generation=d.credential_generation AND NOT EXISTS(SELECT 1 FROM mobile_execution_flow_validations newer WHERE newer.device_id=v.device_id AND newer.flow_version_id=v.flow_version_id AND newer.state<>'pending' AND newer.completed_at>v.completed_at)),'device_validation_required',NOT EXISTS(SELECT 1 FROM mobile_execution_flow_validations v WHERE v.school_id=a.school_id AND v.device_id=a.device_id AND v.flow_version_id=r.flow_version_id AND v.state='passed' AND v.grant_revision=g.revision AND v.consent_id=c.id AND v.consent_revision=c.revision AND v.credential_generation=d.credential_generation AND NOT EXISTS(SELECT 1 FROM mobile_execution_flow_validations newer WHERE newer.device_id=v.device_id AND newer.flow_version_id=v.flow_version_id AND newer.state<>'pending' AND newer.completed_at>v.completed_at))) FROM mobile_execution_assignments a JOIN execution_devices d ON d.id=a.device_id JOIN mobile_device_school_grants g ON g.id=a.grant_id JOIN mobile_execution_flow_releases r ON r.id=a.release_id LEFT JOIN mobile_device_consents c ON c.grant_id=g.id WHERE a.school_id=$1 AND strpos(lower(d.name),lower($2))>0 AND ($3::uuid IS NULL OR a.device_id=$3) ORDER BY a.created_at,a.id LIMIT $4 OFFSET $5"
            }
            Resource::Targets => {
                "SELECT to_jsonb(t) FROM mobile_targets t WHERE school_id=$1 AND archived_at IS NULL AND (strpos(lower(search_text),lower($2))>0 OR strpos(lower(alias),lower($2))>0) AND ($3::uuid IS NULL OR EXISTS(SELECT 1 FROM mobile_device_target_bindings b JOIN mobile_device_school_grants g ON g.id=b.grant_id JOIN execution_devices d ON d.id=b.device_id AND d.enabled WHERE t.enabled AND b.target_id=t.id AND b.school_id=t.school_id AND b.device_id=$3 AND b.enabled AND g.enabled)) ORDER BY created_at,id LIMIT $4 OFFSET $5"
            }
            Resource::PlatformDevices => {
                "SELECT to_jsonb(d)-'client_instance_id' FROM execution_devices d WHERE $1::uuid IS NULL AND strpos(lower(d.name),lower($2))>0 AND ($3::uuid IS NULL OR d.id=$3) ORDER BY d.created_at,d.id LIMIT $4 OFFSET $5"
            }
            Resource::Devices => {
                "SELECT jsonb_build_object('id',d.id,'name',d.name,'enabled',d.enabled,'last_seen_at',d.last_seen_at,'grant_id',g.id,'grant_revision',g.revision,'school_alias',g.school_alias,'school_remark',g.school_remark,'control_state',p.state,'last_control_heartbeat_at',p.last_control_heartbeat_at) FROM mobile_device_school_grants g JOIN execution_devices d ON d.id=g.device_id JOIN device_control_presence p ON p.id=d.id WHERE g.school_id=$1 AND g.enabled AND strpos(lower(d.name),lower($2))>0 AND ($3::uuid IS NULL OR d.id=$3) ORDER BY d.created_at,d.id LIMIT $4 OFFSET $5"
            }
            Resource::Bindings => {
                "SELECT to_jsonb(b)||jsonb_build_object('device_name',d.name,'search_text',t.search_text,'target_alias',t.alias,'available',g.enabled AND d.enabled AND t.enabled AND t.archived_at IS NULL) FROM mobile_device_target_bindings b JOIN mobile_device_school_grants g ON g.id=b.grant_id JOIN execution_devices d ON d.id=b.device_id JOIN mobile_targets t ON t.id=b.target_id AND t.school_id=b.school_id WHERE b.school_id=$1 AND b.enabled AND ($2::text IS NOT NULL) AND ($3::uuid IS NULL OR b.device_id=$3) ORDER BY b.created_at,b.id LIMIT $4 OFFSET $5"
            }
            Resource::Grants => {
                "SELECT to_jsonb(g)||jsonb_build_object('device_name',d.name,'school_name',s.name) FROM mobile_device_school_grants g JOIN execution_devices d ON d.id=g.device_id JOIN schools s ON s.id=g.school_id WHERE ($1::uuid IS NULL OR g.school_id=$1) AND $2::text IS NOT NULL AND ($3::uuid IS NULL OR g.device_id=$3) ORDER BY g.created_at,g.id LIMIT $4 OFFSET $5"
            }
        };
        let rows = sqlx::query_scalar(sql)
            .bind(school)
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
        school: Option<Uuid>,
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
        authorize(&mut tx, actor.id, school, true).await?;
        let scope = format!(
            "{}:{action}:{}",
            school.map(|s| s.to_string()).unwrap_or_default(),
            id.map(|s| s.to_string()).unwrap_or_default()
        );
        let digest = hash(&input.to_string());
        let previous:Option<(String,Value)>=sqlx::query_as("SELECT payload_hash,result FROM mobile_execution_operations WHERE actor_id=$1 AND scope=$2 AND idempotency_key=$3")
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
            "flow" | "validation" | "release" | "revoke-release" | "assignment"
            | "update-assignment" => {
                flows::write(
                    &mut tx,
                    school.ok_or(Error::Forbidden)?,
                    actor.id,
                    action,
                    resource,
                    &input,
                )
                .await?
            }
            "grant" => {
                if school.is_some() {
                    return Err(Error::Forbidden);
                }
                let v: GrantInput = serde_json::from_value(input.clone())
                    .map_err(|_| Error::Invalid("授权字段无效".into()))?;
                validate_text(&v.purpose, 1, 256, 1024)?;
                let active:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM schools WHERE id=$1 AND active) AND EXISTS(SELECT 1 FROM execution_devices WHERE id=$2)").bind(v.school_id).bind(v.device_id).fetch_one(&mut *tx).await?;
                if !active {
                    return Err(Error::NotFound);
                }
                let grant:Uuid=sqlx::query_scalar("INSERT INTO mobile_device_school_grants(id,school_id,device_id,purpose,granted_by) VALUES($1,$2,$3,$4,$5) ON CONFLICT(school_id,device_id) DO UPDATE SET enabled=true,purpose=$4,revision=mobile_device_school_grants.revision+1,granted_by=$5,revoked_by=NULL,revoked_at=NULL RETURNING id")
                    .bind(resource).bind(v.school_id).bind(v.device_id).bind(v.purpose).bind(actor.id).fetch_one(&mut *tx).await?;
                object(&mut tx, "mobile_device_school_grants", grant).await?
            }
            "revoke-grant" => {
                if school.is_some() {
                    return Err(Error::Forbidden);
                }
                let v: RevisionInput = serde_json::from_value(input.clone())
                    .map_err(|_| Error::Invalid("撤权字段无效".into()))?;
                validate_text(&v.reason, 1, 2000, 8000)?;
                let n=sqlx::query("UPDATE mobile_device_school_grants SET enabled=false,revision=revision+1,revoked_by=$2,revoked_at=clock_timestamp() WHERE id=$1 AND revision=$3")
                    .bind(resource).bind(actor.id).bind(v.expected_revision).execute(&mut *tx).await?.rows_affected();
                if n != 1 {
                    return Err(Error::Policy("revision_conflict"));
                }
                object(&mut tx, "mobile_device_school_grants", resource).await?
            }
            "target" => {
                let school = school.ok_or(Error::Forbidden)?;
                let v: TargetInput = serde_json::from_value(input.clone())
                    .map_err(|_| Error::Invalid("目标字段无效".into()))?;
                v.validate()?;
                let candidates:Vec<(Uuid,i64,i64)>=sqlx::query_as("SELECT id,metadata_revision,locator_revision FROM mobile_targets WHERE school_id=$1 AND search_text=$2 AND archived_at IS NULL ORDER BY id")
                    .bind(school).bind(&v.search_text).fetch_all(&mut *tx).await?;
                if !candidates.is_empty() {
                    // Confirmation binds exact input and duplicate candidates, never inferred identity.
                    let confirmation = hash(
                        &json!([
                            school,
                            v.search_text,
                            v.alias,
                            v.remark,
                            v.device_ids,
                            candidates
                        ])
                        .to_string(),
                    );
                    if v.duplicate_confirmation.as_deref() != Some(&confirmation) {
                        let response = json!({"id":resource,"confirmation_required":true,"duplicate_confirmation":confirmation,"candidate_ids":candidates.iter().map(|v|v.0).collect::<Vec<_>>()});
                        sqlx::query("INSERT INTO mobile_execution_operations(actor_id,scope,idempotency_key,payload_hash,result) VALUES($1,$2,$3,$4,$5)")
                            .bind(actor.id).bind(&scope).bind(key).bind(&digest).bind(&response).execute(&mut *tx).await?;
                        tx.commit().await?;
                        return Ok(response);
                    }
                }
                sqlx::query("INSERT INTO mobile_targets(id,school_id,search_text,alias,remark,created_by,updated_by) VALUES($1,$2,$3,$4,$5,$6,$6)")
                    .bind(resource).bind(school).bind(&v.search_text).bind(&v.alias).bind(&v.remark).bind(actor.id).execute(&mut *tx).await?;
                bind(&mut tx, school, actor.id, resource, &v.device_ids).await?;
                object(&mut tx, "mobile_targets", resource).await?
            }
            "update-target" => {
                let school = school.ok_or(Error::Forbidden)?;
                let v: TargetUpdate = serde_json::from_value(input.clone())
                    .map_err(|_| Error::Invalid("目标字段无效".into()))?;
                validate_text(&v.search_text, 1, 256, 1024)?;
                validate_text(&v.alias, 0, 128, 512)?;
                validate_text(&v.remark, 0, 2000, 8000)?;
                let n=sqlx::query("UPDATE mobile_targets SET search_text=$4,alias=$5,remark=$6,enabled=$7,metadata_revision=metadata_revision+1,locator_revision=locator_revision+CASE WHEN search_text<>$4 OR enabled<>$7 THEN 1 ELSE 0 END,updated_by=$8,updated_at=clock_timestamp() WHERE school_id=$1 AND id=$2 AND metadata_revision=$3 AND archived_at IS NULL")
                    .bind(school).bind(resource).bind(v.expected_revision).bind(v.search_text).bind(v.alias).bind(v.remark).bind(v.enabled).bind(actor.id).execute(&mut *tx).await?.rows_affected();
                if n != 1 {
                    return Err(Error::Policy("revision_conflict"));
                }
                object(&mut tx, "mobile_targets", resource).await?
            }
            "archive-target" => {
                let school = school.ok_or(Error::Forbidden)?;
                let v: RevisionInput = serde_json::from_value(input.clone())
                    .map_err(|_| Error::Invalid("归档字段无效".into()))?;
                validate_text(&v.reason, 1, 2000, 8000)?;
                let n=sqlx::query("UPDATE mobile_targets SET enabled=false,archived_at=clock_timestamp(),metadata_revision=metadata_revision+1,locator_revision=locator_revision+1,updated_by=$4 WHERE school_id=$1 AND id=$2 AND metadata_revision=$3 AND archived_at IS NULL")
                    .bind(school).bind(resource).bind(v.expected_revision).bind(actor.id).execute(&mut *tx).await?.rows_affected();
                if n != 1 {
                    return Err(Error::Policy("revision_conflict"));
                }
                object(&mut tx, "mobile_targets", resource).await?
            }
            "bind" => {
                let school = school.ok_or(Error::Forbidden)?;
                let v: BindingInput = serde_json::from_value(input.clone())
                    .map_err(|_| Error::Invalid("绑定字段无效".into()))?;
                if v.device_ids.is_empty() || v.device_ids.len() > 100 {
                    return Err(Error::Invalid("请选择 1–100 台设备".into()));
                }
                let mut unique = v.device_ids.clone();
                unique.sort();
                unique.dedup();
                if unique.len() != v.device_ids.len() {
                    return Err(Error::Invalid("设备不可重复".into()));
                }
                bind(&mut tx, school, actor.id, v.target_id, &v.device_ids).await?;
                json!({"id":resource,"target_id":v.target_id,"device_ids":v.device_ids})
            }
            "revoke-binding" => {
                let school = school.ok_or(Error::Forbidden)?;
                let v: RevisionInput = serde_json::from_value(input.clone())
                    .map_err(|_| Error::Invalid("解绑字段无效".into()))?;
                validate_text(&v.reason, 1, 2000, 8000)?;
                let n=sqlx::query("UPDATE mobile_device_target_bindings SET enabled=false,revision=revision+1,updated_by=$4,updated_at=clock_timestamp() WHERE school_id=$1 AND id=$2 AND revision=$3")
                    .bind(school).bind(resource).bind(v.expected_revision).bind(actor.id).execute(&mut *tx).await?.rows_affected();
                if n != 1 {
                    return Err(Error::Policy("revision_conflict"));
                }
                object(&mut tx, "mobile_device_target_bindings", resource).await?
            }
            _ => return Err(Error::NotFound),
        };
        sqlx::query("INSERT INTO mobile_execution_audit_events(id,school_id,resource_id,actor_id,action,detail) VALUES($1,$2,$3,$4,$5,$6)")
            .bind(Uuid::now_v7()).bind(school.or_else(||result["school_id"].as_str().and_then(|s|s.parse().ok())))
            .bind(result["id"].as_str().and_then(|s|s.parse::<Uuid>().ok()).unwrap_or(resource)).bind(actor.id).bind(action).bind(json!({"input":input,"result":result})).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO mobile_execution_operations(actor_id,scope,idempotency_key,payload_hash,result) VALUES($1,$2,$3,$4,$5)")
            .bind(actor.id).bind(scope).bind(key).bind(digest).bind(&result).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(result)
    }
}

mod consent;

mod flows;
