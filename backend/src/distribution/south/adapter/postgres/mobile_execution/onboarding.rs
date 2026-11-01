use super::*;
use crate::distribution::domain::{
    device_control::Credential,
    mobile_execution::onboarding::{
        self, CancelInput, Environment, OpeningInput, SupportInput, SupportUpdateInput,
    },
};

pub(super) const OPENINGS_QUERY: &str = "SELECT jsonb_build_object('id',d.id,'device_name',d.name,'device_enabled',d.enabled,'online',d.enabled AND p.state='online' AND p.last_control_heartbeat_at>clock_timestamp()-interval '90 seconds','environment',to_jsonb(e),'observed_at',obs.observed_at,'environment_fresh',COALESCE(e.credential_generation=d.credential_generation AND obs.observed_at>clock_timestamp()-interval '5 minutes',false),'configuration_revision',COALESCE(k.revision,0),'opening',to_jsonb(o),'consent_current',COALESCE(c.accepted AND c.configuration_revision=k.revision AND c.credential_generation=d.credential_generation,false),'support_request',(SELECT to_jsonb(s) FROM mobile_adapter_support_requests s WHERE s.device_id=d.id ORDER BY s.created_at DESC,s.id DESC LIMIT 1)) FROM execution_devices d LEFT JOIN device_control_presence p ON p.id=d.id LEFT JOIN mobile_device_environment_observations obs ON obs.device_id=d.id LEFT JOIN mobile_device_environments e ON e.id=obs.snapshot_id LEFT JOIN mobile_device_capabilities k ON k.device_id=d.id LEFT JOIN mobile_device_capability_consents c ON c.capability_id=k.id LEFT JOIN mobile_capability_openings o ON o.device_id=d.id AND o.state='awaiting_consent' WHERE $1::uuid IS NULL AND strpos(lower(d.name),lower($2))>0 AND ($3::uuid IS NULL OR d.id=$3) ORDER BY d.created_at,d.id LIMIT $4 OFFSET $5";
pub(super) async fn enrich(tx: &mut Tx<'_>, rows: Vec<Value>) -> Result<Vec<Value>> {
    let releases: Vec<Value> = sqlx::query_scalar("SELECT to_jsonb(r)-'flow' FROM mobile_adapter_releases r WHERE NOT EXISTS(SELECT 1 FROM mobile_adapter_withdrawals w WHERE w.release_id=r.id) ORDER BY r.id").fetch_all(&mut **tx).await?;
    let mut enriched: Vec<Value> = rows
        .into_iter()
        .map(|mut row| {
            let env =
                serde_json::from_value::<Environment>(row["environment"]["snapshot"].clone()).ok();
            row["release_count"] = json!(releases.len());
            row["matching_releases"] = json!(
                releases
                    .iter()
                    .filter(|r| env.as_ref().is_some_and(|e| e.matches(&r["environment"])))
                    .collect::<Vec<_>>()
            );
            row["release_available"] = json!(
                releases
                    .iter()
                    .any(|r| r["id"] == row["opening"]["release_id"])
            );
            if row["environment_fresh"] != true
                || row["release_available"] != true
                || row["opening"].is_null()
                || row["environment"]["id"] != row["opening"]["environment_snapshot_id"]
            {
                row["consent_current"] = json!(false);
            }
            row
        })
        .collect();
    for row in &mut enriched {
        let latest = if let Ok(id) = serde_json::from_value::<Uuid>(row["opening"]["id"].clone()) {
            super::field_checks::latest(tx, id).await?
        } else {
            None
        };
        row["field_check_current_passed"] = json!(
            latest
                .as_ref()
                .is_some_and(|j| j["state"] == "passed" && j["dependencies_current"] == true)
        );
        *row = onboarding::progress(row.clone());
    }
    Ok(enriched)
}
pub(super) async fn write(
    tx: &mut Tx<'_>,
    actor: Uuid,
    id: Uuid,
    action: &str,
    input: &Value,
) -> Result<Value> {
    match action {
        "update-adapter-support" => {
            let v: SupportUpdateInput = serde_json::from_value(input.clone())
                .map_err(|_| Error::Invalid("支持处理字段无效".into()))?;
            validate_text(&v.response, 1, 2000, 8000)?;
            if !matches!(v.state.as_str(), "investigating" | "resolved" | "declined") {
                return Err(Error::Invalid("支持处理状态无效".into()));
            }
            let n=sqlx::query("UPDATE mobile_adapter_support_requests SET state=$3,response=$4,revision=revision+1,updated_by=$5,updated_at=clock_timestamp() WHERE id=$1 AND revision=$2 AND state IN ('submitted','investigating')")
                .bind(id).bind(v.expected_revision).bind(v.state).bind(v.response).bind(actor).execute(&mut **tx).await?.rows_affected();
            if n != 1 {
                return Err(Error::Policy("revision_conflict"));
            }
            object(tx, "mobile_adapter_support_requests", id).await
        }
        "adapter-support" => {
            let v: SupportInput = serde_json::from_value(input.clone())
                .map_err(|_| Error::Invalid("支持请求字段无效".into()))?;
            validate_text(&v.note, 1, 2000, 8000)?;
            sqlx::query("SELECT id FROM execution_devices WHERE id=$1 FOR SHARE")
                .bind(v.device_id)
                .fetch_optional(&mut **tx)
                .await?
                .ok_or(Error::NotFound)?;
            // Refresh and a second administrator resume the same unresolved request.
            let previous:Option<Value>=sqlx::query_scalar("SELECT to_jsonb(s) FROM mobile_adapter_support_requests s WHERE device_id=$1 AND state IN ('submitted','investigating') FOR UPDATE")
                .bind(v.device_id).fetch_optional(&mut **tx).await?;
            if let Some(result) = previous {
                return Ok(result);
            }
            sqlx::query("INSERT INTO mobile_adapter_support_requests(id,device_id,environment_snapshot_id,note,created_by,updated_by) VALUES($1,$2,(SELECT snapshot_id FROM mobile_device_environment_observations WHERE device_id=$2),$3,$4,$4)")
                .bind(id).bind(v.device_id).bind(v.note).bind(actor).execute(&mut **tx).await?;
            object(tx, "mobile_adapter_support_requests", id).await
        }
        "cancel-opening" => {
            let v: CancelInput = serde_json::from_value(input.clone())
                .map_err(|_| Error::Invalid("取消申请字段无效".into()))?;
            let n=sqlx::query("UPDATE mobile_capability_openings SET state='cancelled',revision=revision+1,updated_at=clock_timestamp() WHERE id=$1 AND revision=$2 AND state='awaiting_consent'")
                .bind(id).bind(v.expected_revision).execute(&mut **tx).await?.rows_affected();
            if n != 1 {
                return Err(Error::Policy("revision_conflict"));
            }
            object(tx, "mobile_capability_openings", id).await
        }
        "open-capability" => {
            let v: OpeningInput = serde_json::from_value(input.clone()).map_err(|_| {
                Error::Invalid("开通申请只接受发行适配与本机环境引用，不接受规则文件".into())
            })?;
            let generation:i64=sqlx::query_scalar("SELECT credential_generation FROM execution_devices WHERE id=$1 AND enabled FOR SHARE")
                .bind(v.device_id).fetch_optional(&mut **tx).await?.ok_or(Error::NotFound)?;
            let env:Value=sqlx::query_scalar("SELECT e.snapshot FROM mobile_device_environments e JOIN mobile_device_environment_observations obs ON obs.snapshot_id=e.id AND obs.device_id=e.device_id WHERE e.id=$1 AND e.device_id=$2 AND e.credential_generation=$3 AND obs.observed_at>clock_timestamp()-interval '5 minutes' FOR SHARE OF obs")
                .bind(v.environment_snapshot_id).bind(v.device_id).bind(generation).fetch_optional(&mut **tx).await?.ok_or(Error::Policy("environment_refresh_required"))?;
            let env: Environment = serde_json::from_value(env).map_err(|_| Error::Conflict)?;
            let release:Value=sqlx::query_scalar("SELECT to_jsonb(r) FROM mobile_adapter_releases r WHERE r.id=$1 AND NOT EXISTS(SELECT 1 FROM mobile_adapter_withdrawals w WHERE w.release_id=r.id) FOR SHARE OF r")
                .bind(v.profile_release_id).fetch_optional(&mut **tx).await?.ok_or(Error::Policy("adapter_release_unavailable"))?;
            if !env.matches(&release["environment"]) {
                return Err(Error::Policy("adapter_environment_mismatch"));
            }
            let matrices:Vec<Value>=sqlx::query_scalar("SELECT r.environment FROM mobile_adapter_releases r WHERE NOT EXISTS(SELECT 1 FROM mobile_adapter_withdrawals w WHERE w.release_id=r.id)").fetch_all(&mut **tx).await?;
            if matrices.iter().filter(|m| env.matches(m)).count() != 1 {
                return Err(Error::Policy("adapter_match_ambiguous"));
            }
            let flow: FlowV2 =
                serde_json::from_value(release["flow"].clone()).map_err(|_| Error::Conflict)?;
            flow.validate_shared()?;
            let flow_digest = hash(&release["flow"].to_string());
            if release["content_digest"] != flow_digest || flow.package_name != env.package_name {
                return Err(Error::Policy("adapter_release_invalid"));
            }
            let previous:Option<Value>=sqlx::query_scalar("SELECT to_jsonb(o) FROM mobile_capability_openings o WHERE device_id=$1 AND state='awaiting_consent' FOR UPDATE").bind(v.device_id).fetch_optional(&mut **tx).await?;
            if let Some(result) = previous {
                if result["release_id"] == json!(v.profile_release_id)
                    && result["environment_snapshot_id"] == json!(v.environment_snapshot_id)
                {
                    return Ok(result);
                }
                return Err(Error::Policy("opening_already_pending"));
            }
            let digest=hash(&json!({"flow":release["flow"],"release_id":v.profile_release_id,"environment_snapshot_id":v.environment_snapshot_id}).to_string());
            let current: Option<(Uuid, i64)> = sqlx::query_as(
                "SELECT id,revision FROM mobile_device_capabilities WHERE device_id=$1 FOR UPDATE",
            )
            .bind(v.device_id)
            .fetch_optional(&mut **tx)
            .await?;
            if current.as_ref().map(|c| c.1).unwrap_or(0) != v.expected_revision {
                return Err(Error::Policy("revision_conflict"));
            }
            let cap = current.map(|c| c.0).unwrap_or_else(Uuid::now_v7);
            let revision = v.expected_revision + 1;
            let name = release["name"].as_str().ok_or(Error::Conflict)?;
            sqlx::query("INSERT INTO mobile_device_capabilities(id,device_id,app_key,capability,name,revision,configuration_digest,enabled,created_by,updated_by) VALUES($1,$2,'wecom','group_text',$3,$4,$5,false,$6,$6) ON CONFLICT(id) DO UPDATE SET name=$3,revision=$4,configuration_digest=$5,enabled=false,updated_by=$6,updated_at=clock_timestamp()")
                .bind(cap).bind(v.device_id).bind(name).bind(revision).bind(&digest).bind(actor).execute(&mut **tx).await?;
            sqlx::query("INSERT INTO mobile_device_capability_versions(capability_id,revision,name,enabled,flow,configuration_digest,created_by) VALUES($1,$2,$3,false,$4,$5,$6)")
                .bind(cap).bind(revision).bind(name).bind(&release["flow"]).bind(digest).bind(actor).execute(&mut **tx).await?;
            sqlx::query("INSERT INTO mobile_capability_openings(id,device_id,capability_id,configuration_revision,release_id,environment_snapshot_id,state,created_by) VALUES($1,$2,$3,$4,$5,$6,'awaiting_consent',$7)")
                .bind(id).bind(v.device_id).bind(cap).bind(revision).bind(v.profile_release_id).bind(v.environment_snapshot_id).bind(actor).execute(&mut **tx).await?;
            object(tx, "mobile_capability_openings", id).await
        }
        _ => Err(Error::NotFound),
    }
}
impl Repository {
    pub(super) async fn device_opening_context(&self, c: &Credential) -> Result<Value> {
        let mut tx = self.0.begin().await?;
        super::consent::authenticate(&mut tx, c).await?;
        let row: Value = sqlx::query_scalar(OPENINGS_QUERY)
            .bind(None::<Uuid>)
            .bind("")
            .bind(c.device_id)
            .bind(1_i64)
            .bind(0_i64)
            .fetch_one(&mut *tx)
            .await?;
        let mut rows = enrich(&mut tx, vec![row]).await?;
        let progress = rows.pop().ok_or(Error::NotFound)?;
        tx.commit().await?;
        Ok(json!({"id":c.device_id,"progress":progress}))
    }

