-- 单次执行授权、交付出站与结果。
-- 空库初始化基线；后续结构变更使用新增递增迁移。

-- function
CREATE FUNCTION notify_execution_superseded() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
 INSERT INTO device_control_commands(id,device_id,kind,resource_id)
 VALUES(gen_random_uuid(),NEW.device_id,'cancel_execution',NEW.id);
 RETURN NEW;
END $$;

-- table
CREATE TABLE mobile_capability_deliveries (
    release_id uuid NOT NULL,
    acceptance_id uuid NOT NULL,
    supported_apks text[] NOT NULL,
    provenance jsonb NOT NULL,
    imported_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    CONSTRAINT mobile_capability_deliveries_supported_apks_check CHECK ((cardinality(supported_apks) > 0))
);

CREATE TABLE mobile_delivery_exchanges (
    id uuid NOT NULL,
    device_id uuid NOT NULL,
    generation bigint NOT NULL,
    key_digest text NOT NULL,
    replay_of uuid,
    intake_id uuid,
    attempt_id uuid,
    execution_id uuid,
    result text DEFAULT 'started'::text NOT NULL,
    reason_code text,
    response_state text DEFAULT 'not_started'::text NOT NULL,
    created_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    finished_at timestamp with time zone,
    response_at timestamp with time zone,
    transport text DEFAULT 'wss'::text NOT NULL,
    connection_id uuid,
    connection_epoch bigint
);

CREATE TABLE mobile_execution_grants (
    id uuid NOT NULL,
    attempt_id uuid,
    inspection_id uuid,
    device_id uuid NOT NULL,
    credential_generation bigint NOT NULL,
    kind text NOT NULL,
    state text DEFAULT 'active'::text NOT NULL,
    session_closed boolean DEFAULT false NOT NULL,
    superseded_reason text,
    assigned_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    accepted_at timestamp with time zone,
    result_deadline_at timestamp with time zone DEFAULT (clock_timestamp() + '00:05:00'::interval) NOT NULL,
    execute_before timestamp with time zone DEFAULT (clock_timestamp() + '00:03:30'::interval) NOT NULL,
    result_received_at timestamp with time zone,
    result_credential_hash text NOT NULL,
    envelope bytea NOT NULL,
    result_id uuid,
    result_body jsonb,
    result_hash text,
    receipt jsonb,
    applied boolean DEFAULT false NOT NULL,
    device_started_at timestamp with time zone,
    device_finished_at timestamp with time zone,
    receipt_deadline_at timestamp with time zone DEFAULT (clock_timestamp() + '00:01:00'::interval) NOT NULL,
    CONSTRAINT mobile_execution_grants_check CHECK ((((kind = 'send'::text) AND (attempt_id IS NOT NULL) AND (inspection_id IS NULL)) OR ((kind = 'inspection'::text) AND (inspection_id IS NOT NULL) AND (attempt_id IS NULL)))),
    CONSTRAINT mobile_execution_grants_kind_check CHECK ((kind = ANY (ARRAY['send'::text, 'inspection'::text]))),
    CONSTRAINT mobile_execution_grants_state_check CHECK ((state = ANY (ARRAY['active'::text, 'completed'::text, 'superseded'::text])))
);

CREATE TABLE mobile_execution_delivery_outbox (
    execution_id uuid NOT NULL,
    next_send_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    finished_at timestamp with time zone,
    attempts integer DEFAULT 0 NOT NULL
);

CREATE TABLE mobile_execution_diagnostics (
    id uuid NOT NULL,
    execution_id uuid NOT NULL,
    device_id uuid NOT NULL,
    credential_generation bigint NOT NULL,
    code text NOT NULL,
    stage text NOT NULL,
    details jsonb NOT NULL,
    created_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL
);

CREATE TABLE mobile_execution_usage_budgets (
    execution_id uuid NOT NULL,
    usage_id uuid NOT NULL,
    budget_root uuid NOT NULL
);

-- constraint
ALTER TABLE ONLY mobile_capability_deliveries
    ADD CONSTRAINT mobile_capability_deliveries_pkey PRIMARY KEY (release_id);

ALTER TABLE ONLY mobile_delivery_exchanges
    ADD CONSTRAINT mobile_delivery_exchanges_pkey PRIMARY KEY (id);

ALTER TABLE ONLY mobile_execution_delivery_outbox
    ADD CONSTRAINT mobile_execution_delivery_outbox_pkey PRIMARY KEY (execution_id);

ALTER TABLE ONLY mobile_execution_diagnostics
    ADD CONSTRAINT mobile_execution_diagnostics_pkey PRIMARY KEY (id);

ALTER TABLE ONLY mobile_execution_grants
    ADD CONSTRAINT mobile_execution_grants_attempt_id_key UNIQUE (attempt_id);

