//! Run database tests against a fresh, disposable local PostgreSQL, never external config.
use postgresql_embedded::{BOOTSTRAP_SUPERUSER, PostgreSQL, SettingsBuilder, V17};
use std::time::Duration;
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let settings = SettingsBuilder::new()
        .host("127.0.0.1")
        .port(0)
        .username(BOOTSTRAP_SUPERUSER)
        .temporary(true)
        .timeout(Some(Duration::from_secs(90)))
        .version(V17.clone())
        .build();
    let mut postgres = PostgreSQL::new(settings);
    postgres.setup().await?;
    postgres.start().await?;
    let result = async {
        postgres.create_database("wekeeper_test").await?;
        let args: Vec<String> = std::env::args().skip(1).collect();
        let status = std::process::Command::new("cargo")
            .args(["test", "--locked", "--manifest-path", "backend/Cargo.toml"])
            .args(args)
            .env(
                "WEKEEPER_TEST_DATABASE_URL",
                postgres.settings().url("wekeeper_test"),
            )
            .status()?;
        Ok::<_, Box<dyn std::error::Error>>(status)
    }
    .await;
    let stopped = postgres.stop().await;
    let status = result?;
    stopped?;
    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}
