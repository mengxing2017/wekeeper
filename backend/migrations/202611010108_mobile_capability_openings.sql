-- Product-owned releases have no administrator write API. No fixture is a production release.
CREATE TABLE mobile_adapter_releases (
 id uuid PRIMARY KEY, name text NOT NULL CHECK(length(name) BETWEEN 1 AND 128),
 version text NOT NULL CHECK(length(version) BETWEEN 1 AND 128), owner text NOT NULL CHECK(length(owner) BETWEEN 1 AND 128), source_revision text NOT NULL CHECK(length(source_revision) BETWEEN 1 AND 256),
 evidence jsonb NOT NULL CHECK(jsonb_typeof(evidence)='array' AND jsonb_array_length(evidence)>0),
 environment jsonb NOT NULL, flow jsonb NOT NULL,
 content_digest text NOT NULL CHECK(content_digest ~ '^[0-9a-f]{64}$'),
 published_at timestamptz NOT NULL DEFAULT clock_timestamp(), UNIQUE(name,version)
);
CREATE TRIGGER mobile_adapter_release_immutable BEFORE UPDATE OR DELETE ON mobile_adapter_releases FOR EACH ROW EXECUTE FUNCTION mobile_capability_evidence_immutable();
CREATE TABLE mobile_adapter_withdrawals (
 release_id uuid PRIMARY KEY REFERENCES mobile_adapter_releases(id), reason text NOT NULL,
 withdrawn_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TRIGGER mobile_adapter_withdrawal_immutable BEFORE UPDATE OR DELETE ON mobile_adapter_withdrawals FOR EACH ROW EXECUTE FUNCTION mobile_capability_evidence_immutable();
CREATE TABLE mobile_device_environments (
 id uuid PRIMARY KEY, device_id uuid NOT NULL REFERENCES execution_devices(id),
 credential_generation bigint NOT NULL CHECK(credential_generation>0),
 snapshot jsonb NOT NULL, digest text NOT NULL CHECK(digest ~ '^[0-9a-f]{64}$'),
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(), UNIQUE(device_id,id)
);
CREATE TRIGGER mobile_environment_immutable BEFORE UPDATE OR DELETE ON mobile_device_environments FOR EACH ROW EXECUTE FUNCTION mobile_capability_evidence_immutable();
CREATE TABLE mobile_device_environment_observations (
 device_id uuid PRIMARY KEY REFERENCES execution_devices(id), snapshot_id uuid NOT NULL,
 observed_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 FOREIGN KEY(device_id,snapshot_id) REFERENCES mobile_device_environments(device_id,id)
);
CREATE TABLE mobile_capability_openings (
 id uuid PRIMARY KEY, device_id uuid NOT NULL REFERENCES execution_devices(id),
 capability_id uuid NOT NULL REFERENCES mobile_device_capabilities(id),
 configuration_revision bigint NOT NULL, release_id uuid NOT NULL REFERENCES mobile_adapter_releases(id),
 environment_snapshot_id uuid NOT NULL, state text NOT NULL CHECK(state IN ('awaiting_consent','cancelled')),
 revision bigint NOT NULL DEFAULT 1 CHECK(revision>0), created_by uuid NOT NULL REFERENCES accounts(id),
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(), updated_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 FOREIGN KEY(device_id,environment_snapshot_id) REFERENCES mobile_device_environments(device_id,id),
 FOREIGN KEY(device_id,capability_id) REFERENCES mobile_device_capabilities(device_id,id),
 UNIQUE(capability_id,configuration_revision),
 FOREIGN KEY(capability_id,configuration_revision) REFERENCES mobile_device_capability_versions(capability_id,revision)
);
CREATE UNIQUE INDEX mobile_capability_one_pending ON mobile_capability_openings(device_id) WHERE state='awaiting_consent';
CREATE TABLE mobile_adapter_support_requests (
 id uuid PRIMARY KEY, device_id uuid NOT NULL REFERENCES execution_devices(id),
 environment_snapshot_id uuid, note text NOT NULL CHECK(length(note) BETWEEN 1 AND 2000),
 owner text NOT NULL DEFAULT '产品研发', state text NOT NULL DEFAULT 'submitted' CHECK(state IN ('submitted','investigating','resolved','declined')),
 response text, revision bigint NOT NULL DEFAULT 1 CHECK(revision>0), created_by uuid NOT NULL REFERENCES accounts(id), updated_by uuid NOT NULL REFERENCES accounts(id),
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(), updated_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 FOREIGN KEY(device_id,environment_snapshot_id) REFERENCES mobile_device_environments(device_id,id)
);
CREATE UNIQUE INDEX mobile_adapter_one_support ON mobile_adapter_support_requests(device_id) WHERE state IN ('submitted','investigating');
-- Untethered manual configurations are historical evidence only. Do not alter immutable versions or consents.
UPDATE mobile_device_capabilities SET enabled=false,updated_at=clock_timestamp();

-- Release withdrawal is serialized with candidate/consent changes, including external release migrations.
CREATE FUNCTION mobile_adapter_serialize_withdrawal() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN PERFORM pg_advisory_xact_lock(78234610243); RETURN NEW; END $$;
CREATE TRIGGER mobile_adapter_withdrawal_lock BEFORE INSERT ON mobile_adapter_withdrawals FOR EACH ROW EXECUTE FUNCTION mobile_adapter_serialize_withdrawal();
