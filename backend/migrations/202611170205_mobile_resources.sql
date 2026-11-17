-- 手机目标、委托与运行配置。
-- 空库初始化基线；后续结构变更使用新增递增迁移。

-- function
CREATE FUNCTION mobile_capability_evidence_immutable() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN RAISE EXCEPTION 'mobile capability evidence is immutable'; END $$;

-- table
CREATE TABLE mobile_device_target_bindings (
    id uuid NOT NULL,
    device_id uuid NOT NULL,
    target_id uuid NOT NULL,
    enabled boolean DEFAULT true NOT NULL,
    revision bigint DEFAULT 1 NOT NULL,
    created_by uuid NOT NULL,
    updated_by uuid NOT NULL,
    created_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    updated_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    locator_revision bigint DEFAULT 1 NOT NULL,
    CONSTRAINT mobile_device_target_bindings_locator_revision_check CHECK ((locator_revision > 0))
);

CREATE TABLE mobile_runtime_configuration_current (
    capability text NOT NULL,
    configuration_id uuid NOT NULL,
    updated_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    CONSTRAINT mobile_runtime_configuration_current_capability_check CHECK ((capability = 'wecom.group_text'::text))
);

CREATE TABLE mobile_runtime_configurations (
    id uuid NOT NULL,
    capability text NOT NULL,
    version text NOT NULL,
    flow jsonb NOT NULL,
    flow_digest text NOT NULL,
    installed_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    CONSTRAINT mobile_runtime_configurations_capability_check CHECK ((capability = 'wecom.group_text'::text))
);

CREATE TABLE mobile_shared_delegation_decisions (
    id uuid NOT NULL,
    delegation_id uuid NOT NULL,
    accepted boolean NOT NULL,
    revision bigint NOT NULL,
    created_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    CONSTRAINT mobile_shared_delegation_decisions_revision_check CHECK ((revision > 0))
);

CREATE TABLE mobile_shared_delegations (
    id uuid NOT NULL,
    device_id uuid NOT NULL,
    credential_generation bigint NOT NULL,
    scope jsonb NOT NULL,
    scope_digest text NOT NULL,
    created_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL
);

CREATE TABLE mobile_shared_platform_identity (
    singleton boolean DEFAULT true NOT NULL,
    id uuid NOT NULL,
    CONSTRAINT mobile_shared_platform_identity_singleton_check CHECK (singleton)
);

CREATE TABLE mobile_shared_runtime (
    device_id uuid NOT NULL,
    credential_generation bigint NOT NULL,
    runtime_epoch uuid NOT NULL,
    ui_ready boolean NOT NULL,
    paused boolean NOT NULL,
    quiescent boolean NOT NULL,
    observed_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL
);

CREATE TABLE mobile_targets (
    id uuid NOT NULL,
    search_text text NOT NULL,
    remark text DEFAULT ''::text NOT NULL,
    enabled boolean DEFAULT true NOT NULL,
    archived_at timestamp with time zone,
    metadata_revision bigint DEFAULT 1 NOT NULL,
    locator_revision bigint DEFAULT 1 NOT NULL,
    created_by uuid NOT NULL,
    updated_by uuid NOT NULL,
    created_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    updated_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    CONSTRAINT mobile_targets_remark_check CHECK ((length(remark) <= 2000)),
    CONSTRAINT mobile_targets_search_text_check CHECK ((((length(search_text) >= 1) AND (length(search_text) <= 256)) AND (octet_length(search_text) <= 1024)))
);

CREATE TABLE shared_mobile_resource_events (
    id uuid NOT NULL,
    resource_id uuid NOT NULL,
    actor_id uuid NOT NULL,
    action text NOT NULL,
    detail jsonb NOT NULL,
    created_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL
);

CREATE TABLE shared_mobile_resource_operations (
    actor_id uuid NOT NULL,
    scope text NOT NULL,
    idempotency_key text NOT NULL,
    payload_hash text NOT NULL,
    result jsonb NOT NULL,
    created_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL
);

-- constraint
ALTER TABLE ONLY mobile_device_target_bindings
    ADD CONSTRAINT mobile_binding_device_target_unique UNIQUE (device_id, target_id);

ALTER TABLE ONLY mobile_device_target_bindings
    ADD CONSTRAINT mobile_binding_scope_identity UNIQUE (device_id, target_id, id);

ALTER TABLE ONLY mobile_device_target_bindings
    ADD CONSTRAINT mobile_device_target_bindings_device_id_target_id_key UNIQUE (device_id, target_id);

ALTER TABLE ONLY mobile_device_target_bindings
    ADD CONSTRAINT mobile_device_target_bindings_id_key UNIQUE (id);

ALTER TABLE ONLY mobile_device_target_bindings
    ADD CONSTRAINT mobile_device_target_bindings_pkey PRIMARY KEY (id);

ALTER TABLE ONLY shared_mobile_resource_events
    ADD CONSTRAINT mobile_execution_audit_events_pkey PRIMARY KEY (id);

ALTER TABLE ONLY shared_mobile_resource_operations
    ADD CONSTRAINT mobile_execution_operations_pkey PRIMARY KEY (actor_id, scope, idempotency_key);

ALTER TABLE ONLY mobile_runtime_configuration_current
    ADD CONSTRAINT mobile_runtime_configuration_current_pkey PRIMARY KEY (capability);

