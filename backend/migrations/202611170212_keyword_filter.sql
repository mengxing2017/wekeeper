-- 敏感词规则版本与全局开关。
-- 空库初始化基线；后续结构变更使用新增递增迁移。

-- function
CREATE FUNCTION keyword_scope_init(scope text, scope_kind text, object_key text, scope_channel text, scope_label text) RETURNS void
    LANGUAGE plpgsql
    AS $$
DECLARE version uuid; inserted integer;
BEGIN
 version=gen_random_uuid();
 INSERT INTO keyword_scope_heads(id,kind,object_id,channel,label,version_id) VALUES(scope,scope_kind,object_key,scope_channel,scope_label,version) ON CONFLICT DO NOTHING;
 GET DIAGNOSTICS inserted=ROW_COUNT;
 IF inserted=1 THEN INSERT INTO keyword_rule_versions(id,scope_id,revision,entries,reason) VALUES(version,scope,0,'[]','initial empty rule set'); END IF;
END $$;

-- table
CREATE TABLE keyword_rule_versions (
    id uuid NOT NULL,
    scope_id text NOT NULL,
    revision bigint NOT NULL,
    entries jsonb NOT NULL,
    actor_id uuid,
    reason text NOT NULL,
    created_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    legacy_revision bigint,
    filter_enabled boolean DEFAULT false NOT NULL
);

CREATE TABLE keyword_scope_heads (
    id text NOT NULL,
    kind text NOT NULL,
    object_id text NOT NULL,
    channel text,
    label text NOT NULL,
    revision bigint DEFAULT 0 NOT NULL,
    version_id uuid NOT NULL,
    CONSTRAINT keyword_scope_heads_channel_check CHECK (((channel IS NULL) OR (channel = ANY (ARRAY['mobile'::text, 'wecom'::text])))),
    CONSTRAINT keyword_scope_heads_check CHECK (((kind = 'workspace'::text) = (channel IS NOT NULL))),
    CONSTRAINT keyword_scope_heads_kind_check CHECK ((kind = ANY (ARRAY['global'::text, 'source'::text, 'unit'::text, 'route'::text, 'workspace'::text])))
);

-- constraint
ALTER TABLE ONLY keyword_rule_versions
    ADD CONSTRAINT keyword_rule_versions_pkey PRIMARY KEY (id);

ALTER TABLE ONLY keyword_rule_versions
    ADD CONSTRAINT keyword_rule_versions_scope_id_revision_key UNIQUE (scope_id, revision);

ALTER TABLE ONLY keyword_scope_heads
    ADD CONSTRAINT keyword_scope_heads_pkey PRIMARY KEY (id);

-- fk constraint
ALTER TABLE ONLY keyword_scope_heads
    ADD CONSTRAINT keyword_head_version FOREIGN KEY (version_id) REFERENCES keyword_rule_versions(id) DEFERRABLE INITIALLY DEFERRED;

ALTER TABLE ONLY keyword_rule_versions
    ADD CONSTRAINT keyword_rule_versions_actor_id_fkey FOREIGN KEY (actor_id) REFERENCES accounts(id);

ALTER TABLE ONLY keyword_rule_versions
    ADD CONSTRAINT keyword_rule_versions_scope_id_fkey FOREIGN KEY (scope_id) REFERENCES keyword_scope_heads(id);

-- trigger
CREATE TRIGGER keyword_versions_immutable BEFORE DELETE OR UPDATE ON keyword_rule_versions FOR EACH ROW EXECUTE FUNCTION source_routing_immutable();

