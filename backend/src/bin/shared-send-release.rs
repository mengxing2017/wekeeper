//! Engineering-only release rendering. No database connection, example selectors or publication side effects.
use clap::Parser;
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{fs, io::Read, path::PathBuf};
use uuid::Uuid;
use wekeeper_api::distribution::domain::mobile_execution::{
    FlowV2, onboarding::Environment, shared_send::digest,
};
#[derive(Parser)]
struct Args {
    #[arg(long)]
    manifest: PathBuf,
    #[arg(long)]
    output: PathBuf,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Artifact {
    path: PathBuf,
    sha256: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    name: String,
    version: String,
    environment: Environment,
    flow: Value,
    source_revision: String,
    evidence: Vec<Artifact>,
}
fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let m: Manifest = serde_json::from_slice(&fs::read(&args.manifest)?)?;
    let flow: FlowV2 = serde_json::from_value(m.flow.clone())?;
    flow.validate_shared_send()
        .map_err(|_| "Flow must be a complete v8 adapter with a safe chat-to-search route")?;
    m.environment
        .validate()
        .map_err(|_| "An actual confirmed device environment report is required")?;
    if m.environment.application_status != "confirmed"
        || m.evidence.is_empty()
        || m.evidence.len() > 100
        || [&m.name, &m.version, &m.source_revision]
            .iter()
            .any(|s| s.trim().is_empty() || s.len() > 256)
    {
        return Err("Release identity and real evidence are required".into());
    }
    let root = args
        .manifest
        .canonicalize()?
        .parent()
        .ok_or("Missing bundle directory")?
        .to_path_buf();
    let mut evidence = vec![];
    for artifact in m.evidence {
        let path = root.join(&artifact.path).canonicalize()?;
        if !path.starts_with(&root) || !path.is_file() {
            return Err("Evidence outside bundle".into());
        }
        let mut file = fs::File::open(path)?;
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 8192];
        let mut total = 0usize;
        loop {
            let size = file.read(&mut buffer)?;
            if size == 0 {
                break;
            }
            total += size;
            hasher.update(&buffer[..size]);
        }
        let actual = format!("{:x}", hasher.finalize());
        if total == 0 || actual != artifact.sha256 {
            return Err("Evidence digest mismatch".into());
        }
        evidence.push(serde_json::json!({"path":artifact.path,"sha256":actual}));
    }
    let mut environment = serde_json::to_value(m.environment)?;
    environment
        .as_object_mut()
        .ok_or("Invalid environment")?
        .remove("instance_ref");
    let id = Uuid::now_v7();
    let sql = format!(
        "-- Reviewed engineering release; verify evidence before applying.\nBEGIN;\nSELECT pg_advisory_xact_lock(78234610243);\nINSERT INTO mobile_send_adapter_releases(id,name,version,environment,flow,content_digest,source_revision,evidence) VALUES({},{},{},{}::jsonb,{}::jsonb,{},{},{}::jsonb);\nCOMMIT;\n",
        quote(&id.to_string()),
        quote(&m.name),
        quote(&m.version),
        quote(&environment.to_string()),
        quote(&m.flow.to_string()),
        quote(&digest(&m.flow).map_err(|_| "Invalid canonical flow")?),
        quote(&m.source_revision),
        quote(&serde_json::to_string(&evidence)?)
    );
    fs::write(args.output, sql)?;
    println!(
        "Release {id} validated and rendered for review. No database or send operation performed."
    );
    Ok(())
}
