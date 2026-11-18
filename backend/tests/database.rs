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

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn empty_baseline_initializes_current_configuration_and_enforces_device_contract() {
    let url = std::env::var("WEKEEPER_TEST_DATABASE_URL")
        .expect("explicit isolated test database required");
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .unwrap();
    let schema = format!("domain_baseline_{}", uuid::Uuid::new_v4().simple());
    sqlx::raw_sql(&format!(
        "CREATE SCHEMA {schema}; SET search_path TO {schema};"
    ))
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
    let obsolete: i64 = sqlx::query_scalar("SELECT count(*) FROM information_schema.columns WHERE table_schema=current_schema() AND (column_name IN ('workspace_id','storage_scope') OR table_name IN ('source_route_managers','media_access_tokens','wecom_workspaces','wecom_workspace_members','mobile_workspaces','mobile_workspace_members','unit_receiving_storage','mobile_v9_task_wake_outbox'))")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(obsolete, 0);
    let legacy_projection: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM pg_proc WHERE pronamespace=current_schema()::regnamespace AND proname='mobile_business_snapshot')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!legacy_projection);
    let configuration: bool = sqlx::query_scalar(
        "SELECT (SELECT count(*)=1 FROM application_metadata WHERE schema_generation=1)
          AND (SELECT count(*)=1 FROM device_control_policy WHERE heartbeat_interval_seconds=60 AND suspect_after_seconds=130 AND offline_after_seconds=190)
          AND (SELECT count(*)=1 FROM wecom_asset_sync_state WHERE dataset='groups' AND freshness='unverified')
          AND (SELECT count(*)=2 FROM distribution_sources WHERE namespace IN ('builtin.direct','builtin.source-route'))
          AND (SELECT count(*)=1 FROM keyword_scope_heads WHERE id='global' AND revision=0)
          AND (SELECT count(*)=1 FROM keyword_rule_versions WHERE scope_id='global' AND NOT filter_enabled AND entries='[]'::jsonb)
          AND NOT EXISTS(SELECT 1 FROM keyword_scope_heads WHERE kind='workspace')",
    ).fetch_one(&pool).await.unwrap();
    assert!(configuration);
    // These checks exercise the final schema, without recreating retired upgrade states.
    sqlx::raw_sql(
        "DO $$ DECLARE device uuid:=gen_random_uuid(); BEGIN
         INSERT INTO execution_devices(id,name) VALUES(device,'baseline phone');
         INSERT INTO device_control_sessions(device_id,connection_id,transport) VALUES(device,gen_random_uuid(),'wss');
         INSERT INTO device_control_commands(id,device_id,kind,state) VALUES(gen_random_uuid(),device,'request_status','queued');
         BEGIN
           INSERT INTO device_control_sessions(device_id,connection_id,transport) VALUES(device,gen_random_uuid(),'https');
           RAISE EXCEPTION 'obsolete HTTPS transport accepted';
         EXCEPTION WHEN check_violation THEN NULL; END;
         BEGIN
           INSERT INTO device_control_commands(id,device_id,kind,state) VALUES(gen_random_uuid(),device,'task_available','queued');
           RAISE EXCEPTION 'obsolete task wake command accepted';
         EXCEPTION WHEN check_violation THEN NULL; END;
         BEGIN
           UPDATE keyword_rule_versions SET entries='[{}]' WHERE scope_id='global';
           RAISE EXCEPTION 'immutable keyword version accepted update';
         EXCEPTION WHEN raise_exception THEN
           IF SQLERRM NOT LIKE '%immutable%' OR SQLERRM='immutable keyword version accepted update' THEN RAISE; END IF;
         END;
         END $$;",
    ).execute(&pool).await.unwrap();
    let device: uuid::Uuid = sqlx::query_scalar("SELECT id FROM execution_devices LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    let observation: serde_json::Value = serde_json::from_str(include_str!(
        "../../api-contract/fixtures/device-environment.json"
    ))
    .unwrap();
    sqlx::query("INSERT INTO mobile_device_environments(device_id,credential_generation,observation_id,observed_at,observation) VALUES($1,1,($2->>'observation_id')::uuid,($2->>'observed_at')::timestamptz,$2)")
        .bind(device).bind(&observation).execute(&pool).await.unwrap();
    for applications in [
        serde_json::json!({"wecom":"logged_in"}),
        serde_json::json!({"wecom":"logged_in","wechat":null}),
        serde_json::json!({"wecom":"logged_in","wechat":"paused"}),
        serde_json::json!({"wecom":"logged_in","wechat":"unknown","other":"unknown"}),
    ] {
        let mut invalid = observation.clone();
        invalid["applications"] = applications;
        let error =
            sqlx::query("UPDATE mobile_device_environments SET observation=$1 WHERE device_id=$2")
                .bind(invalid)
                .bind(device)
                .execute(&pool)
                .await
                .unwrap_err();
        assert_eq!(
            error.as_database_error().unwrap().code().as_deref(),
            Some("23514")
        );
    }
    sqlx::raw_sql(&format!(
        "SET search_path TO public; DROP SCHEMA {schema} CASCADE;"
    ))
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn business_chain_cleanup_applies_forward_and_preserves_current_execution_objects() {
    use sqlx::Executor;
    let url = std::env::var("WEKEEPER_TEST_DATABASE_URL").expect("isolated database required");
    let base = sqlx::PgPool::connect(&url).await.unwrap();
    let schema = format!("cleanup_{}", uuid::Uuid::new_v4().simple());
    base.execute(format!("CREATE SCHEMA {schema}").as_str())
        .await
        .unwrap();
    let search = format!("SET search_path TO {schema},public");
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .after_connect(move |c, _| {
            let search = search.clone();
            Box::pin(async move {
                c.execute(search.as_str()).await?;
                Ok(())
            })
        })
        .connect(&url)
        .await
        .unwrap();
    // Exercise the complete pre-cleanup structure, then the new forward-only migration.
    for migration in database::MIGRATOR.iter() {
        sqlx::raw_sql(&migration.sql).execute(&pool).await.unwrap();
        if migration.version == 202611180224 {
            assert!(
                sqlx::query_scalar::<_, Option<String>>(
                    "SELECT to_regclass('mobile_v9_invocations')::text"
                )
                .fetch_one(&pool)
                .await
                .unwrap()
                .is_some()
            );
        }
    }
    for table in [
        "mobile_v9_invocations",
        "mobile_v9_risk_guards",
        "mobile_v9_send_events",
        "mobile_v9_send_inspections",
        "mobile_v9_diagnostics",
        "mobile_v9_support_requests",
        "mobile_v9_support_events",
        "mobile_v9_flows",
        "mobile_v9_artifacts",
        "mobile_v9_baselines",
        "mobile_v9_human_samples",
        "mobile_v9_adapter_releases",
        "mobile_v9_adapter_withdrawals",
        "mobile_v9_acceptance_runs",
        "mobile_v9_activations",
        "mobile_v9_activation_events",
        "mobile_capability_deliveries",
        "mobile_runtime_configuration_current",
        "mobile_runtime_configurations",
    ] {
        let exists: bool = sqlx::query_scalar("SELECT to_regclass($1) IS NOT NULL")
            .bind(format!("{schema}.{table}"))
            .fetch_one(&pool)
            .await
            .unwrap();
        assert!(!exists, "retired table remains: {table}");
    }
    for table in [
        "mobile_execution_grants",
        "mobile_execution_delivery_outbox",
        "mobile_execution_diagnostics",
        "mobile_execution_resource_charges",
        "mobile_v9_tasks",
        "mobile_v9_task_items",
        "mobile_v9_attempts",
        "mobile_v9_reviews",
        "mobile_v9_dispatch_state",
        "mobile_v9_dispatch_scopes",
        "mobile_v9_quota_reservations",
        "mobile_v9_task_receipts",
        "audit_events",
    ] {
        let exists: bool = sqlx::query_scalar("SELECT to_regclass($1) IS NOT NULL")
            .bind(format!("{schema}.{table}"))
            .fetch_one(&pool)
            .await
            .unwrap();
        assert!(exists, "current table missing: {table}");
    }
    pool.close().await;
    base.execute(format!("DROP SCHEMA {schema} CASCADE").as_str())
        .await
        .unwrap();
    base.close().await;
}
