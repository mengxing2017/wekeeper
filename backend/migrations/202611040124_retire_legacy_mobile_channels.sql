-- One phone sending business: v9. Device identity and current resource history stay intact.
-- No old task is converted into a new executable task, consent or permit.
CREATE TABLE device_registry_operations (
 principal text NOT NULL,operation text NOT NULL,idempotency_key text NOT NULL,
 request_hash text NOT NULL,response jsonb NOT NULL,created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 PRIMARY KEY(principal,operation,idempotency_key)
);
CREATE TABLE device_registry_events (
 id uuid PRIMARY KEY,principal text NOT NULL,operation text NOT NULL,device_id uuid,
 detail jsonb NOT NULL,created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
ALTER TABLE mobile_execution_operations RENAME TO shared_mobile_resource_operations;
ALTER TABLE mobile_execution_audit_events RENAME TO shared_mobile_resource_events;
-- Upgrade does not rotate credentials, change device IDs or invalidate current v9 consent.
ALTER TABLE execution_devices DROP CONSTRAINT execution_devices_execution_protocol_version_check;
ALTER TABLE execution_devices ALTER COLUMN execution_protocol_version SET DEFAULT 9;
ALTER TABLE execution_devices ALTER COLUMN supported_protocol_versions SET DEFAULT ARRAY[9];
UPDATE execution_devices SET execution_protocol_version=9,supported_protocol_versions=ARRAY[9];
ALTER TABLE execution_devices ADD CONSTRAINT execution_devices_execution_protocol_version_check CHECK(execution_protocol_version=9);
-- Limited read-only incident archive; never used by scheduling or permission checks.
CREATE TABLE retired_mobile_incident_archive (
 source_table text NOT NULL,source_id uuid NOT NULL,record jsonb NOT NULL,
 archived_at timestamptz NOT NULL DEFAULT clock_timestamp(),PRIMARY KEY(source_table,source_id)
);
INSERT INTO retired_mobile_incident_archive(source_table,source_id,record) SELECT 'mobile_execution_claims',t.id,(to_jsonb(t) - ARRAY['response','encrypted_response','token_hash','claim_token_hash','result_credential_hash','recovery_credential_hash','invocation_secret_hash','challenge']) FROM mobile_execution_claims t WHERE t.state IN ('claimed','prepared','permit_issued') OR EXISTS(SELECT 1 FROM mobile_delivery_items i WHERE i.id=t.item_id AND i.state='unknown');
INSERT INTO retired_mobile_incident_archive(source_table,source_id,record) SELECT 'mobile_send_permits',t.id,(to_jsonb(t) - ARRAY['response','encrypted_response','token_hash','claim_token_hash','result_credential_hash','recovery_credential_hash','invocation_secret_hash','challenge']) FROM mobile_send_permits t WHERE t.state IN ('issued','unknown');
INSERT INTO retired_mobile_incident_archive(source_table,source_id,record) SELECT 'mobile_delivery_items',t.id,(to_jsonb(t) - ARRAY['response','encrypted_response','token_hash','claim_token_hash','result_credential_hash','recovery_credential_hash','invocation_secret_hash','challenge']) FROM mobile_delivery_items t WHERE t.state='unknown';
INSERT INTO retired_mobile_incident_archive(source_table,source_id,record) SELECT 'mobile_shared_attempts',t.id,(to_jsonb(t) - ARRAY['response','encrypted_response','token_hash','claim_token_hash','result_credential_hash','recovery_credential_hash','invocation_secret_hash','challenge']) FROM mobile_shared_attempts t WHERE t.state IN ('leased','prepared','invocation_issued','unknown');
INSERT INTO retired_mobile_incident_archive(source_table,source_id,record) SELECT 'mobile_shared_send_claims',t.id,(to_jsonb(t) - ARRAY['response','encrypted_response','token_hash','claim_token_hash','result_credential_hash','recovery_credential_hash','invocation_secret_hash','challenge']) FROM mobile_shared_send_claims t WHERE t.state='active';
INSERT INTO retired_mobile_incident_archive(source_table,source_id,record) SELECT 'mobile_shared_invocations',t.id,(to_jsonb(t) - ARRAY['response','encrypted_response','token_hash','claim_token_hash','result_credential_hash','recovery_credential_hash','invocation_secret_hash','challenge']) FROM mobile_shared_invocations t WHERE t.state='risk_held';
-- SQL function has catalog dependencies on old dispatch tables.
DROP FUNCTION delivery_scope_owner(uuid);
DROP TABLE sending_accounts,
 device_account_bindings,
 delivery_groups,
 sender_group_bindings,
 mobile_delivery_settings,
 mobile_delivery_tasks,
 mobile_delivery_items,
 mobile_execution_claims,
 mobile_send_permits,
 delivery_usage,
 delivery_dedup_reservations,
 delivery_scheduler_cursors,
 mobile_delivery_operations,
 mobile_delivery_events,
 device_request_windows,
 device_pairing_windows,
 mobile_delivery_strategies,
 mobile_delivery_strategy_runs,
 client_rule_bundle_versions,
 client_rule_releases,
 delivery_execution_defaults,
 mobile_delivery_strategy_targets,
 mobile_delivery_run_items,
 mobile_delivery_run_progress,
 mobile_delivery_run_execution_revisions,
 delivery_content_scope_owners,
 automation_apps,
 automation_capabilities,
 device_environment_snapshots,
 device_app_bindings,
 automation_component_versions,
 device_validation_jobs,
 device_validation_events,
 automation_component_releases,
 automation_execution_manifests,
 automation_business_previews,
 automation_readiness_tickets,
 device_execution_authorizations,
 automation_pacing_profiles,
 automation_flow_versions,
 automation_flow_validations,
 automation_flow_validation_events,
 automation_flow_releases,
 automation_device_groups,
 automation_device_group_members,
 automation_execution_assignments,
 automation_dispatch_batches,
 automation_dispatch_plans,
 automation_task_deliveries,
 automation_dispatch_preparations,
 automation_dispatch_events,
 automation_execution_failures,
 automation_business_authorizations,
 mobile_device_school_grants,
 mobile_device_consents,
 mobile_device_consent_operations,
 mobile_device_consent_events,
 mobile_execution_flow_versions,
 mobile_execution_flow_validations,
 mobile_execution_flow_releases,
 mobile_execution_assignments,
 mobile_execution_validation_operations,
 mobile_execution_validation_events,
 mobile_device_capabilities,
 mobile_device_capability_versions,
 mobile_device_capability_consents,
 mobile_device_capability_operations,
 mobile_device_capability_events,
 mobile_adapter_releases,
 mobile_adapter_withdrawals,
 mobile_device_environments,
 mobile_device_environment_observations,
 mobile_capability_openings,
 mobile_adapter_support_requests,
 mobile_field_check_invitations,
 mobile_field_checks,
 mobile_field_check_results,
 mobile_field_check_operations,
 legacy_mobile_maintenance_retirement,
 mobile_shared_send_policy_versions,
 mobile_send_adapter_releases,
 mobile_send_adapter_withdrawals,
 mobile_shared_acceptance_runs,
 mobile_shared_release_scopes,
 mobile_shared_send_openings,
 mobile_shared_send_consents,
 mobile_shared_send_inspections,
 mobile_shared_task_drafts,
 mobile_shared_previews,
 mobile_shared_tasks,
 mobile_shared_task_items,
 mobile_shared_plans,
 mobile_shared_attempts,
 mobile_shared_send_claims,
 mobile_shared_preparations,
 mobile_shared_invocations,
 mobile_shared_send_events,
 mobile_shared_risk_guards,
 mobile_shared_quota_reservations,
 mobile_shared_scheduler_turns,
 mobile_shared_reviews,
 mobile_shared_operations,
 mobile_shared_reassignments;
DROP FUNCTION IF EXISTS automation_immutable();
DROP FUNCTION IF EXISTS maintain_delivery_content_scope();
DROP FUNCTION IF EXISTS mobile_adapter_serialize_withdrawal();
DROP FUNCTION IF EXISTS mobile_field_invitation_snapshot_immutable();
DROP FUNCTION IF EXISTS mobile_field_check_snapshot_immutable();
DROP FUNCTION IF EXISTS reject_retired_mobile_creation();
DROP FUNCTION IF EXISTS reject_retired_mobile_activation();
DROP FUNCTION IF EXISTS mobile_shared_scope_acceptance();
CREATE TRIGGER retired_mobile_incident_read_only BEFORE UPDATE OR DELETE ON retired_mobile_incident_archive
 FOR EACH ROW EXECUTE FUNCTION mobile_capability_evidence_immutable();
-- These notifications belonged solely to deleted account/task tables.
DROP FUNCTION notify_device_control_task();
DROP FUNCTION notify_device_control_binding();
DROP FUNCTION notify_device_control_dispatch();
DROP FUNCTION notify_device_control_unbinding();
