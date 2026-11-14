-- Channel resources have no intake route, business-unit or advertising keys.
CREATE TABLE mobile_receiving_configurations (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL REFERENCES mobile_workspaces(id),
    revision bigint NOT NULL CHECK (revision BETWEEN 1 AND 9007199254740991),
    configuration jsonb NOT NULL CHECK (jsonb_typeof(configuration) = 'object'),
    updated_by uuid NOT NULL REFERENCES accounts(id),
    updated_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    UNIQUE (workspace_id, id),
    CHECK (id <> '00000000-0000-0000-0000-000000000000'::uuid)
);
CREATE TABLE wecom_receiving_configurations (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL REFERENCES wecom_workspaces(id),
    revision bigint NOT NULL CHECK (revision BETWEEN 1 AND 9007199254740991),
    configuration jsonb NOT NULL CHECK (jsonb_typeof(configuration) = 'object'),
    updated_by uuid NOT NULL REFERENCES accounts(id),
    updated_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    UNIQUE (workspace_id, id),
    CHECK (id <> '00000000-0000-0000-0000-000000000000'::uuid)
);
CREATE TABLE mobile_receiving_configuration_versions (
    configuration_id uuid NOT NULL REFERENCES mobile_receiving_configurations(id),
    revision bigint NOT NULL,
    configuration jsonb NOT NULL,
    created_by uuid NOT NULL REFERENCES accounts(id),
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY (configuration_id, revision)
);
CREATE TABLE wecom_receiving_configuration_versions (
    configuration_id uuid NOT NULL REFERENCES wecom_receiving_configurations(id),
    revision bigint NOT NULL,
    configuration jsonb NOT NULL,
    created_by uuid NOT NULL REFERENCES accounts(id),
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY (configuration_id, revision)
);
-- Ownership persists when a target is removed or a configuration is disabled.
-- Neither operation can silently transfer a group to another configuration.
CREATE TABLE mobile_receiving_configuration_targets (
    target_id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL,
    configuration_id uuid NOT NULL,
    enabled boolean NOT NULL,
    FOREIGN KEY (workspace_id, target_id) REFERENCES mobile_targets(school_id, id),
    FOREIGN KEY (workspace_id, configuration_id) REFERENCES mobile_receiving_configurations(workspace_id, id)
);
CREATE TABLE wecom_receiving_configuration_targets (
    target_id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    configuration_id uuid NOT NULL,
    enabled boolean NOT NULL,
    PRIMARY KEY (configuration_id, target_id),
    FOREIGN KEY (workspace_id, target_id) REFERENCES wecom_groups(school_id, id),
    FOREIGN KEY (workspace_id, configuration_id) REFERENCES wecom_receiving_configurations(workspace_id, id)
);
-- A phone may serve several configurations. A pair has only one binding, and
-- preference is local to that configuration, never a global device order.
CREATE TABLE mobile_receiving_configuration_devices (
    configuration_id uuid NOT NULL REFERENCES mobile_receiving_configurations(id),
    device_id uuid NOT NULL REFERENCES execution_devices(id),
    ordinal integer NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (configuration_id, device_id),
    UNIQUE (configuration_id, ordinal)
);
CREATE TABLE wecom_receiving_configuration_senders (
    configuration_id uuid NOT NULL REFERENCES wecom_receiving_configurations(id),
    account_id uuid NOT NULL REFERENCES accounts(id),
    ordinal integer NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (configuration_id, account_id),
    UNIQUE (configuration_id, ordinal)
);
CREATE FUNCTION receiving_configuration_version_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN RAISE EXCEPTION 'receiving configuration version is immutable'; END $$;
CREATE TRIGGER mobile_receiving_version_immutable
    BEFORE UPDATE OR DELETE ON mobile_receiving_configuration_versions
    FOR EACH ROW EXECUTE FUNCTION receiving_configuration_version_immutable();
CREATE TRIGGER wecom_receiving_version_immutable
    BEFORE UPDATE OR DELETE ON wecom_receiving_configuration_versions
    FOR EACH ROW EXECUTE FUNCTION receiving_configuration_version_immutable();
