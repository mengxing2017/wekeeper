-- Runtime configuration is independent of development artifacts and release approval.
-- No copy/backfill from development releases; install actual deployment resources.
CREATE TABLE mobile_runtime_configurations (
 id uuid PRIMARY KEY, capability text NOT NULL CHECK(capability='wecom.group_text'),
 version text NOT NULL, flow jsonb NOT NULL, flow_digest text NOT NULL,
 installed_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 UNIQUE(id,capability)
);
CREATE TABLE mobile_runtime_configuration_current (
 capability text PRIMARY KEY CHECK(capability='wecom.group_text'),
 configuration_id uuid NOT NULL, updated_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 FOREIGN KEY(configuration_id,capability) REFERENCES mobile_runtime_configurations(id,capability)
);
ALTER TABLE mobile_v9_runtime_details ADD COLUMN preparation jsonb NOT NULL DEFAULT '{}';
