-- Run before migration 124 in an isolated schema.
INSERT INTO execution_devices(id,name,enabled,credential_generation,execution_protocol_version,client_instance_id)
 VALUES('00000000-0000-4000-8000-000000000124','existing shared phone',true,7,3,'existing-instance');
INSERT INTO device_credentials(token_hash,device_id,generation,expires_at)
 VALUES('synthetic-current-credential','00000000-0000-4000-8000-000000000124',7,clock_timestamp()+interval '1 day');
INSERT INTO mobile_shared_delegations(id,device_id,credential_generation,scope,scope_digest)
 VALUES('00000000-0000-4000-8000-000000000125','00000000-0000-4000-8000-000000000124',7,'{"schema":"shared_mobile_device_delegation/1"}','synthetic-scope');
INSERT INTO mobile_shared_delegation_decisions(id,delegation_id,accepted,revision)
 VALUES('00000000-0000-4000-8000-000000000126','00000000-0000-4000-8000-000000000125',true,1);
CREATE TEMP TABLE preserved_current_mobile_counts AS SELECT
 (SELECT count(*) FROM mobile_v9_plans) plans,
 (SELECT count(*) FROM mobile_v9_attempts) attempts,
 (SELECT count(*) FROM mobile_v9_task_wake_outbox) wakes,
 (SELECT count(*) FROM wecom_groups) official_groups;
-- APPLY MIGRATION 124
DO $$ BEGIN
 IF NOT EXISTS(SELECT 1 FROM execution_devices WHERE id='00000000-0000-4000-8000-000000000124' AND enabled AND credential_generation=7 AND execution_protocol_version=9 AND client_instance_id='existing-instance') THEN RAISE EXCEPTION 'device identity changed'; END IF;
 IF NOT EXISTS(SELECT 1 FROM device_credentials WHERE token_hash='synthetic-current-credential' AND generation=7 AND revoked_at IS NULL) THEN RAISE EXCEPTION 'credential revoked by upgrade'; END IF;
 IF NOT EXISTS(SELECT 1 FROM mobile_shared_delegation_decisions WHERE id='00000000-0000-4000-8000-000000000126' AND accepted) THEN RAISE EXCEPTION 'current consent lost'; END IF;
 IF (SELECT plans FROM preserved_current_mobile_counts)<>(SELECT count(*) FROM mobile_v9_plans) OR (SELECT attempts FROM preserved_current_mobile_counts)<>(SELECT count(*) FROM mobile_v9_attempts) OR (SELECT wakes FROM preserved_current_mobile_counts)<>(SELECT count(*) FROM mobile_v9_task_wake_outbox) OR (SELECT official_groups FROM preserved_current_mobile_counts)<>(SELECT count(*) FROM wecom_groups) THEN RAISE EXCEPTION 'current business rows changed'; END IF;
 IF to_regclass('sending_accounts') IS NOT NULL OR to_regclass('mobile_delivery_items') IS NOT NULL OR to_regclass('mobile_shared_attempts') IS NOT NULL OR to_regclass('automation_flow_validations') IS NOT NULL OR to_regclass('mobile_device_capabilities') IS NOT NULL THEN RAISE EXCEPTION 'retired business table retained'; END IF;
 IF to_regclass('shared_mobile_resource_operations') IS NULL OR to_regclass('device_control_presence') IS NULL OR to_regclass('mobile_shared_device_leases') IS NULL THEN RAISE EXCEPTION 'current infrastructure missing'; END IF;
 -- Exercise the real presence view after deletion, including all of its joins.
 PERFORM state FROM device_control_presence WHERE id='00000000-0000-4000-8000-000000000124';
 RAISE WARNING 'LEGACY_RETIREMENT_UPGRADE_ASSERTIONS_PASSED';
END $$;
