//! Deployment-only, read diagnostics and explicit finite delivery observation. Never sends.
use clap::Parser;
use uuid::Uuid;
#[derive(Parser)]
struct Args {
    #[arg(value_parser=["check","pause"])]
    action: String,
    #[arg(long)]
    actor: Option<Uuid>,
    #[arg(long)]
    permission_review: String,
    #[arg(long,default_value_t=3600,value_parser=clap::value_parser!(i64).range(60..=86400))]
    valid_seconds: i64,
    #[arg(long)]
    apply: bool,
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    if args.permission_review.trim().is_empty() {
        return Err(
            "--permission-review requires a real external permission review reference".into(),
        );
    }
    if !args.apply {
        println!(
            "Arguments validated. No database or external request performed. --apply runs read diagnostics and records the observation."
        );
        return Ok(());
    }
    let actor = args
        .actor
        .ok_or("--actor requires a dedicated inactive non-admin service account")?;
    let pool = sqlx::PgPool::connect(&std::env::var("DATABASE_URL")?).await?;
    let service = wekeeper_api::distribution::north::local::workflows::Service(
        wekeeper_api::distribution::south::adapter::postgres::workflows::Repository(pool.clone()),
    );
    service
        .deliver(
            actor,
            wekeeper_api::integrations::north::local::Service::new(pool.clone()),
            args.action == "check",
            args.permission_review,
            args.valid_seconds,
        )
        .await?;
    println!(
        "Delivery observation recorded. No messages sent; business confirmation remains required."
    );
    pool.close().await;
    Ok(())
}
