-- 广告素材、投放及逐消息决策。
-- 空库初始化基线；后续结构变更使用新增递增迁移。

-- function
CREATE FUNCTION ad_material_version_immutable() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN RAISE EXCEPTION 'advertising material version is immutable'; END $$;

-- table
CREATE TABLE ad_material_versions (
    id uuid NOT NULL,
    material_id uuid NOT NULL,
    version bigint NOT NULL,
    name text NOT NULL,
    body text NOT NULL,
    link text,
    created_by uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT ad_material_versions_body_check CHECK (((length(TRIM(BOTH FROM body)) > 0) AND (octet_length(body) <= 8000))),
    CONSTRAINT ad_material_versions_link_check CHECK (((link IS NULL) OR (length(link) <= 2048))),
    CONSTRAINT ad_material_versions_name_check CHECK (((length(TRIM(BOTH FROM name)) >= 1) AND (length(TRIM(BOTH FROM name)) <= 120))),
    CONSTRAINT ad_material_versions_version_check CHECK ((version > 0))
);

CREATE TABLE ad_materials (
    id uuid NOT NULL,
    current_version_id uuid,
    archived boolean DEFAULT false NOT NULL,
    revision bigint DEFAULT 1 NOT NULL,
    created_by uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT ad_materials_revision_check CHECK ((revision > 0))
);

CREATE TABLE ad_message_decisions (
    platform_id uuid NOT NULL,
    unit_id uuid NOT NULL,
    message_id uuid NOT NULL,
    original text NOT NULL,
    channel text NOT NULL,
    decision jsonb NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT ad_message_decisions_channel_check CHECK ((channel = ANY (ARRAY['mobile'::text, 'wecom'::text])))
);

CREATE TABLE ad_placements (
    id uuid NOT NULL,
    material_id uuid NOT NULL,
    scope_kind text NOT NULL,
    platform_id uuid,
    unit_id uuid,
    enabled boolean DEFAULT false NOT NULL,
    valid_from timestamp with time zone NOT NULL,
    valid_until timestamp with time zone,
    revision bigint DEFAULT 1 NOT NULL,
    created_by uuid NOT NULL,
    updated_by uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT ad_placement_rules_check CHECK (((valid_until IS NULL) OR (valid_until > valid_from))),
    CONSTRAINT ad_placement_rules_check1 CHECK ((((scope_kind = 'global'::text) AND (platform_id IS NULL) AND (unit_id IS NULL)) OR ((scope_kind = 'platform'::text) AND (platform_id IS NOT NULL) AND (unit_id IS NULL)) OR ((scope_kind = 'unit'::text) AND (platform_id IS NOT NULL) AND (unit_id IS NOT NULL)))),
    CONSTRAINT ad_placement_rules_revision_check CHECK ((revision > 0)),
    CONSTRAINT ad_placement_rules_scope_kind_check CHECK ((scope_kind = ANY (ARRAY['global'::text, 'platform'::text, 'unit'::text])))
);

CREATE TABLE ad_unit_counters (
    platform_id uuid NOT NULL,
    unit_id uuid NOT NULL,
    selected_count bigint DEFAULT 0 NOT NULL,
    CONSTRAINT ad_unit_counters_selected_count_check CHECK ((selected_count >= 0))
);

-- constraint
ALTER TABLE ONLY ad_material_versions
    ADD CONSTRAINT ad_material_versions_material_id_id_key UNIQUE (material_id, id);

ALTER TABLE ONLY ad_material_versions
    ADD CONSTRAINT ad_material_versions_material_id_version_key UNIQUE (material_id, version);

ALTER TABLE ONLY ad_material_versions
    ADD CONSTRAINT ad_material_versions_pkey PRIMARY KEY (id);

ALTER TABLE ONLY ad_materials
    ADD CONSTRAINT ad_materials_pkey PRIMARY KEY (id);

ALTER TABLE ONLY ad_message_decisions
    ADD CONSTRAINT ad_message_decisions_pkey PRIMARY KEY (platform_id, unit_id, message_id);

ALTER TABLE ONLY ad_placements
    ADD CONSTRAINT ad_placement_rules_pkey PRIMARY KEY (id);

ALTER TABLE ONLY ad_placements
    ADD CONSTRAINT ad_placements_material_scope_unique UNIQUE NULLS NOT DISTINCT (material_id, scope_kind, platform_id, unit_id);

ALTER TABLE ONLY ad_unit_counters
    ADD CONSTRAINT ad_unit_counters_pkey PRIMARY KEY (platform_id, unit_id);

-- check constraint
ALTER TABLE ad_material_versions
    ADD CONSTRAINT ad_material_body_140_characters CHECK ((char_length(body) <= 140)) NOT VALID;

-- index
CREATE INDEX ad_placements_scope ON ad_placements USING btree (scope_kind, platform_id, unit_id, id) WHERE enabled;

-- comment
COMMENT ON TABLE ad_placements IS 'Direct advertisement material to business scope bindings; not selection rules';

-- fk constraint
ALTER TABLE ONLY ad_materials
    ADD CONSTRAINT ad_material_current_version_fk FOREIGN KEY (id, current_version_id) REFERENCES ad_material_versions(material_id, id);

ALTER TABLE ONLY ad_material_versions
    ADD CONSTRAINT ad_material_versions_created_by_fkey FOREIGN KEY (created_by) REFERENCES accounts(id);

ALTER TABLE ONLY ad_material_versions
    ADD CONSTRAINT ad_material_versions_material_id_fkey FOREIGN KEY (material_id) REFERENCES ad_materials(id);

ALTER TABLE ONLY ad_materials
    ADD CONSTRAINT ad_materials_created_by_fkey FOREIGN KEY (created_by) REFERENCES accounts(id);

ALTER TABLE ONLY ad_message_decisions
    ADD CONSTRAINT ad_message_decisions_platform_id_unit_id_fkey FOREIGN KEY (platform_id, unit_id) REFERENCES business_units(platform_id, id);

ALTER TABLE ONLY ad_placements
    ADD CONSTRAINT ad_placement_rules_created_by_fkey FOREIGN KEY (created_by) REFERENCES accounts(id);

ALTER TABLE ONLY ad_placements
    ADD CONSTRAINT ad_placement_rules_material_id_fkey FOREIGN KEY (material_id) REFERENCES ad_materials(id);

ALTER TABLE ONLY ad_placements
    ADD CONSTRAINT ad_placement_rules_platform_id_fkey FOREIGN KEY (platform_id) REFERENCES source_callers(source_id);

ALTER TABLE ONLY ad_placements
    ADD CONSTRAINT ad_placement_rules_platform_id_unit_id_fkey FOREIGN KEY (platform_id, unit_id) REFERENCES business_units(platform_id, id);

ALTER TABLE ONLY ad_placements
    ADD CONSTRAINT ad_placement_rules_updated_by_fkey FOREIGN KEY (updated_by) REFERENCES accounts(id);

ALTER TABLE ONLY ad_unit_counters
    ADD CONSTRAINT ad_unit_counters_platform_id_unit_id_fkey FOREIGN KEY (platform_id, unit_id) REFERENCES business_units(platform_id, id);

-- trigger
CREATE TRIGGER ad_material_version_immutable BEFORE DELETE OR UPDATE ON ad_material_versions FOR EACH ROW EXECUTE FUNCTION ad_material_version_immutable();

