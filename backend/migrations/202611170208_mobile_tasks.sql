-- 手机任务、候选调度与设备尝试。
-- 空库初始化基线；后续结构变更使用新增递增迁移。

-- function
CREATE FUNCTION mobile_v9_inbox_changed() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
DECLARE r record;
BEGIN
 IF TG_TABLE_NAME='mobile_v9_attempts' THEN
  IF TG_OP='UPDATE' AND NEW.state=OLD.state AND NEW.current=OLD.current THEN RETURN NEW; END IF;
  NEW.inbox_changed_at=clock_timestamp();
  FOR r IN SELECT device_id FROM mobile_v9_plans WHERE id=NEW.plan_id LOOP
   INSERT INTO mobile_v9_device_inbox_versions(device_id,revision) VALUES(r.device_id,1)
   ON CONFLICT(device_id) DO UPDATE SET revision=mobile_v9_device_inbox_versions.revision+1;
  END LOOP;
 ELSE
  IF NEW.stopped=OLD.stopped AND NEW.next_eligible_at=OLD.next_eligible_at AND NEW.confirmed_until=OLD.confirmed_until THEN RETURN NEW; END IF;
  FOR r IN SELECT DISTINCT p.device_id FROM mobile_v9_task_items i JOIN mobile_v9_plans p ON p.item_id=i.id WHERE i.task_id=NEW.id LOOP
   INSERT INTO mobile_v9_device_inbox_versions(device_id,revision) VALUES(r.device_id,1)
   ON CONFLICT(device_id) DO UPDATE SET revision=mobile_v9_device_inbox_versions.revision+1;
  END LOOP;
 END IF;
 RETURN NEW;
END $$;

CREATE FUNCTION mobile_v9_scope_guard() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
 PERFORM pg_advisory_xact_lock(78234610243);
 IF NEW.acceptance_id IS NOT NULL AND NOT EXISTS(
  SELECT 1 FROM mobile_v9_acceptance_runs a
  WHERE a.id=NEW.acceptance_id AND a.release_id=NEW.release_id AND a.stage=NEW.stage
 ) THEN RAISE EXCEPTION 'v9 acceptance scope mismatch'; END IF;
 IF cardinality(NEW.device_ids)=0 OR EXISTS(
  SELECT 1 FROM unnest(NEW.device_ids) d
  WHERE NOT EXISTS(SELECT 1 FROM execution_devices WHERE id=d)
 ) OR EXISTS(
  SELECT 1 FROM unnest(NEW.target_ids) t
  WHERE NOT EXISTS(SELECT 1 FROM mobile_targets WHERE id=t)
 ) THEN RAISE EXCEPTION 'v9 resource scope mismatch'; END IF;
 RETURN NEW;
END $$;

-- table
CREATE TABLE mobile_v9_attempts (
    id uuid NOT NULL,
    item_id uuid NOT NULL,
    plan_id uuid NOT NULL,
    confirmed_by uuid NOT NULL,
    attempt_no integer NOT NULL,
    current boolean DEFAULT true NOT NULL,
    state text DEFAULT 'queued'::text NOT NULL,
    revision bigint DEFAULT 1 NOT NULL,
    not_before timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    invalidation_reason text,
    inbox_assigned_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    inbox_changed_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    invalidated_at timestamp with time zone,
    termination_reason text,
    progress_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    CONSTRAINT mobile_v9_attempts_attempt_no_check CHECK ((attempt_no > 0)),
    CONSTRAINT mobile_v9_attempts_state_check CHECK ((state = ANY (ARRAY['queued'::text, 'leased'::text, 'prepared'::text, 'invocation_issued'::text, 'observed_sent'::text, 'unknown'::text, 'failed_before_call'::text, 'invalidated'::text, 'cancelled'::text, 'verified_not_sent'::text, 'manually_confirmed_sent'::text])))
);

CREATE TABLE mobile_v9_dispatch_state (
    item_id uuid NOT NULL,
    state text DEFAULT 'scheduling'::text NOT NULL,
    reason text DEFAULT 'awaiting_dispatch'::text NOT NULL,
    revision bigint DEFAULT 1 NOT NULL,
    next_retry_at timestamp with time zone DEFAULT clock_timestamp(),
    last_failed_attempt_id uuid,
    updated_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    candidate_cursor integer DEFAULT 0 NOT NULL,
    infrastructure_failures integer DEFAULT 0 NOT NULL,
    max_rounds integer DEFAULT 3 NOT NULL,
    completed_rounds integer DEFAULT 0 NOT NULL,
    round_origin integer DEFAULT 0 NOT NULL,
    assigned_at timestamp with time zone,
    finished_at timestamp with time zone,
    round_started_at timestamp with time zone,
    progress_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    CONSTRAINT mobile_v9_dispatch_state_completed_rounds_check CHECK (((completed_rounds >= 0) AND (completed_rounds <= 3))),
    CONSTRAINT mobile_v9_dispatch_state_infrastructure_failures_check CHECK (((infrastructure_failures >= 0) AND (infrastructure_failures <= 4))),
    CONSTRAINT mobile_v9_dispatch_state_max_rounds_check CHECK ((max_rounds = 3)),
    CONSTRAINT mobile_v9_dispatch_state_round_origin_check CHECK ((round_origin >= 0)),
    CONSTRAINT mobile_v9_dispatch_state_state_check CHECK ((state = ANY (ARRAY['scheduling'::text, 'assigned'::text, 'waiting_resources'::text, 'executing'::text, 'completed'::text, 'needs_review'::text, 'stopped'::text, 'confirmation_expired'::text, 'dispatch_failed'::text])))
);

