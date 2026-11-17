-- 企业微信通讯录、群资源与发送资格。
-- 空库初始化基线；后续结构变更使用新增递增迁移。

-- function
CREATE FUNCTION check_wecom_directory_group() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
 IF NEW.directory_group_id IS NOT NULL AND NOT EXISTS (
  SELECT 1 FROM wecom_asset_objects WHERE id=NEW.directory_group_id AND kind='groups' AND corp_id=NEW.corp_id AND key=NEW.chat_id
 ) THEN RAISE EXCEPTION 'directory group identity mismatch' USING ERRCODE='23514'; END IF;
 RETURN NEW;
END $$;

-- table
CREATE TABLE official_channel_controls (
    credential_scope text NOT NULL,
    paused boolean DEFAULT false NOT NULL,
    revision bigint DEFAULT 1 NOT NULL,
    reason text NOT NULL,
    updated_by uuid,
    updated_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL
);

CREATE TABLE official_channel_health (
    credential_scope text NOT NULL,
    corp_id text,
    generation bigint,
    available boolean DEFAULT false NOT NULL,
    checked_at timestamp with time zone,
    last_success_at timestamp with time zone,
    last_failure_at timestamp with time zone,
    error_code text,
    next_probe_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL
);

CREATE TABLE wecom_asset_objects (
    id uuid NOT NULL,
    kind text NOT NULL,
    corp_id text NOT NULL,
    key text NOT NULL,
    parent_key text,
    member_key text,
    member_type integer,
    data jsonb NOT NULL,
    last_seen_run_id uuid,
    checked_at timestamp with time zone NOT NULL,
    credential_generation bigint NOT NULL,
    visibility text NOT NULL,
    verification text NOT NULL,
    source text DEFAULT 'snapshot'::text NOT NULL,
    parent_kind text GENERATED ALWAYS AS (
CASE kind
    WHEN 'members'::text THEN 'groups'::text
    ELSE NULL::text
END) STORED,
    CONSTRAINT wecom_asset_objects_check CHECK (((kind <> 'members'::text) OR ((parent_key IS NOT NULL) AND (member_key IS NOT NULL)))),
    CONSTRAINT wecom_asset_objects_check1 CHECK (((kind <> 'members'::text) OR (member_type IS NOT NULL))),
    CONSTRAINT wecom_asset_objects_credential_generation_check CHECK ((credential_generation > 0)),
    CONSTRAINT wecom_asset_objects_kind_check CHECK ((kind = ANY (ARRAY['groups'::text, 'members'::text]))),
    CONSTRAINT wecom_asset_objects_member_type_check CHECK ((member_type = ANY (ARRAY[1, 2]))),
    CONSTRAINT wecom_asset_objects_source_check CHECK ((source = ANY (ARRAY['snapshot'::text, 'point'::text]))),
    CONSTRAINT wecom_asset_objects_verification_check CHECK ((verification = ANY (ARRAY['verified'::text, 'unavailable'::text, 'unverified'::text]))),
    CONSTRAINT wecom_asset_objects_visibility_check CHECK ((visibility = ANY (ARRAY['visible'::text, 'not_seen'::text])))
);

CREATE TABLE wecom_asset_owner_exclusions (
    owner_key text NOT NULL,
    generation bigint NOT NULL,
    actor_id uuid NOT NULL,
    reason text NOT NULL,
    confirmed_at timestamp with time zone DEFAULT now() NOT NULL
);

CREATE TABLE wecom_asset_sync_items (
    run_id uuid NOT NULL,
    kind text NOT NULL,
    key text NOT NULL,
    data jsonb NOT NULL
);

CREATE TABLE wecom_asset_sync_runs (
    id uuid NOT NULL,
    dataset text NOT NULL,
    actor_id uuid,
    generation bigint NOT NULL,
    state text DEFAULT 'queued'::text NOT NULL,
    stage text DEFAULT 'queued'::text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    started_at timestamp with time zone,
    finished_at timestamp with time zone,
    lease_id uuid,
    lease_until timestamp with time zone,
    processed bigint DEFAULT 0 NOT NULL,
    pages bigint DEFAULT 0 NOT NULL,
    total bigint DEFAULT 0 NOT NULL,
    error_code text,
    complete boolean DEFAULT false NOT NULL,
    scope jsonb DEFAULT '[]'::jsonb NOT NULL,
    scope_changed boolean DEFAULT false NOT NULL,
    failed_owner text,
    CONSTRAINT wecom_asset_sync_runs_state_check CHECK ((state = ANY (ARRAY['queued'::text, 'running'::text, 'succeeded'::text, 'failed'::text])))
);

