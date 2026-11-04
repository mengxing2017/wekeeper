//! Deployment configuration validation/install/diagnosis. No development artifacts required.
use clap::Parser;
use wekeeper_api::{
    config::Config,
    distribution::{
        domain::shared_mobile::runtime_configuration::RuntimeConfiguration,
        south::adapter::postgres::shared_send_v2::configuration,
    },
};
#[derive(Parser)]
struct Args {
    #[arg(value_parser=["validate","install","status","verify"])]
    action: String,
    #[arg(long)]
    package: Option<std::path::PathBuf>,
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let configuration = if args.action != "status" {
        Some(RuntimeConfiguration::parse(&std::fs::read(
            args.package.ok_or("--package required")?,
        )?)?)
    } else {
        None
    };
    if args.action == "validate" {
        println!("Runtime configuration valid; no database writes.");
        return Ok(());
    }
    let config = Config::parse_from(["mobile-runtime-config"]);
    let pool = config.connect().await?;
    let result = async {
        if let Some(configuration) = configuration {
            if args.action == "verify" {
                return configuration::verify(&pool, &configuration).await;
            }
            configuration::install(&pool, &configuration).await?;
        }
        configuration::status(&pool).await
    }
    .await;
    pool.close().await;
    let state = result?;
    println!("{}", serde_json::to_string_pretty(&state)?);
    if state["available"] != true {
        return Err("shared mobile runtime configuration unavailable".into());
    }
    Ok(())
}
