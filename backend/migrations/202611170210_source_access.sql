-- 来源调用方、凭据、请求防重与访问审计。
-- 空库初始化基线；后续结构变更使用新增递增迁移。

-- table
CREATE TABLE source_access_audit (
    id uuid NOT NULL,
    source_id uuid,
    key_id text,
    actor_id uuid,
    action text NOT NULL,
    result_code text NOT NULL,
    request_id text NOT NULL,
    detail jsonb DEFAULT '{}'::jsonb NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);

CREATE TABLE source_callers (
    source_id uuid NOT NULL,
    revision bigint DEFAULT 1 NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);

CREATE TABLE source_credentials (
    key_id text NOT NULL,
    source_id uuid NOT NULL,
    unit_name text NOT NULL,
    remark text DEFAULT ''::text NOT NULL,
    encrypted_secret bytea,
    encryption_version text NOT NULL,
    status text NOT NULL,
    valid_from timestamp with time zone NOT NULL,
    valid_until timestamp with time zone,
    revision bigint DEFAULT 1 NOT NULL,
    created_by uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    last_used_at timestamp with time zone,
    delivery_pending boolean DEFAULT true NOT NULL,
    CONSTRAINT source_credentials_check CHECK ((valid_until > valid_from)),
    CONSTRAINT source_credentials_status_check CHECK ((status = ANY (ARRAY['active'::text, 'disabled'::text, 'revoked'::text]))),
    CONSTRAINT source_credentials_unit_name_check CHECK ((length(TRIM(BOTH FROM unit_name)) > 0))
);

CREATE TABLE source_query_usage (
    source_id uuid NOT NULL,
    minute timestamp with time zone NOT NULL,
    queries bigint DEFAULT 0 NOT NULL,
    last_queried_at timestamp with time zone DEFAULT now() NOT NULL
);

CREATE TABLE source_request_nonces (
    key_id text NOT NULL,
    source_id uuid NOT NULL,
    nonce uuid NOT NULL,
    expires_at timestamp with time zone NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);

-- constraint
ALTER TABLE ONLY source_access_audit
    ADD CONSTRAINT source_access_audit_pkey PRIMARY KEY (id);

ALTER TABLE ONLY source_credentials
    ADD CONSTRAINT source_credentials_key_id_source_id_key UNIQUE (key_id, source_id);

ALTER TABLE ONLY source_credentials
    ADD CONSTRAINT source_credentials_pkey PRIMARY KEY (key_id);

ALTER TABLE ONLY source_callers
    ADD CONSTRAINT source_platforms_pkey PRIMARY KEY (source_id);

ALTER TABLE ONLY source_query_usage
    ADD CONSTRAINT source_query_usage_pkey PRIMARY KEY (source_id, minute);

ALTER TABLE ONLY source_request_nonces
    ADD CONSTRAINT source_request_nonces_pkey PRIMARY KEY (key_id, nonce);

-- index
CREATE UNIQUE INDEX source_credentials_one_per_platform ON source_credentials USING btree (source_id);

CREATE INDEX source_nonce_expiry ON source_request_nonces USING btree (expires_at);

CREATE INDEX source_nonce_rate ON source_request_nonces USING btree (key_id, created_at);

-- comment
COMMENT ON TABLE source_callers IS 'Internal caller identity retained for historical receipts and audit after credential deletion';

-- fk constraint
ALTER TABLE ONLY source_access_audit
    ADD CONSTRAINT source_access_audit_actor_id_fkey FOREIGN KEY (actor_id) REFERENCES accounts(id);

ALTER TABLE ONLY source_access_audit
    ADD CONSTRAINT source_access_audit_source_id_fkey FOREIGN KEY (source_id) REFERENCES source_callers(source_id);

ALTER TABLE ONLY source_credentials
    ADD CONSTRAINT source_credentials_created_by_fkey FOREIGN KEY (created_by) REFERENCES accounts(id);

ALTER TABLE ONLY source_credentials
    ADD CONSTRAINT source_credentials_source_id_fkey FOREIGN KEY (source_id) REFERENCES source_callers(source_id);

ALTER TABLE ONLY source_query_usage
    ADD CONSTRAINT source_query_usage_source_id_fkey FOREIGN KEY (source_id) REFERENCES source_callers(source_id);

ALTER TABLE ONLY source_request_nonces
    ADD CONSTRAINT source_request_nonces_key_id_source_id_fkey FOREIGN KEY (key_id, source_id) REFERENCES source_credentials(key_id, source_id);

