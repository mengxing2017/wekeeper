-- New shared v8 plane. Historical v4/v5/v6 work and v7 inspections are not upgraded.
CREATE TABLE mobile_shared_send_policy_versions (
 id uuid PRIMARY KEY, revision bigint NOT NULL UNIQUE CHECK(revision>0),
 mode text NOT NULL CHECK(mode IN ('disabled','draft_only','preview_only','test_send','production_allowlist')),
 timezone text NOT NULL, allowed_windows jsonb NOT NULL,
 limits jsonb NOT NULL, created_by uuid NOT NULL REFERENCES accounts(id), reason text NOT NULL,
 created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TRIGGER mobile_shared_policy_immutable BEFORE UPDATE OR DELETE ON mobile_shared_send_policy_versions FOR EACH ROW EXECUTE FUNCTION mobile_capability_evidence_immutable();
CREATE TABLE mobile_send_adapter_releases (
 id uuid PRIMARY KEY, name text NOT NULL, version text NOT NULL, environment jsonb NOT NULL,
 flow jsonb NOT NULL, content_digest text NOT NULL CHECK(content_digest ~ '^[0-9a-f]{64}$'),
 source_revision text NOT NULL, evidence jsonb NOT NULL CHECK(jsonb_array_length(evidence)>0),
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(), UNIQUE(name,version)
);
CREATE TRIGGER mobile_send_release_immutable BEFORE UPDATE OR DELETE ON mobile_send_adapter_releases FOR EACH ROW EXECUTE FUNCTION mobile_capability_evidence_immutable();
CREATE TABLE mobile_send_adapter_withdrawals (
 release_id uuid PRIMARY KEY REFERENCES mobile_send_adapter_releases(id), reason text NOT NULL, created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TRIGGER mobile_send_withdrawal_lock BEFORE INSERT ON mobile_send_adapter_withdrawals FOR EACH ROW EXECUTE FUNCTION mobile_adapter_serialize_withdrawal();
CREATE TRIGGER mobile_send_withdrawal_immutable BEFORE UPDATE OR DELETE ON mobile_send_adapter_withdrawals FOR EACH ROW EXECUTE FUNCTION mobile_capability_evidence_immutable();
CREATE TABLE mobile_shared_acceptance_runs (
 id uuid PRIMARY KEY,device_id uuid NOT NULL REFERENCES execution_devices(id),school_id uuid NOT NULL,target_id uuid NOT NULL,
 release_id uuid NOT NULL REFERENCES mobile_send_adapter_releases(id),environment_snapshot_id uuid NOT NULL,
 stage text NOT NULL CHECK(stage IN ('test_send','production_allowlist')),actual_apk_sha256 text NOT NULL CHECK(actual_apk_sha256 ~ '^[0-9a-f]{64}$'),
 evidence_digest text NOT NULL CHECK(evidence_digest ~ '^[0-9a-f]{64}$'),evidence jsonb NOT NULL,
 reviewer_id uuid NOT NULL REFERENCES accounts(id),created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 FOREIGN KEY(school_id,target_id) REFERENCES mobile_targets(school_id,id),
 FOREIGN KEY(device_id,environment_snapshot_id) REFERENCES mobile_device_environments(device_id,id)
);
CREATE TRIGGER mobile_shared_acceptance_immutable BEFORE UPDATE OR DELETE ON mobile_shared_acceptance_runs FOR EACH ROW EXECUTE FUNCTION mobile_capability_evidence_immutable();
CREATE TABLE mobile_shared_release_scopes (
 id uuid PRIMARY KEY, device_id uuid NOT NULL REFERENCES execution_devices(id), school_id uuid NOT NULL,
 target_id uuid NOT NULL, release_id uuid NOT NULL REFERENCES mobile_send_adapter_releases(id),
 environment_snapshot_id uuid NOT NULL, stage text NOT NULL CHECK(stage IN ('test_send','production_allowlist')),
 acceptance_id uuid NOT NULL REFERENCES mobile_shared_acceptance_runs(id), expires_at timestamptz NOT NULL,
 FOREIGN KEY(school_id,target_id) REFERENCES mobile_targets(school_id,id),
 FOREIGN KEY(device_id,environment_snapshot_id) REFERENCES mobile_device_environments(device_id,id)
);
CREATE TABLE mobile_shared_send_openings (
 id uuid PRIMARY KEY,device_id uuid NOT NULL REFERENCES execution_devices(id),credential_generation bigint NOT NULL,
 release_id uuid NOT NULL REFERENCES mobile_send_adapter_releases(id),environment_snapshot_id uuid NOT NULL,
 scope jsonb NOT NULL,scope_digest text NOT NULL,revision bigint NOT NULL DEFAULT 1,
 created_by uuid NOT NULL REFERENCES accounts(id),created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 UNIQUE(device_id,id),FOREIGN KEY(device_id,environment_snapshot_id) REFERENCES mobile_device_environments(device_id,id)
);
CREATE TABLE mobile_shared_send_consents (
 id uuid PRIMARY KEY,opening_id uuid NOT NULL REFERENCES mobile_shared_send_openings(id),device_id uuid NOT NULL,
 credential_generation bigint NOT NULL,scope_digest text NOT NULL,accepted boolean NOT NULL,revision bigint NOT NULL,
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),UNIQUE(opening_id,revision),
 FOREIGN KEY(device_id,opening_id) REFERENCES mobile_shared_send_openings(device_id,id)
);
CREATE TRIGGER mobile_shared_consent_immutable BEFORE UPDATE OR DELETE ON mobile_shared_send_consents FOR EACH ROW EXECUTE FUNCTION mobile_capability_evidence_immutable();
CREATE TABLE mobile_shared_send_inspections (
 id uuid PRIMARY KEY, school_id uuid NOT NULL, device_id uuid NOT NULL, target_id uuid NOT NULL,binding_id uuid NOT NULL,
 opening_id uuid NOT NULL,consent_id uuid NOT NULL REFERENCES mobile_shared_send_consents(id),
 dependencies jsonb NOT NULL,requested_by uuid NOT NULL REFERENCES accounts(id), state text NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','leased','passed','failed','stale')),
 challenge text NOT NULL,lease_fence bigint,runtime_epoch uuid,result jsonb,expires_at timestamptz NOT NULL DEFAULT clock_timestamp()+interval '24 hours',
 FOREIGN KEY(school_id,device_id,target_id,binding_id) REFERENCES mobile_device_target_bindings(school_id,device_id,target_id,id),
 FOREIGN KEY(device_id,opening_id) REFERENCES mobile_shared_send_openings(device_id,id),UNIQUE(school_id,id)
);
CREATE TABLE mobile_shared_runtime (
 device_id uuid PRIMARY KEY REFERENCES execution_devices(id),credential_generation bigint NOT NULL,runtime_epoch uuid NOT NULL,
 ui_ready boolean NOT NULL,paused boolean NOT NULL,quiescent boolean NOT NULL,
 observed_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE mobile_shared_task_drafts (
 id uuid PRIMARY KEY,school_id uuid NOT NULL REFERENCES schools(id),intent_id uuid NOT NULL UNIQUE,actor_id uuid NOT NULL REFERENCES accounts(id),
 revision bigint NOT NULL DEFAULT 1,target_ids uuid[] NOT NULL,message text NOT NULL,content_digest text NOT NULL,
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),UNIQUE(school_id,id)
);
CREATE TABLE mobile_shared_previews (
 id uuid PRIMARY KEY,school_id uuid NOT NULL,draft_id uuid NOT NULL,draft_revision bigint NOT NULL,
 plan jsonb NOT NULL,digest text NOT NULL,policy_id uuid REFERENCES mobile_shared_send_policy_versions(id),
 expires_at timestamptz NOT NULL DEFAULT clock_timestamp()+interval '10 minutes',
 FOREIGN KEY(school_id,draft_id) REFERENCES mobile_shared_task_drafts(school_id,id),UNIQUE(school_id,id)
);
CREATE TABLE mobile_shared_tasks (
 id uuid PRIMARY KEY,school_id uuid NOT NULL,draft_id uuid NOT NULL UNIQUE,intent_id uuid NOT NULL UNIQUE,
 preview_id uuid NOT NULL UNIQUE,confirmed_by uuid NOT NULL REFERENCES accounts(id),revision bigint NOT NULL DEFAULT 1,
 stopped boolean NOT NULL DEFAULT false,created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 FOREIGN KEY(school_id,draft_id) REFERENCES mobile_shared_task_drafts(school_id,id),
 FOREIGN KEY(school_id,preview_id) REFERENCES mobile_shared_previews(school_id,id),UNIQUE(school_id,id)
);
CREATE TABLE mobile_shared_task_items (
 id uuid PRIMARY KEY,school_id uuid NOT NULL,task_id uuid NOT NULL,intent_id uuid NOT NULL,target_id uuid NOT NULL,position integer NOT NULL,
 FOREIGN KEY(school_id,task_id) REFERENCES mobile_shared_tasks(school_id,id),FOREIGN KEY(school_id,target_id) REFERENCES mobile_targets(school_id,id),
 UNIQUE(school_id,intent_id,target_id),UNIQUE(school_id,id),UNIQUE(school_id,id,target_id)
);
CREATE TABLE mobile_shared_plans (
 id uuid PRIMARY KEY,school_id uuid NOT NULL,item_id uuid NOT NULL,target_id uuid NOT NULL,device_id uuid NOT NULL REFERENCES execution_devices(id),
 snapshot_id uuid NOT NULL,plan jsonb NOT NULL,digest text NOT NULL,content_digest text NOT NULL,
 FOREIGN KEY(school_id,item_id,target_id) REFERENCES mobile_shared_task_items(school_id,id,target_id),
 FOREIGN KEY(school_id,snapshot_id) REFERENCES distribution_message_snapshots(school_id,id),UNIQUE(school_id,id),UNIQUE(school_id,item_id,id)
);
CREATE TRIGGER mobile_shared_plan_immutable BEFORE UPDATE OR DELETE ON mobile_shared_plans FOR EACH ROW EXECUTE FUNCTION mobile_capability_evidence_immutable();
CREATE TABLE mobile_shared_attempts (
 id uuid PRIMARY KEY,school_id uuid NOT NULL,item_id uuid NOT NULL,plan_id uuid NOT NULL,
 confirmed_by uuid NOT NULL REFERENCES accounts(id),
 attempt_no integer NOT NULL CHECK(attempt_no>0),current boolean NOT NULL DEFAULT true,
 state text NOT NULL DEFAULT 'queued' CHECK(state IN ('queued','leased','prepared','invocation_issued','observed_sent','unknown','failed_before_call','invalidated','cancelled','verified_not_sent','manually_confirmed_sent')),
 revision bigint NOT NULL DEFAULT 1,not_before timestamptz NOT NULL DEFAULT clock_timestamp(),
 FOREIGN KEY(school_id,item_id,plan_id) REFERENCES mobile_shared_plans(school_id,item_id,id),
 UNIQUE(item_id,attempt_no),UNIQUE(school_id,id),UNIQUE(school_id,item_id,id)
);
CREATE UNIQUE INDEX mobile_shared_one_current ON mobile_shared_attempts(item_id) WHERE current;
CREATE TABLE mobile_shared_send_claims (
 id uuid PRIMARY KEY,school_id uuid NOT NULL,attempt_id uuid NOT NULL,device_id uuid NOT NULL REFERENCES execution_devices(id),
 credential_generation bigint NOT NULL,runtime_epoch uuid NOT NULL,lease_fence bigint NOT NULL,
 recovery_credential_hash text NOT NULL,encrypted_response bytea,
 state text NOT NULL CHECK(state IN ('active','closed')),created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 expires_at timestamptz NOT NULL,FOREIGN KEY(school_id,attempt_id) REFERENCES mobile_shared_attempts(school_id,id),UNIQUE(school_id,id)
);
CREATE UNIQUE INDEX mobile_shared_claim_device ON mobile_shared_send_claims(device_id) WHERE state='active';
CREATE UNIQUE INDEX mobile_shared_claim_attempt ON mobile_shared_send_claims(attempt_id) WHERE state='active';
CREATE TABLE mobile_shared_preparations (
 id uuid PRIMARY KEY,school_id uuid NOT NULL,claim_id uuid NOT NULL,body jsonb NOT NULL,
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),FOREIGN KEY(school_id,claim_id) REFERENCES mobile_shared_send_claims(school_id,id)
);
CREATE TABLE mobile_shared_invocations (
 id uuid PRIMARY KEY,school_id uuid NOT NULL,attempt_id uuid NOT NULL UNIQUE,claim_id uuid NOT NULL UNIQUE,
 device_id uuid NOT NULL REFERENCES execution_devices(id),credential_generation bigint NOT NULL,lease_fence bigint NOT NULL,
 issued_at timestamptz NOT NULL DEFAULT clock_timestamp(),execute_before timestamptz NOT NULL,
 state text NOT NULL CHECK(state IN ('risk_held','consumed','released')),response jsonb NOT NULL, encrypted_response bytea NOT NULL, invocation_secret_hash text NOT NULL,result_credential_hash text NOT NULL,
 FOREIGN KEY(school_id,attempt_id) REFERENCES mobile_shared_attempts(school_id,id),
 FOREIGN KEY(school_id,claim_id) REFERENCES mobile_shared_send_claims(school_id,id),UNIQUE(school_id,id)
);
CREATE TABLE mobile_shared_send_events (
 id uuid PRIMARY KEY,school_id uuid NOT NULL,attempt_id uuid NOT NULL,invocation_id uuid,device_id uuid NOT NULL,
 body jsonb NOT NULL,encrypted_body bytea,body_hash text NOT NULL,receipt jsonb NOT NULL,created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 FOREIGN KEY(school_id,attempt_id) REFERENCES mobile_shared_attempts(school_id,id),
 FOREIGN KEY(school_id,invocation_id) REFERENCES mobile_shared_invocations(school_id,id)
);
CREATE TRIGGER mobile_shared_event_immutable BEFORE UPDATE OR DELETE ON mobile_shared_send_events FOR EACH ROW EXECUTE FUNCTION mobile_capability_evidence_immutable();
CREATE TABLE mobile_shared_risk_guards (
 school_id uuid NOT NULL,intent_id uuid NOT NULL,target_id uuid NOT NULL,attempt_id uuid NOT NULL,
 invocation_id uuid NOT NULL,state text NOT NULL CHECK(state IN ('active','closed')),
 PRIMARY KEY(school_id,intent_id,target_id),FOREIGN KEY(school_id,attempt_id) REFERENCES mobile_shared_attempts(school_id,id),
 FOREIGN KEY(school_id,invocation_id) REFERENCES mobile_shared_invocations(school_id,id)
);
CREATE TABLE mobile_shared_quota_reservations (
 attempt_id uuid NOT NULL REFERENCES mobile_shared_attempts(id),dimension_key text NOT NULL,
 state text NOT NULL CHECK(state IN ('reserved','risk_held','consumed','released')),
 recorded_at timestamptz NOT NULL DEFAULT clock_timestamp(),PRIMARY KEY(attempt_id,dimension_key)
);
CREATE INDEX mobile_shared_quota_window ON mobile_shared_quota_reservations(dimension_key,recorded_at) WHERE state<>'released';
CREATE TABLE mobile_shared_scheduler_turns (
 device_id uuid NOT NULL REFERENCES execution_devices(id),school_id uuid NOT NULL REFERENCES schools(id),
 sequence bigint GENERATED ALWAYS AS IDENTITY,PRIMARY KEY(device_id,school_id)
);
CREATE TABLE mobile_shared_reviews (
 id uuid PRIMARY KEY,school_id uuid NOT NULL,attempt_id uuid NOT NULL,reviewer uuid NOT NULL REFERENCES accounts(id),
 decision text NOT NULL CHECK(decision IN ('manually_confirmed_sent','verified_not_sent','still_unknown')),
 evidence text NOT NULL,created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 FOREIGN KEY(school_id,attempt_id) REFERENCES mobile_shared_attempts(school_id,id)
);
CREATE TRIGGER mobile_shared_review_immutable BEFORE UPDATE OR DELETE ON mobile_shared_reviews FOR EACH ROW EXECUTE FUNCTION mobile_capability_evidence_immutable();
CREATE TABLE mobile_shared_operations (
 principal_id uuid NOT NULL,generation bigint NOT NULL,scope text NOT NULL,idempotency_key text NOT NULL,
 request_hash text NOT NULL,response jsonb NOT NULL,created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 PRIMARY KEY(principal_id,generation,scope,idempotency_key)
);
-- No example release, acceptance scope, consent or unlimited production policy is seeded.

