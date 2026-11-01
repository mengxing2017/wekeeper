use super::*;
use crate::distribution::domain::{
    device_control::Credential,
    mobile_execution::field_check::{ClaimInput, FieldResultInput, InvitationInput, RequestInput},
};
// Caller holds the same transaction guard as every phone resource write.
async fn invitation_valid(tx: &mut Tx<'_>, id: Uuid) -> Result<bool> {
    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_field_check_invitations i JOIN mobile_device_target_bindings b ON b.id=i.binding_id JOIN mobile_targets t ON t.id=i.target_id JOIN schools s ON s.id=i.school_id JOIN accounts a ON a.id=i.created_by JOIN school_memberships m ON m.account_id=a.id AND m.school_id=s.id WHERE i.id=$1 AND i.revoked_at IS NULL AND i.expires_at>clock_timestamp() AND b.enabled AND b.revision=i.binding_revision AND t.enabled AND t.archived_at IS NULL AND t.locator_revision=i.target_locator_revision AND s.active AND a.active AND m.role='admin' FOR SHARE OF i,b,t,s,a,m)").bind(id).fetch_one(&mut **tx).await.map_err(Into::into)
}
async fn opening(tx: &mut Tx<'_>, id: Uuid) -> Result<Value> {
    sqlx::query_scalar("SELECT to_jsonb(o)||jsonb_build_object('credential_generation',d.credential_generation,'consent_revision',c.revision,'configuration_digest',k.configuration_digest,'configuration_name',k.name,'scope_digest',c.scope_digest,'flow',v.flow,'flow_digest',r.content_digest,'environment',e.snapshot) FROM mobile_capability_openings o JOIN execution_devices d ON d.id=o.device_id JOIN mobile_device_capabilities k ON k.id=o.capability_id AND k.revision=o.configuration_revision JOIN mobile_device_capability_versions v ON v.capability_id=k.id AND v.revision=k.revision JOIN mobile_adapter_releases r ON r.id=o.release_id JOIN mobile_device_environments e ON e.id=o.environment_snapshot_id AND e.credential_generation=d.credential_generation JOIN mobile_device_environment_observations obs ON obs.device_id=d.id AND obs.snapshot_id=e.id JOIN mobile_device_capability_consents c ON c.capability_id=k.id AND c.configuration_revision=k.revision AND c.credential_generation=d.credential_generation AND c.accepted WHERE o.id=$1 AND o.state='awaiting_consent' AND d.enabled AND obs.observed_at>clock_timestamp()-interval '5 minutes' AND NOT EXISTS(SELECT 1 FROM mobile_adapter_withdrawals w WHERE w.release_id=r.id) FOR SHARE OF o,d,k,v,r,e,obs,c")
 .bind(id).fetch_optional(&mut **tx).await?.ok_or(Error::Policy("field_check_dependencies_changed"))
}
fn uid(v: &Value, key: &str) -> Result<Uuid> {
    serde_json::from_value(v[key].clone()).map_err(|_| Error::Conflict)
}
fn number(v: &Value, key: &str) -> Result<i64> {
    v[key].as_i64().ok_or(Error::Conflict)
}
async fn current(tx: &mut Tx<'_>, j: &Value) -> Result<bool> {
    if !invitation_valid(tx, uid(j, "invitation_id")?).await? {
        return Ok(false);
    }
    let o = match opening(tx, uid(j, "opening_id")?).await {
        Ok(o) => o,
        Err(Error::Policy(_)) => return Ok(false),
        Err(e) => return Err(e),
    };
    let invitation = object(
        tx,
        "mobile_field_check_invitations",
        uid(j, "invitation_id")?,
    )
    .await?;
    Ok(invitation["revision"] == j["invitation_revision"]
        && o["device_id"] == j["device_id"]
        && [
            "configuration_revision",
            "environment_snapshot_id",
            "consent_revision",
            "credential_generation",
        ]
        .iter()
        .all(|k| o[k] == j[k]))
}
pub(super) async fn inspect(tx: &mut Tx<'_>, mut row: Value, invitation: bool) -> Result<Value> {
    if invitation {
        let used: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM mobile_field_checks WHERE invitation_id=$1)",
        )
        .bind(uid(&row, "id")?)
        .fetch_one(&mut **tx)
        .await?;
        row["available"] = json!(!used && invitation_valid(tx, uid(&row, "id")?).await?);
        row["consumed"] = json!(used);
    } else {
        row["dependencies_current"] = json!(current(tx, &row).await?);
    }
    Ok(row)
}
pub(super) async fn latest(tx: &mut Tx<'_>, id: Uuid) -> Result<Option<Value>> {
    let job:Option<Value>=sqlx::query_scalar("SELECT to_jsonb(j)-'challenge' FROM mobile_field_checks j WHERE opening_id=$1 ORDER BY created_at DESC,id DESC LIMIT 1").bind(id).fetch_optional(&mut **tx).await?;
    match job {
        Some(j) => Ok(Some(inspect(tx, j, false).await?)),
        None => Ok(None),
    }
}
pub(super) async fn write(
    tx: &mut Tx<'_>,
    actor: Uuid,
    school: Option<Uuid>,
    id: Uuid,
    action: &str,
    input: &Value,
) -> Result<Value> {
    match action {
        "field-invitation" => {
            let school = school.ok_or(Error::Forbidden)?;
            let v: InvitationInput = serde_json::from_value(input.clone())
                .map_err(|_| Error::Invalid("检查邀请字段无效".into()))?;
            let row:Option<(Uuid,Uuid,String)>=sqlx::query_as("SELECT b.device_id,b.target_id,COALESCE(b.search_text_override,t.search_text) FROM mobile_device_target_bindings b JOIN mobile_targets t ON t.id=b.target_id JOIN execution_devices d ON d.id=b.device_id WHERE b.id=$1 AND b.school_id=$2 AND b.enabled AND b.revision=$3 AND t.enabled AND t.archived_at IS NULL AND t.locator_revision=$4 AND d.enabled FOR SHARE OF b,t,d")
 .bind(v.binding_id).bind(school).bind(v.expected_binding_revision).bind(v.expected_target_locator_revision).fetch_optional(&mut **tx).await?;
            let (device, target, text) = row.ok_or(Error::Policy("revision_conflict"))?;
            let previous:Option<Value>=sqlx::query_scalar("SELECT to_jsonb(i) FROM mobile_field_check_invitations i WHERE binding_id=$1 AND binding_revision=$2 AND target_locator_revision=$3 AND revoked_at IS NULL AND expires_at>clock_timestamp() AND created_by=$4 AND NOT EXISTS(SELECT 1 FROM mobile_field_checks j WHERE j.invitation_id=i.id) ORDER BY created_at DESC,id DESC LIMIT 1")
 .bind(v.binding_id).bind(v.expected_binding_revision).bind(v.expected_target_locator_revision).bind(actor).fetch_optional(&mut **tx).await?;
            if let Some(result) = previous {
                return Ok(result);
            }
            sqlx::query("INSERT INTO mobile_field_check_invitations(id,school_id,device_id,target_id,binding_id,binding_revision,target_locator_revision,search_text,created_by) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)")
 .bind(id).bind(school).bind(device).bind(target).bind(v.binding_id).bind(v.expected_binding_revision).bind(v.expected_target_locator_revision).bind(text).bind(actor).execute(&mut **tx).await?;
            object(tx, "mobile_field_check_invitations", id).await
        }
        "revoke-field-invitation" => {
            let v: ClaimInput = serde_json::from_value(input.clone())
                .map_err(|_| Error::Invalid("撤销邀请字段无效".into()))?;
            let n=sqlx::query("UPDATE mobile_field_check_invitations SET revoked_at=clock_timestamp(),revision=revision+1 WHERE id=$1 AND school_id=$2 AND revision=$3 AND revoked_at IS NULL")
 .bind(id).bind(school.ok_or(Error::Forbidden)?).bind(v.expected_revision).execute(&mut **tx).await?.rows_affected();
            if n != 1 {
                return Err(Error::Policy("revision_conflict"));
            }
            // Do not free an in-flight physical lease: the device may still be stopping.
            object(tx, "mobile_field_check_invitations", id).await
        }
        "request-field-check" => {
            if school.is_some() {
                return Err(Error::Forbidden);
            }
            let v: RequestInput = serde_json::from_value(input.clone())
                .map_err(|_| Error::Invalid("现场检查字段无效".into()))?;
            let o = opening(tx, v.opening_id).await?;
            let i = object(tx, "mobile_field_check_invitations", v.invitation_id).await?;
            if i["device_id"] != o["device_id"]
                || number(&i, "revision")? != v.expected_invitation_revision
                || !invitation_valid(tx, v.invitation_id).await?
            {
                return Err(Error::Policy("field_invitation_invalid"));
            }
            let device = uid(&o, "device_id")?;
            sqlx::query("UPDATE mobile_field_checks SET state='expired',revision=revision+1 WHERE device_id=$1 AND state IN ('pending','leased') AND expires_at<=clock_timestamp() AND NOT EXISTS(SELECT 1 FROM mobile_shared_device_leases l WHERE l.holder_id=mobile_field_checks.id AND l.expires_at>clock_timestamp())").bind(device).execute(&mut **tx).await?;
            // Unclaimed stale work is safe to invalidate; never release a running lease here.
            let pending:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(j) FROM mobile_field_checks j WHERE device_id=$1 AND state='pending'").bind(device).fetch_all(&mut **tx).await?;
            for job in pending {
                if !current(tx, &job).await? {
                    sqlx::query("UPDATE mobile_field_checks SET state='stale',revision=revision+1 WHERE id=$1 AND state='pending'").bind(uid(&job,"id")?).execute(&mut **tx).await?;
                }
            }
            let previous:Option<Value>=sqlx::query_scalar("SELECT to_jsonb(j)-'challenge' FROM mobile_field_checks j WHERE device_id=$1 AND state IN ('pending','leased')").bind(device).fetch_optional(&mut **tx).await?;
            if let Some(result) = previous {
                if result["opening_id"] == json!(v.opening_id)
                    && result["invitation_id"] == json!(v.invitation_id)
                {
                    return Ok(result);
                }
                return Err(Error::Policy("field_check_already_pending"));
            }
            let used: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM mobile_field_checks WHERE invitation_id=$1)",
            )
            .bind(v.invitation_id)
            .fetch_one(&mut **tx)
            .await?;
            if used {
                return Err(Error::Policy("field_invitation_consumed"));
            }
            sqlx::query("INSERT INTO mobile_field_checks(id,device_id,opening_id,school_id,invitation_id,invitation_revision,configuration_revision,environment_snapshot_id,consent_revision,credential_generation,challenge,created_by) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)")
 .bind(id).bind(device).bind(v.opening_id).bind(uid(&i,"school_id")?).bind(v.invitation_id).bind(v.expected_invitation_revision).bind(number(&o,"configuration_revision")?).bind(uid(&o,"environment_snapshot_id")?).bind(number(&o,"consent_revision")?).bind(number(&o,"credential_generation")?).bind(Uuid::new_v4().simple().to_string()).bind(actor).execute(&mut **tx).await?;
            let mut result = object(tx, "mobile_field_checks", id).await?;
            result
                .as_object_mut()
                .ok_or(Error::Conflict)?
                .remove("challenge");
            Ok(result)
        }
        _ => Err(Error::NotFound),
    }
}
impl Repository {
    pub(super) async fn field_check_jobs(&self, c: &Credential) -> Result<Vec<Value>> {
        let mut tx = self.0.begin().await?;
        super::consent::authenticate(&mut tx, c).await?;
        let rows:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(j)-'challenge' FROM mobile_field_checks j WHERE device_id=$1 AND credential_generation=$2 AND state='pending' AND expires_at>clock_timestamp() ORDER BY created_at,id LIMIT 30").bind(c.device_id).bind(c.generation).fetch_all(&mut *tx).await?;
        let mut out = vec![];
        for mut j in rows {
            if current(&mut tx, &j).await? {
                let i = object(
                    &mut tx,
                    "mobile_field_check_invitations",
                    uid(&j, "invitation_id")?,
                )
                .await?;
                j["search_text"] = i["search_text"].clone();
                out.push(j);
            }
        }
        tx.commit().await?;
        Ok(out)
    }
    pub(super) async fn field_check_device_write(
        &self,
        c: &Credential,
        id: Uuid,
        key: &str,
        action: &str,
        input: Value,
    ) -> Result<Value> {
        let mut tx = self.0.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(78234610243)")
            .execute(&mut *tx)
            .await?;
        super::consent::authenticate(&mut tx, c).await?;
        let scope = format!("{action}:{id}");
        let payload = hash(&input.to_string());
        let j:Value=sqlx::query_scalar("SELECT to_jsonb(j) FROM mobile_field_checks j WHERE id=$1 AND device_id=$2 AND credential_generation=$3 FOR UPDATE").bind(id).bind(c.device_id).bind(c.generation).fetch_optional(&mut *tx).await?.ok_or(Error::Forbidden)?;
        let previous:Option<(String,Value)>=sqlx::query_as("SELECT payload_hash,result FROM mobile_field_check_operations WHERE device_id=$1 AND credential_generation=$2 AND scope=$3 AND idempotency_key=$4")
 .bind(c.device_id).bind(c.generation).bind(&scope).bind(key).fetch_optional(&mut *tx).await?;
        if let Some((old, result)) = previous {
            return if old == payload {
                Ok(result)
            } else {
                Err(Error::Conflict)
            };
        }
        let result = match action {
            "claim" => {
                let v: ClaimInput = serde_json::from_value(input)
                    .map_err(|_| Error::Invalid("领取检查字段无效".into()))?;
                let live: bool = sqlx::query_scalar(
                    "SELECT expires_at>clock_timestamp() FROM mobile_field_checks WHERE id=$1",
                )
                .bind(id)
                .fetch_one(&mut *tx)
                .await?;
                if j["state"] != "pending"
                    || number(&j, "revision")? != v.expected_revision
                    || !live
                    || !current(&mut tx, &j).await?
                {
                    return Err(Error::Policy("field_check_dependencies_changed"));
                }
                // Legacy reservations and ambiguous outcomes remain barriers, irrespective of their ages.
                let busy:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_shared_runtime WHERE device_id=$1 AND NOT quiescent) OR EXISTS(SELECT 1 FROM mobile_shared_send_claims WHERE device_id=$1 AND state='active') OR EXISTS(SELECT 1 FROM mobile_shared_invocations WHERE device_id=$1 AND state='risk_held') OR EXISTS(SELECT 1 FROM mobile_execution_claims c JOIN mobile_delivery_items i ON i.id=c.item_id WHERE c.device_id=$1 AND (c.state IN ('claimed','prepared','permit_issued') OR i.state='unknown')) OR EXISTS(SELECT 1 FROM automation_flow_validations WHERE device_id=$1 AND state='accepted' AND expires_at>clock_timestamp()) OR EXISTS(SELECT 1 FROM device_validation_jobs WHERE device_id=$1 AND state IN ('accepted','running') AND expires_at>clock_timestamp()) OR EXISTS(SELECT 1 FROM mobile_shared_device_leases WHERE device_id=$1 AND expires_at>clock_timestamp())")
 .bind(c.device_id).fetch_one(&mut *tx).await?;
                if busy {
                    return Err(Error::Policy("physical_device_busy"));
                }
                let fence:i64=sqlx::query_scalar("INSERT INTO mobile_shared_device_leases(device_id,fence,holder_id,purpose,expires_at) VALUES($1,1,$2,'field_check',LEAST((SELECT expires_at FROM mobile_field_checks WHERE id=$2),clock_timestamp()+interval '240 seconds')) ON CONFLICT(device_id) DO UPDATE SET fence=mobile_shared_device_leases.fence+1,holder_id=$2,purpose='field_check',expires_at=LEAST((SELECT expires_at FROM mobile_field_checks WHERE id=$2),clock_timestamp()+interval '240 seconds') RETURNING fence")
 .bind(c.device_id).bind(id).fetch_one(&mut *tx).await?;
                sqlx::query("UPDATE mobile_field_checks SET state='leased',revision=revision+1,lease_fence=$2 WHERE id=$1").bind(id).bind(fence).execute(&mut *tx).await?;
                let o = opening(&mut tx, uid(&j, "opening_id")?).await?;
                let i = object(
                    &mut tx,
                    "mobile_field_check_invitations",
                    uid(&j, "invitation_id")?,
                )
                .await?;
                let timing:Value=sqlx::query_scalar("SELECT jsonb_build_object('server_time',clock_timestamp(),'expires_at',expires_at) FROM mobile_shared_device_leases WHERE device_id=$1").bind(c.device_id).fetch_one(&mut *tx).await?;
                json!({"id":id,"device_id":c.device_id,"credential_generation":c.generation,"lease_fence":fence,"challenge":j["challenge"],"timing":timing,"flow":o["flow"],"flow_digest":o["flow_digest"],"environment":o["environment"],"capability_id":o["capability_id"],"configuration_revision":j["configuration_revision"],"scope_digest":o["scope_digest"],"consent_revision":j["consent_revision"],"search_text":i["search_text"]})
            }
            "result" => {
                let v: FieldResultInput = serde_json::from_value(input)
                    .map_err(|_| Error::Invalid("现场检查结果字段无效".into()))?;
                v.validate()?;
                if !matches!(j["state"].as_str(), Some("leased" | "expired"))
                    || j["challenge"] != v.challenge
                    || j["lease_fence"] != v.lease_fence
                {
                    return Err(Error::Policy("field_check_permit_mismatch"));
                }
                let existing: Option<Value> = sqlx::query_scalar(
                    "SELECT to_jsonb(r) FROM mobile_field_check_results r WHERE id=$1",
                )
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?;
                if existing.is_some() {
                    return Err(Error::Conflict);
                }
                let live:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_shared_device_leases WHERE device_id=$1 AND holder_id=$2 AND fence=$3 AND expires_at>clock_timestamp())").bind(c.device_id).bind(id).bind(v.lease_fence).fetch_one(&mut *tx).await?;
                let disposition = if !live || !current(&mut tx, &j).await? {
                    "stale"
                } else if v.passed() {
                    "passed"
                } else {
                    "failed"
                };
                let body = serde_json::to_value(&v).map_err(|_| Error::Conflict)?;
                sqlx::query("INSERT INTO mobile_field_check_results(id,device_id,body,disposition) VALUES($1,$2,$3,$4)").bind(id).bind(c.device_id).bind(&body).bind(disposition).execute(&mut *tx).await?;
                sqlx::query(
                    "UPDATE mobile_field_checks SET state=$2,revision=revision+1 WHERE id=$1",
                )
                .bind(id)
                .bind(disposition)
                .execute(&mut *tx)
                .await?;
                // Keep the fence monotonic. Release only this holder, never a successor's lease.
                sqlx::query("UPDATE mobile_shared_device_leases SET expires_at=LEAST(expires_at,clock_timestamp()) WHERE device_id=$1 AND holder_id=$2 AND fence=$3").bind(c.device_id).bind(id).bind(v.lease_fence).execute(&mut *tx).await?;
                json!({"id":id,"disposition":disposition,"body":body})
            }
            _ => return Err(Error::NotFound),
        };
        sqlx::query("INSERT INTO mobile_field_check_operations(device_id,credential_generation,scope,idempotency_key,payload_hash,result) VALUES($1,$2,$3,$4,$5,$6)")
 .bind(c.device_id).bind(c.generation).bind(scope).bind(key).bind(payload).bind(&result).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(result)
    }
}
