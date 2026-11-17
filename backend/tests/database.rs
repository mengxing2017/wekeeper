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
async fn hosting_removal_upgrades_previous_schema_without_losing_execution_evidence() {
    use serde_json::{Value, json};
    let url = std::env::var("WEKEEPER_TEST_DATABASE_URL").expect("isolated database required");
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .unwrap();
    let schema = format!("hosting_upgrade_{}", uuid::Uuid::new_v4().simple());
    sqlx::raw_sql(&format!(
        "CREATE SCHEMA {schema}; SET search_path TO {schema};"
    ))
    .execute(&pool)
    .await
    .unwrap();
    let migrations: Vec<_> = database::MIGRATOR.iter().collect();
    for migration in &migrations[..migrations.len() - 1] {
        sqlx::raw_sql(&migration.sql).execute(&pool).await.unwrap();
    }
    // Upgrade real rows, including immutable consent evidence and a completed
    // signed execution. Neither credentials nor results may be rewritten.
    sqlx::raw_sql(r#"
      INSERT INTO accounts(id,username,display_name,platform_admin) VALUES('00000000-0000-4000-8000-000000000001','upgrade','Upgrade',true);
      INSERT INTO execution_devices(id,name) VALUES('00000000-0000-4000-8000-000000000002','Upgrade phone');
      INSERT INTO mobile_targets(id,search_text,created_by,updated_by) VALUES('00000000-0000-4000-8000-000000000003','Upgrade group','00000000-0000-4000-8000-000000000001','00000000-0000-4000-8000-000000000001');
      INSERT INTO mobile_device_target_bindings(id,device_id,target_id,created_by,updated_by) VALUES('00000000-0000-4000-8000-000000000004','00000000-0000-4000-8000-000000000002','00000000-0000-4000-8000-000000000003','00000000-0000-4000-8000-000000000001','00000000-0000-4000-8000-000000000001');
      INSERT INTO mobile_shared_delegations(id,device_id,credential_generation,scope,scope_digest) VALUES('00000000-0000-4000-8000-000000000005','00000000-0000-4000-8000-000000000002',1,'{}','old-scope');
      INSERT INTO mobile_shared_delegation_decisions(id,delegation_id,accepted,revision) VALUES(gen_random_uuid(),'00000000-0000-4000-8000-000000000005',false,1);
      INSERT INTO mobile_shared_runtime(device_id,credential_generation,runtime_epoch,ui_ready,paused,quiescent) VALUES('00000000-0000-4000-8000-000000000002',1,gen_random_uuid(),false,true,true);
      INSERT INTO mobile_v9_send_inspections(id,device_id,target_id,binding_id,delegation_id,dependencies,requested_by,purpose,challenge,automatic,expires_at) VALUES(gen_random_uuid(),'00000000-0000-4000-8000-000000000002','00000000-0000-4000-8000-000000000003','00000000-0000-4000-8000-000000000004','00000000-0000-4000-8000-000000000005','{"delegation_id":"old","binding_locator_revision":1}','00000000-0000-4000-8000-000000000001','automatic','challenge',true,clock_timestamp()+interval '1 hour');
      INSERT INTO mobile_execution_grants(id,inspection_id,device_id,credential_generation,kind,state,session_closed,result_credential_hash,envelope,result_id,result_body,result_hash,receipt) VALUES('00000000-0000-4000-8000-000000000006','00000000-0000-4000-8000-000000000004','00000000-0000-4000-8000-000000000002',1,'inspection','completed',true,'credential-evidence',decode('01020304','hex'),gen_random_uuid(),'{"outcome":"inspection_failed","reason":"device_unavailable"}','result-evidence','{"recorded":true}');
      INSERT INTO mobile_v9_operations(principal_id,generation,scope,idempotency_key,request_hash,response) VALUES('00000000-0000-4000-8000-000000000002',1,'delegation-decision','old','old','{"accepted":false}');
    "#).execute(&pool).await.unwrap();
    let before: Value = sqlx::query_scalar("SELECT to_jsonb(g) FROM mobile_execution_grants g")
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::raw_sql(&migrations.last().unwrap().sql)
        .execute(&pool)
        .await
        .unwrap();
    let after: Value = sqlx::query_scalar("SELECT to_jsonb(g) FROM mobile_execution_grants g")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(after, before);
    let dependencies: Value =
        sqlx::query_scalar("SELECT dependencies FROM mobile_v9_send_inspections")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(dependencies, json!({"binding_locator_revision":1}));
    let retired: i64 = sqlx::query_scalar("SELECT count(*) FROM information_schema.tables WHERE table_schema=current_schema() AND table_name IN ('mobile_shared_delegations','mobile_shared_delegation_decisions','mobile_shared_platform_identity','mobile_shared_runtime','mobile_v9_runtime_details')").fetch_one(&pool).await.unwrap();
    assert_eq!(retired, 0);
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM mobile_device_environments")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 0, "mixed snapshots must not become environment facts");
    let projection: Value = sqlx::query_scalar("SELECT mobile_business_snapshot('{\"candidates\":[{\"delegation_id\":\"old\",\"accepted\":false,\"consent_state\":\"revoked\",\"device_id\":\"phone\"}],\"accepted\":[{\"attempt_id\":\"receipt\"}]}'::jsonb)").fetch_one(&pool).await.unwrap();
    assert_eq!(
        projection,
        json!({"candidates":[{"device_id":"phone"}],"accepted":[{"attempt_id":"receipt"}]})
    );
    let operations: i64 = sqlx::query_scalar("SELECT count(*) FROM mobile_v9_operations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(operations, 0);
    sqlx::raw_sql(&format!(
        "SET search_path TO public; DROP SCHEMA {schema} CASCADE;"
    ))
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;
}
