-- 手机使用授权、范围与额度。
-- 空库初始化基线；后续结构变更使用新增递增迁移。

-- table
CREATE TABLE mobile_usage_confirmations (
    preview_id uuid NOT NULL,
    usage_id uuid NOT NULL
);

CREATE TABLE mobile_usage_events (
    id uuid NOT NULL,
    usage_id uuid NOT NULL,
    actor_id uuid NOT NULL,
    action text NOT NULL,
    body jsonb NOT NULL,
    created_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL
);

CREATE TABLE mobile_usage_invocation_budgets (
    invocation_id uuid NOT NULL,
    usage_id uuid NOT NULL,
    budget_root uuid NOT NULL
);

CREATE TABLE mobile_usage_issues (
    id uuid NOT NULL,
    usage_id uuid NOT NULL,
    actor_id uuid NOT NULL,
    description text NOT NULL,
    context jsonb NOT NULL,
    revision bigint DEFAULT 1 NOT NULL,
    response text DEFAULT ''::text NOT NULL,
    state text DEFAULT 'submitted'::text NOT NULL,
    created_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    CONSTRAINT mobile_usage_issues_state_check CHECK ((state = ANY (ARRAY['submitted'::text, 'investigating'::text, 'resolved'::text, 'declined'::text])))
);

CREATE TABLE mobile_usage_jobs (
    usage_id uuid NOT NULL,
    state text DEFAULT 'pending'::text NOT NULL,
    reason text,
    updated_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL
);

CREATE TABLE mobile_usage_previews (
    id uuid NOT NULL,
    actor_id uuid NOT NULL,
    body jsonb NOT NULL,
    digest text NOT NULL,
    expires_at timestamp with time zone DEFAULT (clock_timestamp() + '00:10:00'::interval) NOT NULL
);

CREATE TABLE mobile_usage_scopes (
    scope_id uuid NOT NULL,
    resource_snapshot jsonb NOT NULL,
    usage_id uuid NOT NULL,
    release_id uuid,
    target_id uuid,
    device_id uuid
);

CREATE TABLE mobile_usages (
    id uuid NOT NULL,
    actor_id uuid NOT NULL,
    approval text NOT NULL,
    state text DEFAULT 'active'::text NOT NULL,
    revision bigint DEFAULT 1 NOT NULL,
    body jsonb NOT NULL,
    confirmed_until timestamp with time zone NOT NULL,
    expires_at timestamp with time zone NOT NULL,
    created_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    budget_root uuid DEFAULT gen_random_uuid() NOT NULL,
    approved_by uuid,
    supersedes uuid,
    CONSTRAINT mobile_usages_approval_check CHECK ((approval = ANY (ARRAY['pending'::text, 'approved'::text, 'declined'::text]))),
    CONSTRAINT mobile_usages_state_check CHECK ((state = ANY (ARRAY['active'::text, 'paused'::text, 'revoked'::text])))
);

-- constraint
ALTER TABLE ONLY mobile_usage_confirmations
    ADD CONSTRAINT mobile_usage_confirmations_pkey PRIMARY KEY (preview_id);

ALTER TABLE ONLY mobile_usage_events
    ADD CONSTRAINT mobile_usage_events_pkey PRIMARY KEY (id);

ALTER TABLE ONLY mobile_usage_invocation_budgets
    ADD CONSTRAINT mobile_usage_invocation_budgets_pkey PRIMARY KEY (invocation_id, budget_root);

ALTER TABLE ONLY mobile_usage_issues
    ADD CONSTRAINT mobile_usage_issues_pkey PRIMARY KEY (id);

ALTER TABLE ONLY mobile_usage_jobs
    ADD CONSTRAINT mobile_usage_jobs_pkey PRIMARY KEY (usage_id);

ALTER TABLE ONLY mobile_usage_previews
    ADD CONSTRAINT mobile_usage_previews_pkey PRIMARY KEY (id);

ALTER TABLE ONLY mobile_usage_scopes
    ADD CONSTRAINT mobile_usage_scopes_pkey PRIMARY KEY (scope_id);

ALTER TABLE ONLY mobile_usage_scopes
    ADD CONSTRAINT mobile_usage_scopes_usage_id_release_id_target_id_device_id_key UNIQUE (usage_id, release_id, target_id, device_id);

ALTER TABLE ONLY mobile_usages
    ADD CONSTRAINT mobile_usages_pkey PRIMARY KEY (id);

-- index
CREATE UNIQUE INDEX mobile_usage_business_scope ON mobile_usage_scopes USING btree (usage_id, target_id, device_id) WHERE (release_id IS NULL);

CREATE INDEX mobile_usage_invocation_budgets_root ON mobile_usage_invocation_budgets USING btree (budget_root);

