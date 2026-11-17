-- 设备注册、凭据与 WSS 控制连接。
-- 空库初始化基线；后续结构变更使用新增递增迁移。

-- table
CREATE TABLE device_control_commands (
    id uuid NOT NULL,
    sequence bigint NOT NULL,
    device_id uuid NOT NULL,
    kind text NOT NULL,
    state text DEFAULT 'queued'::text NOT NULL,
    resource_id uuid,
    desired_revision bigint,
    request_key text,
    request_digest text,
    actor_id uuid,
    created_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    expires_at timestamp with time zone DEFAULT (clock_timestamp() + '1 day'::interval) NOT NULL,
    accepted_at timestamp with time zone,
    completed_at timestamp with time zone,
    result_code text,
    CONSTRAINT device_control_commands_kind_check CHECK ((kind = ANY (ARRAY['cancel_execution'::text, 'configuration_changed'::text, 'request_status'::text]))),
    CONSTRAINT device_control_commands_state_check CHECK ((state = ANY (ARRAY['queued'::text, 'delivered'::text, 'accepted'::text, 'completed'::text, 'rejected'::text, 'expired'::text])))
);

CREATE TABLE device_control_policy (
    singleton boolean DEFAULT true NOT NULL,
    heartbeat_interval_seconds smallint DEFAULT 60 NOT NULL,
    suspect_after_seconds smallint DEFAULT 130 NOT NULL,
    offline_after_seconds smallint DEFAULT 190 NOT NULL,
    CONSTRAINT device_control_policy_check CHECK ((suspect_after_seconds >= ((2 * heartbeat_interval_seconds) + 10))),
    CONSTRAINT device_control_policy_check1 CHECK (((offline_after_seconds >= ((3 * heartbeat_interval_seconds) + 10)) AND (offline_after_seconds > suspect_after_seconds) AND (offline_after_seconds <= 1810))),
    CONSTRAINT device_control_policy_heartbeat_interval_seconds_check CHECK (((heartbeat_interval_seconds >= 60) AND (heartbeat_interval_seconds <= 300))),
    CONSTRAINT device_control_policy_singleton_check CHECK (singleton)
);

CREATE TABLE device_control_sessions (
    device_id uuid NOT NULL,
    connection_id uuid NOT NULL,
    epoch bigint DEFAULT 1 NOT NULL,
    transport text NOT NULL,
    connected boolean DEFAULT true NOT NULL,
    last_heartbeat_at timestamp with time zone,
    readiness text DEFAULT 'waiting'::text NOT NULL,
    block_reason text,
    desired_paused boolean DEFAULT false NOT NULL,
    desired_revision bigint DEFAULT 0 NOT NULL,
    credential_generation bigint,
    connected_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    last_disconnected_at timestamp with time zone,
    disconnect_reason text,
    presence_started_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    runtime_epoch uuid,
    CONSTRAINT device_control_sessions_transport_check CHECK ((transport = 'wss'::text)),
    CONSTRAINT remote_hosting_pause_retired CHECK ((NOT desired_paused))
);

CREATE TABLE device_credentials (
    token_hash text NOT NULL,
    device_id uuid NOT NULL,
    generation bigint NOT NULL,
    expires_at timestamp with time zone NOT NULL,
    revoked_at timestamp with time zone
);

CREATE TABLE execution_devices (
    id uuid NOT NULL,
    name text NOT NULL,
    enabled boolean DEFAULT true NOT NULL,
    revision bigint DEFAULT 1 NOT NULL,
    credential_generation bigint DEFAULT 0 NOT NULL,
    client_instance_id text,
    last_seen_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    execution_protocol_version integer DEFAULT 10 NOT NULL,
    supported_protocol_versions integer[] DEFAULT ARRAY[10] NOT NULL,
    CONSTRAINT execution_devices_execution_protocol_version_check CHECK ((execution_protocol_version = 10))
);

CREATE TABLE device_control_pulse_receipts (
    device_id uuid NOT NULL,
    connection_id uuid NOT NULL,
    heartbeat_id uuid NOT NULL,
    heartbeat_sequence bigint DEFAULT 0 NOT NULL,
    CONSTRAINT device_control_pulse_receipts_heartbeat_sequence_check CHECK ((heartbeat_sequence >= 0))
);

CREATE TABLE device_credential_renewals (
    candidate_hash text NOT NULL,
    previous_hash text NOT NULL,
    activated_at timestamp with time zone
);

CREATE TABLE device_enrollments (
    id uuid NOT NULL,
    device_id uuid NOT NULL,
    code_hash text NOT NULL,
    expires_at timestamp with time zone NOT NULL,
    consumed_at timestamp with time zone
);

CREATE TABLE device_pairing_attempt_windows (
    scope text NOT NULL,
    requests integer NOT NULL,
    reset_at timestamp with time zone NOT NULL,
    CONSTRAINT device_pairing_attempt_windows_requests_check CHECK (((requests >= 1) AND (requests <= 101)))
);

CREATE TABLE device_registry_events (
    id uuid NOT NULL,
    principal text NOT NULL,
    operation text NOT NULL,
    device_id uuid,
    detail jsonb NOT NULL,
    created_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL
);

