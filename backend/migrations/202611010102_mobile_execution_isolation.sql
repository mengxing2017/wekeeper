-- Independent phone resources. No official customer group or sending-account foreign keys.
CREATE TABLE mobile_device_school_grants (
 id uuid PRIMARY KEY, school_id uuid NOT NULL REFERENCES schools(id),
 device_id uuid NOT NULL REFERENCES execution_devices(id), purpose text NOT NULL CHECK(length(purpose) BETWEEN 1 AND 256),
 enabled boolean NOT NULL DEFAULT true, revision bigint NOT NULL DEFAULT 1 CHECK(revision>0),
 school_alias text NOT NULL DEFAULT '', school_remark text NOT NULL DEFAULT '',
 granted_by uuid NOT NULL REFERENCES accounts(id), revoked_by uuid REFERENCES accounts(id),
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(), revoked_at timestamptz,
 UNIQUE(school_id,device_id), UNIQUE(school_id,device_id,id)
);
ALTER TABLE execution_devices ADD COLUMN remark text NOT NULL DEFAULT '';
ALTER TABLE execution_devices ADD COLUMN custodian_actor_id uuid REFERENCES accounts(id);
ALTER TABLE execution_devices ADD COLUMN metadata_revision bigint NOT NULL DEFAULT 1;
CREATE TABLE mobile_targets (
 id uuid PRIMARY KEY, school_id uuid NOT NULL REFERENCES schools(id),
 search_text text NOT NULL CHECK(length(search_text) BETWEEN 1 AND 256 AND octet_length(search_text)<=1024),
 alias text NOT NULL DEFAULT '' CHECK(length(alias)<=128), remark text NOT NULL DEFAULT '' CHECK(length(remark)<=2000),
 enabled boolean NOT NULL DEFAULT true, archived_at timestamptz,
 metadata_revision bigint NOT NULL DEFAULT 1, locator_revision bigint NOT NULL DEFAULT 1,
 created_by uuid NOT NULL REFERENCES accounts(id), updated_by uuid NOT NULL REFERENCES accounts(id),
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(), updated_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 UNIQUE(school_id,id)
);
CREATE INDEX mobile_targets_school_page ON mobile_targets(school_id,created_at,id) WHERE archived_at IS NULL;
CREATE TABLE mobile_device_target_bindings (
 id uuid PRIMARY KEY, school_id uuid NOT NULL, device_id uuid NOT NULL, target_id uuid NOT NULL,
 grant_id uuid NOT NULL, enabled boolean NOT NULL DEFAULT true, revision bigint NOT NULL DEFAULT 1,
 created_by uuid NOT NULL REFERENCES accounts(id), updated_by uuid NOT NULL REFERENCES accounts(id),
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(), updated_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 UNIQUE(school_id,device_id,target_id), UNIQUE(school_id,id),
 FOREIGN KEY(school_id,target_id) REFERENCES mobile_targets(school_id,id),
 FOREIGN KEY(school_id,device_id,grant_id) REFERENCES mobile_device_school_grants(school_id,device_id,id)
);
CREATE TABLE mobile_execution_audit_events (
 id uuid PRIMARY KEY, school_id uuid REFERENCES schools(id), resource_id uuid NOT NULL,
 actor_id uuid NOT NULL REFERENCES accounts(id), action text NOT NULL, detail jsonb NOT NULL,
 created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE mobile_execution_operations (
 actor_id uuid NOT NULL REFERENCES accounts(id), scope text NOT NULL, idempotency_key text NOT NULL,
 payload_hash text NOT NULL, result jsonb NOT NULL, created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 PRIMARY KEY(actor_id,scope,idempotency_key)
);
