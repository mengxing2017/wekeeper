//! Deployment tool: dry-run by default. Credentials and signing keys never enter browser APIs.
use clap::Parser;
use sqlx::PgPool;
use uuid::Uuid;
use wekeeper_api::distribution::south::adapter::postgres::shared_send_v2::delivery;
#[derive(Parser)]
struct Args {
    #[arg(value_parser=["import","activate","withdraw"])]
    action: String,
    #[arg(long)]
    package: Option<std::path::PathBuf>,
    #[arg(long)]
    actor: Option<Uuid>,
    #[arg(long)]
    release: Option<Uuid>,
    #[arg(long, default_value_t = 0)]
    expected_revision: i64,
    #[arg(long)]
    apply: bool,
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let package = if args.action == "import" {
        let raw = std::fs::read(args.package.ok_or("--package required")?)?;
        Some(delivery::verify(
            serde_json::from_slice(&raw)?,
            &std::env::var("WEKEEPER_CAPABILITY_PUBLIC_KEY")?,
        )?)
    } else {
        args.release.ok_or("--release required")?;
        None
    };
    if !args.apply {
        println!(
            "Validated {}. No database changes; use --apply to execute.",
            args.action
        );
        return Ok(());
    }
    let pool = PgPool::connect(&std::env::var("DATABASE_URL")?).await?;
    let actor = args
        .actor
        .ok_or("--actor requires a dedicated inactive, non-admin service identity")?;
    if let Some(package) = package {
        let release = delivery::import(&pool, actor, package).await?;
        println!("Imported {release}; activate separately.");
    } else {
        delivery::publish(
            &pool,
            actor,
            args.release.unwrap(),
            args.expected_revision,
            args.action == "withdraw",
        )
        .await?;
        println!("Completed {}", args.action);
    }
    pool.close().await;
    Ok(())
}
