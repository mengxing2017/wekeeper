//! Deployment-only signed product delivery. No browser session can import or publish a release.
use super::*;
use crate::distribution::domain::mobile_execution::FlowV3;
use serde::{Deserialize, Serialize};
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Delivery {
    pub name: String,
    pub version: String,
    pub flow: FlowV3,
    pub measurement_source: String,
    pub measured_at: chrono::DateTime<chrono::Utc>,
    pub samples_ms: Vec<i64>,
    pub evidence: Vec<DeliveryArtifact>,
    pub apks: Vec<DeliveryArtifact>,
    pub validation_evidence: Value,
    pub cases: std::collections::BTreeMap<String, bool>,
    pub soak_hours: i64,
    pub soak_passed: bool,
    pub historical_debt_closed: bool,
    pub reviewer: String,
    pub policy: PolicyInput,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeliveryArtifact {
    pub name: String,
    pub content_hex: String,
    pub sha256: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Envelope {
    pub payload: Value,
    pub signature_hex: String,
}
pub fn unhex(s: &str) -> Result<Vec<u8>> {
    require(
        s.len().is_multiple_of(2) && s.len() <= 268435456,
        "invalid_hex",
    )?;
    (0..s.len())
        .step_by(2)
        .map(|i| {
            s.get(i..i + 2)
                .and_then(|v| u8::from_str_radix(v, 16).ok())
                .ok_or(Error::Policy("invalid_hex"))
        })
        .collect()
}
pub fn verify(envelope: Envelope, key: &str) -> Result<Delivery> {
    let public = unhex(key)?;
    let sig = unhex(&envelope.signature_hex)?;
    ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, public)
        .verify(
            &serde_json::to_vec(&envelope.payload).map_err(|_| Error::Conflict)?,
            &sig,
        )
        .map_err(|_| Error::Policy("delivery_signature_invalid"))?;
    let d: Delivery =
        serde_json::from_value(envelope.payload).map_err(|e| Error::Invalid(e.to_string()))?;
    d.validate()?;
    Ok(d)
}
impl Delivery {
    pub fn validate(&self) -> Result<()> {
        self.flow.validate()?;
        self.policy.validate()?;
        require(
            self.policy.allowed_stages == ["production_allowlist"] && self.policy.mode == "enabled",
            "delivery_policy_invalid",
        )?;
        crate::business::text(&self.name, 1, 200)?;
        crate::business::text(&self.version, 1, 100)?;
        crate::business::text(&self.reviewer, 1, 200)?;
        crate::business::text(&self.measurement_source, 1, 500)?;
        require(
            self.measured_at <= chrono::Utc::now()
                && self.samples_ms.len() >= 3
                && self.samples_ms.len() <= 100
                && self.samples_ms.iter().all(|v| *v >= 1000)
                && self.samples_ms.iter().max().copied().unwrap_or(i64::MAX)
                    <= self.flow.pacing.between_targets_min_ms,
            "delivery_baseline_invalid",
        )?;
        require(
            !self.evidence.is_empty()
                && self.evidence.len() <= 100
                && !self.apks.is_empty()
                && self.apks.len() <= 20
                && self.validation_evidence["artifact_sha256s"]
                    .as_array()
                    .is_some_and(|refs| {
                        !refs.is_empty()
                            && refs
                                .iter()
                                .all(|v| self.evidence.iter().any(|a| v == &a.sha256))
                    })
                && self.validation_evidence["launch_paths"]
                    .as_array()
                    .is_some_and(|paths| {
                        ["search", "all_apps"]
                            .iter()
                            .all(|p| paths.contains(&json!(p)))
                    })
                && self.validation_evidence["apk_sha256s"]
                    .as_array()
                    .is_some_and(|refs| self.apks.iter().all(|a| refs.contains(&json!(a.sha256)))),
            "delivery_evidence_required",
        )?;
        require(
            self.soak_hours >= 72
                && self.soak_passed
                && self.historical_debt_closed
                && (1..=40).all(|n| self.cases.get(&format!("S{n:02}")) == Some(&true))
                && (1..=14).all(|n| self.cases.get(&format!("T{n:02}")) == Some(&true)),
            "delivery_acceptance_required",
        )?;
        for a in self.evidence.iter().chain(&self.apks) {
            let bytes = unhex(&a.content_hex)?;
            require(
                !bytes.is_empty()
                    && bytes.len() <= 128 * 1024 * 1024
                    && hash_bytes(&bytes) == a.sha256,
                "delivery_artifact_invalid",
            )?;
        }
        for apk in &self.apks {
            require(
                super::platform::apk_structure(&unhex(&apk.content_hex)?),
                "shared_send_apk_invalid",
            )?;
        }
        Ok(())
    }
}
pub async fn import(pool: &PgPool, actor: Uuid, d: Delivery) -> Result<Uuid> {
    d.validate()?;
    let mut tx = pool.begin().await?;
    guard(&mut tx).await?;
    service_identity(&mut tx, actor).await?;
    let current = policy(&mut tx)
        .await?
        .and_then(|p| p["revision"].as_i64())
        .unwrap_or(0);
    require(current == d.policy.expected_revision, "revision_conflict")?;
    let mut evidence = Vec::new();
    let mut apk_ids = Vec::new();
    for (kind, list) in [("evidence", &d.evidence), ("apk", &d.apks)] {
        for a in list {
            let id = Uuid::now_v7();
            let bytes = unhex(&a.content_hex)?;
            if kind == "apk" {
                require(
                    super::platform::apk_structure(&bytes),
                    "shared_send_apk_invalid",
                )?;
            }
            sqlx::query("INSERT INTO mobile_v9_artifacts(id,kind,name,sha256,content,created_by) VALUES($1,$2,$3,$4,$5,$6)").bind(id).bind(kind).bind(&a.name).bind(&a.sha256).bind(bytes).bind(actor).execute(&mut *tx).await?;
            if kind == "apk" {
                apk_ids.push(id)
            } else {
                evidence.push(id)
            }
        }
    }
    let flow = Uuid::now_v7();
    let baseline = Uuid::now_v7();
    let release = Uuid::now_v7();
    let acceptance = Uuid::now_v7();
    let f = json!(d.flow);
    let digest = digest(&f)?;
    sqlx::query(
        "INSERT INTO mobile_v9_flows(id,name,flow,digest,created_by) VALUES($1,$2,$3,$4,$5)",
    )
    .bind(flow)
    .bind(&d.name)
    .bind(&f)
    .bind(&digest)
    .bind(actor)
    .execute(&mut *tx)
    .await?;
    let provenance = json!({"measurement_source":d.measurement_source,"measured_at":d.measured_at,"reviewer":d.reviewer,"validation_evidence":d.validation_evidence,"signature_verified":true});
    sqlx::query("INSERT INTO mobile_v9_baselines(id,flow_id,device_id,samples,evidence_ids,min_interval_ms,recorded_by,external_source) VALUES($1,$2,NULL,$3,$4,$5,$6,$7)").bind(baseline).bind(flow).bind(json!(d.samples_ms)).bind(&evidence).bind(d.samples_ms.iter().max().copied().unwrap()).bind(actor).bind(&provenance).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO mobile_v9_adapter_releases(id,name,version,flow_id,flow,content_digest,baseline_id,evidence_ids,created_by) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)").bind(release).bind(&d.name).bind(&d.version).bind(flow).bind(&f).bind(&digest).bind(baseline).bind(&evidence).bind(actor).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO mobile_v9_acceptance_runs(id,release_id,stage,apk_id,evidence_ids,validation_ids,cases,soak_hours,soak_passed,historical_debt_closed,reviewer_id,reason) VALUES($1,$2,'production_allowlist',$3,$4,'{}',$5,$6,true,true,$7,'核验签名的外部产品交付；原始验证见交付来源')").bind(acceptance).bind(release).bind(apk_ids[0]).bind(&evidence).bind(json!(d.cases)).bind(d.soak_hours).bind(actor).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO mobile_capability_deliveries(release_id,acceptance_id,supported_apks,provenance,policy) VALUES($1,$2,$3,$4,$5)").bind(release).bind(acceptance).bind(d.apks.iter().map(|a|a.sha256.clone()).collect::<Vec<_>>()).bind(provenance).bind(json!(d.policy)).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(release)
}
async fn service_identity(tx: &mut Tx<'_>, id: Uuid) -> Result<()> {
    require(sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM accounts WHERE id=$1 AND NOT active AND NOT platform_admin)").bind(id).fetch_one(&mut **tx).await?,"delivery_service_identity_required")
}
pub async fn publish(
    pool: &PgPool,
    actor: Uuid,
    release: Uuid,
    revision: i64,
    withdraw: bool,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    guard(&mut tx).await?;
    service_identity(&mut tx, actor).await?;
    if withdraw {
        sqlx::query("INSERT INTO mobile_v9_adapter_withdrawals(release_id,reason,created_by) VALUES($1,'产品交付撤回',$2) ON CONFLICT DO NOTHING").bind(release).bind(actor).execute(&mut *tx).await?;
    } else {
        require(sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM mobile_capability_deliveries d JOIN mobile_v9_acceptance_runs c ON c.id=d.acceptance_id AND c.release_id=d.release_id WHERE d.release_id=$1 AND c.stage='production_allowlist') AND NOT EXISTS(SELECT 1 FROM mobile_v9_adapter_withdrawals WHERE release_id=$1)").bind(release).fetch_one(&mut *tx).await?,"delivery_acceptance_required")?;
        let p: PolicyInput = serde_json::from_value(
            sqlx::query_scalar::<_, Value>(
                "SELECT policy FROM mobile_capability_deliveries WHERE release_id=$1",
            )
            .bind(release)
            .fetch_one(&mut *tx)
            .await?,
        )
        .map_err(|_| Error::Conflict)?;
        let current = policy(&mut tx)
            .await?
            .and_then(|p| p["revision"].as_i64())
            .unwrap_or(0);
        require(current == p.expected_revision, "policy_revision_conflict")?;
        let valid: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_timezone_names WHERE name=$1)")
                .bind(&p.timezone)
                .fetch_one(&mut *tx)
                .await?;
        require(valid, "时区无效")?;
        sqlx::query("INSERT INTO mobile_v9_send_policy_versions(id,revision,mode,limits,created_by,reason,timezone,allowed_windows,allowed_stages,confirmation_seconds,inspection_seconds,retry_seconds,retry_limit) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)").bind(Uuid::now_v7()).bind(current+1).bind(p.mode).bind(json!(p.limits)).bind(actor).bind(p.reason).bind(p.timezone).bind(json!(p.allowed_windows)).bind(json!(p.allowed_stages)).bind(p.confirmation_seconds).bind(p.inspection_seconds).bind(p.retry_seconds).bind(p.retry_limit).execute(&mut *tx).await?;
        let rev: i64 = sqlx::query_scalar(
            "SELECT revision FROM mobile_v9_activations WHERE capability='wecom.group_text'",
        )
        .fetch_optional(&mut *tx)
        .await?
        .unwrap_or(0);
        require(rev == revision, "revision_conflict")?;
        sqlx::query("INSERT INTO mobile_v9_activations(capability,release_id,revision,updated_by) VALUES('wecom.group_text',$1,$2,$3) ON CONFLICT(capability) DO UPDATE SET release_id=$1,revision=$2,updated_by=$3").bind(release).bind(rev+1).bind(actor).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO mobile_v9_activation_events(id,release_id,revision,actor_id,reason) VALUES($1,$2,$3,$4,'产品交付发布')").bind(Uuid::now_v7()).bind(release).bind(rev+1).bind(actor).execute(&mut *tx).await?;
    }
    super::usage::reconcile(&mut tx).await?;
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::signature::KeyPair;
    fn hex(b: &[u8]) -> String {
        b.iter().map(|v| format!("{v:02x}")).collect()
    }
    #[test]
    fn delivery_requires_trusted_signature_and_real_evidence_not_default_passes() {
        let signer = ring::signature::Ed25519KeyPair::from_seed_unchecked(&[7; 32]).unwrap();
        let fixture: Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../api-contract/fixtures/shared-mobile-send-v9.json"
        )))
        .unwrap();
        let payload = json!({"flow":fixture["flow"]});
        let signature = signer.sign(&serde_json::to_vec(&payload).unwrap());
        let signed = || Envelope {
            payload: payload.clone(),
            signature_hex: hex(signature.as_ref()),
        };
        assert!(
            matches!(
                verify(signed(), &hex(signer.public_key().as_ref())),
                Err(Error::Invalid(_))
            ),
            "A valid signature cannot supply missing measurement or acceptance evidence"
        );
        assert!(verify(signed(), &hex(&[8; 32])).is_err());
        let mut tampered = signed();
        tampered.payload["name"] = json!("changed");
        assert!(verify(tampered, &hex(signer.public_key().as_ref())).is_err());
        assert!(unhex("not-hex").is_err());
    }
}
