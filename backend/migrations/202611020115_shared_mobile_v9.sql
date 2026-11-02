-- v9 has independent wire/intent ledgers; physical leases and delegation remain shared.
CREATE TABLE mobile_v9_send_policy_versions (
 id uuid PRIMARY KEY,revision bigint NOT NULL UNIQUE CHECK(revision>0),mode text NOT NULL CHECK(mode IN ('disabled','draft_only','preview_only','enabled')),
 allowed_stages jsonb NOT NULL,confirmation_seconds bigint NOT NULL,inspection_seconds bigint NOT NULL,retry_seconds bigint NOT NULL,retry_limit bigint NOT NULL,
 timezone text NOT NULL,allowed_windows jsonb NOT NULL,limits jsonb NOT NULL,reason text NOT NULL,created_by uuid NOT NULL REFERENCES accounts(id),created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE mobile_v9_artifacts (
 id uuid PRIMARY KEY,kind text NOT NULL CHECK(kind IN ('apk','evidence')),name text NOT NULL,sha256 text NOT NULL CHECK(sha256~'^[0-9a-f]{64}$'),
 content bytea NOT NULL CHECK(octet_length(content)>0 AND octet_length(content)<=134217728),created_by uuid NOT NULL REFERENCES accounts(id),created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE mobile_v9_flows (
 id uuid PRIMARY KEY,name text NOT NULL,revision bigint NOT NULL DEFAULT 1,flow jsonb NOT NULL,digest text NOT NULL,
 created_by uuid NOT NULL REFERENCES accounts(id),created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE mobile_v9_baselines (
 id uuid PRIMARY KEY,flow_id uuid NOT NULL REFERENCES mobile_v9_flows(id),device_id uuid NOT NULL REFERENCES execution_devices(id),
 samples jsonb NOT NULL,evidence_ids uuid[] NOT NULL,min_interval_ms bigint NOT NULL CHECK(min_interval_ms>=1000),
 recorded_by uuid NOT NULL REFERENCES accounts(id),created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE mobile_v9_adapter_releases (
 id uuid PRIMARY KEY,name text NOT NULL,version text NOT NULL,flow_id uuid NOT NULL REFERENCES mobile_v9_flows(id),
 flow jsonb NOT NULL,content_digest text NOT NULL,baseline_id uuid NOT NULL REFERENCES mobile_v9_baselines(id),evidence_ids uuid[] NOT NULL,
 created_by uuid NOT NULL REFERENCES accounts(id),created_at timestamptz NOT NULL DEFAULT clock_timestamp(),UNIQUE(name,version)
);
CREATE TABLE mobile_v9_adapter_withdrawals (
 release_id uuid PRIMARY KEY REFERENCES mobile_v9_adapter_releases(id),reason text NOT NULL,created_by uuid NOT NULL REFERENCES accounts(id),created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE mobile_v9_activations (
 capability text PRIMARY KEY CHECK(capability='wecom.group_text'),release_id uuid NOT NULL REFERENCES mobile_v9_adapter_releases(id),revision bigint NOT NULL,updated_by uuid NOT NULL REFERENCES accounts(id)
);
CREATE TABLE mobile_v9_activation_events (
 id uuid PRIMARY KEY,release_id uuid NOT NULL REFERENCES mobile_v9_adapter_releases(id),revision bigint NOT NULL,actor_id uuid NOT NULL REFERENCES accounts(id),reason text NOT NULL,created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE mobile_v9_acceptance_runs (
 id uuid PRIMARY KEY,release_id uuid NOT NULL REFERENCES mobile_v9_adapter_releases(id),stage text NOT NULL CHECK(stage IN ('test_send','production_allowlist')),
 apk_id uuid NOT NULL REFERENCES mobile_v9_artifacts(id),evidence_ids uuid[] NOT NULL,validation_ids uuid[] NOT NULL,
 cases jsonb NOT NULL,soak_hours bigint NOT NULL,soak_passed boolean NOT NULL,historical_debt_closed boolean NOT NULL,
 reviewer_id uuid NOT NULL REFERENCES accounts(id),created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE mobile_v9_scope_grants (
 id uuid PRIMARY KEY,school_id uuid NOT NULL REFERENCES schools(id),release_id uuid NOT NULL REFERENCES mobile_v9_adapter_releases(id),acceptance_id uuid NOT NULL REFERENCES mobile_v9_acceptance_runs(id),
 stage text NOT NULL CHECK(stage IN ('test_send','production_allowlist')),device_ids uuid[] NOT NULL,target_ids uuid[] NOT NULL,auto_targets boolean NOT NULL DEFAULT false,
 max_calls bigint NOT NULL CHECK(max_calls>0),expires_at timestamptz NOT NULL,reason text NOT NULL,created_by uuid NOT NULL REFERENCES accounts(id),created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE mobile_v9_scope_revocations (grant_id uuid PRIMARY KEY REFERENCES mobile_v9_scope_grants(id),reason text NOT NULL,actor_id uuid NOT NULL REFERENCES accounts(id),created_at timestamptz NOT NULL DEFAULT clock_timestamp());
CREATE TABLE mobile_v9_send_inspections (
 id uuid PRIMARY KEY,school_id uuid NOT NULL REFERENCES schools(id),device_id uuid NOT NULL REFERENCES execution_devices(id),target_id uuid NOT NULL REFERENCES mobile_targets(id),binding_id uuid NOT NULL REFERENCES mobile_device_target_bindings(id),
 flow_id uuid REFERENCES mobile_v9_flows(id),release_id uuid REFERENCES mobile_v9_adapter_releases(id),delegation_id uuid NOT NULL REFERENCES mobile_shared_delegations(id),
 dependencies jsonb NOT NULL,requested_by uuid NOT NULL REFERENCES accounts(id),state text NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','leased','passed','failed','stale')),
 purpose text NOT NULL CHECK(purpose IN ('maintenance','automatic')),launch_path text CHECK(launch_path IN ('search','all_apps')),
 challenge text NOT NULL,lease_fence bigint,runtime_epoch uuid,result jsonb,automatic boolean NOT NULL,
 expires_at timestamptz NOT NULL,created_at timestamptz NOT NULL DEFAULT clock_timestamp(),CHECK((purpose='maintenance')=(flow_id IS NOT NULL))
);
CREATE INDEX mobile_v9_inspection_dependencies ON mobile_v9_send_inspections(binding_id,created_at DESC);
CREATE TABLE mobile_v9_runtime_details (
 device_id uuid PRIMARY KEY REFERENCES execution_devices(id),credential_generation bigint NOT NULL,ui_context_epoch text NOT NULL,conditions jsonb NOT NULL,apk_sha256 text NOT NULL,
 observed_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE mobile_v9_support_requests (
 id uuid PRIMARY KEY,device_id uuid NOT NULL REFERENCES execution_devices(id),school_id uuid REFERENCES schools(id),reason text NOT NULL,
 state text NOT NULL CHECK(state IN ('submitted','investigating','resolved','declined')),revision bigint NOT NULL DEFAULT 1,response text NOT NULL DEFAULT '',release_id uuid REFERENCES mobile_v9_adapter_releases(id),
 created_by uuid REFERENCES accounts(id),created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE mobile_v9_diagnostics (
 id uuid PRIMARY KEY,device_id uuid NOT NULL REFERENCES execution_devices(id),credential_generation bigint NOT NULL,inspection_id uuid REFERENCES mobile_v9_send_inspections(id),claim_id uuid,
 code text NOT NULL,stage text NOT NULL,details jsonb NOT NULL,created_at timestamptz NOT NULL DEFAULT clock_timestamp(),CHECK(inspection_id IS NOT NULL OR claim_id IS NOT NULL)
);
CREATE TABLE mobile_v9_task_drafts (
 id uuid PRIMARY KEY,school_id uuid NOT NULL REFERENCES schools(id),intent_id uuid NOT NULL UNIQUE,actor_id uuid NOT NULL REFERENCES accounts(id),
 revision bigint NOT NULL DEFAULT 1,target_ids uuid[] NOT NULL,message text NOT NULL,content_digest text NOT NULL,
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),UNIQUE(school_id,id)
);
CREATE TABLE mobile_v9_previews (
 id uuid PRIMARY KEY,school_id uuid NOT NULL,draft_id uuid NOT NULL,draft_revision bigint NOT NULL,
 plan jsonb NOT NULL,digest text NOT NULL,policy_id uuid REFERENCES mobile_v9_send_policy_versions(id),
 expires_at timestamptz NOT NULL DEFAULT clock_timestamp()+interval '10 minutes',
 FOREIGN KEY(school_id,draft_id) REFERENCES mobile_v9_task_drafts(school_id,id),UNIQUE(school_id,id)
);
CREATE TABLE mobile_v9_tasks (
 id uuid PRIMARY KEY,school_id uuid NOT NULL,draft_id uuid NOT NULL UNIQUE,intent_id uuid NOT NULL UNIQUE,
 preview_id uuid NOT NULL UNIQUE,confirmed_by uuid NOT NULL REFERENCES accounts(id),revision bigint NOT NULL DEFAULT 1,
 stopped boolean NOT NULL DEFAULT false,created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 FOREIGN KEY(school_id,draft_id) REFERENCES mobile_v9_task_drafts(school_id,id),
 FOREIGN KEY(school_id,preview_id) REFERENCES mobile_v9_previews(school_id,id),UNIQUE(school_id,id)
);
CREATE TABLE mobile_v9_task_items (
 id uuid PRIMARY KEY,school_id uuid NOT NULL,task_id uuid NOT NULL,intent_id uuid NOT NULL,target_id uuid NOT NULL,position integer NOT NULL,
 FOREIGN KEY(school_id,task_id) REFERENCES mobile_v9_tasks(school_id,id),FOREIGN KEY(school_id,target_id) REFERENCES mobile_targets(school_id,id),
 UNIQUE(school_id,intent_id,target_id),UNIQUE(school_id,id),UNIQUE(school_id,id,target_id)
);
CREATE TABLE mobile_v9_plans (
 id uuid PRIMARY KEY,school_id uuid NOT NULL,item_id uuid NOT NULL,target_id uuid NOT NULL,device_id uuid NOT NULL REFERENCES execution_devices(id),
 snapshot_id uuid NOT NULL,plan jsonb NOT NULL,digest text NOT NULL,content_digest text NOT NULL,
 FOREIGN KEY(school_id,item_id,target_id) REFERENCES mobile_v9_task_items(school_id,id,target_id),
 FOREIGN KEY(school_id,snapshot_id) REFERENCES distribution_message_snapshots(school_id,id),UNIQUE(school_id,id),UNIQUE(school_id,item_id,id)
);
CREATE TRIGGER mobile_v9_plan_immutable BEFORE UPDATE OR DELETE ON mobile_v9_plans FOR EACH ROW EXECUTE FUNCTION mobile_capability_evidence_immutable();
CREATE TABLE mobile_v9_attempts (
 id uuid PRIMARY KEY,school_id uuid NOT NULL,item_id uuid NOT NULL,plan_id uuid NOT NULL,
 confirmed_by uuid NOT NULL REFERENCES accounts(id),
 attempt_no integer NOT NULL CHECK(attempt_no>0),current boolean NOT NULL DEFAULT true,
 state text NOT NULL DEFAULT 'queued' CHECK(state IN ('queued','leased','prepared','invocation_issued','observed_sent','unknown','failed_before_call','invalidated','cancelled','verified_not_sent','manually_confirmed_sent')),
 revision bigint NOT NULL DEFAULT 1,not_before timestamptz NOT NULL DEFAULT clock_timestamp(),
 FOREIGN KEY(school_id,item_id,plan_id) REFERENCES mobile_v9_plans(school_id,item_id,id),
 UNIQUE(item_id,attempt_no),UNIQUE(school_id,id),UNIQUE(school_id,item_id,id)
);
CREATE UNIQUE INDEX mobile_v9_one_current ON mobile_v9_attempts(item_id) WHERE current;
CREATE TABLE mobile_v9_send_claims (
 id uuid PRIMARY KEY,school_id uuid NOT NULL,attempt_id uuid NOT NULL,device_id uuid NOT NULL REFERENCES execution_devices(id),
 credential_generation bigint NOT NULL,runtime_epoch uuid NOT NULL,lease_fence bigint NOT NULL,
 recovery_credential_hash text NOT NULL,encrypted_response bytea,
 state text NOT NULL CHECK(state IN ('active','closed')),created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 expires_at timestamptz NOT NULL,FOREIGN KEY(school_id,attempt_id) REFERENCES mobile_v9_attempts(school_id,id),UNIQUE(school_id,id)
);
CREATE UNIQUE INDEX mobile_v9_claim_device ON mobile_v9_send_claims(device_id) WHERE state='active';
CREATE UNIQUE INDEX mobile_v9_claim_attempt ON mobile_v9_send_claims(attempt_id) WHERE state='active';
CREATE TABLE mobile_v9_preparations (
 id uuid PRIMARY KEY,school_id uuid NOT NULL,claim_id uuid NOT NULL,body jsonb NOT NULL,
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),FOREIGN KEY(school_id,claim_id) REFERENCES mobile_v9_send_claims(school_id,id)
);
CREATE TABLE mobile_v9_invocations (
 id uuid PRIMARY KEY,school_id uuid NOT NULL,attempt_id uuid NOT NULL UNIQUE,claim_id uuid NOT NULL UNIQUE,
 device_id uuid NOT NULL REFERENCES execution_devices(id),credential_generation bigint NOT NULL,lease_fence bigint NOT NULL,
 issued_at timestamptz NOT NULL DEFAULT clock_timestamp(),execute_before timestamptz NOT NULL,
 state text NOT NULL CHECK(state IN ('risk_held','consumed','released')),response jsonb NOT NULL, encrypted_response bytea NOT NULL, invocation_secret_hash text NOT NULL,result_credential_hash text NOT NULL,
 FOREIGN KEY(school_id,attempt_id) REFERENCES mobile_v9_attempts(school_id,id),
 FOREIGN KEY(school_id,claim_id) REFERENCES mobile_v9_send_claims(school_id,id),UNIQUE(school_id,id)
);
CREATE TABLE mobile_v9_send_events (
 id uuid PRIMARY KEY,school_id uuid NOT NULL,attempt_id uuid NOT NULL,invocation_id uuid,device_id uuid NOT NULL,
 body jsonb NOT NULL,encrypted_body bytea,body_hash text NOT NULL,receipt jsonb NOT NULL,created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 FOREIGN KEY(school_id,attempt_id) REFERENCES mobile_v9_attempts(school_id,id),
 FOREIGN KEY(school_id,invocation_id) REFERENCES mobile_v9_invocations(school_id,id)
);
CREATE TRIGGER mobile_v9_event_immutable BEFORE UPDATE OR DELETE ON mobile_v9_send_events FOR EACH ROW EXECUTE FUNCTION mobile_capability_evidence_immutable();
CREATE TABLE mobile_v9_risk_guards (
 school_id uuid NOT NULL,intent_id uuid NOT NULL,target_id uuid NOT NULL,attempt_id uuid NOT NULL,
 invocation_id uuid NOT NULL,state text NOT NULL CHECK(state IN ('active','closed')),
 PRIMARY KEY(school_id,intent_id,target_id),FOREIGN KEY(school_id,attempt_id) REFERENCES mobile_v9_attempts(school_id,id),
 FOREIGN KEY(school_id,invocation_id) REFERENCES mobile_v9_invocations(school_id,id)
);
CREATE TABLE mobile_v9_quota_reservations (
 attempt_id uuid NOT NULL REFERENCES mobile_v9_attempts(id),dimension_key text NOT NULL,
 state text NOT NULL CHECK(state IN ('reserved','risk_held','consumed','released')),
 recorded_at timestamptz NOT NULL DEFAULT clock_timestamp(),PRIMARY KEY(attempt_id,dimension_key)
);
CREATE INDEX mobile_v9_quota_window ON mobile_v9_quota_reservations(dimension_key,recorded_at) WHERE state<>'released';
CREATE TABLE mobile_v9_scheduler_turns (
 device_id uuid NOT NULL REFERENCES execution_devices(id),school_id uuid NOT NULL REFERENCES schools(id),
 sequence bigint GENERATED ALWAYS AS IDENTITY,PRIMARY KEY(device_id,school_id)
);
CREATE TABLE mobile_v9_reviews (
 id uuid PRIMARY KEY,school_id uuid NOT NULL,attempt_id uuid NOT NULL,reviewer uuid NOT NULL REFERENCES accounts(id),
 decision text NOT NULL CHECK(decision IN ('manually_confirmed_sent','verified_not_sent','still_unknown')),
 evidence text NOT NULL,created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 FOREIGN KEY(school_id,attempt_id) REFERENCES mobile_v9_attempts(school_id,id)
);
CREATE TRIGGER mobile_v9_review_immutable BEFORE UPDATE OR DELETE ON mobile_v9_reviews FOR EACH ROW EXECUTE FUNCTION mobile_capability_evidence_immutable();
CREATE TABLE mobile_v9_operations (
 principal_id uuid NOT NULL,generation bigint NOT NULL,scope text NOT NULL,idempotency_key text NOT NULL,
 request_hash text NOT NULL,response jsonb NOT NULL,created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 PRIMARY KEY(principal_id,generation,scope,idempotency_key)
);
CREATE TABLE mobile_v9_reassignments (
 id uuid PRIMARY KEY, school_id uuid NOT NULL, old_attempt_id uuid NOT NULL, new_attempt_id uuid NOT NULL UNIQUE,
 actor_id uuid NOT NULL REFERENCES accounts(id),reason text NOT NULL,created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 FOREIGN KEY(school_id,old_attempt_id) REFERENCES mobile_v9_attempts(school_id,id),
 FOREIGN KEY(school_id,new_attempt_id) REFERENCES mobile_v9_attempts(school_id,id)
);
CREATE TRIGGER mobile_v9_reassignment_immutable BEFORE UPDATE OR DELETE ON mobile_v9_reassignments FOR EACH ROW EXECUTE FUNCTION mobile_capability_evidence_immutable();

ALTER TABLE mobile_v9_task_drafts ADD COLUMN stage text NOT NULL CHECK(stage IN ('test_send','production_allowlist'));
ALTER TABLE mobile_v9_task_drafts ADD COLUMN between_targets_seconds bigint NOT NULL CHECK(between_targets_seconds BETWEEN 1 AND 86400);
ALTER TABLE mobile_v9_tasks ADD COLUMN confirmed_until timestamptz NOT NULL;
ALTER TABLE mobile_v9_tasks ADD COLUMN cursor_position integer NOT NULL DEFAULT 0;
ALTER TABLE mobile_v9_tasks ADD COLUMN next_eligible_at timestamptz NOT NULL DEFAULT clock_timestamp();
CREATE TABLE mobile_v9_settlements (
 attempt_id uuid PRIMARY KEY REFERENCES mobile_v9_attempts(id),receipt_id uuid NOT NULL,event_id uuid NOT NULL REFERENCES mobile_v9_send_events(id),device_id uuid NOT NULL REFERENCES execution_devices(id),
 credential_generation bigint NOT NULL,runtime_epoch uuid NOT NULL,lease_fence bigint NOT NULL,confirmed_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
ALTER TABLE mobile_v9_diagnostics ADD FOREIGN KEY(claim_id) REFERENCES mobile_v9_send_claims(id);
-- All mutations share guard 78234610243 with the existing physical UI leases.
CREATE FUNCTION mobile_v9_scope_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 PERFORM pg_advisory_xact_lock(78234610243);
 IF NOT EXISTS(SELECT 1 FROM mobile_v9_acceptance_runs a WHERE a.id=NEW.acceptance_id AND a.release_id=NEW.release_id AND a.stage=NEW.stage) THEN RAISE EXCEPTION 'v9 acceptance scope mismatch'; END IF;
 IF NOT EXISTS(SELECT 1 FROM execution_devices WHERE id=ANY(NEW.device_ids)) OR EXISTS(SELECT 1 FROM unnest(NEW.target_ids) t WHERE NOT EXISTS(SELECT 1 FROM mobile_targets WHERE id=t AND school_id=NEW.school_id)) THEN RAISE EXCEPTION 'v9 resource scope mismatch'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER mobile_v9_scope_guard BEFORE INSERT ON mobile_v9_scope_grants FOR EACH ROW EXECUTE FUNCTION mobile_v9_scope_guard();
DO $$ DECLARE t text; BEGIN
 FOREACH t IN ARRAY ARRAY['mobile_v9_send_policy_versions','mobile_v9_artifacts','mobile_v9_flows','mobile_v9_baselines','mobile_v9_adapter_releases','mobile_v9_adapter_withdrawals','mobile_v9_activation_events','mobile_v9_acceptance_runs','mobile_v9_scope_grants','mobile_v9_scope_revocations','mobile_v9_diagnostics','mobile_v9_settlements'] LOOP
 EXECUTE format('CREATE TRIGGER %I BEFORE UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION mobile_capability_evidence_immutable()',t||'_immutable',t);
 END LOOP;
END $$;
-- No actual adapter, consent, acceptance or production scope is fabricated by migration.
ALTER TABLE mobile_v9_send_inspections ADD COLUMN apk_id uuid REFERENCES mobile_v9_artifacts(id);
CREATE TABLE mobile_v9_human_samples (
 id uuid PRIMARY KEY,flow_id uuid NOT NULL REFERENCES mobile_v9_flows(id),device_id uuid NOT NULL REFERENCES execution_devices(id),credential_generation bigint NOT NULL,
 requested_by uuid NOT NULL REFERENCES accounts(id),challenge text NOT NULL,state text NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','running','passed','failed')),
 runtime_epoch uuid,elapsed_ms bigint,started_at timestamptz,completed_at timestamptz,expires_at timestamptz NOT NULL DEFAULT clock_timestamp()+interval '1 day',created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
ALTER TABLE mobile_v9_runtime_details ADD COLUMN settled_receipt_id uuid;
ALTER TABLE mobile_v9_scope_grants ADD COLUMN resource_revisions jsonb NOT NULL;

ALTER TABLE mobile_v9_acceptance_runs ADD COLUMN reason text NOT NULL;
CREATE TABLE mobile_v9_support_events (id uuid PRIMARY KEY,request_id uuid NOT NULL REFERENCES mobile_v9_support_requests(id),actor_id uuid NOT NULL REFERENCES accounts(id),revision bigint NOT NULL,state text NOT NULL,response text NOT NULL,release_id uuid REFERENCES mobile_v9_adapter_releases(id),created_at timestamptz NOT NULL DEFAULT clock_timestamp(),UNIQUE(request_id,revision));
CREATE TRIGGER mobile_v9_support_events_immutable BEFORE UPDATE OR DELETE ON mobile_v9_support_events FOR EACH ROW EXECUTE FUNCTION mobile_capability_evidence_immutable();
