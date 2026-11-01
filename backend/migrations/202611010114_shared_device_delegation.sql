-- Stable device delegation is independent of adapter/environment release identities.
-- No prior consent is promoted: the new scope requires one explicit native decision.
CREATE TABLE mobile_shared_platform_identity (
 singleton boolean PRIMARY KEY DEFAULT true CHECK(singleton), id uuid NOT NULL UNIQUE
);
INSERT INTO mobile_shared_platform_identity(singleton,id) VALUES(true,gen_random_uuid());
CREATE TABLE mobile_shared_delegations (
 id uuid PRIMARY KEY,device_id uuid NOT NULL REFERENCES execution_devices(id),
 credential_generation bigint NOT NULL,scope jsonb NOT NULL,scope_digest text NOT NULL,
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),UNIQUE(device_id,credential_generation),
 UNIQUE(device_id,id)
);
CREATE TABLE mobile_shared_delegation_decisions (
 id uuid PRIMARY KEY,delegation_id uuid NOT NULL REFERENCES mobile_shared_delegations(id),
 accepted boolean NOT NULL,revision bigint NOT NULL CHECK(revision>0),
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),UNIQUE(delegation_id,revision)
);
CREATE TRIGGER mobile_shared_delegation_immutable BEFORE UPDATE OR DELETE ON mobile_shared_delegations FOR EACH ROW EXECUTE FUNCTION mobile_capability_evidence_immutable();
CREATE TRIGGER mobile_shared_decision_immutable BEFORE UPDATE OR DELETE ON mobile_shared_delegation_decisions FOR EACH ROW EXECUTE FUNCTION mobile_capability_evidence_immutable();
ALTER TABLE mobile_shared_send_inspections ALTER COLUMN opening_id DROP NOT NULL;
ALTER TABLE mobile_shared_send_inspections ALTER COLUMN consent_id DROP NOT NULL;
ALTER TABLE mobile_shared_send_inspections ADD COLUMN delegation_id uuid REFERENCES mobile_shared_delegations(id);
ALTER TABLE mobile_shared_send_inspections ADD COLUMN automatic boolean NOT NULL DEFAULT false;
ALTER TABLE mobile_shared_send_inspections ADD COLUMN created_at timestamptz NOT NULL DEFAULT clock_timestamp();
CREATE INDEX mobile_shared_auto_checks ON mobile_shared_send_inspections(device_id,binding_id,created_at DESC);
-- Optional platform-approved school pool removes per-target platform handoffs.
CREATE TABLE mobile_shared_school_access (
 device_id uuid NOT NULL REFERENCES execution_devices(id),school_id uuid NOT NULL REFERENCES schools(id),
 enabled boolean NOT NULL,revision bigint NOT NULL CHECK(revision>0),updated_by uuid NOT NULL REFERENCES accounts(id),
 PRIMARY KEY(device_id,school_id)
);
ALTER TABLE mobile_device_target_bindings ADD COLUMN source text NOT NULL DEFAULT 'platform' CHECK(source IN ('platform','school_pool'));
CREATE TABLE mobile_shared_access_events (
 id uuid PRIMARY KEY,device_id uuid NOT NULL,school_id uuid NOT NULL,revision bigint NOT NULL,
 enabled boolean NOT NULL,actor_id uuid NOT NULL REFERENCES accounts(id),created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 FOREIGN KEY(device_id,school_id) REFERENCES mobile_shared_school_access(device_id,school_id),UNIQUE(device_id,school_id,revision)
);
CREATE TRIGGER mobile_shared_access_event_immutable BEFORE UPDATE OR DELETE ON mobile_shared_access_events FOR EACH ROW EXECUTE FUNCTION mobile_capability_evidence_immutable();