CREATE TABLE mobile_v9_plans (
    id uuid NOT NULL,
    item_id uuid NOT NULL,
    target_id uuid NOT NULL,
    device_id uuid NOT NULL,
    snapshot_id uuid NOT NULL,
    plan jsonb NOT NULL,
    digest text NOT NULL,
    content_digest text NOT NULL
);

CREATE TABLE mobile_v9_task_items (
    id uuid NOT NULL,
    task_id uuid NOT NULL,
    intent_id uuid NOT NULL,
    target_id uuid NOT NULL,
    "position" integer NOT NULL
);

CREATE TABLE mobile_v9_tasks (
    id uuid NOT NULL,
    draft_id uuid NOT NULL,
    intent_id uuid NOT NULL,
    preview_id uuid NOT NULL,
    confirmed_by uuid NOT NULL,
    revision bigint DEFAULT 1 NOT NULL,
    stopped boolean DEFAULT false NOT NULL,
    created_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    confirmed_until timestamp with time zone NOT NULL,
    next_eligible_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    authorization_kind text DEFAULT 'manual'::text NOT NULL,
    CONSTRAINT mobile_v9_tasks_authorization_kind_check CHECK ((authorization_kind = ANY (ARRAY['manual'::text, 'source_route'::text])))
);

CREATE TABLE mobile_v9_device_inbox_versions (
    device_id uuid NOT NULL,
    revision bigint DEFAULT 0 NOT NULL
);

CREATE TABLE mobile_v9_diagnostics (
    id uuid NOT NULL,
    device_id uuid NOT NULL,
    credential_generation bigint NOT NULL,
    inspection_id uuid,
    claim_id uuid,
    code text NOT NULL,
    stage text NOT NULL,
    details jsonb NOT NULL,
    created_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    CONSTRAINT mobile_v9_diagnostics_check CHECK (((inspection_id IS NOT NULL) OR (claim_id IS NOT NULL)))
);

CREATE TABLE mobile_v9_dispatch_events (
    id bigint NOT NULL,
    item_id uuid,
    round_no integer,
    kind text NOT NULL,
    device_id uuid,
    attempt_id uuid,
    reason text NOT NULL,
    created_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    intake_id uuid,
    operation_id uuid,
    delivery_request_id uuid,
    execution_id uuid,
    result text DEFAULT 'recorded'::text NOT NULL,
    evidence jsonb DEFAULT '{}'::jsonb NOT NULL,
    CONSTRAINT mobile_v9_dispatch_events_round_no_check CHECK (((round_no >= 1) AND (round_no <= 3)))
);

CREATE TABLE mobile_v9_dispatch_scopes (
    item_id uuid NOT NULL,
    candidates jsonb NOT NULL,
    CONSTRAINT mobile_v9_dispatch_scopes_candidates_check CHECK (((jsonb_typeof(candidates) = 'array'::text) AND (jsonb_array_length(candidates) > 0)))
);

CREATE TABLE mobile_v9_execution_wait_observations (
    attempt_id uuid NOT NULL,
    conditions jsonb NOT NULL,
    observed_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    CONSTRAINT mobile_v9_execution_wait_observations_conditions_check CHECK ((jsonb_typeof(conditions) = 'array'::text))
);

CREATE TABLE mobile_v9_inbox_receipt_operations (
    device_id uuid NOT NULL,
    generation bigint NOT NULL,
    idempotency_key text NOT NULL,
    request_hash text NOT NULL,
    response jsonb NOT NULL,
    created_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL
);

CREATE TABLE mobile_v9_invocations (
    id uuid NOT NULL,
    attempt_id uuid NOT NULL,
    claim_id uuid NOT NULL,
    device_id uuid NOT NULL,
    credential_generation bigint NOT NULL,
    lease_fence bigint NOT NULL,
    issued_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    execute_before timestamp with time zone NOT NULL,
    state text NOT NULL,
    response jsonb NOT NULL,
    encrypted_response bytea NOT NULL,
    invocation_secret_hash text NOT NULL,
    result_credential_hash text NOT NULL,
    CONSTRAINT mobile_v9_invocations_state_check CHECK ((state = ANY (ARRAY['risk_held'::text, 'consumed'::text, 'released'::text])))
);

