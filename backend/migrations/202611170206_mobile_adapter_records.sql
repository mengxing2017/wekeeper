-- 手机适配及支持记录（不作为业务执行前置条件）。
-- 空库初始化基线；后续结构变更使用新增递增迁移。

-- table
CREATE TABLE mobile_v9_acceptance_runs (
    id uuid NOT NULL,
    release_id uuid NOT NULL,
    stage text NOT NULL,
    apk_id uuid NOT NULL,
    evidence_ids uuid[] NOT NULL,
    validation_ids uuid[] NOT NULL,
    cases jsonb NOT NULL,
    soak_hours bigint NOT NULL,
    soak_passed boolean NOT NULL,
    historical_debt_closed boolean NOT NULL,
    reviewer_id uuid NOT NULL,
    created_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    reason text NOT NULL,
    CONSTRAINT mobile_v9_acceptance_runs_stage_check CHECK ((stage = ANY (ARRAY['test_send'::text, 'production_allowlist'::text])))
);

CREATE TABLE mobile_v9_activation_events (
    id uuid NOT NULL,
    release_id uuid NOT NULL,
    revision bigint NOT NULL,
    actor_id uuid NOT NULL,
    reason text NOT NULL,
    created_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL
);

CREATE TABLE mobile_v9_activations (
    capability text NOT NULL,
    release_id uuid NOT NULL,
    revision bigint NOT NULL,
    updated_by uuid NOT NULL,
    CONSTRAINT mobile_v9_activations_capability_check CHECK ((capability = 'wecom.group_text'::text))
);

CREATE TABLE mobile_v9_adapter_releases (
    id uuid NOT NULL,
    name text NOT NULL,
    version text NOT NULL,
    flow_id uuid NOT NULL,
    flow jsonb NOT NULL,
    content_digest text NOT NULL,
    baseline_id uuid NOT NULL,
    evidence_ids uuid[] NOT NULL,
    created_by uuid NOT NULL,
    created_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL
);

CREATE TABLE mobile_v9_adapter_withdrawals (
    release_id uuid NOT NULL,
    reason text NOT NULL,
    created_by uuid NOT NULL,
    created_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL
);

CREATE TABLE mobile_v9_artifacts (
    id uuid NOT NULL,
    kind text NOT NULL,
    name text NOT NULL,
    sha256 text NOT NULL,
    content bytea NOT NULL,
    created_by uuid NOT NULL,
    created_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    CONSTRAINT mobile_v9_artifacts_content_check CHECK (((octet_length(content) > 0) AND (octet_length(content) <= 134217728))),
    CONSTRAINT mobile_v9_artifacts_kind_check CHECK ((kind = ANY (ARRAY['apk'::text, 'evidence'::text]))),
    CONSTRAINT mobile_v9_artifacts_sha256_check CHECK ((sha256 ~ '^[0-9a-f]{64}$'::text))
);

CREATE TABLE mobile_v9_baselines (
    id uuid NOT NULL,
    flow_id uuid NOT NULL,
    device_id uuid,
    samples jsonb NOT NULL,
    evidence_ids uuid[] NOT NULL,
    min_interval_ms bigint NOT NULL,
    recorded_by uuid NOT NULL,
    created_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    external_source jsonb,
    CONSTRAINT baseline_source CHECK ((((device_id IS NOT NULL) AND (external_source IS NULL)) OR ((device_id IS NULL) AND (external_source IS NOT NULL)))),
    CONSTRAINT mobile_v9_baselines_min_interval_ms_check CHECK ((min_interval_ms >= 1000))
);

CREATE TABLE mobile_v9_flows (
    id uuid NOT NULL,
    name text NOT NULL,
    revision bigint DEFAULT 1 NOT NULL,
    flow jsonb NOT NULL,
    digest text NOT NULL,
    created_by uuid NOT NULL,
    created_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL
);

CREATE TABLE mobile_v9_human_samples (
    id uuid NOT NULL,
    flow_id uuid NOT NULL,
    device_id uuid NOT NULL,
    credential_generation bigint NOT NULL,
    requested_by uuid NOT NULL,
    challenge text NOT NULL,
    state text DEFAULT 'pending'::text NOT NULL,
    runtime_epoch uuid,
    elapsed_ms bigint,
    started_at timestamp with time zone,
    completed_at timestamp with time zone,
    expires_at timestamp with time zone DEFAULT (clock_timestamp() + '1 day'::interval) NOT NULL,
    created_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    CONSTRAINT mobile_v9_human_samples_state_check CHECK ((state = ANY (ARRAY['pending'::text, 'running'::text, 'passed'::text, 'failed'::text])))
);

