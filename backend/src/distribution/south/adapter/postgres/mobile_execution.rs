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
impl Store for Repository {
    async fn opening_context(
        &self,
        c: &crate::distribution::domain::device_control::Credential,
    ) -> Result<Value> {
        self.device_opening_context(c).await
    }
    async fn environment(
        &self,
        c: &crate::distribution::domain::device_control::Credential,
        key: &str,
        input: crate::distribution::domain::mobile_execution::onboarding::Environment,
    ) -> Result<Value> {
        Repository::environment(self, c, key, input).await
    }
    async fn capability_context(
        &self,
        c: &crate::distribution::domain::device_control::Credential,
    ) -> Result<Vec<Value>> {
        Repository::capability_context(self, c).await
    }
    async fn capability_decision(
        &self,
        c: &crate::distribution::domain::device_control::Credential,
        key: &str,
        input: shared_capability::DecisionInput,
    ) -> Result<Value> {
        Repository::capability_decision(self, c, key, input).await
    }

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
        if matches!(
            resource,
            Resource::Openings
                | Resource::AdapterReleases
                | Resource::AdapterSupport
                | Resource::PlatformDevices
                | Resource::PlatformBindings
                | Resource::PlatformTargets
        ) && school.is_some()
        {
            return Err(Error::Forbidden);
        }
        let mut tx = self.0.begin().await?;
        authorize(&mut tx, actor.id, school, false).await?;
        let (limit, offset) = page.fetch_bounds()?;
        let sql = match resource {
            Resource::Openings => onboarding::OPENINGS_QUERY,
            Resource::AdapterReleases => {
                "SELECT to_jsonb(r)-'flow' FROM mobile_adapter_releases r WHERE $1::uuid IS NULL AND strpos(lower(r.name),lower($2))>0 AND $3::uuid IS NULL AND NOT EXISTS(SELECT 1 FROM mobile_adapter_withdrawals w WHERE w.release_id=r.id) ORDER BY r.published_at,r.id LIMIT $4 OFFSET $5"
            }
            Resource::AdapterSupport => {
                "SELECT to_jsonb(s) FROM mobile_adapter_support_requests s WHERE $1::uuid IS NULL AND strpos(lower(s.note),lower($2))>0 AND ($3::uuid IS NULL OR s.device_id=$3) ORDER BY s.created_at DESC,s.id DESC LIMIT $4 OFFSET $5"
            }

            Resource::Tasks => {
                "SELECT to_jsonb(t)||jsonb_build_object('device_name',d.name,'plan',p.snapshot,'plan_digest',p.digest,'items',(SELECT COALESCE(jsonb_agg(to_jsonb(i) ORDER BY i.position),'[]'::jsonb) FROM mobile_execution_task_items i WHERE i.school_id=t.school_id AND i.task_id=t.id)) FROM mobile_execution_tasks t JOIN execution_devices d ON d.id=t.device_id JOIN mobile_execution_plans p ON p.task_id=t.id AND p.school_id=t.school_id WHERE t.school_id=$1 AND $2::text IS NOT NULL AND ($3::uuid IS NULL OR t.device_id=$3) ORDER BY t.created_at DESC,t.id DESC LIMIT $4 OFFSET $5"
            }
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
                "SELECT to_jsonb(a)||jsonb_build_object('device_name',d.name,'release_name',r.name,'consent_required',NOT COALESCE(c.accepted AND c.grant_revision=g.revision,false),'available',false,'historical_configuration_valid',a.enabled AND g.enabled AND d.enabled AND r.enabled AND COALESCE(c.accepted AND c.grant_revision=g.revision,false) AND EXISTS(SELECT 1 FROM mobile_execution_flow_validations v WHERE v.school_id=a.school_id AND v.device_id=a.device_id AND v.flow_version_id=r.flow_version_id AND v.state='passed' AND v.grant_revision=g.revision AND v.consent_id=c.id AND v.consent_revision=c.revision AND v.credential_generation=d.credential_generation AND NOT EXISTS(SELECT 1 FROM mobile_execution_flow_validations newer WHERE newer.device_id=v.device_id AND newer.flow_version_id=v.flow_version_id AND newer.state<>'pending' AND newer.completed_at>v.completed_at)),'device_validation_required',NOT EXISTS(SELECT 1 FROM mobile_execution_flow_validations v WHERE v.school_id=a.school_id AND v.device_id=a.device_id AND v.flow_version_id=r.flow_version_id AND v.state='passed' AND v.grant_revision=g.revision AND v.consent_id=c.id AND v.consent_revision=c.revision AND v.credential_generation=d.credential_generation AND NOT EXISTS(SELECT 1 FROM mobile_execution_flow_validations newer WHERE newer.device_id=v.device_id AND newer.flow_version_id=v.flow_version_id AND newer.state<>'pending' AND newer.completed_at>v.completed_at))) FROM mobile_execution_assignments a JOIN execution_devices d ON d.id=a.device_id JOIN mobile_device_school_grants g ON g.id=a.grant_id JOIN mobile_execution_flow_releases r ON r.id=a.release_id LEFT JOIN mobile_device_consents c ON c.grant_id=g.id WHERE a.school_id=$1 AND strpos(lower(d.name),lower($2))>0 AND ($3::uuid IS NULL OR a.device_id=$3) ORDER BY a.created_at,a.id LIMIT $4 OFFSET $5"
            }
            Resource::Targets => {
                "SELECT to_jsonb(t) FROM mobile_targets t WHERE school_id=$1 AND archived_at IS NULL AND (strpos(lower(search_text),lower($2))>0 OR strpos(lower(alias),lower($2))>0) AND ($3::uuid IS NULL OR EXISTS(SELECT 1 FROM mobile_device_target_bindings b JOIN execution_devices d ON d.id=b.device_id AND d.enabled WHERE t.enabled AND b.target_id=t.id AND b.school_id=t.school_id AND b.device_id=$3 AND b.enabled)) ORDER BY created_at,id LIMIT $4 OFFSET $5"
            }
            Resource::PlatformDevices => {
                "SELECT to_jsonb(d)-'client_instance_id' FROM execution_devices d WHERE $1::uuid IS NULL AND strpos(lower(d.name),lower($2))>0 AND ($3::uuid IS NULL OR d.id=$3) ORDER BY d.created_at,d.id LIMIT $4 OFFSET $5"
            }
            Resource::Devices => {
                "SELECT jsonb_build_object('id',d.id,'name',d.name,'enabled',d.enabled,'last_seen_at',d.last_seen_at,'control_state',p.state,'last_control_heartbeat_at',p.last_control_heartbeat_at) FROM execution_devices d LEFT JOIN device_control_presence p ON p.id=d.id WHERE EXISTS(SELECT 1 FROM mobile_device_target_bindings b JOIN mobile_targets t ON t.id=b.target_id AND t.school_id=b.school_id WHERE b.device_id=d.id AND b.school_id=$1 AND b.enabled AND t.enabled AND t.archived_at IS NULL) AND strpos(lower(d.name),lower($2))>0 AND ($3::uuid IS NULL OR d.id=$3) ORDER BY d.created_at,d.id LIMIT $4 OFFSET $5"
            }
            Resource::Bindings | Resource::PlatformBindings => {
                "SELECT to_jsonb(b)||jsonb_build_object('school_name',s.name,'device_name',d.name,'search_text',COALESCE(b.search_text_override,t.search_text),'search_source',CASE WHEN b.search_text_override IS NULL THEN 'target_default' ELSE 'binding_override' END,'source_locator_revision',CASE WHEN b.search_text_override IS NULL THEN t.locator_revision ELSE b.locator_revision END,'target_alias',t.alias,'available',b.enabled AND d.enabled AND t.enabled AND t.archived_at IS NULL) FROM mobile_device_target_bindings b JOIN execution_devices d ON d.id=b.device_id JOIN mobile_targets t ON t.id=b.target_id AND t.school_id=b.school_id JOIN schools s ON s.id=t.school_id WHERE ($1::uuid IS NULL OR b.school_id=$1) AND (strpos(lower(t.search_text),lower($2))>0 OR strpos(lower(d.name),lower($2))>0 OR strpos(lower(t.alias),lower($2))>0 OR strpos(lower(COALESCE(b.search_text_override,'')),lower($2))>0) AND ($3::uuid IS NULL OR b.device_id=$3) ORDER BY b.created_at,b.id LIMIT $4 OFFSET $5"
            }
            Resource::PlatformTargets => {
                "SELECT jsonb_build_object('id',t.id,'school_id',t.school_id,'school_name',s.name,'search_text',t.search_text,'alias',t.alias,'enabled',t.enabled) FROM mobile_targets t JOIN schools s ON s.id=t.school_id WHERE $1::uuid IS NULL AND s.active AND t.archived_at IS NULL AND (strpos(lower(t.search_text),lower($2))>0 OR strpos(lower(t.alias),lower($2))>0 OR strpos(lower(s.name),lower($2))>0) AND $3::uuid IS NULL ORDER BY t.created_at,t.id LIMIT $4 OFFSET $5"
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
        let rows = if matches!(resource, Resource::Openings) {
            onboarding::enrich(&mut tx, rows).await?
        } else {
            rows
        };
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
        if matches!(
            action,
            "task-preview"
                | "confirm-task"
                | "flow"
                | "validation"
                | "release"
                | "revoke-release"
                | "assignment"
                | "update-assignment"
        ) {
            return Err(Error::Invalid("共享手机执行链路正在升级，暂不能登记或启用旧学校设备执行计划；已有记录仍可查看和取消未领取任务".into()));
        }
        if matches!(action, "bind" | "update-binding" | "revoke-binding") && school.is_some() {
            return Err(Error::Forbidden);
        }
        let mut tx = self.0.begin().await?;
        // All current resource changes share one channel-local guard, including idempotency.
        sqlx::query("SELECT pg_advisory_xact_lock(78234610243)")
            .execute(&mut *tx)
            .await?;
        authorize(
            &mut tx,
            actor.id,
            school,
            !matches!(action, "task-preview" | "cancel-task"),
        )
        .await?;
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
            "open-capability" | "cancel-opening" | "adapter-support" | "update-adapter-support" => {
                if school.is_some() {
                    return Err(Error::Forbidden);
                }
                onboarding::write(&mut tx, actor.id, resource, action, &input).await?
            }

            "task-preview" | "confirm-task" | "cancel-task" => {
                tasks::write(
                    &mut tx,
                    school.ok_or(Error::Forbidden)?,
                    actor.id,
                    action,
                    resource,
                    &input,
                )
                .await?
            }
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
                        &json!([school, v.search_text, v.alias, v.remark, candidates]).to_string(),
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
                let active:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_execution_task_items WHERE school_id=$1 AND target_id=$2 AND state IN ('pending','claimed','prepared','invocation_issued','unknown'))").bind(school).bind(resource).fetch_one(&mut *tx).await?;
                if active {
                    return Err(Error::Policy("mobile_target_unresolved"));
                }
                let n=sqlx::query("UPDATE mobile_targets SET enabled=false,archived_at=clock_timestamp(),metadata_revision=metadata_revision+1,locator_revision=locator_revision+1,updated_by=$4 WHERE school_id=$1 AND id=$2 AND metadata_revision=$3 AND archived_at IS NULL")
                    .bind(school).bind(resource).bind(v.expected_revision).bind(actor.id).execute(&mut *tx).await?.rows_affected();
                if n != 1 {
                    return Err(Error::Policy("revision_conflict"));
                }
                object(&mut tx, "mobile_targets", resource).await?
            }
            "bind" => {
                let v: SharedBindingInput = serde_json::from_value(input.clone())
                    .map_err(|_| Error::Invalid("绑定字段无效".into()))?;
                v.validate()?;
                let target_school: Uuid = sqlx::query_scalar("SELECT t.school_id FROM mobile_targets t JOIN schools s ON s.id=t.school_id WHERE t.id=$1 AND t.enabled AND t.archived_at IS NULL AND s.active FOR SHARE OF t,s")
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
                    let binding: Uuid = sqlx::query_scalar("INSERT INTO mobile_device_target_bindings(id,school_id,device_id,target_id,search_text_override,created_by,updated_by) VALUES($1,$2,$3,$4,$5,$6,$6) ON CONFLICT(device_id,target_id) DO UPDATE SET enabled=true,search_text_override=$5,revision=mobile_device_target_bindings.revision+CASE WHEN NOT mobile_device_target_bindings.enabled OR mobile_device_target_bindings.search_text_override IS DISTINCT FROM $5 THEN 1 ELSE 0 END,locator_revision=mobile_device_target_bindings.locator_revision+CASE WHEN mobile_device_target_bindings.search_text_override IS DISTINCT FROM $5 THEN 1 ELSE 0 END,updated_by=$6,updated_at=clock_timestamp() RETURNING id")
                        .bind(Uuid::now_v7()).bind(target_school).bind(device).bind(v.target_id).bind(&v.search_text_override).bind(actor.id).fetch_one(&mut *tx).await?;
                    bindings.push(object(&mut tx, "mobile_device_target_bindings", binding).await?);
                }
                json!({"id":resource,"school_id":target_school,"target_id":v.target_id,"bindings":bindings})
            }
            "update-binding" => {
                let v: SharedBindingUpdate = serde_json::from_value(input.clone())
                    .map_err(|_| Error::Invalid("绑定字段无效".into()))?;
                if let Some(value) = &v.search_text_override {
                    validate_text(value, 1, 256, 1024)?;
                }
                let n=sqlx::query("UPDATE mobile_device_target_bindings b SET enabled=$3,search_text_override=$4,revision=revision+CASE WHEN enabled<>$3 OR search_text_override IS DISTINCT FROM $4 THEN 1 ELSE 0 END,locator_revision=locator_revision+CASE WHEN search_text_override IS DISTINCT FROM $4 THEN 1 ELSE 0 END,updated_by=$5,updated_at=clock_timestamp() WHERE id=$1 AND revision=$2 AND (NOT $3 OR EXISTS(SELECT 1 FROM mobile_targets t JOIN schools s ON s.id=t.school_id JOIN execution_devices d ON d.id=b.device_id WHERE t.id=b.target_id AND t.enabled AND t.archived_at IS NULL AND s.active AND d.enabled))")
                    .bind(resource).bind(v.expected_revision).bind(v.enabled).bind(v.search_text_override).bind(actor.id).execute(&mut *tx).await?.rows_affected();
                if n != 1 {
                    return Err(Error::Policy("revision_conflict"));
                }
                object(&mut tx, "mobile_device_target_bindings", resource).await?
            }
            "revoke-binding" => {
                let v: RevisionInput = serde_json::from_value(input.clone())
                    .map_err(|_| Error::Invalid("解绑字段无效".into()))?;
                validate_text(&v.reason, 1, 2000, 8000)?;
                let n=sqlx::query("UPDATE mobile_device_target_bindings SET enabled=false,revision=revision+1,updated_by=$3,updated_at=clock_timestamp() WHERE id=$1 AND revision=$2")
                    .bind(resource).bind(v.expected_revision).bind(actor.id).execute(&mut *tx).await?.rows_affected();
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

mod tasks;

mod capabilities;

mod onboarding;