ALTER TABLE ONLY mobile_execution_grants
    ADD CONSTRAINT mobile_execution_grants_pkey PRIMARY KEY (id);

ALTER TABLE ONLY mobile_execution_grants
    ADD CONSTRAINT mobile_execution_grants_result_id_key UNIQUE (result_id);

ALTER TABLE ONLY mobile_execution_usage_budgets
    ADD CONSTRAINT mobile_execution_usage_budgets_pkey PRIMARY KEY (execution_id, budget_root);

-- index
CREATE INDEX mobile_delivery_due ON mobile_execution_delivery_outbox USING btree (next_send_at) WHERE (finished_at IS NULL);

CREATE INDEX mobile_exchange_device ON mobile_delivery_exchanges USING btree (device_id, created_at DESC, id DESC);

CREATE INDEX mobile_exchange_intake ON mobile_delivery_exchanges USING btree (intake_id, created_at DESC, id DESC);

CREATE INDEX mobile_exchange_key ON mobile_delivery_exchanges USING btree (device_id, generation, key_digest, created_at DESC);

CREATE UNIQUE INDEX mobile_execution_device_active ON mobile_execution_grants USING btree (device_id) WHERE (NOT session_closed);

CREATE INDEX mobile_execution_due ON mobile_execution_grants USING btree (result_deadline_at) WHERE (state = 'active'::text);

CREATE INDEX mobile_execution_unapplied ON mobile_execution_grants USING btree (id) WHERE ((result_id IS NOT NULL) AND (NOT applied));

CREATE INDEX mobile_execution_usage_root ON mobile_execution_usage_budgets USING btree (budget_root);

CREATE UNIQUE INDEX mobile_send_attempt_once ON mobile_execution_grants USING btree (attempt_id) WHERE (kind = 'send'::text);

-- fk constraint
ALTER TABLE ONLY mobile_capability_deliveries
    ADD CONSTRAINT mobile_capability_deliveries_acceptance_id_fkey FOREIGN KEY (acceptance_id) REFERENCES mobile_v9_acceptance_runs(id);

ALTER TABLE ONLY mobile_capability_deliveries
    ADD CONSTRAINT mobile_capability_deliveries_release_id_fkey FOREIGN KEY (release_id) REFERENCES mobile_v9_adapter_releases(id);

ALTER TABLE ONLY mobile_delivery_exchanges
    ADD CONSTRAINT mobile_delivery_exchanges_replay_of_fkey FOREIGN KEY (replay_of) REFERENCES mobile_delivery_exchanges(id);

ALTER TABLE ONLY mobile_execution_delivery_outbox
    ADD CONSTRAINT mobile_execution_delivery_outbox_execution_id_fkey FOREIGN KEY (execution_id) REFERENCES mobile_execution_grants(id);

ALTER TABLE ONLY mobile_execution_diagnostics
    ADD CONSTRAINT mobile_execution_diagnostics_device_id_fkey FOREIGN KEY (device_id) REFERENCES execution_devices(id);

ALTER TABLE ONLY mobile_execution_diagnostics
    ADD CONSTRAINT mobile_execution_diagnostics_execution_id_fkey FOREIGN KEY (execution_id) REFERENCES mobile_execution_grants(id);

ALTER TABLE ONLY mobile_execution_grants
    ADD CONSTRAINT mobile_execution_grants_attempt_id_fkey FOREIGN KEY (attempt_id) REFERENCES mobile_v9_attempts(id);

ALTER TABLE ONLY mobile_execution_grants
    ADD CONSTRAINT mobile_execution_grants_device_id_fkey FOREIGN KEY (device_id) REFERENCES execution_devices(id);

ALTER TABLE ONLY mobile_execution_grants
    ADD CONSTRAINT mobile_execution_grants_inspection_id_fkey FOREIGN KEY (inspection_id) REFERENCES mobile_device_target_bindings(id);

ALTER TABLE ONLY mobile_execution_usage_budgets
    ADD CONSTRAINT mobile_execution_usage_budgets_execution_id_fkey FOREIGN KEY (execution_id) REFERENCES mobile_execution_grants(id);

ALTER TABLE ONLY mobile_execution_usage_budgets
    ADD CONSTRAINT mobile_execution_usage_budgets_usage_id_fkey FOREIGN KEY (usage_id) REFERENCES mobile_usages(id);

-- trigger
CREATE TRIGGER execution_superseded AFTER UPDATE OF state ON mobile_execution_grants FOR EACH ROW WHEN (((new.state = 'superseded'::text) AND (old.state = 'active'::text))) EXECUTE FUNCTION notify_execution_superseded();

CREATE TRIGGER mobile_capability_deliveries_immutable BEFORE DELETE OR UPDATE ON mobile_capability_deliveries FOR EACH ROW EXECUTE FUNCTION mobile_capability_evidence_immutable();