CREATE TABLE mobile_v9_scope_grants (
    id uuid NOT NULL,
    release_id uuid,
    acceptance_id uuid,
    stage text NOT NULL,
    device_ids uuid[] NOT NULL,
    target_ids uuid[] NOT NULL,
    auto_targets boolean DEFAULT false NOT NULL,
    max_calls bigint NOT NULL,
    expires_at timestamp with time zone NOT NULL,
    reason text NOT NULL,
    created_by uuid NOT NULL,
    created_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    resource_revisions jsonb NOT NULL,
    CONSTRAINT mobile_scope_technical_pair CHECK (((release_id IS NULL) = (acceptance_id IS NULL))),
    CONSTRAINT mobile_v9_scope_grants_max_calls_check CHECK ((max_calls > 0)),
    CONSTRAINT mobile_v9_scope_grants_stage_check CHECK ((stage = ANY (ARRAY['test_send'::text, 'production_allowlist'::text])))
);

CREATE TABLE mobile_v9_scope_revocations (
    grant_id uuid NOT NULL,
    reason text NOT NULL,
    actor_id uuid NOT NULL,
    created_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL
);

CREATE TABLE mobile_v9_support_events (
    id uuid NOT NULL,
    request_id uuid NOT NULL,
    actor_id uuid NOT NULL,
    revision bigint NOT NULL,
    state text NOT NULL,
    response text NOT NULL,
    release_id uuid,
    created_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL
);

CREATE TABLE mobile_v9_support_requests (
    id uuid NOT NULL,
    device_id uuid NOT NULL,
    reason text NOT NULL,
    state text NOT NULL,
    revision bigint DEFAULT 1 NOT NULL,
    response text DEFAULT ''::text NOT NULL,
    release_id uuid,
    created_by uuid,
    created_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    task_id uuid,
    wait_code text,
    CONSTRAINT mobile_v9_support_requests_state_check CHECK ((state = ANY (ARRAY['submitted'::text, 'investigating'::text, 'resolved'::text, 'declined'::text])))
);

-- constraint
ALTER TABLE ONLY mobile_v9_acceptance_runs
    ADD CONSTRAINT mobile_v9_acceptance_runs_pkey PRIMARY KEY (id);

ALTER TABLE ONLY mobile_v9_activation_events
    ADD CONSTRAINT mobile_v9_activation_events_pkey PRIMARY KEY (id);

ALTER TABLE ONLY mobile_v9_activations
    ADD CONSTRAINT mobile_v9_activations_pkey PRIMARY KEY (capability);

ALTER TABLE ONLY mobile_v9_adapter_releases
    ADD CONSTRAINT mobile_v9_adapter_releases_name_version_key UNIQUE (name, version);

ALTER TABLE ONLY mobile_v9_adapter_releases
    ADD CONSTRAINT mobile_v9_adapter_releases_pkey PRIMARY KEY (id);

ALTER TABLE ONLY mobile_v9_adapter_withdrawals
    ADD CONSTRAINT mobile_v9_adapter_withdrawals_pkey PRIMARY KEY (release_id);

ALTER TABLE ONLY mobile_v9_artifacts
    ADD CONSTRAINT mobile_v9_artifacts_pkey PRIMARY KEY (id);

ALTER TABLE ONLY mobile_v9_baselines
    ADD CONSTRAINT mobile_v9_baselines_pkey PRIMARY KEY (id);

ALTER TABLE ONLY mobile_v9_flows
    ADD CONSTRAINT mobile_v9_flows_pkey PRIMARY KEY (id);

ALTER TABLE ONLY mobile_v9_human_samples
    ADD CONSTRAINT mobile_v9_human_samples_pkey PRIMARY KEY (id);

ALTER TABLE ONLY mobile_v9_scope_grants
    ADD CONSTRAINT mobile_v9_scope_grants_pkey PRIMARY KEY (id);

ALTER TABLE ONLY mobile_v9_scope_revocations
    ADD CONSTRAINT mobile_v9_scope_revocations_pkey PRIMARY KEY (grant_id);

ALTER TABLE ONLY mobile_v9_support_events
    ADD CONSTRAINT mobile_v9_support_events_pkey PRIMARY KEY (id);

ALTER TABLE ONLY mobile_v9_support_events
    ADD CONSTRAINT mobile_v9_support_events_request_id_revision_key UNIQUE (request_id, revision);

ALTER TABLE ONLY mobile_v9_support_requests
    ADD CONSTRAINT mobile_v9_support_requests_pkey PRIMARY KEY (id);

-- index
CREATE UNIQUE INDEX mobile_v9_wait_feedback_once ON mobile_v9_support_requests USING btree (task_id, wait_code) WHERE ((task_id IS NOT NULL) AND (wait_code IS NOT NULL));

