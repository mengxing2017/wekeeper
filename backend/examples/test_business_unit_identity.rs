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
        let status = Command::new("cargo")
            .args([
                "test",
                "--test",
                "business_unit_identity",
                "--test",
                "source_access",
                "--test",
                "advertising_target_scopes",
                "--test",
                "message_orchestration",
                "--",
                "--ignored",
            ])
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
            .status()?;
        if !status.success() {
            return Err("business-unit integration test failed".into());
        }
        for (test, filter) in [
            (
                "shared_mobile_v9",
                "dispatch_exhausted_target_does_not_block_other_targets",
            ),
            (
                "source_routing",
                "ordered_pool_bounded_skips_no_candidate_no_consumption_and_preview",
            ),
        ] {
            let status = Command::new("cargo")
                .args(["test", "--test", test, filter, "--", "--ignored"])
                .current_dir(env!("CARGO_MANIFEST_DIR"))
                .env(
                    "WEKEEPER_TEST_DATABASE_URL",
                    postgres.settings().url("business_unit_test"),
                )
                .env(
                    "WEKEEPER_SOURCE_MASTER_KEY_V1",
                    "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
                )
                .status()?;
            if !status.success() {
                return Err(format!("{test}: {filter} integration test failed").into());
            }
        }
        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;
    let stopped = postgres.stop().await;
    result?;
    stopped?;
    Ok(())
}
