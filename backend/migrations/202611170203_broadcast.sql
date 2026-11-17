-- 企业微信分发任务、批次与结果。
-- 空库初始化基线；后续结构变更使用新增递增迁移。

-- table
CREATE TABLE broadcast_batches (
    id uuid NOT NULL,
    task_id uuid NOT NULL,
    ordinal integer NOT NULL,
    sender text NOT NULL,
    sender_account uuid NOT NULL,
    binding_revision bigint NOT NULL,
    state text DEFAULT 'queued'::text NOT NULL,
    invocation_id uuid,
    claimed_at timestamp with time zone,
    msgid text,
    corp_id text NOT NULL,
    error_code text,
    member_status integer,
    last_synced_at timestamp with time zone,
    sync_error text,
    next_sync_at timestamp with time zone DEFAULT now() NOT NULL,
    stop_state text DEFAULT 'none'::text NOT NULL,
    reconciliation text,
    reconciliation_note text,
    sync_failures integer DEFAULT 0 NOT NULL,
    request_digest text,
    submitted_at timestamp with time zone,
    progress_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    CONSTRAINT broadcast_batches_reconciliation_check CHECK ((reconciliation = ANY (ARRAY['not_created'::text, 'sent'::text, 'linked'::text]))),
    CONSTRAINT broadcast_batches_state_check CHECK ((state = ANY (ARRAY['queued'::text, 'submitting'::text, 'created'::text, 'rejected'::text, 'unknown'::text, 'cancelled'::text]))),
    CONSTRAINT broadcast_batches_stop_state_check CHECK ((stop_state = ANY (ARRAY['none'::text, 'requested'::text, 'stopped'::text, 'unknown'::text, 'rejected'::text]))),
    CONSTRAINT broadcast_batches_sync_failures_check CHECK ((sync_failures >= 0))
);

CREATE TABLE broadcast_draft_requests (
    source_namespace text NOT NULL,
    actor_id uuid NOT NULL,
    request_key text NOT NULL,
    request_hash text NOT NULL,
    task_id uuid NOT NULL,
    CONSTRAINT broadcast_draft_requests_request_key_check CHECK (((length(request_key) >= 1) AND (length(request_key) <= 128)))
);

CREATE TABLE broadcast_events (
    id uuid NOT NULL,
    task_id uuid NOT NULL,
    actor_id uuid,
    action text NOT NULL,
    detail jsonb DEFAULT '{}'::jsonb NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);

CREATE TABLE broadcast_evidence_queries (
    actor_id uuid NOT NULL,
    task_id uuid NOT NULL,
    batch_id uuid NOT NULL,
    queried_at timestamp with time zone DEFAULT now() NOT NULL
);

CREATE TABLE broadcast_operations (
    id uuid NOT NULL,
    task_id uuid NOT NULL,
    batch_id uuid,
    actor_id uuid NOT NULL,
    operation text NOT NULL,
    idempotency_key text NOT NULL,
    request_hash text NOT NULL,
    state text DEFAULT 'pending'::text NOT NULL,
    claimed_at timestamp with time zone,
    error_code text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT broadcast_operations_state_check CHECK ((state = ANY (ARRAY['pending'::text, 'calling'::text, 'done'::text, 'unknown'::text, 'rejected'::text])))
);

CREATE TABLE broadcast_preview_intents (
    actor_id uuid NOT NULL,
    request_key text NOT NULL,
    input_hash text NOT NULL,
    task_id uuid NOT NULL,
    response jsonb NOT NULL
);

CREATE TABLE broadcast_previews (
    task_id uuid NOT NULL,
    revision bigint NOT NULL,
    digest text NOT NULL,
    expires_at timestamp with time zone NOT NULL,
    actor_id uuid
);

CREATE TABLE broadcast_targets (
    id uuid NOT NULL,
    task_id uuid NOT NULL,
    batch_id uuid NOT NULL,
    group_id uuid NOT NULL,
    chat_id text NOT NULL,
    group_name text NOT NULL,
    group_revision bigint NOT NULL,
    rejected boolean DEFAULT false NOT NULL,
    official_status integer,
    send_time bigint,
    progress_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL
);

CREATE TABLE broadcast_task_snapshots (
    task_id uuid NOT NULL,
    task_revision bigint NOT NULL,
    snapshot_id uuid NOT NULL
);

