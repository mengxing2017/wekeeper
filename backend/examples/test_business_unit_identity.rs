//! Run the business-unit integration test against an isolated temporary database.
use postgresql_embedded::{BOOTSTRAP_SUPERUSER, PostgreSQL, SettingsBuilder, V17};
use std::process::Command;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let settings = SettingsBuilder::new()
        .host("127.0.0.1")
        .port(0)
        .username(BOOTSTRAP_SUPERUSER)
        .temporary(true)
        .version(V17.clone())
        .build();
    let mut postgres = PostgreSQL::new(settings);
    postgres.setup().await?;
    postgres.start().await?;
    let result = async {
        postgres.create_database("business_unit_test").await?;
        let phone_test_key = format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        let source_routing_only = std::env::args()
            .skip(1)
            .any(|arg| arg == "--source-routing");
        let broadcast_only = std::env::args().skip(1).any(|arg| arg == "--broadcast");
        let suites: &[&str] = if broadcast_only {
            &["broadcast"]
        } else if source_routing_only {
            &["source_routing"]
        } else {
            &[
                "business_unit_identity",
                "source_access",
                "advertising_target_scopes",
                "message_orchestration",
                "receiving_configurations",
                "source_routing",
                "shared_mobile_v9",
                "broadcast",
            ]
        };
        let mut arguments = vec!["test"];
        for suite in suites {
            arguments.extend(["--test", suite]);
        }
        arguments.extend(["--", "--ignored", "--test-threads=2"]);
        let status = Command::new("cargo")
            .args(arguments)
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .env(
                "WEKEEPER_TEST_DATABASE_URL",
                postgres.settings().url("business_unit_test"),
            )
            // A fixed test-only key is safe because the database is temporary.
            .env(
                "WEKEEPER_SOURCE_MASTER_KEY_V1",
                "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            )
            .env("WEKEEPER_SHARED_SEND_KEY", phone_test_key)
            .status()?;
        if !status.success() {
            return Err("business-unit integration test failed".into());
        }
        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;
    let stopped = postgres.stop().await;
    result?;
    stopped?;
    Ok(())
}