ALTER TABLE ONLY mobile_runtime_configurations
    ADD CONSTRAINT mobile_runtime_configurations_id_capability_key UNIQUE (id, capability);

ALTER TABLE ONLY mobile_runtime_configurations
    ADD CONSTRAINT mobile_runtime_configurations_pkey PRIMARY KEY (id);

ALTER TABLE ONLY mobile_shared_delegation_decisions
    ADD CONSTRAINT mobile_shared_delegation_decisions_delegation_id_revision_key UNIQUE (delegation_id, revision);

ALTER TABLE ONLY mobile_shared_delegation_decisions
    ADD CONSTRAINT mobile_shared_delegation_decisions_pkey PRIMARY KEY (id);

ALTER TABLE ONLY mobile_shared_delegations
    ADD CONSTRAINT mobile_shared_delegations_device_id_credential_generation_key UNIQUE (device_id, credential_generation);

ALTER TABLE ONLY mobile_shared_delegations
    ADD CONSTRAINT mobile_shared_delegations_device_id_id_key UNIQUE (device_id, id);

ALTER TABLE ONLY mobile_shared_delegations
    ADD CONSTRAINT mobile_shared_delegations_pkey PRIMARY KEY (id);

ALTER TABLE ONLY mobile_shared_platform_identity
    ADD CONSTRAINT mobile_shared_platform_identity_id_key UNIQUE (id);

ALTER TABLE ONLY mobile_shared_platform_identity
    ADD CONSTRAINT mobile_shared_platform_identity_pkey PRIMARY KEY (singleton);

ALTER TABLE ONLY mobile_shared_runtime
    ADD CONSTRAINT mobile_shared_runtime_pkey PRIMARY KEY (device_id);

ALTER TABLE ONLY mobile_targets
    ADD CONSTRAINT mobile_targets_id_key UNIQUE (id);

ALTER TABLE ONLY mobile_targets
    ADD CONSTRAINT mobile_targets_pkey PRIMARY KEY (id);

-- index
CREATE INDEX mobile_binding_target_candidates ON mobile_device_target_bindings USING btree (target_id, device_id) WHERE enabled;

CREATE INDEX mobile_targets_page ON mobile_targets USING btree (created_at, id) WHERE (archived_at IS NULL);

-- fk constraint
ALTER TABLE ONLY mobile_device_target_bindings
    ADD CONSTRAINT mobile_binding_device_fk FOREIGN KEY (device_id) REFERENCES execution_devices(id);

ALTER TABLE ONLY mobile_device_target_bindings
    ADD CONSTRAINT mobile_device_target_bindings_created_by_fkey FOREIGN KEY (created_by) REFERENCES accounts(id);

ALTER TABLE ONLY mobile_device_target_bindings
    ADD CONSTRAINT mobile_device_target_bindings_target_id_fkey FOREIGN KEY (target_id) REFERENCES mobile_targets(id);

ALTER TABLE ONLY mobile_device_target_bindings
    ADD CONSTRAINT mobile_device_target_bindings_updated_by_fkey FOREIGN KEY (updated_by) REFERENCES accounts(id);

ALTER TABLE ONLY shared_mobile_resource_events
    ADD CONSTRAINT mobile_execution_audit_events_actor_id_fkey FOREIGN KEY (actor_id) REFERENCES accounts(id);

ALTER TABLE ONLY shared_mobile_resource_operations
    ADD CONSTRAINT mobile_execution_operations_actor_id_fkey FOREIGN KEY (actor_id) REFERENCES accounts(id);

ALTER TABLE ONLY mobile_runtime_configuration_current
    ADD CONSTRAINT mobile_runtime_configuration_c_configuration_id_capability_fkey FOREIGN KEY (configuration_id, capability) REFERENCES mobile_runtime_configurations(id, capability);

ALTER TABLE ONLY mobile_shared_delegation_decisions
    ADD CONSTRAINT mobile_shared_delegation_decisions_delegation_id_fkey FOREIGN KEY (delegation_id) REFERENCES mobile_shared_delegations(id);

ALTER TABLE ONLY mobile_shared_delegations
    ADD CONSTRAINT mobile_shared_delegations_device_id_fkey FOREIGN KEY (device_id) REFERENCES execution_devices(id);

ALTER TABLE ONLY mobile_shared_runtime
    ADD CONSTRAINT mobile_shared_runtime_device_id_fkey FOREIGN KEY (device_id) REFERENCES execution_devices(id);

ALTER TABLE ONLY mobile_targets
    ADD CONSTRAINT mobile_targets_created_by_fkey FOREIGN KEY (created_by) REFERENCES accounts(id);

ALTER TABLE ONLY mobile_targets
    ADD CONSTRAINT mobile_targets_updated_by_fkey FOREIGN KEY (updated_by) REFERENCES accounts(id);

-- trigger
CREATE TRIGGER mobile_shared_decision_immutable BEFORE DELETE OR UPDATE ON mobile_shared_delegation_decisions FOR EACH ROW EXECUTE FUNCTION mobile_capability_evidence_immutable();

CREATE TRIGGER mobile_shared_delegation_immutable BEFORE DELETE OR UPDATE ON mobile_shared_delegations FOR EACH ROW EXECUTE FUNCTION mobile_capability_evidence_immutable();