CREATE TABLE mobile_v9_operations (
    principal_id uuid NOT NULL,
    generation bigint NOT NULL,
    scope text NOT NULL,
    idempotency_key text NOT NULL,
    request_hash text NOT NULL,
    response jsonb NOT NULL,
    created_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL
);

CREATE TABLE mobile_v9_previews (
    id uuid NOT NULL,
    draft_id uuid NOT NULL,
    draft_revision bigint NOT NULL,
    plan jsonb NOT NULL,
    digest text NOT NULL,
    policy_id uuid,
    expires_at timestamp with time zone DEFAULT (clock_timestamp() + '00:10:00'::interval) NOT NULL
);

CREATE TABLE mobile_v9_quota_reservations (
    attempt_id uuid NOT NULL,
    dimension_key text NOT NULL,
    state text NOT NULL,
    recorded_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    CONSTRAINT mobile_v9_quota_reservations_state_check CHECK ((state = ANY (ARRAY['reserved'::text, 'risk_held'::text, 'consumed'::text, 'released'::text])))
);

CREATE TABLE mobile_v9_reassignments (
    id uuid NOT NULL,
    old_attempt_id uuid NOT NULL,
    new_attempt_id uuid NOT NULL,
    actor_id uuid NOT NULL,
    reason text NOT NULL,
    created_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    source text DEFAULT 'manual'::text NOT NULL,
    CONSTRAINT mobile_v9_reassignments_source_check CHECK ((source = ANY (ARRAY['manual'::text, 'automatic'::text])))
);

CREATE TABLE mobile_v9_reviews (
    id uuid NOT NULL,
    attempt_id uuid NOT NULL,
    reviewer uuid NOT NULL,
    decision text NOT NULL,
    evidence text NOT NULL,
    created_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    CONSTRAINT mobile_v9_reviews_decision_check CHECK ((decision = ANY (ARRAY['manually_confirmed_sent'::text, 'verified_not_sent'::text, 'still_unknown'::text])))
);

CREATE TABLE mobile_v9_risk_guards (
    intent_id uuid NOT NULL,
    target_id uuid NOT NULL,
    attempt_id uuid NOT NULL,
    invocation_id uuid NOT NULL,
    state text NOT NULL,
    CONSTRAINT mobile_v9_risk_guards_state_check CHECK ((state = ANY (ARRAY['active'::text, 'closed'::text])))
);

CREATE TABLE mobile_v9_runtime_details (
    device_id uuid NOT NULL,
    credential_generation bigint NOT NULL,
    ui_context_epoch text NOT NULL,
    conditions jsonb NOT NULL,
    apk_sha256 text NOT NULL,
    observed_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    settled_receipt_id uuid,
    preparation jsonb DEFAULT '{}'::jsonb NOT NULL,
    ready_reason text,
    CONSTRAINT runtime_ready_reason_code CHECK (((ready_reason IS NULL) OR (ready_reason ~ '^[a-z0-9_]{1,100}$'::text)))
);

CREATE TABLE mobile_v9_send_events (
    id uuid NOT NULL,
    attempt_id uuid NOT NULL,
    invocation_id uuid,
    device_id uuid NOT NULL,
    body jsonb NOT NULL,
    encrypted_body bytea,
    body_hash text NOT NULL,
    receipt jsonb NOT NULL,
    created_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL
);

CREATE TABLE mobile_v9_send_inspections (
    id uuid NOT NULL,
    device_id uuid NOT NULL,
    target_id uuid NOT NULL,
    binding_id uuid NOT NULL,
    flow_id uuid,
    release_id uuid,
    delegation_id uuid NOT NULL,
    dependencies jsonb NOT NULL,
    requested_by uuid NOT NULL,
    state text DEFAULT 'pending'::text NOT NULL,
    purpose text NOT NULL,
    launch_path text,
    challenge text NOT NULL,
    lease_fence bigint,
    runtime_epoch uuid,
    result jsonb,
    automatic boolean NOT NULL,
    expires_at timestamp with time zone NOT NULL,
    created_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    apk_id uuid,
    started_at timestamp with time zone,
    finished_at timestamp with time zone,
    termination_reason text,
    flow jsonb,
    flow_digest text,
    CONSTRAINT mobile_v9_send_inspections_check CHECK (((purpose = 'maintenance'::text) = (flow_id IS NOT NULL))),
    CONSTRAINT mobile_v9_send_inspections_launch_path_check CHECK ((launch_path = ANY (ARRAY['search'::text, 'all_apps'::text]))),
    CONSTRAINT mobile_v9_send_inspections_purpose_check CHECK ((purpose = ANY (ARRAY['maintenance'::text, 'automatic'::text]))),
    CONSTRAINT mobile_v9_send_inspections_state_check CHECK ((state = ANY (ARRAY['pending'::text, 'leased'::text, 'passed'::text, 'failed'::text, 'stale'::text])))
);

