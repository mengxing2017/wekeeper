//! Restricted host operation. Secret is written only to an explicitly created 0600 file.
use clap::Parser;
use std::{io::Write, os::unix::fs::OpenOptionsExt};
#[derive(Parser)]
struct Args {
    #[arg(long)]
    key_id: String,
    #[arg(long)]
    operator: String,
    #[arg(long)]
    output: std::path::PathBuf,
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let config = wekeeper_api::config::Config::parse_from(["source-secret-delivery"]);
    let pool = config.connect().await?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&args.output)?;
    let value = wekeeper_api::source_access::south::adapter::postgres::Repository(pool)
        .deliver(&args.key_id, &args.operator)
        .await
        .map_err(|e| format!("delivery failed: {}", e.0))?;
    file.write_all(value.as_bytes())?;
    file.sync_all()?;
    Ok(())
}
