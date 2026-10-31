-- Append-only minimal local authorization receipts. A later revoke fences late grants.
CREATE TABLE device_execution_authorizations (
 device_id uuid NOT NULL REFERENCES execution_devices(id), local_revision bigint NOT NULL CHECK(local_revision>0),
 authorization_id uuid NOT NULL, body jsonb NOT NULL, received_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 PRIMARY KEY(device_id,local_revision)
);
ALTER TABLE device_validation_jobs DROP CONSTRAINT device_validation_jobs_state_check;
ALTER TABLE device_validation_jobs ADD CHECK(state IN ('queued','offered','accepted','running','succeeded','failed','interrupted','cancelled','expired','declined'));
ALTER TABLE device_validation_jobs ADD COLUMN authorization_id uuid;
ALTER TABLE device_validation_jobs ADD COLUMN decision_source text CHECK(decision_source IN ('local_action','standing_authorization','local_decline'));
ALTER TABLE automation_readiness_tickets ADD COLUMN authorization_id uuid;
