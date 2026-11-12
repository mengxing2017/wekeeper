-- Business units have identities independent of source route identifiers.
-- source_callers is the existing stable identity of a downstream platform.
CREATE TABLE business_units (
    id uuid PRIMARY KEY CHECK (id <> '00000000-0000-0000-0000-000000000000'::uuid),
    platform_id uuid NOT NULL REFERENCES source_callers(source_id),
    name text NOT NULL CHECK (length(trim(name)) BETWEEN 1 AND 120),
    description text NOT NULL DEFAULT '' CHECK (length(description) <= 2000),
    active boolean NOT NULL DEFAULT true,
    revision bigint NOT NULL DEFAULT 1 CHECK (revision > 0),
    created_by uuid NOT NULL REFERENCES accounts(id),
    updated_by uuid NOT NULL REFERENCES accounts(id),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (platform_id, id)
);
CREATE INDEX business_units_platform ON business_units(platform_id, id);
