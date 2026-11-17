-- 账号、会话、系统审计与应用元数据。
-- 空库初始化基线；后续结构变更使用新增递增迁移。

-- table
CREATE TABLE accounts (
    id uuid NOT NULL,
    username text NOT NULL,
    password_hash text,
    display_name text NOT NULL,
    platform_admin boolean DEFAULT false NOT NULL,
    active boolean DEFAULT true NOT NULL,
    wecom_member_id uuid,
    wecom_binding_revision bigint DEFAULT 0 NOT NULL,
    wx_openid text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    avatar_url text DEFAULT ''::text NOT NULL
);

CREATE TABLE application_metadata (
    singleton boolean DEFAULT true NOT NULL,
    schema_generation integer NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT application_metadata_schema_generation_check CHECK ((schema_generation > 0)),
    CONSTRAINT application_metadata_singleton_check CHECK (singleton)
);

CREATE TABLE audit_events (
    id uuid NOT NULL,
    actor_id uuid,
    action text NOT NULL,
    object_id uuid,
    detail jsonb DEFAULT '{}'::jsonb NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    request_id text
);

CREATE TABLE login_attempts (
    username text NOT NULL,
    failures integer NOT NULL,
    reset_at timestamp with time zone NOT NULL
);

CREATE TABLE media_assets (
    id uuid NOT NULL,
    owner_id uuid NOT NULL,
    mime_type text NOT NULL,
    size_bytes bigint NOT NULL,
    content bytea NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);

CREATE TABLE oauth_states (
    state_hash text NOT NULL,
    session_hash text,
    return_path text NOT NULL,
    expires_at timestamp with time zone NOT NULL
);

CREATE TABLE platform_cache (
    key text NOT NULL,
    value text NOT NULL,
    expires_at timestamp with time zone NOT NULL
);

CREATE TABLE sessions (
    token_hash text NOT NULL,
    account_id uuid NOT NULL,
    csrf text NOT NULL,
    expires_at timestamp with time zone NOT NULL,
    last_seen_at timestamp with time zone DEFAULT now() NOT NULL,
    credential_generation bigint,
    kind text NOT NULL,
    wecom_authenticated boolean DEFAULT false NOT NULL,
    wecom_binding_revision bigint DEFAULT 0 NOT NULL,
    CONSTRAINT sessions_kind_check CHECK ((kind = ANY (ARRAY['browser'::text, 'miniapp'::text])))
);

-- constraint
ALTER TABLE ONLY accounts
    ADD CONSTRAINT accounts_pkey PRIMARY KEY (id);

ALTER TABLE ONLY accounts
    ADD CONSTRAINT accounts_username_key UNIQUE (username);

ALTER TABLE ONLY accounts
    ADD CONSTRAINT accounts_wecom_member_id_key UNIQUE (wecom_member_id);

ALTER TABLE ONLY accounts
    ADD CONSTRAINT accounts_wx_openid_key UNIQUE (wx_openid);

ALTER TABLE ONLY application_metadata
    ADD CONSTRAINT application_metadata_pkey PRIMARY KEY (singleton);

ALTER TABLE ONLY audit_events
    ADD CONSTRAINT audit_events_pkey PRIMARY KEY (id);

ALTER TABLE ONLY login_attempts
    ADD CONSTRAINT login_attempts_pkey PRIMARY KEY (username);

ALTER TABLE ONLY media_assets
    ADD CONSTRAINT media_assets_pkey PRIMARY KEY (id);

ALTER TABLE ONLY oauth_states
    ADD CONSTRAINT oauth_states_pkey PRIMARY KEY (state_hash);

ALTER TABLE ONLY platform_cache
    ADD CONSTRAINT platform_cache_pkey PRIMARY KEY (key);

ALTER TABLE ONLY sessions
    ADD CONSTRAINT sessions_pkey PRIMARY KEY (token_hash);

-- index
CREATE INDEX audit_created_page ON audit_events USING btree (created_at DESC, id DESC);

CREATE INDEX sessions_expiry ON sessions USING btree (expires_at);

-- fk constraint
ALTER TABLE ONLY audit_events
    ADD CONSTRAINT audit_events_actor_id_fkey FOREIGN KEY (actor_id) REFERENCES accounts(id);

ALTER TABLE ONLY media_assets
    ADD CONSTRAINT media_assets_owner_id_fkey FOREIGN KEY (owner_id) REFERENCES accounts(id);

ALTER TABLE ONLY sessions
    ADD CONSTRAINT sessions_account_id_fkey FOREIGN KEY (account_id) REFERENCES accounts(id);