CREATE INDEX mobile_usages_budget ON mobile_usages USING btree (budget_root);

CREATE INDEX mobile_usages_created_page ON mobile_usages USING btree (created_at DESC);

CREATE INDEX mobile_usages_predecessor ON mobile_usages USING btree (supersedes) WHERE (supersedes IS NOT NULL);

-- fk constraint
ALTER TABLE ONLY mobile_usage_confirmations
    ADD CONSTRAINT mobile_usage_confirmations_preview_id_fkey FOREIGN KEY (preview_id) REFERENCES mobile_usage_previews(id);

ALTER TABLE ONLY mobile_usage_confirmations
    ADD CONSTRAINT mobile_usage_confirmations_usage_id_fkey FOREIGN KEY (usage_id) REFERENCES mobile_usages(id);

ALTER TABLE ONLY mobile_usage_events
    ADD CONSTRAINT mobile_usage_events_actor_id_fkey FOREIGN KEY (actor_id) REFERENCES accounts(id);

ALTER TABLE ONLY mobile_usage_events
    ADD CONSTRAINT mobile_usage_events_usage_id_fkey FOREIGN KEY (usage_id) REFERENCES mobile_usages(id);

ALTER TABLE ONLY mobile_usage_invocation_budgets
    ADD CONSTRAINT mobile_usage_invocation_budgets_usage_id_fkey FOREIGN KEY (usage_id) REFERENCES mobile_usages(id);

ALTER TABLE ONLY mobile_usage_issues
    ADD CONSTRAINT mobile_usage_issues_actor_id_fkey FOREIGN KEY (actor_id) REFERENCES accounts(id);

ALTER TABLE ONLY mobile_usage_issues
    ADD CONSTRAINT mobile_usage_issues_usage_id_fkey FOREIGN KEY (usage_id) REFERENCES mobile_usages(id);

ALTER TABLE ONLY mobile_usage_jobs
    ADD CONSTRAINT mobile_usage_jobs_usage_id_fkey FOREIGN KEY (usage_id) REFERENCES mobile_usages(id);

ALTER TABLE ONLY mobile_usage_previews
    ADD CONSTRAINT mobile_usage_previews_actor_id_fkey FOREIGN KEY (actor_id) REFERENCES accounts(id);

ALTER TABLE ONLY mobile_usage_scopes
    ADD CONSTRAINT mobile_usage_scopes_device_id_fkey FOREIGN KEY (device_id) REFERENCES execution_devices(id);

ALTER TABLE ONLY mobile_usage_scopes
    ADD CONSTRAINT mobile_usage_scopes_release_id_fkey FOREIGN KEY (release_id) REFERENCES mobile_v9_adapter_releases(id);

ALTER TABLE ONLY mobile_usage_scopes
    ADD CONSTRAINT mobile_usage_scopes_scope_id_fkey FOREIGN KEY (scope_id) REFERENCES mobile_v9_scope_grants(id);

ALTER TABLE ONLY mobile_usage_scopes
    ADD CONSTRAINT mobile_usage_scopes_target_id_fkey FOREIGN KEY (target_id) REFERENCES mobile_targets(id);

ALTER TABLE ONLY mobile_usage_scopes
    ADD CONSTRAINT mobile_usage_scopes_usage_id_fkey FOREIGN KEY (usage_id) REFERENCES mobile_usages(id);

ALTER TABLE ONLY mobile_usages
    ADD CONSTRAINT mobile_usages_actor_id_fkey FOREIGN KEY (actor_id) REFERENCES accounts(id);

ALTER TABLE ONLY mobile_usages
    ADD CONSTRAINT mobile_usages_approved_by_fkey FOREIGN KEY (approved_by) REFERENCES accounts(id);

ALTER TABLE ONLY mobile_usages
    ADD CONSTRAINT mobile_usages_supersedes_fkey FOREIGN KEY (supersedes) REFERENCES mobile_usages(id);

-- trigger
CREATE TRIGGER mobile_usage_confirmations_immutable BEFORE DELETE OR UPDATE ON mobile_usage_confirmations FOR EACH ROW EXECUTE FUNCTION mobile_capability_evidence_immutable();

CREATE TRIGGER mobile_usage_events_immutable BEFORE DELETE OR UPDATE ON mobile_usage_events FOR EACH ROW EXECUTE FUNCTION mobile_capability_evidence_immutable();

CREATE TRIGGER mobile_usage_invocation_budgets_immutable BEFORE DELETE OR UPDATE ON mobile_usage_invocation_budgets FOR EACH ROW EXECUTE FUNCTION mobile_capability_evidence_immutable();