-- fk constraint
ALTER TABLE ONLY mobile_v9_acceptance_runs
    ADD CONSTRAINT mobile_v9_acceptance_runs_apk_id_fkey FOREIGN KEY (apk_id) REFERENCES mobile_v9_artifacts(id);

ALTER TABLE ONLY mobile_v9_acceptance_runs
    ADD CONSTRAINT mobile_v9_acceptance_runs_release_id_fkey FOREIGN KEY (release_id) REFERENCES mobile_v9_adapter_releases(id);

ALTER TABLE ONLY mobile_v9_acceptance_runs
    ADD CONSTRAINT mobile_v9_acceptance_runs_reviewer_id_fkey FOREIGN KEY (reviewer_id) REFERENCES accounts(id);

ALTER TABLE ONLY mobile_v9_activation_events
    ADD CONSTRAINT mobile_v9_activation_events_actor_id_fkey FOREIGN KEY (actor_id) REFERENCES accounts(id);

ALTER TABLE ONLY mobile_v9_activation_events
    ADD CONSTRAINT mobile_v9_activation_events_release_id_fkey FOREIGN KEY (release_id) REFERENCES mobile_v9_adapter_releases(id);

ALTER TABLE ONLY mobile_v9_activations
    ADD CONSTRAINT mobile_v9_activations_release_id_fkey FOREIGN KEY (release_id) REFERENCES mobile_v9_adapter_releases(id);

ALTER TABLE ONLY mobile_v9_activations
    ADD CONSTRAINT mobile_v9_activations_updated_by_fkey FOREIGN KEY (updated_by) REFERENCES accounts(id);

ALTER TABLE ONLY mobile_v9_adapter_releases
    ADD CONSTRAINT mobile_v9_adapter_releases_baseline_id_fkey FOREIGN KEY (baseline_id) REFERENCES mobile_v9_baselines(id);

ALTER TABLE ONLY mobile_v9_adapter_releases
    ADD CONSTRAINT mobile_v9_adapter_releases_created_by_fkey FOREIGN KEY (created_by) REFERENCES accounts(id);

ALTER TABLE ONLY mobile_v9_adapter_releases
    ADD CONSTRAINT mobile_v9_adapter_releases_flow_id_fkey FOREIGN KEY (flow_id) REFERENCES mobile_v9_flows(id);

ALTER TABLE ONLY mobile_v9_adapter_withdrawals
    ADD CONSTRAINT mobile_v9_adapter_withdrawals_created_by_fkey FOREIGN KEY (created_by) REFERENCES accounts(id);

ALTER TABLE ONLY mobile_v9_adapter_withdrawals
    ADD CONSTRAINT mobile_v9_adapter_withdrawals_release_id_fkey FOREIGN KEY (release_id) REFERENCES mobile_v9_adapter_releases(id);

ALTER TABLE ONLY mobile_v9_artifacts
    ADD CONSTRAINT mobile_v9_artifacts_created_by_fkey FOREIGN KEY (created_by) REFERENCES accounts(id);

ALTER TABLE ONLY mobile_v9_baselines
    ADD CONSTRAINT mobile_v9_baselines_device_id_fkey FOREIGN KEY (device_id) REFERENCES execution_devices(id);

ALTER TABLE ONLY mobile_v9_baselines
    ADD CONSTRAINT mobile_v9_baselines_flow_id_fkey FOREIGN KEY (flow_id) REFERENCES mobile_v9_flows(id);

ALTER TABLE ONLY mobile_v9_baselines
    ADD CONSTRAINT mobile_v9_baselines_recorded_by_fkey FOREIGN KEY (recorded_by) REFERENCES accounts(id);

ALTER TABLE ONLY mobile_v9_flows
    ADD CONSTRAINT mobile_v9_flows_created_by_fkey FOREIGN KEY (created_by) REFERENCES accounts(id);

ALTER TABLE ONLY mobile_v9_human_samples
    ADD CONSTRAINT mobile_v9_human_samples_device_id_fkey FOREIGN KEY (device_id) REFERENCES execution_devices(id);

ALTER TABLE ONLY mobile_v9_human_samples
    ADD CONSTRAINT mobile_v9_human_samples_flow_id_fkey FOREIGN KEY (flow_id) REFERENCES mobile_v9_flows(id);

ALTER TABLE ONLY mobile_v9_human_samples
    ADD CONSTRAINT mobile_v9_human_samples_requested_by_fkey FOREIGN KEY (requested_by) REFERENCES accounts(id);

ALTER TABLE ONLY mobile_v9_scope_grants
    ADD CONSTRAINT mobile_v9_scope_grants_acceptance_id_fkey FOREIGN KEY (acceptance_id) REFERENCES mobile_v9_acceptance_runs(id);

