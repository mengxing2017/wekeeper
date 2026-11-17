-- 业务请求、预览与决策审计。
-- 空库初始化基线；后续结构变更使用新增递增迁移。

-- function
CREATE FUNCTION product_events_immutable() RETURNS trigger
    LANGUAGE plpgsql
    AS $$ BEGIN RAISE EXCEPTION 'immutable business event'; END $$;

-- table
CREATE TABLE product_decision_replays (
    request_id uuid NOT NULL,
    previous_revision bigint NOT NULL,
    actor_id uuid NOT NULL,
    input_hash text NOT NULL,
    result jsonb NOT NULL
);

CREATE TABLE product_previews (
    id uuid NOT NULL,
    actor_id uuid NOT NULL,
    kind text NOT NULL,
    payload jsonb NOT NULL,
    dependencies jsonb DEFAULT '{}'::jsonb NOT NULL,
    digest text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    expires_at timestamp with time zone DEFAULT (now() + '00:15:00'::interval) NOT NULL,
    result jsonb,
    confirmed_at timestamp with time zone,
    CONSTRAINT product_previews_check CHECK (((result IS NULL) = (confirmed_at IS NULL)))
);

CREATE TABLE product_request_events (
    id uuid NOT NULL,
    request_id uuid NOT NULL,
    actor_id uuid NOT NULL,
    state text NOT NULL,
    response text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);

CREATE TABLE product_requests (
    id uuid NOT NULL,
    kind text NOT NULL,
    actor_id uuid NOT NULL,
    context_key text NOT NULL,
    payload jsonb NOT NULL,
    state text DEFAULT 'pending'::text NOT NULL,
    owner_id uuid,
    response text DEFAULT ''::text NOT NULL,
    revision bigint DEFAULT 1 NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT product_requests_kind_check CHECK ((kind = ANY (ARRAY['channel'::text, 'official_usage'::text, 'broadcast_issue'::text]))),
    CONSTRAINT product_requests_state_check CHECK ((state = ANY (ARRAY['pending'::text, 'investigating'::text, 'waiting_user'::text, 'approved'::text, 'declined'::text, 'resolved'::text, 'revoked'::text])))
);

-- constraint
ALTER TABLE ONLY product_decision_replays
    ADD CONSTRAINT product_decision_replays_pkey PRIMARY KEY (request_id, previous_revision);

ALTER TABLE ONLY product_previews
    ADD CONSTRAINT product_previews_pkey PRIMARY KEY (id);

ALTER TABLE ONLY product_request_events
    ADD CONSTRAINT product_request_events_pkey PRIMARY KEY (id);

ALTER TABLE ONLY product_requests
    ADD CONSTRAINT product_requests_pkey PRIMARY KEY (id);

-- index
CREATE INDEX product_previews_expiry ON product_previews USING btree (expires_at) WHERE (result IS NULL);

CREATE UNIQUE INDEX product_requests_open ON product_requests USING btree (kind, context_key) WHERE (state = ANY (ARRAY['pending'::text, 'investigating'::text, 'waiting_user'::text]));

-- fk constraint
ALTER TABLE ONLY product_decision_replays
    ADD CONSTRAINT product_decision_replays_actor_id_fkey FOREIGN KEY (actor_id) REFERENCES accounts(id);

ALTER TABLE ONLY product_decision_replays
    ADD CONSTRAINT product_decision_replays_request_id_fkey FOREIGN KEY (request_id) REFERENCES product_requests(id);

ALTER TABLE ONLY product_previews
    ADD CONSTRAINT product_previews_actor_id_fkey FOREIGN KEY (actor_id) REFERENCES accounts(id);

ALTER TABLE ONLY product_request_events
    ADD CONSTRAINT product_request_events_actor_id_fkey FOREIGN KEY (actor_id) REFERENCES accounts(id);

ALTER TABLE ONLY product_request_events
    ADD CONSTRAINT product_request_events_request_id_fkey FOREIGN KEY (request_id) REFERENCES product_requests(id);

ALTER TABLE ONLY product_requests
    ADD CONSTRAINT product_requests_actor_id_fkey FOREIGN KEY (actor_id) REFERENCES accounts(id);

ALTER TABLE ONLY product_requests
    ADD CONSTRAINT product_requests_owner_id_fkey FOREIGN KEY (owner_id) REFERENCES accounts(id);

-- trigger
CREATE TRIGGER product_request_events_immutable BEFORE DELETE OR UPDATE ON product_request_events FOR EACH ROW EXECUTE FUNCTION product_events_immutable();