CREATE TABLE wecom_asset_sync_state (
    dataset text NOT NULL,
    current_run_id uuid,
    last_success_at timestamp with time zone,
    published_generation bigint,
    freshness text DEFAULT 'unverified'::text NOT NULL,
    scope jsonb DEFAULT '[]'::jsonb NOT NULL,
    scope_changed boolean DEFAULT false NOT NULL,
    CONSTRAINT wecom_asset_sync_state_dataset_check CHECK ((dataset = 'groups'::text))
);

CREATE TABLE wecom_departments (
    id bigint NOT NULL,
    parentid bigint NOT NULL,
    name text NOT NULL,
    "order" bigint DEFAULT 0 NOT NULL
);

CREATE TABLE wecom_directory_state (
    singleton boolean DEFAULT true NOT NULL,
    corp_id text NOT NULL,
    credential_scope text NOT NULL,
    credential_generation bigint DEFAULT 1 NOT NULL,
    snapshot_id uuid,
    last_success_at timestamp with time zone,
    next_call_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT wecom_directory_state_credential_generation_check CHECK ((credential_generation > 0)),
    CONSTRAINT wecom_directory_state_singleton_check CHECK (singleton)
);

CREATE TABLE wecom_directory_sync_runs (
    id uuid NOT NULL,
    actor_id uuid,
    generation bigint NOT NULL,
    state text DEFAULT 'queued'::text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    started_at timestamp with time zone,
    finished_at timestamp with time zone,
    lease_id uuid,
    lease_until timestamp with time zone,
    error_code text,
    department_count integer DEFAULT 0 NOT NULL,
    member_count integer DEFAULT 0 NOT NULL,
    snapshot jsonb,
    CONSTRAINT wecom_directory_sync_runs_generation_check CHECK ((generation > 0)),
    CONSTRAINT wecom_directory_sync_runs_state_check CHECK ((state = ANY (ARRAY['queued'::text, 'running'::text, 'succeeded'::text, 'failed'::text])))
);

CREATE TABLE wecom_members (
    id uuid NOT NULL,
    userid text NOT NULL,
    userid_key text GENERATED ALWAYS AS (lower((userid COLLATE "C"))) STORED,
    name text NOT NULL,
    status integer,
    department bigint[] DEFAULT '{}'::bigint[] NOT NULL,
    main_department bigint,
    visibility text DEFAULT 'visible'::text NOT NULL,
    verification text DEFAULT 'unverified'::text NOT NULL,
    checked_at timestamp with time zone,
    snapshot_id uuid,
    CONSTRAINT wecom_members_verification_check CHECK ((verification = ANY (ARRAY['verified'::text, 'unavailable'::text, 'unverified'::text]))),
    CONSTRAINT wecom_members_visibility_check CHECK ((visibility = ANY (ARRAY['visible'::text, 'not_seen'::text])))
);

CREATE TABLE wecom_groups (
    id uuid NOT NULL,
    corp_id text NOT NULL,
    chat_id text NOT NULL,
    name text NOT NULL,
    active boolean DEFAULT true NOT NULL,
    revision bigint DEFAULT 1 NOT NULL,
    directory_group_id uuid
);

CREATE TABLE wecom_sender_qualifications (
    account_id uuid NOT NULL,
    enabled boolean NOT NULL,
    revision bigint DEFAULT 1 NOT NULL,
    updated_by uuid NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);

-- constraint
ALTER TABLE ONLY official_channel_controls
    ADD CONSTRAINT official_channel_controls_pkey PRIMARY KEY (credential_scope);

ALTER TABLE ONLY official_channel_health
    ADD CONSTRAINT official_channel_health_pkey PRIMARY KEY (credential_scope);

ALTER TABLE ONLY wecom_asset_objects
    ADD CONSTRAINT wecom_asset_objects_corp_id_kind_key_key UNIQUE (corp_id, kind, key);

ALTER TABLE ONLY wecom_asset_objects
    ADD CONSTRAINT wecom_asset_objects_pkey PRIMARY KEY (id);

ALTER TABLE ONLY wecom_asset_owner_exclusions
    ADD CONSTRAINT wecom_asset_owner_exclusions_pkey PRIMARY KEY (owner_key);

ALTER TABLE ONLY wecom_asset_sync_items
    ADD CONSTRAINT wecom_asset_sync_items_pkey PRIMARY KEY (run_id, kind, key);

ALTER TABLE ONLY wecom_asset_sync_runs
    ADD CONSTRAINT wecom_asset_sync_runs_pkey PRIMARY KEY (id);

ALTER TABLE ONLY wecom_asset_sync_state
    ADD CONSTRAINT wecom_asset_sync_state_pkey PRIMARY KEY (dataset);

ALTER TABLE ONLY wecom_departments
    ADD CONSTRAINT wecom_departments_pkey PRIMARY KEY (id);