CREATE TABLE mobile_v9_send_policy_versions (
    id uuid NOT NULL,
    revision bigint NOT NULL,
    mode text NOT NULL,
    confirmation_seconds bigint NOT NULL,
    inspection_seconds bigint NOT NULL,
    retry_seconds bigint NOT NULL,
    retry_limit bigint NOT NULL,
    timezone text NOT NULL,
    allowed_windows jsonb NOT NULL,
    limits jsonb NOT NULL,
    reason text NOT NULL,
    created_by uuid NOT NULL,
    created_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    CONSTRAINT mobile_v9_send_policy_versions_mode_check CHECK ((mode = ANY (ARRAY['disabled'::text, 'draft_only'::text, 'preview_only'::text, 'enabled'::text]))),
    CONSTRAINT mobile_v9_send_policy_versions_revision_check CHECK ((revision > 0))
);

CREATE TABLE mobile_v9_task_drafts (
    id uuid NOT NULL,
    intent_id uuid NOT NULL,
    actor_id uuid NOT NULL,
    revision bigint DEFAULT 1 NOT NULL,
    target_ids uuid[] NOT NULL,
    message text NOT NULL,
    content_digest text NOT NULL,
    created_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    between_targets_seconds bigint NOT NULL,
    CONSTRAINT mobile_v9_task_drafts_between_targets_seconds_check CHECK (((between_targets_seconds >= 1) AND (between_targets_seconds <= 86400)))
);

CREATE TABLE mobile_v9_task_receipts (
    attempt_id uuid NOT NULL,
    device_id uuid NOT NULL,
    credential_generation bigint NOT NULL,
    assignment_revision bigint NOT NULL,
    received_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    CONSTRAINT mobile_v9_task_receipts_assignment_revision_check CHECK ((assignment_revision = 1))
);

-- sequence
ALTER TABLE mobile_v9_dispatch_events ALTER COLUMN id ADD GENERATED ALWAYS AS IDENTITY (
    SEQUENCE NAME mobile_v9_dispatch_events_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);

-- constraint
ALTER TABLE ONLY mobile_v9_attempts
    ADD CONSTRAINT mobile_v9_attempts_id_key UNIQUE (id);

ALTER TABLE ONLY mobile_v9_attempts
    ADD CONSTRAINT mobile_v9_attempts_item_id_attempt_no_key UNIQUE (item_id, attempt_no);

ALTER TABLE ONLY mobile_v9_attempts
    ADD CONSTRAINT mobile_v9_attempts_item_id_id_key UNIQUE (item_id, id);

ALTER TABLE ONLY mobile_v9_attempts
    ADD CONSTRAINT mobile_v9_attempts_pkey PRIMARY KEY (id);

ALTER TABLE ONLY mobile_v9_device_inbox_versions
    ADD CONSTRAINT mobile_v9_device_inbox_versions_pkey PRIMARY KEY (device_id);

ALTER TABLE ONLY mobile_v9_diagnostics
    ADD CONSTRAINT mobile_v9_diagnostics_pkey PRIMARY KEY (id);

ALTER TABLE ONLY mobile_v9_dispatch_events
    ADD CONSTRAINT mobile_v9_dispatch_events_pkey PRIMARY KEY (id);

ALTER TABLE ONLY mobile_v9_dispatch_scopes
    ADD CONSTRAINT mobile_v9_dispatch_scopes_pkey PRIMARY KEY (item_id);

ALTER TABLE ONLY mobile_v9_dispatch_state
    ADD CONSTRAINT mobile_v9_dispatch_state_pkey PRIMARY KEY (item_id);

ALTER TABLE ONLY mobile_v9_execution_wait_observations
    ADD CONSTRAINT mobile_v9_execution_wait_observations_pkey PRIMARY KEY (attempt_id);

ALTER TABLE ONLY mobile_v9_inbox_receipt_operations
    ADD CONSTRAINT mobile_v9_inbox_receipt_operations_pkey PRIMARY KEY (device_id, generation, idempotency_key);

ALTER TABLE ONLY mobile_v9_invocations
    ADD CONSTRAINT mobile_v9_invocations_attempt_id_key UNIQUE (attempt_id);

ALTER TABLE ONLY mobile_v9_invocations
    ADD CONSTRAINT mobile_v9_invocations_claim_id_key UNIQUE (claim_id);

ALTER TABLE ONLY mobile_v9_invocations
    ADD CONSTRAINT mobile_v9_invocations_id_key UNIQUE (id);

ALTER TABLE ONLY mobile_v9_invocations
    ADD CONSTRAINT mobile_v9_invocations_pkey PRIMARY KEY (id);

ALTER TABLE ONLY mobile_v9_operations
    ADD CONSTRAINT mobile_v9_operations_pkey PRIMARY KEY (principal_id, generation, scope, idempotency_key);

