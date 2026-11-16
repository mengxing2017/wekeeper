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
    let obsolete: i64 = sqlx::query_scalar("SELECT count(*) FROM information_schema.columns WHERE table_schema=current_schema() AND (column_name IN ('school_id','workspace_id','storage_scope') OR table_name IN ('source_route_managers','media_access_tokens','distribution_forum_materials','distribution_materials','distribution_source_organizations','broadcast_task_posts','broadcast_content_references','templates','template_versions','wecom_workspaces','wecom_workspace_members','mobile_workspaces','mobile_workspace_members','unit_receiving_storage','mobile_shared_school_access','mobile_shared_access_events'))").fetch_one(&pool).await.unwrap();
    assert_eq!(
        obsolete, 0,
        "workspace tables and partition columns must be absent"
    );
    let source_obsolete: i64 = sqlx::query_scalar("SELECT count(*) FROM information_schema.columns WHERE table_schema=current_schema() AND ((table_name='source_receipts' AND column_name IN ('observed_sequence','canonical_receipt_id','keyword_terminal_rejected')) OR (table_name='source_inbox' AND column_name IN ('event_type','reason_class','projected','failures','last_error','next_attempt_at','job_state')))")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(
        source_obsolete, 0,
        "obsolete source projection and withdrawal fields must be absent"
    );
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
    let removed: bool = sqlx::query_scalar(
        "SELECT to_regclass('legacy_public_id') IS NULL AND to_regclass('legacy_aliases') IS NULL",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        removed,
        "final retirement removes the obsolete alias table and owned sequence"
    );
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

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL; run make test-integration"]
async fn unused_storage_upgrade_preserves_current_business_and_trigger_behavior() {
    let url = std::env::var("WEKEEPER_TEST_DATABASE_URL")
        .expect("explicit isolated test database required");
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .unwrap();
    let schema = format!("unused_fields_{}", uuid::Uuid::new_v4().simple());
    sqlx::raw_sql(&format!(
        "CREATE SCHEMA {schema}; SET search_path TO {schema},public;"
    ))
    .execute(&pool)
    .await
    .unwrap();
    for migration in database::MIGRATOR
        .iter()
        .filter(|m| m.version < 202611150183)
    {
        sqlx::raw_sql(&migration.sql).execute(&pool).await.unwrap();
    }
    sqlx::raw_sql(r#"
DO $$
DECLARE actor uuid=gen_random_uuid(); platform uuid=gen_random_uuid(); unit uuid=gen_random_uuid();
 workspace uuid=gen_random_uuid(); device uuid=gen_random_uuid(); receipt uuid=gen_random_uuid();
 related uuid=gen_random_uuid(); draft uuid=gen_random_uuid(); preview uuid=gen_random_uuid(); intent uuid=gen_random_uuid();
BEGIN
 INSERT INTO accounts(id,username,display_name,platform_admin) VALUES(actor,'upgrade-admin','Admin',true);
 INSERT INTO source_callers(source_id) VALUES(platform);
 INSERT INTO business_units(id,platform_id,name,created_by,updated_by) VALUES(unit,platform,'Receipt unit',actor,actor);
 INSERT INTO source_routes(route_id,source_id,business_unit_id,route_name,status,config_revision) VALUES('receipt-route',platform,unit,'Receipt route','active',12);
 INSERT INTO source_route_tombstones(route_id,source_id,reason,config_revision) VALUES('already-retired',platform,'credential_deleted',9);
 INSERT INTO source_receipts(receipt_id,source_id,business_unit_id,event_id,route_id,body_sha256,schema_version)
 VALUES(receipt,platform,unit,gen_random_uuid(),'receipt-route','original',2),(related,platform,unit,gen_random_uuid(),'receipt-route','duplicate',2);
 INSERT INTO source_content_checks(receipt_id,rule_revision,fingerprint,outcome) VALUES(receipt,7,'original','passed');
 INSERT INTO source_receipt_relations(receipt_id,related_receipt_id,kind) VALUES(related,receipt,'duplicate');
 INSERT INTO mobile_workspaces(id,name) VALUES(workspace,'Upgrade workspace');
 INSERT INTO execution_devices(id,name,enabled,remark,custodian_actor_id,metadata_revision) VALUES(device,'Existing phone',false,'unused',actor,8);
 INSERT INTO mobile_v9_task_drafts(id,school_id,intent_id,actor_id,target_ids,message,content_digest,between_targets_seconds) VALUES(draft,workspace,intent,actor,'{}','Business text','digest',30);
 INSERT INTO mobile_v9_previews(id,school_id,draft_id,draft_revision,plan,digest) VALUES(preview,workspace,draft,1,'{}','digest');
 INSERT INTO mobile_v9_tasks(id,school_id,draft_id,intent_id,preview_id,confirmed_by,confirmed_until,cursor_position) VALUES(gen_random_uuid(),workspace,draft,intent,preview,actor,clock_timestamp()+interval '1 hour',4);
END $$;
"#).execute(&pool).await.unwrap();
    let migration = database::MIGRATOR
        .iter()
        .find(|m| m.version == 202611150183)
        .unwrap();
    sqlx::raw_sql(&migration.sql).execute(&pool).await.unwrap();
    let remaining: i64 = sqlx::query_scalar(r#"
SELECT count(*) FROM information_schema.columns c
JOIN (VALUES ('execution_devices','remark'),('execution_devices','custodian_actor_id'),
 ('execution_devices','metadata_revision'),('source_routes','config_revision'),
 ('source_route_tombstones','config_revision'),('mobile_v9_tasks','cursor_position')) removed(table_name,column_name)
USING(table_name,column_name) WHERE c.table_schema=current_schema()
"#).fetch_one(&pool).await.unwrap();
    assert_eq!(remaining, 0);
    let retired: bool = sqlx::query_scalar("SELECT to_regclass('source_content_checks') IS NULL AND to_regclass('source_receipt_relations') IS NULL")
        .fetch_one(&pool).await.unwrap();
    assert!(retired);
    let facts: i64 = sqlx::query_scalar("SELECT count(*) FROM retired_distribution_evidence WHERE kind IN ('source_content_checks','source_receipt_relations')")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(
        facts, 2,
        "intermediate migration contains evidence before the final archive retirement"
    );
    sqlx::raw_sql(r#"
DO $$ DECLARE unit uuid=gen_random_uuid(); source uuid; actor uuid; BEGIN
 IF NOT EXISTS(SELECT 1 FROM execution_devices WHERE name='Existing phone' AND NOT enabled) THEN RAISE EXCEPTION 'device changed'; END IF;
 IF NOT EXISTS(SELECT 1 FROM mobile_v9_tasks t JOIN mobile_v9_task_drafts d ON d.id=t.draft_id WHERE d.message='Business text' AND NOT t.stopped) THEN RAISE EXCEPTION 'task changed'; END IF;
 IF NOT EXISTS(SELECT 1 FROM source_route_tombstones WHERE route_id='already-retired') THEN RAISE EXCEPTION 'tombstone lost'; END IF;
 SELECT source_id INTO source FROM source_routes WHERE route_id='receipt-route';
 SELECT id INTO actor FROM accounts WHERE username='upgrade-admin';
 INSERT INTO business_units(id,platform_id,name,created_by,updated_by) VALUES(unit,source,'Delete unit',actor,actor);
 INSERT INTO source_routes(route_id,source_id,business_unit_id,route_name,status) VALUES('delete-route',source,unit,'Delete route','active');
 DELETE FROM source_routes WHERE route_id='delete-route';
 IF NOT EXISTS(SELECT 1 FROM source_route_tombstones WHERE route_id='delete-route' AND source_id=source) THEN RAISE EXCEPTION 'delete did not create tombstone'; END IF;
 BEGIN
  INSERT INTO source_routes(route_id,source_id,business_unit_id,route_name,status) VALUES('delete-route',source,unit,'Reused route','active');
  RAISE EXCEPTION 'retired route reused';
 EXCEPTION WHEN raise_exception THEN
  IF SQLERRM<>'retired route identifier' THEN RAISE; END IF;
 END;
END $$;
UPDATE mobile_v9_tasks SET revision=revision+1;
UPDATE mobile_v9_tasks SET stopped=true;
"#).execute(&pool).await.unwrap();
    let migration = database::MIGRATOR
        .iter()
        .find(|m| m.version == 202611150184)
        .unwrap();
    sqlx::raw_sql(&migration.sql).execute(&pool).await.unwrap();
    let removed = [
        "distribution_source_events",
        "keyword_admissions",
        "keyword_check_results",
        "receiving_configuration_evidence",
        "retired_distribution_evidence",
        "retired_message_handoffs",
        "source_advertisement_decisions",
        "source_automation_task_events",
        "source_automation_tasks",
        "source_check_policy",
        "source_check_scan",
        "source_content_history",
        "source_execution_generations",
        "source_message_compositions",
        "source_receipt_history",
        "source_route_revisions",
        "source_routing_decisions",
        "source_summary_queue",
        "source_execution_tasks",
    ];
    for table in removed {
        let absent: bool = sqlx::query_scalar("SELECT to_regclass($1) IS NULL")
            .bind(table)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert!(absent, "unused table {table} must be physically removed");
    }
    let dangling: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace WHERE n.nspname=current_schema() AND EXISTS(SELECT 1 FROM unnest($1::text[]) retired(name) WHERE position(retired.name in p.prosrc)>0)",
    ).bind(removed.to_vec()).fetch_one(&pool).await.unwrap();
    assert_eq!(dangling, 0, "functions must not reference retired storage");
    sqlx::raw_sql(r#"
DO $$ DECLARE old_revision bigint; old_updated timestamptz; receipt uuid; BEGIN
 SELECT receipt_id,processing_revision,updated_at INTO receipt,old_revision,old_updated
 FROM source_receipts WHERE body_sha256='original';
 UPDATE source_receipts SET reason_code='upgrade_test' WHERE receipt_id=receipt;
 IF NOT EXISTS(SELECT 1 FROM source_receipts WHERE receipt_id=receipt AND processing_revision=old_revision+1 AND updated_at>old_updated) THEN
  RAISE EXCEPTION 'receipt transition stopped working';
 END IF;
 IF NOT EXISTS(SELECT 1 FROM execution_devices WHERE name='Existing phone' AND NOT enabled) THEN RAISE EXCEPTION 'device lost'; END IF;
 IF NOT EXISTS(SELECT 1 FROM mobile_v9_tasks t JOIN mobile_v9_task_drafts d ON d.id=t.draft_id WHERE d.message='Business text' AND t.stopped) THEN RAISE EXCEPTION 'current task lost'; END IF;
 IF NOT EXISTS(SELECT 1 FROM source_route_tombstones WHERE route_id='delete-route') THEN RAISE EXCEPTION 'tombstone lost'; END IF;
END $$;
UPDATE mobile_v9_tasks SET revision=revision+1,stopped=false;
UPDATE mobile_v9_tasks SET stopped=true;
"#).execute(&pool).await.unwrap();
    let migration = database::MIGRATOR
        .iter()
        .find(|m| m.version == 202611150185)
        .unwrap();
    sqlx::raw_sql(&migration.sql).execute(&pool).await.unwrap();
    let tasks: i64 = sqlx::query_scalar("SELECT count(*) FROM mobile_v9_tasks")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        tasks, 0,
        "unreleased workspace execution data is deliberately discarded"
    );
    let obsolete: bool = sqlx::query_scalar("SELECT to_regclass('mobile_workspaces') IS NULL AND to_regclass('wecom_workspaces') IS NULL AND to_regclass('unit_receiving_storage') IS NULL").fetch_one(&pool).await.unwrap();
    assert!(obsolete);
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
async fn route_viewer_retirement_preserves_business_and_audit_data() {
    let url = std::env::var("WEKEEPER_TEST_DATABASE_URL")
        .expect("explicit isolated test database required");
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .unwrap();
    let schema = format!("retire_viewers_{}", uuid::Uuid::new_v4().simple());
    sqlx::raw_sql(&format!(
        "CREATE SCHEMA {schema}; SET search_path TO {schema},public;"
    ))
    .execute(&pool)
    .await
    .unwrap();
    for migration in database::MIGRATOR
        .iter()
        .filter(|m| m.version < 202611160188)
    {
        sqlx::raw_sql(&migration.sql).execute(&pool).await.unwrap();
    }
    sqlx::raw_sql(r#"
DO $$
DECLARE actor uuid=gen_random_uuid(); platform uuid=gen_random_uuid(); unit uuid=gen_random_uuid();
 receipt uuid=gen_random_uuid(); draft uuid=gen_random_uuid(); preview uuid=gen_random_uuid(); intent uuid=gen_random_uuid();
BEGIN
 INSERT INTO accounts(id,username,display_name,platform_admin) VALUES(actor,'retirement-admin','Admin',true);
 INSERT INTO source_callers(source_id) VALUES(platform);
 INSERT INTO business_units(id,platform_id,name,created_by,updated_by) VALUES(unit,platform,'Unit',actor,actor);
 INSERT INTO source_routes(route_id,source_id,business_unit_id,route_name,status) VALUES('kept-route',platform,unit,'Route','active');
 INSERT INTO source_route_managers(route_id,account_id,granted_by,can_recheck) VALUES('kept-route',actor,actor,true);
 INSERT INTO source_receipts(receipt_id,source_id,business_unit_id,event_id,route_id,body_sha256,schema_version)
 VALUES(receipt,platform,unit,gen_random_uuid(),'kept-route','original',2);
 INSERT INTO orchestration_messages(id,platform_id,unit_id,source_event_id,evidence_ref,original)
 VALUES(gen_random_uuid(),platform,unit,gen_random_uuid(),receipt,'{"content":"Frozen message"}');
 INSERT INTO channel_receiving_configurations(id,business_unit_id,channel,configuration,authorized_by)
 VALUES(gen_random_uuid(),unit,'mobile','{"enabled":true,"target_ids":[],"device_ids":[]}',actor);
 INSERT INTO source_access_audit(id,source_id,actor_id,action,result_code,request_id,detail)
 VALUES(gen_random_uuid(),platform,actor,'source.manager.changed','ok','retirement-test','{"route_id":"kept-route","granted":true}');
 INSERT INTO mobile_v9_task_drafts(id,intent_id,actor_id,target_ids,message,content_digest,between_targets_seconds)
 VALUES(draft,intent,actor,'{}','Frozen task','digest',30);
 INSERT INTO mobile_v9_previews(id,draft_id,draft_revision,plan,digest) VALUES(preview,draft,1,'{}','digest');
 INSERT INTO mobile_v9_tasks(id,draft_id,intent_id,preview_id,confirmed_by,confirmed_until)
 VALUES(gen_random_uuid(),draft,intent,preview,actor,clock_timestamp()+interval '1 hour');
END $$;
"#).execute(&pool).await.unwrap();
    let tables = [
        "source_routes",
        "source_receipts",
        "orchestration_messages",
        "channel_receiving_configurations",
        "source_access_audit",
        "accounts",
        "mobile_v9_task_drafts",
        "mobile_v9_previews",
        "mobile_v9_tasks",
    ];
    let mut before = Vec::new();
    for table in tables {
        let rows: serde_json::Value =
            sqlx::query_scalar(&format!("SELECT jsonb_agg(to_jsonb(t)) FROM {table} t"))
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(
            rows.as_array().is_some_and(|rows| !rows.is_empty()),
            "{table}"
        );
        before.push(rows);
    }
    let migration = database::MIGRATOR
        .iter()
        .find(|m| m.version == 202611160188)
        .unwrap();
    sqlx::raw_sql(&migration.sql).execute(&pool).await.unwrap();
    let removed: bool = sqlx::query_scalar("SELECT to_regclass('source_route_managers') IS NULL")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(removed);
    for (table, expected) in tables.into_iter().zip(before) {
        let rows: serde_json::Value =
            sqlx::query_scalar(&format!("SELECT jsonb_agg(to_jsonb(t)) FROM {table} t"))
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(rows, expected, "{table} must remain intact");
    }
    sqlx::raw_sql(&format!(
        "SET search_path TO public; DROP SCHEMA {schema} CASCADE;"
    ))
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;
}
