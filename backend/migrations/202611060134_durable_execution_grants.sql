-- Single execution protocol. Historical v9 evidence stays immutable for audit only.
CREATE TABLE mobile_execution_grants (
 id uuid PRIMARY KEY, attempt_id uuid UNIQUE REFERENCES mobile_v9_attempts(id),
 inspection_id uuid REFERENCES mobile_device_target_bindings(id),
 device_id uuid NOT NULL REFERENCES execution_devices(id), credential_generation bigint NOT NULL,
 kind text NOT NULL CHECK(kind IN ('send','inspection')),
 state text NOT NULL DEFAULT 'active' CHECK(state IN ('active','completed','superseded')),
 session_closed boolean NOT NULL DEFAULT false, superseded_reason text,
 assigned_at timestamptz NOT NULL DEFAULT clock_timestamp(), accepted_at timestamptz,
 result_deadline_at timestamptz NOT NULL DEFAULT clock_timestamp()+interval '5 minutes',
 execute_before timestamptz NOT NULL DEFAULT clock_timestamp()+interval '210 seconds',
 result_received_at timestamptz, result_credential_hash text NOT NULL,
 envelope bytea NOT NULL, result_id uuid UNIQUE, result_body jsonb, result_hash text,
 receipt jsonb, applied boolean NOT NULL DEFAULT false,
 CHECK ((kind='send' AND attempt_id IS NOT NULL AND inspection_id IS NULL) OR
        (kind='inspection' AND inspection_id IS NOT NULL AND attempt_id IS NULL))
);
CREATE UNIQUE INDEX mobile_execution_device_active ON mobile_execution_grants(device_id) WHERE NOT session_closed;
CREATE INDEX mobile_execution_due ON mobile_execution_grants(result_deadline_at) WHERE state='active';
CREATE INDEX mobile_execution_unapplied ON mobile_execution_grants(id) WHERE result_id IS NOT NULL AND NOT applied;
ALTER TABLE mobile_v9_dispatch_state ADD COLUMN candidate_cursor integer NOT NULL DEFAULT 0;
-- Historical singleton scopes retain exactly the confirmed phone; no authorization expansion.
INSERT INTO mobile_v9_dispatch_scopes(item_id,candidates)
 SELECT a.item_id,jsonb_build_array(p.plan) FROM mobile_v9_attempts a JOIN mobile_v9_plans p ON p.id=a.plan_id
 WHERE a.current AND NOT EXISTS(SELECT 1 FROM mobile_v9_dispatch_scopes s WHERE s.item_id=a.item_id);
INSERT INTO mobile_v9_dispatch_state(item_id) SELECT item_id FROM mobile_v9_dispatch_scopes ON CONFLICT DO NOTHING;
-- Do not resume old UI attempts; preserve issued-call uncertainty and all old evidence.
UPDATE mobile_v9_attempts a SET state=CASE WHEN EXISTS(SELECT 1 FROM mobile_v9_invocations v WHERE v.attempt_id=a.id) THEN 'unknown' ELSE 'failed_before_call' END,
 revision=revision+1 WHERE a.state IN ('leased','prepared','invocation_issued');
UPDATE mobile_v9_send_claims SET state='closed' WHERE state='active';
UPDATE mobile_v9_quota_reservations q SET state=CASE WHEN a.state='unknown' THEN 'risk_held' ELSE 'released' END FROM mobile_v9_attempts a WHERE a.id=q.attempt_id AND q.state='reserved' AND a.state IN ('unknown','failed_before_call');
UPDATE mobile_v9_send_inspections SET state='stale',termination_reason='durable_protocol_replaced',finished_at=clock_timestamp() WHERE state IN ('pending','leased');

ALTER TABLE execution_devices DROP CONSTRAINT execution_devices_execution_protocol_version_check;
ALTER TABLE execution_devices ALTER COLUMN execution_protocol_version SET DEFAULT 10;
ALTER TABLE execution_devices ALTER COLUMN supported_protocol_versions SET DEFAULT ARRAY[10];
UPDATE execution_devices SET execution_protocol_version=10,supported_protocol_versions=ARRAY[10];
ALTER TABLE execution_devices ADD CONSTRAINT execution_devices_execution_protocol_version_check CHECK(execution_protocol_version=10);
CREATE TABLE mobile_execution_usage_budgets (
 execution_id uuid NOT NULL REFERENCES mobile_execution_grants(id), usage_id uuid NOT NULL REFERENCES mobile_usages(id),budget_root uuid NOT NULL,
 PRIMARY KEY(execution_id,budget_root)
);
CREATE INDEX mobile_execution_usage_root ON mobile_execution_usage_budgets(budget_root);

-- Cancellation is scoped to the immutable execution identity, never to a later task.
ALTER TABLE device_control_commands DROP CONSTRAINT device_control_commands_kind_check;
ALTER TABLE device_control_commands ADD CONSTRAINT device_control_commands_kind_check
 CHECK(kind IN ('task_available','configuration_changed','pause_execution','resume_execution','request_status','cancel_execution'));

CREATE FUNCTION notify_execution_superseded() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 INSERT INTO device_control_commands(id,device_id,kind,resource_id)
 VALUES(gen_random_uuid(),NEW.device_id,'cancel_execution',NEW.id);
 RETURN NEW;
END $$;
CREATE TRIGGER execution_superseded AFTER UPDATE OF state ON mobile_execution_grants
 FOR EACH ROW WHEN (NEW.state='superseded' AND OLD.state='active')
 EXECUTE FUNCTION notify_execution_superseded();

CREATE TABLE mobile_execution_diagnostics (
 id uuid PRIMARY KEY, execution_id uuid NOT NULL REFERENCES mobile_execution_grants(id),
 device_id uuid NOT NULL REFERENCES execution_devices(id), credential_generation bigint NOT NULL,
 code text NOT NULL, stage text NOT NULL, details jsonb NOT NULL,
 created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);

ALTER TABLE mobile_v9_attempts ADD COLUMN termination_reason text;