ALTER TABLE ONLY mobile_v9_scope_grants
    ADD CONSTRAINT mobile_v9_scope_grants_created_by_fkey FOREIGN KEY (created_by) REFERENCES accounts(id);

ALTER TABLE ONLY mobile_v9_scope_grants
    ADD CONSTRAINT mobile_v9_scope_grants_release_id_fkey FOREIGN KEY (release_id) REFERENCES mobile_v9_adapter_releases(id);

ALTER TABLE ONLY mobile_v9_scope_revocations
    ADD CONSTRAINT mobile_v9_scope_revocations_actor_id_fkey FOREIGN KEY (actor_id) REFERENCES accounts(id);

ALTER TABLE ONLY mobile_v9_scope_revocations
    ADD CONSTRAINT mobile_v9_scope_revocations_grant_id_fkey FOREIGN KEY (grant_id) REFERENCES mobile_v9_scope_grants(id);

ALTER TABLE ONLY mobile_v9_support_events
    ADD CONSTRAINT mobile_v9_support_events_actor_id_fkey FOREIGN KEY (actor_id) REFERENCES accounts(id);

ALTER TABLE ONLY mobile_v9_support_events
    ADD CONSTRAINT mobile_v9_support_events_release_id_fkey FOREIGN KEY (release_id) REFERENCES mobile_v9_adapter_releases(id);

ALTER TABLE ONLY mobile_v9_support_events
    ADD CONSTRAINT mobile_v9_support_events_request_id_fkey FOREIGN KEY (request_id) REFERENCES mobile_v9_support_requests(id);

ALTER TABLE ONLY mobile_v9_support_requests
    ADD CONSTRAINT mobile_v9_support_requests_created_by_fkey FOREIGN KEY (created_by) REFERENCES accounts(id);

ALTER TABLE ONLY mobile_v9_support_requests
    ADD CONSTRAINT mobile_v9_support_requests_device_id_fkey FOREIGN KEY (device_id) REFERENCES execution_devices(id);

ALTER TABLE ONLY mobile_v9_support_requests
    ADD CONSTRAINT mobile_v9_support_requests_release_id_fkey FOREIGN KEY (release_id) REFERENCES mobile_v9_adapter_releases(id);

-- trigger
CREATE TRIGGER mobile_v9_acceptance_runs_immutable BEFORE DELETE OR UPDATE ON mobile_v9_acceptance_runs FOR EACH ROW EXECUTE FUNCTION mobile_capability_evidence_immutable();

CREATE TRIGGER mobile_v9_activation_events_immutable BEFORE DELETE OR UPDATE ON mobile_v9_activation_events FOR EACH ROW EXECUTE FUNCTION mobile_capability_evidence_immutable();

CREATE TRIGGER mobile_v9_adapter_releases_immutable BEFORE DELETE OR UPDATE ON mobile_v9_adapter_releases FOR EACH ROW EXECUTE FUNCTION mobile_capability_evidence_immutable();

CREATE TRIGGER mobile_v9_adapter_withdrawals_immutable BEFORE DELETE OR UPDATE ON mobile_v9_adapter_withdrawals FOR EACH ROW EXECUTE FUNCTION mobile_capability_evidence_immutable();

CREATE TRIGGER mobile_v9_artifacts_immutable BEFORE DELETE OR UPDATE ON mobile_v9_artifacts FOR EACH ROW EXECUTE FUNCTION mobile_capability_evidence_immutable();

CREATE TRIGGER mobile_v9_baselines_immutable BEFORE DELETE OR UPDATE ON mobile_v9_baselines FOR EACH ROW EXECUTE FUNCTION mobile_capability_evidence_immutable();

CREATE TRIGGER mobile_v9_flows_immutable BEFORE DELETE OR UPDATE ON mobile_v9_flows FOR EACH ROW EXECUTE FUNCTION mobile_capability_evidence_immutable();

CREATE TRIGGER mobile_v9_scope_grants_immutable BEFORE DELETE OR UPDATE ON mobile_v9_scope_grants FOR EACH ROW EXECUTE FUNCTION mobile_capability_evidence_immutable();

CREATE TRIGGER mobile_v9_scope_revocations_immutable BEFORE DELETE OR UPDATE ON mobile_v9_scope_revocations FOR EACH ROW EXECUTE FUNCTION mobile_capability_evidence_immutable();

CREATE TRIGGER mobile_v9_support_events_immutable BEFORE DELETE OR UPDATE ON mobile_v9_support_events FOR EACH ROW EXECUTE FUNCTION mobile_capability_evidence_immutable();

