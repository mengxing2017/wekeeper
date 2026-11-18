-- Current execution grants and independent human evidence are the only result sources.
ALTER TABLE mobile_v9_reviews ADD COLUMN execution_id uuid NOT NULL REFERENCES mobile_execution_grants(id);
ALTER TABLE mobile_v9_reviews ADD COLUMN attempt_revision bigint NOT NULL CHECK (attempt_revision>0);
CREATE INDEX mobile_review_attempt_evidence ON mobile_v9_reviews(attempt_id,created_at DESC,id DESC);
ALTER TABLE mobile_v9_attempts DROP CONSTRAINT mobile_v9_attempts_state_check;
ALTER TABLE mobile_v9_attempts ADD CONSTRAINT mobile_v9_attempts_state_check CHECK (state IN (
 'queued','leased','observed_sent','unknown','failed_before_call','invalidated','cancelled',
 'verified_not_sent','manually_confirmed_sent'
));

-- Retire unused maintenance, platform UI configuration and legacy execution objects.
-- Explicit dependency order; current grants, diagnostics, dispatch and audit remain intact.
DROP TABLE mobile_capability_deliveries;
DROP TABLE mobile_v9_diagnostics;
DROP TABLE mobile_v9_send_inspections;
DROP TABLE mobile_v9_send_events;
DROP TABLE mobile_v9_risk_guards;
DROP TABLE mobile_v9_invocations;
DROP TABLE mobile_v9_support_events;
DROP TABLE mobile_v9_support_requests;
DROP TABLE mobile_v9_activations;
DROP TABLE mobile_v9_activation_events;
DROP TABLE mobile_v9_acceptance_runs;
DROP TABLE mobile_v9_adapter_withdrawals;
DROP TABLE mobile_v9_adapter_releases;
DROP TABLE mobile_v9_baselines;
DROP TABLE mobile_v9_human_samples;
DROP TABLE mobile_v9_flows;
DROP TABLE mobile_v9_artifacts;
DROP TABLE mobile_runtime_configuration_current;
DROP TABLE mobile_runtime_configurations;

-- Human observations retain their real origin and point to the original execution.
CREATE OR REPLACE FUNCTION channel_mobile_review_observation() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
 INSERT INTO channel_execution_observations(channel,task_id,target_id,attempt_id,execution_id,evidence_type,observation_source,normalized_result,detail)
 SELECT 'mobile',i.task_id,i.target_id,a.id,NEW.execution_id,'operator_reconciled','operator',
 CASE WHEN NEW.decision='manually_confirmed_sent' THEN 'sent' WHEN NEW.decision='verified_not_sent' THEN 'failed' ELSE 'unknown' END,
 jsonb_build_object('review_id',NEW.id,'decision',NEW.decision,'actor_id',NEW.reviewer,'attempt_revision',NEW.attempt_revision)
 FROM mobile_v9_attempts a JOIN mobile_v9_task_items i ON i.id=a.item_id WHERE a.id=NEW.attempt_id;
 RETURN NEW;
END $$;
CREATE OR REPLACE FUNCTION mobile_record_review() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
 INSERT INTO mobile_v9_dispatch_events(intake_id,item_id,attempt_id,execution_id,kind,reason,result,evidence)
 SELECT i.id,a.item_id,a.id,NEW.execution_id,'manual_review',NEW.decision,NEW.decision,
 jsonb_build_object('review_id',NEW.id,'reviewer',NEW.reviewer,'attempt_revision',NEW.attempt_revision)
 FROM mobile_v9_attempts a JOIN mobile_dispatch_intakes i ON i.item_id=a.item_id WHERE a.id=NEW.attempt_id;
 RETURN NEW;
END $$;
