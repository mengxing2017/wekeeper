-- Independent advertising materials and source scopes. They do not reference
-- source routes, execution workspaces, groups, or device state.
CREATE TABLE ad_materials (
    id uuid PRIMARY KEY,
    current_version_id uuid,
    archived boolean NOT NULL DEFAULT false,
    revision bigint NOT NULL DEFAULT 1 CHECK (revision > 0),
    created_by uuid NOT NULL REFERENCES accounts(id),
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE ad_material_versions (
    id uuid PRIMARY KEY,
    material_id uuid NOT NULL REFERENCES ad_materials(id),
    version bigint NOT NULL CHECK (version > 0),
    name text NOT NULL CHECK (length(trim(name)) BETWEEN 1 AND 120),
    body text NOT NULL CHECK (length(trim(body)) > 0 AND octet_length(body) <= 8000),
    link text CHECK (link IS NULL OR length(link) <= 2048),
    created_by uuid NOT NULL REFERENCES accounts(id),
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE(material_id, version),
    UNIQUE(material_id, id)
);
ALTER TABLE ad_materials ADD CONSTRAINT ad_material_current_version_fk
    FOREIGN KEY (id, current_version_id) REFERENCES ad_material_versions(material_id, id);
CREATE FUNCTION ad_material_version_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN RAISE EXCEPTION 'advertising material version is immutable'; END $$;
CREATE TRIGGER ad_material_version_immutable BEFORE UPDATE OR DELETE ON ad_material_versions
    FOR EACH ROW EXECUTE FUNCTION ad_material_version_immutable();

CREATE TABLE ad_placement_rules (
    id uuid PRIMARY KEY,
    material_id uuid NOT NULL REFERENCES ad_materials(id),
    scope_kind text NOT NULL CHECK (scope_kind IN ('global','platform','unit')),
    platform_id uuid REFERENCES source_callers(source_id),
    unit_id uuid,
    enabled boolean NOT NULL DEFAULT false,
    valid_from timestamptz NOT NULL,
    valid_until timestamptz,
    revision bigint NOT NULL DEFAULT 1 CHECK (revision > 0),
    created_by uuid NOT NULL REFERENCES accounts(id),
    updated_by uuid NOT NULL REFERENCES accounts(id),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    FOREIGN KEY (platform_id,unit_id) REFERENCES business_units(platform_id,id),
    CHECK (valid_until IS NULL OR valid_until > valid_from),
    CHECK ((scope_kind='global' AND platform_id IS NULL AND unit_id IS NULL)
        OR (scope_kind='platform' AND platform_id IS NOT NULL AND unit_id IS NULL)
        OR (scope_kind='unit' AND platform_id IS NOT NULL AND unit_id IS NOT NULL))
);
CREATE INDEX ad_placement_rules_scope ON ad_placement_rules(scope_kind,platform_id,unit_id,id) WHERE enabled;
