-- New phone-only flows. No reuse or conversion of old automation releases.
CREATE TABLE mobile_execution_flow_versions (
 id uuid PRIMARY KEY, school_id uuid NOT NULL REFERENCES schools(id),
 name text NOT NULL CHECK(length(name) BETWEEN 1 AND 128), flow jsonb NOT NULL,
 content_digest text NOT NULL CHECK(length(content_digest)=64),
 created_by uuid NOT NULL REFERENCES accounts(id),created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 UNIQUE(school_id,id),CHECK(flow->>'schema_version'='2' AND flow->>'protocol_version'='6' AND flow->>'selection_policy'='first_group_result')
);
CREATE TABLE mobile_execution_flow_validations (
 id uuid PRIMARY KEY,school_id uuid NOT NULL,flow_version_id uuid NOT NULL,
 device_id uuid NOT NULL,grant_id uuid NOT NULL,grant_revision bigint NOT NULL,
 consent_id uuid NOT NULL REFERENCES mobile_device_consents(id),consent_revision bigint NOT NULL,
 search_text text NOT NULL CHECK(length(search_text) BETWEEN 1 AND 256 AND octet_length(search_text)<=1024),
 challenge text NOT NULL, state text NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','passed','failed')),
 result jsonb,credential_generation bigint,created_by uuid NOT NULL REFERENCES accounts(id),
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),expires_at timestamptz NOT NULL DEFAULT clock_timestamp()+interval '30 minutes',
 completed_at timestamptz, UNIQUE(school_id,flow_version_id,id),
 FOREIGN KEY(school_id,flow_version_id) REFERENCES mobile_execution_flow_versions(school_id,id),
 FOREIGN KEY(school_id,device_id,grant_id) REFERENCES mobile_device_school_grants(school_id,device_id,id)
);
CREATE INDEX mobile_execution_pending_validations ON mobile_execution_flow_validations(device_id,created_at,id) WHERE state='pending';
CREATE TABLE mobile_execution_flow_releases (
 id uuid PRIMARY KEY,school_id uuid NOT NULL,flow_version_id uuid NOT NULL,validation_id uuid NOT NULL,
 name text NOT NULL CHECK(length(name) BETWEEN 1 AND 128),enabled boolean NOT NULL DEFAULT true,revision bigint NOT NULL DEFAULT 1 CHECK(revision>0),
 created_by uuid NOT NULL REFERENCES accounts(id),created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 revoked_by uuid REFERENCES accounts(id),revoked_at timestamptz,revocation_reason text,
 UNIQUE(school_id,id),UNIQUE(validation_id),
 FOREIGN KEY(school_id,flow_version_id) REFERENCES mobile_execution_flow_versions(school_id,id),
 FOREIGN KEY(school_id,flow_version_id,validation_id) REFERENCES mobile_execution_flow_validations(school_id,flow_version_id,id)
);
CREATE TABLE mobile_execution_assignments (
 id uuid PRIMARY KEY,school_id uuid NOT NULL,device_id uuid NOT NULL,grant_id uuid NOT NULL,release_id uuid NOT NULL,
 enabled boolean NOT NULL DEFAULT true,execution_revision bigint NOT NULL DEFAULT 1 CHECK(execution_revision>0),
 metadata_revision bigint NOT NULL DEFAULT 1 CHECK(metadata_revision>0),
 created_by uuid NOT NULL REFERENCES accounts(id),updated_by uuid NOT NULL REFERENCES accounts(id),
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),updated_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 UNIQUE(school_id,device_id),UNIQUE(school_id,id),
 FOREIGN KEY(school_id,device_id,grant_id) REFERENCES mobile_device_school_grants(school_id,device_id,id),
 FOREIGN KEY(school_id,release_id) REFERENCES mobile_execution_flow_releases(school_id,id)
);
CREATE TABLE mobile_execution_validation_operations (
 device_id uuid NOT NULL REFERENCES execution_devices(id),idempotency_key text NOT NULL,payload_hash text NOT NULL,result jsonb NOT NULL,
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),PRIMARY KEY(device_id,idempotency_key)
);
CREATE TABLE mobile_execution_validation_events (
 id uuid PRIMARY KEY,validation_id uuid NOT NULL REFERENCES mobile_execution_flow_validations(id),device_id uuid NOT NULL REFERENCES execution_devices(id),
 credential_generation bigint NOT NULL,detail jsonb NOT NULL,created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
