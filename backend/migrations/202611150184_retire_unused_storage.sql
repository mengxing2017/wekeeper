-- Retire the unused pre-orchestration storage and compatibility archives outright.
-- No current business table depends on these tables; deliberately avoid CASCADE.
DROP TRIGGER source_wecom_automation_stop ON broadcast_tasks;
DROP TRIGGER source_mobile_automation_stop ON mobile_v9_tasks;
DROP TRIGGER source_automation_receipt_event ON source_receipts;
DROP TRIGGER source_batch_summary ON broadcast_batches;
DROP TRIGGER source_official_summary ON broadcast_targets;
DROP TRIGGER source_grant_summary ON mobile_execution_grants;
DROP TRIGGER source_mobile_summary ON mobile_v9_attempts;
DROP TRIGGER source_dispatch_summary ON mobile_v9_dispatch_state;
DROP TRIGGER source_mobile_task_summary ON mobile_v9_tasks;

-- Current receipt revisions still advance; the unused history ledger is retired.
CREATE OR REPLACE FUNCTION source_receipt_transition() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF (NEW.status,NEW.processing_stage,NEW.reason_code,NEW.observed_sequence)
  IS DISTINCT FROM (OLD.status,OLD.processing_stage,OLD.reason_code,OLD.observed_sequence) THEN
  NEW.processing_revision=OLD.processing_revision+1;
  NEW.updated_at=clock_timestamp();
 END IF;
 RETURN NEW;
END $$;

DROP TABLE
 distribution_source_events,
 keyword_admissions,
 keyword_check_results,
 receiving_configuration_evidence,
 retired_distribution_evidence,
 retired_message_handoffs,
 source_advertisement_decisions,
 source_automation_task_events,
 source_automation_tasks,
 source_check_policy,
 source_check_scan,
 source_content_history,
 source_execution_generations,
 source_message_compositions,
 source_receipt_history,
 source_route_revisions,
 source_routing_decisions,
 source_summary_queue,
 source_execution_tasks;

DROP FUNCTION source_automation_channel_stopped();
DROP FUNCTION source_automation_generation_event();
DROP FUNCTION source_automation_receipt_event();
DROP FUNCTION source_automation_snapshot_fixed();
DROP FUNCTION source_check_policy_revision();
DROP FUNCTION source_execution_generation_saved();
DROP FUNCTION source_summary_dirty();
