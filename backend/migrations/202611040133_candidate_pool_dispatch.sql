-- Historical tasks keep their original singleton authorization and sequence.
ALTER TABLE mobile_v9_tasks ADD COLUMN dispatch_mode text NOT NULL DEFAULT 'legacy_sequence'
 CHECK(dispatch_mode IN ('legacy_sequence','candidate_pool'));
CREATE TABLE mobile_v9_dispatch_scopes (
 item_id uuid PRIMARY KEY REFERENCES mobile_v9_task_items(id),
 candidates jsonb NOT NULL CHECK(jsonb_typeof(candidates)='array' AND jsonb_array_length(candidates)>0)
);
CREATE TRIGGER mobile_v9_dispatch_scope_immutable BEFORE UPDATE OR DELETE ON mobile_v9_dispatch_scopes
 FOR EACH ROW EXECUTE FUNCTION mobile_capability_evidence_immutable();
CREATE TABLE mobile_v9_dispatch_state (
 item_id uuid PRIMARY KEY REFERENCES mobile_v9_dispatch_scopes(item_id),
 state text NOT NULL DEFAULT 'scheduling' CHECK(state IN ('scheduling','assigned','waiting_resources','executing','completed','needs_review','stopped','confirmation_expired')),
 reason text NOT NULL DEFAULT 'awaiting_dispatch',
 revision bigint NOT NULL DEFAULT 1,
 next_retry_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 last_failed_attempt_id uuid REFERENCES mobile_v9_attempts(id),
 updated_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE mobile_v9_dispatch_failures (
 item_id uuid NOT NULL REFERENCES mobile_v9_dispatch_scopes(item_id),
 device_id uuid NOT NULL REFERENCES execution_devices(id),
 failures integer NOT NULL DEFAULT 1 CHECK(failures>0),retry_at timestamptz NOT NULL,
 PRIMARY KEY(item_id,device_id)
);
ALTER TABLE mobile_v9_reassignments ADD COLUMN source text NOT NULL DEFAULT 'manual' CHECK(source IN ('manual','automatic'));
CREATE INDEX mobile_v9_dispatch_due ON mobile_v9_dispatch_state(next_retry_at,item_id)
 WHERE state NOT IN ('completed','stopped','confirmation_expired');