ALTER TABLE ONLY mobile_v9_plans
    ADD CONSTRAINT mobile_v9_plans_id_key UNIQUE (id);

ALTER TABLE ONLY mobile_v9_plans
    ADD CONSTRAINT mobile_v9_plans_item_id_id_key UNIQUE (item_id, id);

ALTER TABLE ONLY mobile_v9_plans
    ADD CONSTRAINT mobile_v9_plans_pkey PRIMARY KEY (id);

ALTER TABLE ONLY mobile_v9_previews
    ADD CONSTRAINT mobile_v9_previews_id_key UNIQUE (id);

ALTER TABLE ONLY mobile_v9_previews
    ADD CONSTRAINT mobile_v9_previews_pkey PRIMARY KEY (id);

ALTER TABLE ONLY mobile_v9_quota_reservations
    ADD CONSTRAINT mobile_v9_quota_reservations_pkey PRIMARY KEY (attempt_id, dimension_key);

ALTER TABLE ONLY mobile_v9_reassignments
    ADD CONSTRAINT mobile_v9_reassignments_new_attempt_id_key UNIQUE (new_attempt_id);

ALTER TABLE ONLY mobile_v9_reassignments
    ADD CONSTRAINT mobile_v9_reassignments_pkey PRIMARY KEY (id);

ALTER TABLE ONLY mobile_v9_reviews
    ADD CONSTRAINT mobile_v9_reviews_pkey PRIMARY KEY (id);

ALTER TABLE ONLY mobile_v9_risk_guards
    ADD CONSTRAINT mobile_v9_risk_guards_pkey PRIMARY KEY (intent_id, target_id);

ALTER TABLE ONLY mobile_v9_runtime_details
    ADD CONSTRAINT mobile_v9_runtime_details_pkey PRIMARY KEY (device_id);

ALTER TABLE ONLY mobile_v9_send_events
    ADD CONSTRAINT mobile_v9_send_events_pkey PRIMARY KEY (id);

ALTER TABLE ONLY mobile_v9_send_inspections
    ADD CONSTRAINT mobile_v9_send_inspections_pkey PRIMARY KEY (id);

ALTER TABLE ONLY mobile_v9_send_policy_versions
    ADD CONSTRAINT mobile_v9_send_policy_versions_pkey PRIMARY KEY (id);

ALTER TABLE ONLY mobile_v9_send_policy_versions
    ADD CONSTRAINT mobile_v9_send_policy_versions_revision_key UNIQUE (revision);

ALTER TABLE ONLY mobile_v9_task_drafts
    ADD CONSTRAINT mobile_v9_task_drafts_id_key UNIQUE (id);

ALTER TABLE ONLY mobile_v9_task_drafts
    ADD CONSTRAINT mobile_v9_task_drafts_intent_id_key UNIQUE (intent_id);

ALTER TABLE ONLY mobile_v9_task_drafts
    ADD CONSTRAINT mobile_v9_task_drafts_pkey PRIMARY KEY (id);

ALTER TABLE ONLY mobile_v9_task_items
    ADD CONSTRAINT mobile_v9_task_items_id_key UNIQUE (id);

ALTER TABLE ONLY mobile_v9_task_items
    ADD CONSTRAINT mobile_v9_task_items_id_target_id_key UNIQUE (id, target_id);

ALTER TABLE ONLY mobile_v9_task_items
    ADD CONSTRAINT mobile_v9_task_items_intent_id_target_id_key UNIQUE (intent_id, target_id);

ALTER TABLE ONLY mobile_v9_task_items
    ADD CONSTRAINT mobile_v9_task_items_pkey PRIMARY KEY (id);

ALTER TABLE ONLY mobile_v9_task_receipts
    ADD CONSTRAINT mobile_v9_task_receipts_pkey PRIMARY KEY (attempt_id, device_id, credential_generation, assignment_revision);

ALTER TABLE ONLY mobile_v9_tasks
    ADD CONSTRAINT mobile_v9_tasks_draft_id_key UNIQUE (draft_id);

ALTER TABLE ONLY mobile_v9_tasks
    ADD CONSTRAINT mobile_v9_tasks_id_key UNIQUE (id);

ALTER TABLE ONLY mobile_v9_tasks
    ADD CONSTRAINT mobile_v9_tasks_intent_id_key UNIQUE (intent_id);

ALTER TABLE ONLY mobile_v9_tasks
    ADD CONSTRAINT mobile_v9_tasks_pkey PRIMARY KEY (id);

ALTER TABLE ONLY mobile_v9_tasks
    ADD CONSTRAINT mobile_v9_tasks_preview_id_key UNIQUE (preview_id);

-- index
CREATE INDEX message_mobile_target_lookup ON mobile_v9_task_items USING btree (task_id, target_id);

CREATE INDEX mobile_event_intake ON mobile_v9_dispatch_events USING btree (intake_id, created_at DESC, id DESC);

