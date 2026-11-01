-- New v7 records start empty. Never convert school grants/consents/assignments.
CREATE TABLE mobile_device_capabilities (
 id uuid PRIMARY KEY,
 device_id uuid NOT NULL REFERENCES execution_devices(id),
 app_key text NOT NULL CHECK(app_key='wecom'),
 capability text NOT NULL CHECK(capability='group_text'),
 name text NOT NULL CHECK(length(name) BETWEEN 1 AND 128),
 revision bigint NOT NULL CHECK(revision>0),
 configuration_digest text NOT NULL CHECK(configuration_digest ~ '^[0-9a-f]{64}$'),
 enabled boolean NOT NULL,
 created_by uuid NOT NULL REFERENCES accounts(id), updated_by uuid NOT NULL REFERENCES accounts(id),
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(), updated_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 UNIQUE(device_id,app_key,capability), UNIQUE(device_id,id)
);
CREATE TABLE mobile_device_capability_versions (
 capability_id uuid NOT NULL REFERENCES mobile_device_capabilities(id),
 revision bigint NOT NULL CHECK(revision>0),
 name text NOT NULL, enabled boolean NOT NULL, flow jsonb NOT NULL,
 configuration_digest text NOT NULL CHECK(configuration_digest ~ '^[0-9a-f]{64}$'),
 created_by uuid NOT NULL REFERENCES accounts(id), created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 PRIMARY KEY(capability_id,revision),
 CHECK(flow->>'protocol_version'='7' AND flow->>'schema_version'='2' AND flow->>'app_key'='wecom' AND flow->>'capability'='group_text' AND flow->>'selection_policy'='first_group_result')
);
ALTER TABLE mobile_device_capabilities ADD FOREIGN KEY(id,revision) REFERENCES mobile_device_capability_versions(capability_id,revision) DEFERRABLE INITIALLY DEFERRED;
CREATE TABLE mobile_device_capability_consents (
 id uuid PRIMARY KEY,
 device_id uuid NOT NULL, capability_id uuid NOT NULL UNIQUE,
 configuration_revision bigint NOT NULL,
 credential_generation bigint NOT NULL CHECK(credential_generation>0),
 scope_digest text NOT NULL CHECK(scope_digest ~ '^[0-9a-f]{64}$'), scope jsonb NOT NULL,
 revision bigint NOT NULL CHECK(revision>0), accepted boolean NOT NULL,
 updated_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 FOREIGN KEY(device_id,capability_id) REFERENCES mobile_device_capabilities(device_id,id),
 FOREIGN KEY(capability_id,configuration_revision) REFERENCES mobile_device_capability_versions(capability_id,revision)
);
CREATE TABLE mobile_device_capability_operations (
 device_id uuid NOT NULL REFERENCES execution_devices(id), credential_generation bigint NOT NULL,
 idempotency_key text NOT NULL, payload_hash text NOT NULL, result jsonb NOT NULL,
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 PRIMARY KEY(device_id,credential_generation,idempotency_key)
);
CREATE TABLE mobile_device_capability_events (
 id uuid PRIMARY KEY, device_id uuid NOT NULL REFERENCES execution_devices(id),
 capability_id uuid NOT NULL REFERENCES mobile_device_capabilities(id),
 credential_generation bigint NOT NULL, detail jsonb NOT NULL,
 created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE FUNCTION mobile_capability_evidence_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN RAISE EXCEPTION 'mobile capability evidence is immutable'; END $$;
CREATE TRIGGER mobile_capability_version_immutable BEFORE UPDATE OR DELETE ON mobile_device_capability_versions FOR EACH ROW EXECUTE FUNCTION mobile_capability_evidence_immutable();
CREATE TRIGGER mobile_capability_event_immutable BEFORE UPDATE OR DELETE ON mobile_device_capability_events FOR EACH ROW EXECUTE FUNCTION mobile_capability_evidence_immutable();