    pub(super) async fn environment(
        &self,
        c: &Credential,
        key: &str,
        input: Environment,
    ) -> Result<Value> {
        input.validate()?;
        let mut tx = self.0.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(78234610243)")
            .execute(&mut *tx)
            .await?;
        super::consent::authenticate(&mut tx, c).await?;
        let snapshot = serde_json::to_value(&input).map_err(|_| Error::Conflict)?;
        let payload = hash(&snapshot.to_string());
        // Environment and consent journals use disjoint scopes even if a caller reuses a key.
        let scoped_key = format!("environment:{key}");
        let previous:Option<(String,Value)>=sqlx::query_as("SELECT payload_hash,result FROM mobile_device_capability_operations WHERE device_id=$1 AND credential_generation=$2 AND idempotency_key=$3")
        .bind(c.device_id).bind(c.generation).bind(&scoped_key).fetch_optional(&mut *tx).await?;
        if let Some((old, result)) = previous {
            return if old == payload {
                Ok(result)
            } else {
                Err(Error::Conflict)
            };
        }
        let existing:Option<Uuid>=sqlx::query_scalar("SELECT e.id FROM mobile_device_environments e JOIN mobile_device_environment_observations obs ON obs.device_id=e.device_id AND obs.snapshot_id=e.id WHERE e.device_id=$1 AND e.credential_generation=$2 AND e.digest=$3")
        .bind(c.device_id).bind(c.generation).bind(&payload).fetch_optional(&mut *tx).await?;
        let id = existing.unwrap_or_else(Uuid::now_v7);
        if existing.is_none() {
            sqlx::query("INSERT INTO mobile_device_environments(id,device_id,credential_generation,snapshot,digest) VALUES($1,$2,$3,$4,$5)")
            .bind(id).bind(c.device_id).bind(c.generation).bind(snapshot).bind(&payload).execute(&mut *tx).await?;
        }
        sqlx::query("INSERT INTO mobile_device_environment_observations(device_id,snapshot_id) VALUES($1,$2) ON CONFLICT(device_id) DO UPDATE SET snapshot_id=$2,observed_at=clock_timestamp()")
        .bind(c.device_id).bind(id).execute(&mut *tx).await?;
        let result = object(&mut tx, "mobile_device_environments", id).await?;
        sqlx::query("INSERT INTO mobile_device_capability_operations(device_id,credential_generation,idempotency_key,payload_hash,result) VALUES($1,$2,$3,$4,$5)")
        .bind(c.device_id).bind(c.generation).bind(scoped_key).bind(payload).bind(&result).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(result)
    }
}