CREATE TRIGGER mobile_shared_opening_immutable BEFORE UPDATE OR DELETE ON mobile_shared_send_openings FOR EACH ROW EXECUTE FUNCTION mobile_capability_evidence_immutable();
CREATE TRIGGER mobile_shared_preparation_immutable BEFORE UPDATE OR DELETE ON mobile_shared_preparations FOR EACH ROW EXECUTE FUNCTION mobile_capability_evidence_immutable();

CREATE TABLE mobile_shared_reassignments (
 id uuid PRIMARY KEY, school_id uuid NOT NULL, old_attempt_id uuid NOT NULL, new_attempt_id uuid NOT NULL UNIQUE,
 actor_id uuid NOT NULL REFERENCES accounts(id),reason text NOT NULL,created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 FOREIGN KEY(school_id,old_attempt_id) REFERENCES mobile_shared_attempts(school_id,id),
 FOREIGN KEY(school_id,new_attempt_id) REFERENCES mobile_shared_attempts(school_id,id)
);
CREATE TRIGGER mobile_shared_reassignment_immutable BEFORE UPDATE OR DELETE ON mobile_shared_reassignments FOR EACH ROW EXECUTE FUNCTION mobile_capability_evidence_immutable();

-- Acceptance is published by a reviewed engineering artifact, never by a generic admin flag.
CREATE FUNCTION mobile_shared_scope_acceptance() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE r mobile_shared_acceptance_runs%ROWTYPE;
BEGIN
 PERFORM pg_advisory_xact_lock(78234610243);
 SELECT * INTO r FROM mobile_shared_acceptance_runs WHERE id=NEW.acceptance_id;
 IF NOT FOUND OR (r.device_id,r.school_id,r.target_id,r.release_id,r.environment_snapshot_id,r.stage) IS DISTINCT FROM (NEW.device_id,NEW.school_id,NEW.target_id,NEW.release_id,NEW.environment_snapshot_id,NEW.stage) THEN
  RAISE EXCEPTION 'acceptance identity mismatch';
 END IF;
 IF r.evidence->>'preflight_passed' IS DISTINCT FROM 'true' THEN RAISE EXCEPTION 'preflight evidence required'; END IF;
 IF NEW.stage='production_allowlist' AND (jsonb_typeof(r.evidence->'cases') IS DISTINCT FROM 'object' OR coalesce((r.evidence->>'soak_hours')::numeric,0)<72 OR r.evidence->>'soak_passed' IS DISTINCT FROM 'true' OR r.evidence->>'historical_debt_closed' IS DISTINCT FROM 'true') THEN
  RAISE EXCEPTION 'production acceptance incomplete';
 END IF;
 IF NEW.stage='production_allowlist' AND EXISTS(SELECT 1 FROM generate_series(1,40) n WHERE r.evidence->'cases'->>('S'||lpad(n::text,2,'0')) IS DISTINCT FROM 'passed') THEN
  RAISE EXCEPTION 'S01-S40 required';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER mobile_shared_scope_acceptance BEFORE INSERT ON mobile_shared_release_scopes FOR EACH ROW EXECUTE FUNCTION mobile_shared_scope_acceptance();
CREATE TRIGGER mobile_shared_scope_immutable BEFORE UPDATE OR DELETE ON mobile_shared_release_scopes FOR EACH ROW EXECUTE FUNCTION mobile_capability_evidence_immutable();

CREATE TRIGGER mobile_shared_preview_immutable BEFORE UPDATE OR DELETE ON mobile_shared_previews FOR EACH ROW EXECUTE FUNCTION mobile_capability_evidence_immutable();