CREATE TABLE device_registry_operations (
    principal text NOT NULL,
    operation text NOT NULL,
    idempotency_key text NOT NULL,
    request_hash text NOT NULL,
    response jsonb NOT NULL,
    created_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL
);

-- sequence
CREATE SEQUENCE device_control_commands_sequence_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;

-- sequence owned by
ALTER SEQUENCE device_control_commands_sequence_seq OWNED BY device_control_commands.sequence;

-- default
ALTER TABLE ONLY device_control_commands ALTER COLUMN sequence SET DEFAULT nextval('device_control_commands_sequence_seq'::regclass);

-- constraint
ALTER TABLE ONLY device_control_commands
    ADD CONSTRAINT device_control_commands_device_id_request_key_key UNIQUE (device_id, request_key);

ALTER TABLE ONLY device_control_commands
    ADD CONSTRAINT device_control_commands_pkey PRIMARY KEY (id);

ALTER TABLE ONLY device_control_commands
    ADD CONSTRAINT device_control_commands_sequence_key UNIQUE (sequence);

ALTER TABLE ONLY device_control_policy
    ADD CONSTRAINT device_control_policy_pkey PRIMARY KEY (singleton);

ALTER TABLE ONLY device_control_pulse_receipts
    ADD CONSTRAINT device_control_pulse_receipts_pkey PRIMARY KEY (device_id);

ALTER TABLE ONLY device_control_sessions
    ADD CONSTRAINT device_control_sessions_pkey PRIMARY KEY (device_id);

ALTER TABLE ONLY device_credential_renewals
    ADD CONSTRAINT device_credential_renewals_pkey PRIMARY KEY (candidate_hash);

ALTER TABLE ONLY device_credentials
    ADD CONSTRAINT device_credentials_pkey PRIMARY KEY (token_hash);

ALTER TABLE ONLY device_enrollments
    ADD CONSTRAINT device_enrollments_code_hash_key UNIQUE (code_hash);

ALTER TABLE ONLY device_enrollments
    ADD CONSTRAINT device_enrollments_pkey PRIMARY KEY (id);

ALTER TABLE ONLY device_pairing_attempt_windows
    ADD CONSTRAINT device_pairing_attempt_windows_pkey PRIMARY KEY (scope);

ALTER TABLE ONLY device_registry_events
    ADD CONSTRAINT device_registry_events_pkey PRIMARY KEY (id);

ALTER TABLE ONLY device_registry_operations
    ADD CONSTRAINT device_registry_operations_pkey PRIMARY KEY (principal, operation, idempotency_key);

ALTER TABLE ONLY execution_devices
    ADD CONSTRAINT execution_devices_pkey PRIMARY KEY (id);

-- check constraint
ALTER TABLE device_control_commands
    ADD CONSTRAINT remote_hosting_commands_retired CHECK ((kind <> ALL (ARRAY['pause_execution'::text, 'resume_execution'::text]))) NOT VALID;

-- index
CREATE INDEX device_control_pending ON device_control_commands USING btree (device_id, sequence) WHERE (state = ANY (ARRAY['queued'::text, 'delivered'::text, 'accepted'::text]));

CREATE UNIQUE INDEX device_credential_pending_renewal ON device_credential_renewals USING btree (previous_hash) WHERE (activated_at IS NULL);

CREATE INDEX device_pairing_attempt_windows_reset ON device_pairing_attempt_windows USING btree (reset_at);

-- fk constraint
ALTER TABLE ONLY device_control_commands
    ADD CONSTRAINT device_control_commands_actor_id_fkey FOREIGN KEY (actor_id) REFERENCES accounts(id);

ALTER TABLE ONLY device_control_commands
    ADD CONSTRAINT device_control_commands_device_id_fkey FOREIGN KEY (device_id) REFERENCES execution_devices(id);

ALTER TABLE ONLY device_control_pulse_receipts
    ADD CONSTRAINT device_control_pulse_receipts_device_id_fkey FOREIGN KEY (device_id) REFERENCES execution_devices(id) ON DELETE CASCADE;

ALTER TABLE ONLY device_control_sessions
    ADD CONSTRAINT device_control_sessions_device_id_fkey FOREIGN KEY (device_id) REFERENCES execution_devices(id);

ALTER TABLE ONLY device_credential_renewals
    ADD CONSTRAINT device_credential_renewals_candidate_hash_fkey FOREIGN KEY (candidate_hash) REFERENCES device_credentials(token_hash);

ALTER TABLE ONLY device_credential_renewals
    ADD CONSTRAINT device_credential_renewals_previous_hash_fkey FOREIGN KEY (previous_hash) REFERENCES device_credentials(token_hash);

ALTER TABLE ONLY device_credentials
    ADD CONSTRAINT device_credentials_device_id_fkey FOREIGN KEY (device_id) REFERENCES execution_devices(id);

ALTER TABLE ONLY device_enrollments
    ADD CONSTRAINT device_enrollments_device_id_fkey FOREIGN KEY (device_id) REFERENCES execution_devices(id);