CREATE INDEX mobile_event_reason ON mobile_v9_dispatch_events USING btree (reason, created_at DESC, id DESC);

CREATE INDEX mobile_v9_dispatch_due ON mobile_v9_dispatch_state USING btree (next_retry_at, item_id) WHERE (state <> ALL (ARRAY['completed'::text, 'stopped'::text, 'confirmation_expired'::text, 'dispatch_failed'::text]));

CREATE UNIQUE INDEX mobile_v9_dispatch_failed_once ON mobile_v9_dispatch_events USING btree (item_id) WHERE (kind = 'dispatch_failed'::text);

CREATE INDEX mobile_v9_inspection_dependencies ON mobile_v9_send_inspections USING btree (binding_id, created_at DESC);

CREATE INDEX mobile_v9_inspection_executed_failures ON mobile_v9_send_inspections USING btree (binding_id, finished_at) WHERE ((state = 'failed'::text) AND ((started_at IS NOT NULL) OR (((result -> 'checks'::text) ->> 'application_opened'::text) = 'true'::text)));

CREATE UNIQUE INDEX mobile_v9_one_current ON mobile_v9_attempts USING btree (item_id) WHERE current;

CREATE INDEX mobile_v9_plan_device ON mobile_v9_plans USING btree (device_id, item_id);

CREATE INDEX mobile_v9_quota_window ON mobile_v9_quota_reservations USING btree (dimension_key, recorded_at) WHERE (state <> 'released'::text);

CREATE INDEX mobile_v9_receipt_device ON mobile_v9_task_receipts USING btree (device_id, credential_generation);

-- comment
COMMENT ON COLUMN mobile_v9_tasks.confirmed_by IS 'Authorizing principal; authorization_kind distinguishes human per-task confirmation from explicit source-route authorization';

-- fk constraint
ALTER TABLE ONLY mobile_v9_attempts
    ADD CONSTRAINT mobile_v9_attempts_confirmed_by_fkey FOREIGN KEY (confirmed_by) REFERENCES accounts(id);

ALTER TABLE ONLY mobile_v9_attempts
    ADD CONSTRAINT mobile_v9_attempts_item_id_plan_id_fkey FOREIGN KEY (item_id, plan_id) REFERENCES mobile_v9_plans(item_id, id);

ALTER TABLE ONLY mobile_v9_device_inbox_versions
    ADD CONSTRAINT mobile_v9_device_inbox_versions_device_id_fkey FOREIGN KEY (device_id) REFERENCES execution_devices(id);

ALTER TABLE ONLY mobile_v9_diagnostics
    ADD CONSTRAINT mobile_v9_diagnostics_device_id_fkey FOREIGN KEY (device_id) REFERENCES execution_devices(id);

ALTER TABLE ONLY mobile_v9_diagnostics
    ADD CONSTRAINT mobile_v9_diagnostics_inspection_id_fkey FOREIGN KEY (inspection_id) REFERENCES mobile_v9_send_inspections(id);

ALTER TABLE ONLY mobile_v9_dispatch_events
    ADD CONSTRAINT mobile_v9_dispatch_events_attempt_id_fkey FOREIGN KEY (attempt_id) REFERENCES mobile_v9_attempts(id);

ALTER TABLE ONLY mobile_v9_dispatch_events
    ADD CONSTRAINT mobile_v9_dispatch_events_device_id_fkey FOREIGN KEY (device_id) REFERENCES execution_devices(id);

ALTER TABLE ONLY mobile_v9_dispatch_events
    ADD CONSTRAINT mobile_v9_dispatch_events_item_id_fkey FOREIGN KEY (item_id) REFERENCES mobile_v9_dispatch_state(item_id);

ALTER TABLE ONLY mobile_v9_dispatch_scopes
    ADD CONSTRAINT mobile_v9_dispatch_scopes_item_id_fkey FOREIGN KEY (item_id) REFERENCES mobile_v9_task_items(id);

ALTER TABLE ONLY mobile_v9_dispatch_state
    ADD CONSTRAINT mobile_v9_dispatch_state_item_id_fkey FOREIGN KEY (item_id) REFERENCES mobile_v9_dispatch_scopes(item_id);

ALTER TABLE ONLY mobile_v9_dispatch_state
    ADD CONSTRAINT mobile_v9_dispatch_state_last_failed_attempt_id_fkey FOREIGN KEY (last_failed_attempt_id) REFERENCES mobile_v9_attempts(id);

ALTER TABLE ONLY mobile_v9_execution_wait_observations
    ADD CONSTRAINT mobile_v9_execution_wait_observations_attempt_id_fkey FOREIGN KEY (attempt_id) REFERENCES mobile_v9_attempts(id);

ALTER TABLE ONLY mobile_v9_invocations
    ADD CONSTRAINT mobile_v9_invocations_attempt_id_fkey FOREIGN KEY (attempt_id) REFERENCES mobile_v9_attempts(id);

