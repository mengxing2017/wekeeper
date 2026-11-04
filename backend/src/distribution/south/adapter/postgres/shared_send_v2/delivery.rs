//! Deployment-only product delivery. No browser session can import or publish a release.
use super::*;
use crate::distribution::domain::shared_mobile::FlowV3;
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
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeliveryArtifact {
    pub name: String,
    pub content_hex: String,
    pub sha256: String,
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
/// Parse the actual product object; no signature envelope or signing configuration.
pub fn parse(raw: &[u8]) -> Result<Delivery> {
    let d: Delivery = serde_json::from_slice(raw).map_err(|e| Error::Invalid(e.to_string()))?;
    d.validate()?;
    Ok(d)
}
impl Delivery {
    pub fn validate(&self) -> Result<()> {
        self.flow.validate()?;
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
    let provenance = json!({"measurement_source":d.measurement_source,"measured_at":d.measured_at,"reviewer":d.reviewer,"validation_evidence":d.validation_evidence,"import_method":"deployment_cli"});
    sqlx::query("INSERT INTO mobile_v9_baselines(id,flow_id,device_id,samples,evidence_ids,min_interval_ms,recorded_by,external_source) VALUES($1,$2,NULL,$3,$4,$5,$6,$7)").bind(baseline).bind(flow).bind(json!(d.samples_ms)).bind(&evidence).bind(d.samples_ms.iter().max().copied().unwrap()).bind(actor).bind(&provenance).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO mobile_v9_adapter_releases(id,name,version,flow_id,flow,content_digest,baseline_id,evidence_ids,created_by) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)").bind(release).bind(&d.name).bind(&d.version).bind(flow).bind(&f).bind(&digest).bind(baseline).bind(&evidence).bind(actor).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO mobile_v9_acceptance_runs(id,release_id,stage,apk_id,evidence_ids,validation_ids,cases,soak_hours,soak_passed,historical_debt_closed,reviewer_id,reason) VALUES($1,$2,'production_allowlist',$3,$4,'{}',$5,$6,true,true,$7,'受控部署导入的产品交付；原始验证见交付来源')").bind(acceptance).bind(release).bind(apk_ids[0]).bind(&evidence).bind(json!(d.cases)).bind(d.soak_hours).bind(actor).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO mobile_capability_deliveries(release_id,acceptance_id,supported_apks,provenance) VALUES($1,$2,$3,$4)").bind(release).bind(acceptance).bind(d.apks.iter().map(|a|a.sha256.clone()).collect::<Vec<_>>()).bind(provenance).execute(&mut *tx).await?;
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
    tx.commit().await?;
    Ok(())
}

/// Read-only deployment diagnosis. No artifacts, keys or account credentials are returned.
pub async fn status(pool: &PgPool, actor: Uuid) -> Result<Value> {
    let mut tx = pool.begin().await?;
    service_identity(&mut tx, actor).await?;
    let status = super::business_scheme::status(&mut tx).await?;
    let activation:Option<Value>=sqlx::query_scalar("SELECT jsonb_build_object('release_id',release_id,'revision',revision) FROM mobile_v9_activations WHERE capability='wecom.group_text'").fetch_optional(&mut *tx).await?;
    tx.commit().await?;
    Ok(json!({"status":status,"activation":activation}))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn package() -> Value {
        let fixture: Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../api-contract/fixtures/shared-mobile-send-v9.json"
        )))
        .unwrap();
        // Synthetic evidence only. Empty material must never become an accepted product.
        json!({"name":"Synthetic","version":"1","flow":fixture["flow"],"measurement_source":"Synthetic test only","measured_at":chrono::Utc::now(),"samples_ms":[1000,1000,1000],"evidence":[],"apks":[],"validation_evidence":{},"cases":{},"soak_hours":0,"soak_passed":false,"historical_debt_closed":false,"reviewer":"Synthetic reviewer"})
    }
    #[test]
    fn plain_delivery_still_requires_real_evidence() {
        assert!(matches!(
            parse(&serde_json::to_vec(&package()).unwrap()),
            Err(Error::Policy("delivery_evidence_required"))
        ));
        assert!(matches!(parse(b"{}"), Err(Error::Invalid(_))));
        assert!(parse(b"not json").is_err());
        assert!(unhex("not-hex").is_err());
    }
    #[test]
    fn obsolete_envelopes_and_unknown_fields_are_rejected() {
        assert!(matches!(
            parse(
                &serde_json::to_vec(&json!({"payload":package(),"signature_hex":"obsolete"}))
                    .unwrap()
            ),
            Err(Error::Invalid(_))
        ));
        let mut p = package();
        p["policy"] = json!({});
        assert!(matches!(
            parse(&serde_json::to_vec(&p).unwrap()),
            Err(Error::Invalid(_))
        ));
    }
}
