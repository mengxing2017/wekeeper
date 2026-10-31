use sqlx::postgres::PgPoolOptions;
use wekeeper_api::{bootstrap, database};

#[test]
fn embedded_migration_versions_are_unique_and_ordered() {
    assert!(!database::MIGRATOR.iter().as_slice().is_empty());
    let mut previous = None;
    for migration in database::MIGRATOR.iter() {
        if let Some(version) = previous {
            assert!(
                migration.version > version,
                "migration versions must be unique and increasing: {} ({}) follows {}",
                migration.version,
                migration.description,
                version
            );
        }
        previous = Some(migration.version);
    }
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL; run make test-integration"]
async fn migrations_handle_residual_objects_after_table_only_reset() {
    let url = std::env::var("WEKEEPER_TEST_DATABASE_URL")
        .expect("WEKEEPER_TEST_DATABASE_URL must point to an isolated disposable database");
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .unwrap();
    let schema = format!("migration_residual_{}", uuid::Uuid::new_v4().simple());
    sqlx::raw_sql(&format!(
        "CREATE SCHEMA {schema}; SET search_path TO {schema};"
    ))
    .execute(&pool)
    .await
    .unwrap();
    // Dropping tables leaves standalone functions and unowned sequences behind. Reproduce that state
    // without touching public or any pre-existing schema in the test database.
    sqlx::raw_sql(
        "CREATE FUNCTION check_wecom_directory_group() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RETURN NEW; END $$;
         CREATE FUNCTION maintain_delivery_content_scope() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RETURN NEW; END $$;
         CREATE FUNCTION automation_immutable() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RETURN NEW; END $$;
         CREATE FUNCTION delivery_scope_owner(run uuid) RETURNS uuid LANGUAGE sql AS $$ SELECT run $$;
         CREATE SEQUENCE legacy_public_id START 1000000000050;
         SELECT nextval('legacy_public_id');",
    )
    .execute(&pool)
    .await
    .unwrap();
    database::migrate(&pool).await.unwrap();
    database::migrate(&pool).await.unwrap();
    let applied: i64 = sqlx::query_scalar("SELECT count(*) FROM _sqlx_migrations WHERE success")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(applied as usize, database::MIGRATOR.iter().count());
    let definition: String = sqlx::query_scalar(
        "SELECT pg_get_functiondef('check_wecom_directory_group()'::regprocedure)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(definition.contains("directory group identity mismatch"));
    let next: i64 = sqlx::query_scalar("SELECT nextval('legacy_public_id')")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        next, 1_000_000_000_051,
        "migration must not reset the counter"
    );
    let owned: bool = sqlx::query_scalar(
        "SELECT pg_get_serial_sequence('legacy_aliases','public_id')::regclass = 'legacy_public_id'::regclass",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(owned, "sequence must be owned by the alias column");
    sqlx::query("DROP TABLE legacy_aliases")
        .execute(&pool)
        .await
        .unwrap();
    let removed: bool = sqlx::query_scalar("SELECT to_regclass('legacy_public_id') IS NULL")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(removed, "dropping the table must also remove its sequence");
    sqlx::raw_sql(&format!(
        "SET search_path TO public; DROP SCHEMA {schema} CASCADE;"
    ))
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL; run make test-integration"]
async fn migrations_are_repeatable_and_readiness_checks_schema() {
    let url = std::env::var("WEKEEPER_TEST_DATABASE_URL")
        .expect("WEKEEPER_TEST_DATABASE_URL must point to an isolated disposable database");
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .unwrap();
    database::migrate(&pool).await.unwrap();
    database::migrate(&pool).await.unwrap();
    assert!(
        bootstrap::build_application(pool.clone())
            .system
            .ready()
            .await
    );
    pool.close().await;
}