ALTER TABLE ONLY mobile_v9_invocations
    ADD CONSTRAINT mobile_v9_invocations_device_id_fkey FOREIGN KEY (device_id) REFERENCES execution_devices(id);

ALTER TABLE ONLY mobile_v9_plans
    ADD CONSTRAINT mobile_v9_plans_device_id_fkey FOREIGN KEY (device_id) REFERENCES execution_devices(id);

ALTER TABLE ONLY mobile_v9_plans
    ADD CONSTRAINT mobile_v9_plans_item_id_target_id_fkey FOREIGN KEY (item_id, target_id) REFERENCES mobile_v9_task_items(id, target_id);

ALTER TABLE ONLY mobile_v9_previews
    ADD CONSTRAINT mobile_v9_previews_draft_id_fkey FOREIGN KEY (draft_id) REFERENCES mobile_v9_task_drafts(id);

ALTER TABLE ONLY mobile_v9_previews
    ADD CONSTRAINT mobile_v9_previews_policy_id_fkey FOREIGN KEY (policy_id) REFERENCES mobile_v9_send_policy_versions(id);

ALTER TABLE ONLY mobile_v9_quota_reservations
    ADD CONSTRAINT mobile_v9_quota_reservations_attempt_id_fkey FOREIGN KEY (attempt_id) REFERENCES mobile_v9_attempts(id);

ALTER TABLE ONLY mobile_v9_reassignments
    ADD CONSTRAINT mobile_v9_reassignments_actor_id_fkey FOREIGN KEY (actor_id) REFERENCES accounts(id);

ALTER TABLE ONLY mobile_v9_reassignments
    ADD CONSTRAINT mobile_v9_reassignments_new_attempt_id_fkey FOREIGN KEY (new_attempt_id) REFERENCES mobile_v9_attempts(id);

ALTER TABLE ONLY mobile_v9_reassignments
    ADD CONSTRAINT mobile_v9_reassignments_old_attempt_id_fkey FOREIGN KEY (old_attempt_id) REFERENCES mobile_v9_attempts(id);

ALTER TABLE ONLY mobile_v9_reviews
    ADD CONSTRAINT mobile_v9_reviews_attempt_id_fkey FOREIGN KEY (attempt_id) REFERENCES mobile_v9_attempts(id);

ALTER TABLE ONLY mobile_v9_reviews
    ADD CONSTRAINT mobile_v9_reviews_reviewer_fkey FOREIGN KEY (reviewer) REFERENCES accounts(id);

ALTER TABLE ONLY mobile_v9_risk_guards
    ADD CONSTRAINT mobile_v9_risk_guards_attempt_id_fkey FOREIGN KEY (attempt_id) REFERENCES mobile_v9_attempts(id);

ALTER TABLE ONLY mobile_v9_risk_guards
    ADD CONSTRAINT mobile_v9_risk_guards_invocation_id_fkey FOREIGN KEY (invocation_id) REFERENCES mobile_v9_invocations(id);

ALTER TABLE ONLY mobile_v9_runtime_details
    ADD CONSTRAINT mobile_v9_runtime_details_device_id_fkey FOREIGN KEY (device_id) REFERENCES execution_devices(id);

ALTER TABLE ONLY mobile_v9_send_events
    ADD CONSTRAINT mobile_v9_send_events_attempt_id_fkey FOREIGN KEY (attempt_id) REFERENCES mobile_v9_attempts(id);

ALTER TABLE ONLY mobile_v9_send_events
    ADD CONSTRAINT mobile_v9_send_events_invocation_id_fkey FOREIGN KEY (invocation_id) REFERENCES mobile_v9_invocations(id);

ALTER TABLE ONLY mobile_v9_send_inspections
    ADD CONSTRAINT mobile_v9_send_inspections_apk_id_fkey FOREIGN KEY (apk_id) REFERENCES mobile_v9_artifacts(id);

ALTER TABLE ONLY mobile_v9_send_inspections
    ADD CONSTRAINT mobile_v9_send_inspections_binding_id_fkey FOREIGN KEY (binding_id) REFERENCES mobile_device_target_bindings(id);

ALTER TABLE ONLY mobile_v9_send_inspections
    ADD CONSTRAINT mobile_v9_send_inspections_delegation_id_fkey FOREIGN KEY (delegation_id) REFERENCES mobile_shared_delegations(id);

ALTER TABLE ONLY mobile_v9_send_inspections
    ADD CONSTRAINT mobile_v9_send_inspections_device_id_fkey FOREIGN KEY (device_id) REFERENCES execution_devices(id);

ALTER TABLE ONLY mobile_v9_send_inspections
    ADD CONSTRAINT mobile_v9_send_inspections_flow_id_fkey FOREIGN KEY (flow_id) REFERENCES mobile_v9_flows(id);