ALTER TABLE ONLY wecom_directory_state
    ADD CONSTRAINT wecom_directory_state_pkey PRIMARY KEY (singleton);

ALTER TABLE ONLY wecom_directory_sync_runs
    ADD CONSTRAINT wecom_directory_sync_runs_pkey PRIMARY KEY (id);

ALTER TABLE ONLY wecom_groups
    ADD CONSTRAINT wecom_groups_corp_id_chat_id_key UNIQUE (corp_id, chat_id);

ALTER TABLE ONLY wecom_groups
    ADD CONSTRAINT wecom_groups_directory_group_id_key UNIQUE (directory_group_id);

ALTER TABLE ONLY wecom_groups
    ADD CONSTRAINT wecom_groups_id_key UNIQUE (id);

ALTER TABLE ONLY wecom_groups
    ADD CONSTRAINT wecom_groups_pkey PRIMARY KEY (id);

ALTER TABLE ONLY wecom_members
    ADD CONSTRAINT wecom_members_pkey PRIMARY KEY (id);

ALTER TABLE ONLY wecom_members
    ADD CONSTRAINT wecom_members_userid_key_key UNIQUE (userid_key);

ALTER TABLE ONLY wecom_sender_qualifications
    ADD CONSTRAINT wecom_sender_qualifications_pkey PRIMARY KEY (account_id);

-- index
CREATE INDEX wecom_assets_member ON wecom_asset_objects USING btree (kind, member_key);

CREATE UNIQUE INDEX wecom_assets_one_active ON wecom_asset_sync_runs USING btree (dataset) WHERE (state = ANY (ARRAY['queued'::text, 'running'::text]));

CREATE INDEX wecom_assets_parent ON wecom_asset_objects USING btree (kind, parent_key);

CREATE UNIQUE INDEX wecom_directory_one_job ON wecom_directory_sync_runs USING btree ((true)) WHERE (state = ANY (ARRAY['queued'::text, 'running'::text]));

CREATE UNIQUE INDEX wecom_group_member_identity ON wecom_asset_objects USING btree (corp_id, parent_key, member_type, member_key) WHERE (kind = 'members'::text);

-- fk constraint
ALTER TABLE ONLY official_channel_controls
    ADD CONSTRAINT official_channel_controls_updated_by_fkey FOREIGN KEY (updated_by) REFERENCES accounts(id);

ALTER TABLE ONLY wecom_asset_owner_exclusions
    ADD CONSTRAINT wecom_asset_owner_exclusions_actor_id_fkey FOREIGN KEY (actor_id) REFERENCES accounts(id);

ALTER TABLE ONLY wecom_asset_objects
    ADD CONSTRAINT wecom_asset_parent_identity FOREIGN KEY (corp_id, parent_kind, parent_key) REFERENCES wecom_asset_objects(corp_id, kind, key) DEFERRABLE INITIALLY DEFERRED;

ALTER TABLE ONLY wecom_asset_sync_items
    ADD CONSTRAINT wecom_asset_sync_items_run_id_fkey FOREIGN KEY (run_id) REFERENCES wecom_asset_sync_runs(id) ON DELETE CASCADE;

ALTER TABLE ONLY wecom_asset_sync_runs
    ADD CONSTRAINT wecom_asset_sync_runs_actor_id_fkey FOREIGN KEY (actor_id) REFERENCES accounts(id);

ALTER TABLE ONLY wecom_asset_sync_runs
    ADD CONSTRAINT wecom_asset_sync_runs_dataset_fkey FOREIGN KEY (dataset) REFERENCES wecom_asset_sync_state(dataset);

ALTER TABLE ONLY wecom_directory_sync_runs
    ADD CONSTRAINT wecom_directory_sync_runs_actor_id_fkey FOREIGN KEY (actor_id) REFERENCES accounts(id);

ALTER TABLE ONLY wecom_groups
    ADD CONSTRAINT wecom_groups_directory_group_id_fkey FOREIGN KEY (directory_group_id) REFERENCES wecom_asset_objects(id);

ALTER TABLE ONLY wecom_sender_qualifications
    ADD CONSTRAINT wecom_sender_qualifications_account_id_fkey FOREIGN KEY (account_id) REFERENCES accounts(id);

ALTER TABLE ONLY wecom_sender_qualifications
    ADD CONSTRAINT wecom_sender_qualifications_updated_by_fkey FOREIGN KEY (updated_by) REFERENCES accounts(id);

-- trigger
CREATE TRIGGER wecom_group_directory_identity BEFORE INSERT OR UPDATE ON wecom_groups FOR EACH ROW EXECUTE FUNCTION check_wecom_directory_group();

