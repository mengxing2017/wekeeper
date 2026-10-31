-- Device-authenticated school consent, independent of sender and official business.
CREATE TABLE mobile_device_consents (
 id uuid PRIMARY KEY,
 school_id uuid NOT NULL,
 device_id uuid NOT NULL,
 grant_id uuid NOT NULL,
 grant_revision bigint NOT NULL CHECK(grant_revision > 0),
 scope_digest text NOT NULL CHECK(length(scope_digest)=64),
 scope jsonb NOT NULL,
 revision bigint NOT NULL CHECK(revision > 0),
 accepted boolean NOT NULL,
 credential_generation bigint NOT NULL,
 updated_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 UNIQUE(grant_id),
 FOREIGN KEY(school_id,device_id,grant_id) REFERENCES mobile_device_school_grants(school_id,device_id,id)
);
CREATE TABLE mobile_device_consent_operations (
 device_id uuid NOT NULL REFERENCES execution_devices(id),
 idempotency_key text NOT NULL,
 payload_hash text NOT NULL,
 result jsonb NOT NULL,
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 PRIMARY KEY(device_id,idempotency_key)
);
CREATE TABLE mobile_device_consent_events (
 id uuid PRIMARY KEY,
 device_id uuid NOT NULL REFERENCES execution_devices(id),
 grant_id uuid NOT NULL REFERENCES mobile_device_school_grants(id),
 credential_generation bigint NOT NULL,
 detail jsonb NOT NULL,
 created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