CREATE TABLE broadcast_tasks (
    id uuid NOT NULL,
    creator_id uuid NOT NULL,
    content text NOT NULL,
    content_hash text NOT NULL,
    revision bigint DEFAULT 1 NOT NULL,
    state text DEFAULT 'draft'::text NOT NULL,
    corp_id text,
    generation bigint,
    confirmed_by uuid,
    confirmed_at timestamp with time zone,
    stopped boolean DEFAULT false NOT NULL,
    source_task_id uuid,
    resend_reason text DEFAULT ''::text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    content_source text DEFAULT 'text'::text NOT NULL,
    source_namespace text DEFAULT 'builtin.direct'::text NOT NULL,
    draft_input jsonb NOT NULL,
    authorization_kind text DEFAULT 'manual'::text NOT NULL,
    authorized_by uuid,
    execution_mode text DEFAULT 'official_customer_group_broadcast'::text NOT NULL,
    CONSTRAINT broadcast_confirmed_scope CHECK (((state <> 'confirmed'::text) OR ((corp_id IS NOT NULL) AND (generation IS NOT NULL)))),
    CONSTRAINT broadcast_content_source CHECK ((content_source = 'text'::text)),
    CONSTRAINT broadcast_resolved_scope CHECK (((corp_id IS NULL) = (generation IS NULL))),
    CONSTRAINT broadcast_tasks_authorization_kind_check CHECK ((authorization_kind = ANY (ARRAY['manual'::text, 'source_route'::text]))),
    CONSTRAINT broadcast_tasks_content_check CHECK (((octet_length(content) >= 1) AND (octet_length(content) <= 4000))),
    CONSTRAINT broadcast_tasks_execution_mode_check CHECK ((execution_mode = 'official_customer_group_broadcast'::text)),
    CONSTRAINT broadcast_tasks_state_check CHECK ((state = ANY (ARRAY['draft'::text, 'confirmed'::text, 'abandoned'::text])))
);

-- constraint
ALTER TABLE ONLY broadcast_batches
    ADD CONSTRAINT broadcast_batches_corp_id_msgid_key UNIQUE (corp_id, msgid);

ALTER TABLE ONLY broadcast_batches
    ADD CONSTRAINT broadcast_batches_invocation_id_key UNIQUE (invocation_id);

ALTER TABLE ONLY broadcast_batches
    ADD CONSTRAINT broadcast_batches_pkey PRIMARY KEY (id);

ALTER TABLE ONLY broadcast_batches
    ADD CONSTRAINT broadcast_batches_task_id_id_key UNIQUE (task_id, id);

ALTER TABLE ONLY broadcast_batches
    ADD CONSTRAINT broadcast_batches_task_id_ordinal_key UNIQUE (task_id, ordinal);

ALTER TABLE ONLY broadcast_draft_requests
    ADD CONSTRAINT broadcast_draft_requests_pkey PRIMARY KEY (source_namespace, actor_id, request_key);

ALTER TABLE ONLY broadcast_events
    ADD CONSTRAINT broadcast_events_pkey PRIMARY KEY (id);

ALTER TABLE ONLY broadcast_evidence_queries
    ADD CONSTRAINT broadcast_evidence_queries_pkey PRIMARY KEY (actor_id, task_id, batch_id);

ALTER TABLE ONLY broadcast_operations
    ADD CONSTRAINT broadcast_operations_actor_id_operation_idempoten_key UNIQUE (actor_id, operation, idempotency_key);

ALTER TABLE ONLY broadcast_operations
    ADD CONSTRAINT broadcast_operations_pkey PRIMARY KEY (id);

ALTER TABLE ONLY broadcast_preview_intents
    ADD CONSTRAINT broadcast_preview_intents_pkey PRIMARY KEY (actor_id, request_key);

ALTER TABLE ONLY broadcast_previews
    ADD CONSTRAINT broadcast_previews_pkey PRIMARY KEY (task_id);

ALTER TABLE ONLY broadcast_targets
    ADD CONSTRAINT broadcast_targets_batch_id_chat_id_key UNIQUE (batch_id, chat_id);

ALTER TABLE ONLY broadcast_targets
    ADD CONSTRAINT broadcast_targets_pkey PRIMARY KEY (id);

ALTER TABLE ONLY broadcast_targets
    ADD CONSTRAINT broadcast_targets_task_id_group_id_key UNIQUE (task_id, group_id);

ALTER TABLE ONLY broadcast_task_snapshots
    ADD CONSTRAINT broadcast_task_snapshots_pkey PRIMARY KEY (task_id, task_revision);

ALTER TABLE ONLY broadcast_tasks
    ADD CONSTRAINT broadcast_tasks_id_key UNIQUE (id);

ALTER TABLE ONLY broadcast_tasks
    ADD CONSTRAINT broadcast_tasks_pkey PRIMARY KEY (id);

-- index
CREATE INDEX broadcast_events_task ON broadcast_events USING btree (task_id, created_at DESC, id);

CREATE INDEX broadcast_pending ON broadcast_batches USING btree (state, next_sync_at);

CREATE INDEX broadcast_task_created_page ON broadcast_tasks USING btree (created_at DESC, id);