ALTER TABLE ONLY mobile_v9_send_inspections
    ADD CONSTRAINT mobile_v9_send_inspections_release_id_fkey FOREIGN KEY (release_id) REFERENCES mobile_v9_adapter_releases(id);

ALTER TABLE ONLY mobile_v9_send_inspections
    ADD CONSTRAINT mobile_v9_send_inspections_requested_by_fkey FOREIGN KEY (requested_by) REFERENCES accounts(id);

ALTER TABLE ONLY mobile_v9_send_inspections
    ADD CONSTRAINT mobile_v9_send_inspections_target_id_fkey FOREIGN KEY (target_id) REFERENCES mobile_targets(id);

ALTER TABLE ONLY mobile_v9_send_policy_versions
    ADD CONSTRAINT mobile_v9_send_policy_versions_created_by_fkey FOREIGN KEY (created_by) REFERENCES accounts(id);

ALTER TABLE ONLY mobile_v9_task_drafts
    ADD CONSTRAINT mobile_v9_task_drafts_actor_id_fkey FOREIGN KEY (actor_id) REFERENCES accounts(id);

ALTER TABLE ONLY mobile_v9_task_items
    ADD CONSTRAINT mobile_v9_task_items_target_id_fkey FOREIGN KEY (target_id) REFERENCES mobile_targets(id);

ALTER TABLE ONLY mobile_v9_task_items
    ADD CONSTRAINT mobile_v9_task_items_task_id_fkey FOREIGN KEY (task_id) REFERENCES mobile_v9_tasks(id);

ALTER TABLE ONLY mobile_v9_task_receipts
    ADD CONSTRAINT mobile_v9_task_receipts_attempt_id_fkey FOREIGN KEY (attempt_id) REFERENCES mobile_v9_attempts(id);

ALTER TABLE ONLY mobile_v9_task_receipts
    ADD CONSTRAINT mobile_v9_task_receipts_device_id_fkey FOREIGN KEY (device_id) REFERENCES execution_devices(id);

ALTER TABLE ONLY mobile_v9_tasks
    ADD CONSTRAINT mobile_v9_tasks_confirmed_by_fkey FOREIGN KEY (confirmed_by) REFERENCES accounts(id);

ALTER TABLE ONLY mobile_v9_tasks
    ADD CONSTRAINT mobile_v9_tasks_draft_id_fkey FOREIGN KEY (draft_id) REFERENCES mobile_v9_task_drafts(id);

ALTER TABLE ONLY mobile_v9_tasks
    ADD CONSTRAINT mobile_v9_tasks_preview_id_fkey FOREIGN KEY (preview_id) REFERENCES mobile_v9_previews(id);

-- trigger
CREATE TRIGGER mobile_dispatch_event_immutable BEFORE DELETE OR UPDATE ON mobile_v9_dispatch_events FOR EACH ROW EXECUTE FUNCTION mobile_capability_evidence_immutable();

CREATE TRIGGER mobile_v9_attempt_inbox BEFORE INSERT OR UPDATE ON mobile_v9_attempts FOR EACH ROW EXECUTE FUNCTION mobile_v9_inbox_changed();

CREATE TRIGGER mobile_v9_diagnostics_immutable BEFORE DELETE OR UPDATE ON mobile_v9_diagnostics FOR EACH ROW EXECUTE FUNCTION mobile_capability_evidence_immutable();

CREATE TRIGGER mobile_v9_dispatch_scope_immutable BEFORE DELETE OR UPDATE ON mobile_v9_dispatch_scopes FOR EACH ROW EXECUTE FUNCTION mobile_capability_evidence_immutable();

CREATE TRIGGER mobile_v9_event_immutable BEFORE DELETE OR UPDATE ON mobile_v9_send_events FOR EACH ROW EXECUTE FUNCTION mobile_capability_evidence_immutable();

CREATE TRIGGER mobile_v9_plan_immutable BEFORE DELETE OR UPDATE ON mobile_v9_plans FOR EACH ROW EXECUTE FUNCTION mobile_capability_evidence_immutable();

CREATE TRIGGER mobile_v9_reassignment_immutable BEFORE DELETE OR UPDATE ON mobile_v9_reassignments FOR EACH ROW EXECUTE FUNCTION mobile_capability_evidence_immutable();

CREATE TRIGGER mobile_v9_review_immutable BEFORE DELETE OR UPDATE ON mobile_v9_reviews FOR EACH ROW EXECUTE FUNCTION mobile_capability_evidence_immutable();

CREATE TRIGGER mobile_v9_send_policy_versions_immutable BEFORE DELETE OR UPDATE ON mobile_v9_send_policy_versions FOR EACH ROW EXECUTE FUNCTION mobile_capability_evidence_immutable();

CREATE TRIGGER mobile_v9_task_inbox AFTER UPDATE ON mobile_v9_tasks FOR EACH ROW EXECUTE FUNCTION mobile_v9_inbox_changed();

