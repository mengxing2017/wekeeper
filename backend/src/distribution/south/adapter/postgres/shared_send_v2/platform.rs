//! Platform maintenance uses the same domain validation and immutable evidence as execution.
use super::*;
use crate::distribution::domain::shared_mobile::{FlowV3, validate_text};
use sha2::{Digest, Sha256};

async fn artifacts(tx: &mut Tx<'_>, ids: &[Uuid]) -> Result<()> {
    require(
        !ids.is_empty() && ids.len() <= 100,
        "shared_send_evidence_required",
    )?;
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM mobile_v9_artifacts WHERE id=ANY($1)")
        .bind(ids)
        .fetch_one(&mut **tx)
        .await?;
    require(n == ids.len() as i64, "shared_send_evidence_required")
}
fn text(s: &str) -> Result<()> {
    validate_text(s, 1, 2000, 8000)
}
pub(super) async fn artifact(tx: &mut Tx<'_>, a: &Actor, v: ArtifactUpload) -> Result<Value> {
    text(&v.name)?;
    require(
        matches!(v.kind.as_str(), "apk" | "evidence")
            && !v.content.is_empty()
            && v.content.len() <= 128 * 1024 * 1024,
        "shared_send_artifact_invalid",
    )?;
    if v.kind == "apk" {
        require(apk_structure(&v.content), "shared_send_apk_invalid")?;
    }
    let id = Uuid::now_v7();
    let sha = format!("{:x}", Sha256::digest(&v.content));
    sqlx::query("INSERT INTO mobile_v9_artifacts(id,kind,name,sha256,content,created_by) VALUES($1,$2,$3,$4,$5,$6)").bind(id).bind(&v.kind).bind(&v.name).bind(&sha).bind(&v.content).bind(a.id).execute(&mut **tx).await?;
    Ok(json!({"id":id,"name":v.name,"kind":v.kind,"sha256":sha,"size_bytes":v.content.len()}))
}
// Only inspect ZIP directory records; never decompress untrusted APK content.
pub(super) fn apk_structure(b: &[u8]) -> bool {
    fn u16at(b: &[u8], i: usize) -> Option<usize> {
        Some(u16::from_le_bytes(b.get(i..i + 2)?.try_into().ok()?) as usize)
    }
    fn u32at(b: &[u8], i: usize) -> Option<usize> {
        Some(u32::from_le_bytes(b.get(i..i + 4)?.try_into().ok()?) as usize)
    }
    let Some(end) = (b.len().saturating_sub(65557)..b.len().saturating_sub(21))
        .rev()
        .find(|i| b.get(*i..*i + 4) == Some(b"PK\x05\x06"))
    else {
        return false;
    };
    let (Some(mut at), Some(count)) = (u32at(b, end + 16), u16at(b, end + 10)) else {
        return false;
    };
    let mut manifest = false;
    let mut dex = false;
    for _ in 0..count {
        if b.get(at..at + 4) != Some(b"PK\x01\x02") {
            return false;
        }
        let (Some(n), Some(e), Some(c)) = (u16at(b, at + 28), u16at(b, at + 30), u16at(b, at + 32))
        else {
            return false;
        };
        let Some(name) = b.get(at + 46..at + 46 + n) else {
            return false;
        };
        manifest |= name == b"AndroidManifest.xml";
        dex |= name == b"classes.dex";
        at += 46 + n + e + c;
        if at > end {
            return false;
        }
    }
    manifest && dex
}
pub(super) async fn write(tx: &mut Tx<'_>, a: &Actor, command: ManagementCommand) -> Result<Value> {
    let id = Uuid::now_v7();
    match command {
        ManagementCommand::Flows(v) => {
            text(&v.name)?;
            v.flow.validate()?;
            let flow = json!(v.flow);
            let h = digest(&flow)?;
            sqlx::query("INSERT INTO mobile_v9_flows(id,name,flow,digest,created_by) VALUES($1,$2,$3,$4,$5)").bind(id).bind(v.name).bind(flow).bind(h).bind(a.id).execute(&mut **tx).await?;
            Ok(json!({"id":id}))
        }
        ManagementCommand::HumanSamples(v) => {
            require(sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM mobile_v9_flows f, execution_devices d WHERE f.id=$1 AND d.id=$2 AND d.enabled)").bind(v.flow_id).bind(v.device_id).fetch_one(&mut **tx).await?,"shared_send_resource_invalid")?;
            let generation: i64 = sqlx::query_scalar(
                "SELECT credential_generation FROM execution_devices WHERE id=$1",
            )
            .bind(v.device_id)
            .fetch_one(&mut **tx)
            .await?;
            sqlx::query("INSERT INTO mobile_v9_human_samples(id,flow_id,device_id,credential_generation,requested_by,challenge) VALUES($1,$2,$3,$4,$5,$6)").bind(id).bind(v.flow_id).bind(v.device_id).bind(generation).bind(a.id).bind(secrets::token()).execute(&mut **tx).await?;
            Ok(json!({"id":id}))
        }
        ManagementCommand::Baselines(v) => {
            artifacts(tx, &v.evidence_ids).await?;
            require(
                (3..=100).contains(&v.sample_ids.len()),
                "shared_send_baseline_required",
            )?;
            let samples:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('id',id,'device_id',device_id,'elapsed_ms',elapsed_ms) FROM mobile_v9_human_samples WHERE id=ANY($1) AND flow_id=$2 AND state='passed'").bind(&v.sample_ids).bind(v.flow_id).fetch_all(&mut **tx).await?;
            require(
                samples.len() == v.sample_ids.len(),
                "shared_send_baseline_required",
            )?;
            let min = samples
                .iter()
                .filter_map(|s| s["elapsed_ms"].as_i64())
                .max()
                .ok_or(Error::Conflict)?;
            sqlx::query("INSERT INTO mobile_v9_baselines(id,flow_id,device_id,samples,evidence_ids,min_interval_ms,recorded_by) VALUES($1,$2,$3,$4,$5,$6,$7)").bind(id).bind(v.flow_id).bind(uid(&samples[0],"device_id")?).bind(json!(samples)).bind(v.evidence_ids).bind(min).bind(a.id).execute(&mut **tx).await?;
            Ok(json!({"id":id,"min_interval_ms":min}))
        }
        ManagementCommand::Releases(v) => {
            text(&v.name)?;
            text(&v.version)?;
            artifacts(tx, &v.evidence_ids).await?;
            let f: Value = sqlx::query_scalar("SELECT flow FROM mobile_v9_flows WHERE id=$1")
                .bind(v.flow_id)
                .fetch_optional(&mut **tx)
                .await?
                .ok_or(Error::NotFound)?;
            let flow: FlowV3 = serde_json::from_value(f.clone()).map_err(|_| Error::Conflict)?;
            flow.validate()?;
            let minimum: Option<i64> = sqlx::query_scalar(
                "SELECT min_interval_ms FROM mobile_v9_baselines WHERE id=$1 AND flow_id=$2",
            )
            .bind(v.baseline_id)
            .bind(v.flow_id)
            .fetch_optional(&mut **tx)
            .await?;
            require(
                minimum.is_some_and(|m| flow.pacing.between_targets_min_ms >= m),
                "shared_send_baseline_required",
            )?;
            sqlx::query("INSERT INTO mobile_v9_adapter_releases(id,name,version,flow_id,flow,content_digest,baseline_id,evidence_ids,created_by) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)").bind(id).bind(v.name).bind(v.version).bind(v.flow_id).bind(&f).bind(digest(&f)?).bind(v.baseline_id).bind(v.evidence_ids).bind(a.id).execute(&mut **tx).await?;
            Ok(json!({"id":id}))
        }
        ManagementCommand::Activations(v) => {
            text(&v.reason)?;
            require(sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM mobile_v9_acceptance_runs a WHERE a.release_id=$1) AND NOT EXISTS(SELECT 1 FROM mobile_v9_adapter_withdrawals WHERE release_id=$1)").bind(v.release_id).fetch_one(&mut **tx).await?,"shared_send_acceptance_required")?;
            let old: Option<i64> = sqlx::query_scalar(
                "SELECT revision FROM mobile_v9_activations WHERE capability='wecom.group_text'",
            )
            .fetch_optional(&mut **tx)
            .await?;
            require(old.unwrap_or(0) == v.expected_revision, "revision_conflict")?;
            let rev = v.expected_revision + 1;
            sqlx::query("INSERT INTO mobile_v9_activations(capability,release_id,revision,updated_by) VALUES('wecom.group_text',$1,$2,$3) ON CONFLICT(capability) DO UPDATE SET release_id=$1,revision=$2,updated_by=$3").bind(v.release_id).bind(rev).bind(a.id).execute(&mut **tx).await?;
            sqlx::query("INSERT INTO mobile_v9_activation_events(id,release_id,revision,actor_id,reason) VALUES($1,$2,$3,$4,$5)").bind(id).bind(v.release_id).bind(rev).bind(a.id).bind(v.reason).execute(&mut **tx).await?;
            Ok(json!({"id":id,"revision":rev}))
        }
        ManagementCommand::Withdrawals(v) => {
            text(&v.reason)?;
            sqlx::query("INSERT INTO mobile_v9_adapter_withdrawals(release_id,reason,created_by) VALUES($1,$2,$3)").bind(v.release_id).bind(v.reason).bind(a.id).execute(&mut **tx).await?;
            Ok(json!({"id":v.release_id}))
        }
        ManagementCommand::Validations(v) => {
            require(
                matches!(v.launch_path.as_str(), "search" | "all_apps"),
                "shared_send_path_required",
            )?;
            require(
                sqlx::query_scalar::<_, bool>(
                    "SELECT EXISTS(SELECT 1 FROM mobile_v9_artifacts WHERE id=$1 AND kind='apk')",
                )
                .bind(v.apk_id)
                .fetch_one(&mut **tx)
                .await?,
                "shared_send_apk_invalid",
            )?;
            let b:Value=sqlx::query_scalar("SELECT to_jsonb(b)||jsonb_build_object('credential_generation',d.credential_generation) FROM mobile_device_target_bindings b JOIN execution_devices d ON d.id=b.device_id WHERE b.id=$1 AND b.enabled AND d.enabled").bind(v.binding_id).fetch_optional(&mut **tx).await?.ok_or(Error::NotFound)?;
            let flow: Value = sqlx::query_scalar("SELECT flow FROM mobile_v9_flows WHERE id=$1")
                .bind(v.flow_id)
                .fetch_optional(&mut **tx)
                .await?
                .ok_or(Error::NotFound)?;
            let d = delegation::context(
                tx,
                &Credential {
                    device_id: uid(&b, "device_id")?,
                    generation: b["credential_generation"].as_i64().ok_or(Error::Conflict)?,
                    token_hash: String::new(),
                },
            )
            .await?;
            let deps = json!({"flow_id":v.flow_id,"flow_digest":digest(&flow)?,"binding_id":b["id"],"binding_locator_revision":b["locator_revision"],"credential_generation":b["credential_generation"],"delegation_id":d["id"]});
            sqlx::query("INSERT INTO mobile_v9_send_inspections(id,school_id,device_id,target_id,binding_id,flow_id,delegation_id,dependencies,requested_by,purpose,launch_path,challenge,automatic,expires_at,apk_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,'maintenance',$10,$11,false,clock_timestamp()+interval '1 day',$12)").bind(id).bind(uid(&b,"school_id")?).bind(uid(&b,"device_id")?).bind(uid(&b,"target_id")?).bind(v.binding_id).bind(v.flow_id).bind(uid(&d,"id")?).bind(deps).bind(a.id).bind(v.launch_path).bind(secrets::token()).bind(v.apk_id).execute(&mut **tx).await?;
            Ok(json!({"id":id}))
        }
        ManagementCommand::Acceptances(v) => {
            text(&v.reason)?;
            artifacts(tx, &v.evidence_ids).await?;
            require(
                matches!(v.stage.as_str(), "test_send" | "production_allowlist"),
                "shared_send_policy_invalid",
            )?;
            let flow_id:Uuid=sqlx::query_scalar("SELECT flow_id FROM mobile_v9_adapter_releases r WHERE r.id=$1 AND NOT EXISTS(SELECT 1 FROM mobile_v9_adapter_withdrawals w WHERE w.release_id=r.id)").bind(v.release_id).fetch_optional(&mut **tx).await?.ok_or(Error::NotFound)?;
            let paths:Vec<String>=sqlx::query_scalar("SELECT DISTINCT launch_path FROM mobile_v9_send_inspections WHERE id=ANY($1) AND purpose='maintenance' AND state='passed' AND flow_id=$2 AND apk_id=$3 AND expires_at>clock_timestamp()").bind(&v.validation_ids).bind(flow_id).bind(v.apk_id).fetch_all(&mut **tx).await?;
            require(
                paths.iter().any(|p| p == "search") && paths.iter().any(|p| p == "all_apps"),
                "shared_send_validation_required",
            )?;
            if v.stage == "production_allowlist" {
                require(
                    v.soak_hours >= 72
                        && v.soak_passed
                        && v.historical_debt_closed
                        && (1..=40).all(|n| v.cases.get(&format!("S{n:02}")) == Some(&true))
                        && (1..=14).all(|n| v.cases.get(&format!("T{n:02}")) == Some(&true)),
                    "shared_send_acceptance_required",
                )?;
                require(sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM mobile_v9_send_events e JOIN mobile_v9_attempts a ON a.id=e.attempt_id JOIN mobile_v9_plans p ON p.id=a.plan_id WHERE p.plan->>'release_id'=$1 AND p.plan->>'stage'='test_send' AND e.body->>'outcome'='observed_sent')").bind(v.release_id.to_string()).fetch_one(&mut **tx).await?,"shared_send_trial_required")?;
            }
            sqlx::query("INSERT INTO mobile_v9_acceptance_runs(id,release_id,stage,apk_id,evidence_ids,validation_ids,cases,soak_hours,soak_passed,historical_debt_closed,reviewer_id,reason) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)").bind(id).bind(v.release_id).bind(v.stage).bind(v.apk_id).bind(v.evidence_ids).bind(v.validation_ids).bind(json!(v.cases)).bind(v.soak_hours).bind(v.soak_passed).bind(v.historical_debt_closed).bind(a.id).bind(v.reason).execute(&mut **tx).await?;
            Ok(json!({"id":id}))
        }
        ManagementCommand::Scopes(v) => {
            text(&v.reason)?;
            require(
                !v.device_ids.is_empty()
                    && v.device_ids.len() <= 100
                    && (!v.target_ids.is_empty() || v.auto_targets)
                    && v.target_ids.len() <= 100
                    && (1..=100000).contains(&v.max_calls)
                    && v.expires_at > chrono::Utc::now()
                    && v.expires_at < chrono::Utc::now() + chrono::Duration::days(366),
                "shared_send_scope_invalid",
            )?;
            let ac: Value = sqlx::query_scalar(
                "SELECT to_jsonb(a) FROM mobile_v9_acceptance_runs a WHERE id=$1",
            )
            .bind(v.acceptance_id)
            .fetch_optional(&mut **tx)
            .await?
            .ok_or(Error::NotFound)?;
            if ac["stage"] == "test_send" {
                require(
                    !v.auto_targets
                        && v.max_calls <= 20
                        && v.expires_at < chrono::Utc::now() + chrono::Duration::days(1),
                    "shared_send_trial_limit",
                )?;
            }
            for d in &v.device_ids {
                let rev:i64=sqlx::query_scalar("SELECT coalesce((SELECT revision FROM mobile_shared_school_access WHERE school_id=$1 AND device_id=$2),0)").bind(v.school_id).bind(d).fetch_one(&mut **tx).await?;
                require(v.expected_access.get(d) == Some(&rev), "revision_conflict")?;
            }
            let n: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM execution_devices WHERE id=ANY($1) AND enabled",
            )
            .bind(&v.device_ids)
            .fetch_one(&mut **tx)
            .await?;
            require(
                n == v.device_ids.len() as i64,
                "shared_send_resource_invalid",
            )?;
            sqlx::query("INSERT INTO mobile_v9_scope_grants(id,school_id,release_id,acceptance_id,stage,device_ids,target_ids,auto_targets,max_calls,expires_at,reason,created_by,resource_revisions) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)").bind(id).bind(v.school_id).bind(uid(&ac,"release_id")?).bind(v.acceptance_id).bind(ac["stage"].as_str()).bind(v.device_ids).bind(v.target_ids).bind(v.auto_targets).bind(v.max_calls).bind(v.expires_at).bind(v.reason).bind(a.id).bind(json!(v.expected_access)).execute(&mut **tx).await?;
            Ok(json!({"id":id}))
        }
        ManagementCommand::Revocations(v) => {
            text(&v.reason)?;
            sqlx::query("INSERT INTO mobile_v9_scope_revocations(grant_id,reason,actor_id) VALUES($1,$2,$3)").bind(v.grant_id).bind(v.reason).bind(a.id).execute(&mut **tx).await?;
            Ok(json!({"id":v.grant_id}))
        }
        ManagementCommand::Support(v) => {
            text(&v.reason)?;
            sqlx::query("INSERT INTO mobile_v9_support_requests(id,device_id,school_id,reason,state,created_by) VALUES($1,$2,$3,$4,'submitted',$5)").bind(id).bind(v.device_id).bind(v.school_id).bind(v.reason).bind(a.id).execute(&mut **tx).await?;
            Ok(json!({"id":id}))
        }
        ManagementCommand::SupportUpdates(v) => {
            text(&v.response)?;
            require(
                matches!(v.state.as_str(), "investigating" | "resolved" | "declined"),
                "shared_send_support_invalid",
            )?;
            let n=sqlx::query("UPDATE mobile_v9_support_requests SET state=$1,response=$2,release_id=$3,revision=revision+1 WHERE id=$4 AND revision=$5").bind(&v.state).bind(&v.response).bind(v.release_id).bind(v.id).bind(v.expected_revision).execute(&mut **tx).await?.rows_affected();
            require(n == 1, "revision_conflict")?;
            sqlx::query("INSERT INTO mobile_v9_support_events(id,request_id,actor_id,revision,state,response,release_id) VALUES($1,$2,$3,$4,$5,$6,$7)").bind(id).bind(v.id).bind(a.id).bind(v.expected_revision+1).bind(v.state).bind(v.response).bind(v.release_id).execute(&mut **tx).await?;
            Ok(json!({"id":v.id}))
        }
    }
}
pub(super) async fn read(tx: &mut Tx<'_>, page: u32) -> Result<Value> {
    let mut data = serde_json::Map::new();
    let offset = i64::from(page.saturating_sub(1)) * 100;
    let mut more = false;
    let mut counts = serde_json::Map::new();
    // Static table names; no user-controlled SQL identifiers.
    for (key, table) in [
        ("flows", "mobile_v9_flows"),
        ("baselines", "mobile_v9_baselines"),
        ("samples", "mobile_v9_human_samples"),
        ("releases", "mobile_v9_adapter_releases"),
        ("acceptances", "mobile_v9_acceptance_runs"),
        ("scopes", "mobile_v9_scope_grants"),
        ("revocations", "mobile_v9_scope_revocations"),
        ("validations", "mobile_v9_send_inspections"),
        ("support", "mobile_v9_support_requests"),
        ("withdrawals", "mobile_v9_adapter_withdrawals"),
        ("diagnostics", "mobile_v9_diagnostics"),
    ] {
        let mut values: Vec<Value> = sqlx::query_scalar(&format!(
            "SELECT to_jsonb(t)-'challenge' FROM {table} t ORDER BY created_at DESC LIMIT 101 OFFSET $1"
        ))
        .bind(offset).fetch_all(&mut **tx)
        .await?;
        let total: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
            .fetch_one(&mut **tx)
            .await?;
        counts.insert(key.into(), json!(total));
        more |= values.len() > 100;
        values.truncate(100);
        data.insert(key.into(), json!(values));
    }
    let mut assets:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(a)-'content' FROM mobile_v9_artifacts a ORDER BY created_at DESC LIMIT 101 OFFSET $1").bind(offset).fetch_all(&mut **tx).await?;
    more |= assets.len() > 100;
    assets.truncate(100);
    data.insert("artifacts".into(), json!(assets));
    let activation: Option<Value> = sqlx::query_scalar(
        "SELECT to_jsonb(a) FROM mobile_v9_activations a WHERE capability='wecom.group_text'",
    )
    .fetch_optional(&mut **tx)
    .await?;
    data.insert("activation".into(), json!(activation));
    let bindings:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(b)||jsonb_build_object('device_name',d.name,'school_name',s.name,'target_name',coalesce(nullif(t.alias,''),t.search_text)) FROM mobile_device_target_bindings b JOIN execution_devices d ON d.id=b.device_id JOIN schools s ON s.id=b.school_id JOIN mobile_targets t ON t.id=b.target_id WHERE b.enabled ORDER BY b.id LIMIT 100").fetch_all(&mut **tx).await?;
    data.insert("bindings".into(), json!(bindings));
    data.insert("counts".into(), Value::Object(counts));
    data.insert(
        "page".into(),
        json!({"number":page,"size":100,"has_more":more}),
    );
    Ok(Value::Object(data))
}