-- fk constraint
ALTER TABLE ONLY broadcast_batches
    ADD CONSTRAINT broadcast_batches_sender_account_fkey FOREIGN KEY (sender_account) REFERENCES accounts(id);

ALTER TABLE ONLY broadcast_batches
    ADD CONSTRAINT broadcast_batches_task_id_fkey FOREIGN KEY (task_id) REFERENCES broadcast_tasks(id);

ALTER TABLE ONLY broadcast_draft_requests
    ADD CONSTRAINT broadcast_draft_requests_actor_id_fkey FOREIGN KEY (actor_id) REFERENCES accounts(id);

ALTER TABLE ONLY broadcast_draft_requests
    ADD CONSTRAINT broadcast_draft_requests_task_id_fkey FOREIGN KEY (task_id) REFERENCES broadcast_tasks(id);

ALTER TABLE ONLY broadcast_events
    ADD CONSTRAINT broadcast_events_actor_id_fkey FOREIGN KEY (actor_id) REFERENCES accounts(id);

ALTER TABLE ONLY broadcast_events
    ADD CONSTRAINT broadcast_events_task_id_fkey FOREIGN KEY (task_id) REFERENCES broadcast_tasks(id);

ALTER TABLE ONLY broadcast_evidence_queries
    ADD CONSTRAINT broadcast_evidence_queries_actor_id_fkey FOREIGN KEY (actor_id) REFERENCES accounts(id);

ALTER TABLE ONLY broadcast_evidence_queries
    ADD CONSTRAINT broadcast_evidence_queries_batch_id_fkey FOREIGN KEY (batch_id) REFERENCES broadcast_batches(id);

ALTER TABLE ONLY broadcast_evidence_queries
    ADD CONSTRAINT broadcast_evidence_queries_task_id_fkey FOREIGN KEY (task_id) REFERENCES broadcast_tasks(id);

ALTER TABLE ONLY broadcast_operations
    ADD CONSTRAINT broadcast_operations_actor_id_fkey FOREIGN KEY (actor_id) REFERENCES accounts(id);

ALTER TABLE ONLY broadcast_operations
    ADD CONSTRAINT broadcast_operations_task_id_batch_id_fkey FOREIGN KEY (task_id, batch_id) REFERENCES broadcast_batches(task_id, id);

ALTER TABLE ONLY broadcast_operations
    ADD CONSTRAINT broadcast_operations_task_id_fkey FOREIGN KEY (task_id) REFERENCES broadcast_tasks(id);

ALTER TABLE ONLY broadcast_preview_intents
    ADD CONSTRAINT broadcast_preview_intents_actor_id_fkey FOREIGN KEY (actor_id) REFERENCES accounts(id);

ALTER TABLE ONLY broadcast_preview_intents
    ADD CONSTRAINT broadcast_preview_intents_task_id_fkey FOREIGN KEY (task_id) REFERENCES broadcast_tasks(id);

ALTER TABLE ONLY broadcast_previews
    ADD CONSTRAINT broadcast_previews_actor_id_fkey FOREIGN KEY (actor_id) REFERENCES accounts(id);

ALTER TABLE ONLY broadcast_previews
    ADD CONSTRAINT broadcast_previews_task_id_fkey FOREIGN KEY (task_id) REFERENCES broadcast_tasks(id);

ALTER TABLE ONLY broadcast_targets
    ADD CONSTRAINT broadcast_targets_group_id_fkey FOREIGN KEY (group_id) REFERENCES wecom_groups(id);

ALTER TABLE ONLY broadcast_targets
    ADD CONSTRAINT broadcast_targets_task_id_batch_id_fkey FOREIGN KEY (task_id, batch_id) REFERENCES broadcast_batches(task_id, id);

ALTER TABLE ONLY broadcast_task_snapshots
    ADD CONSTRAINT broadcast_task_snapshots_task_id_fkey FOREIGN KEY (task_id) REFERENCES broadcast_tasks(id);

ALTER TABLE ONLY broadcast_tasks
    ADD CONSTRAINT broadcast_tasks_authorized_by_fkey FOREIGN KEY (authorized_by) REFERENCES accounts(id);

ALTER TABLE ONLY broadcast_tasks
    ADD CONSTRAINT broadcast_tasks_confirmed_by_fkey FOREIGN KEY (confirmed_by) REFERENCES accounts(id);

ALTER TABLE ONLY broadcast_tasks
    ADD CONSTRAINT broadcast_tasks_creator_id_fkey FOREIGN KEY (creator_id) REFERENCES accounts(id);

ALTER TABLE ONLY broadcast_tasks
    ADD CONSTRAINT broadcast_tasks_source_task_id_fkey FOREIGN KEY (source_task_id) REFERENCES broadcast_tasks(id);

