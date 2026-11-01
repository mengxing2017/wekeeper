ALTER TABLE mobile_capability_openings ADD CONSTRAINT mobile_opening_device_identity UNIQUE(device_id,id);
-- An invitation authorizes one no-message inspection, never a school-wide device grant.
CREATE TABLE mobile_field_check_invitations (
 id uuid PRIMARY KEY,school_id uuid NOT NULL REFERENCES schools(id),device_id uuid NOT NULL REFERENCES execution_devices(id),
 target_id uuid NOT NULL,binding_id uuid NOT NULL,binding_revision bigint NOT NULL,target_locator_revision bigint NOT NULL,
 search_text text NOT NULL,revision bigint NOT NULL DEFAULT 1 CHECK(revision>0),revoked_at timestamptz,
 created_by uuid NOT NULL REFERENCES accounts(id),created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 expires_at timestamptz NOT NULL DEFAULT clock_timestamp()+interval '24 hours',
 FOREIGN KEY(school_id,device_id,target_id,binding_id) REFERENCES mobile_device_target_bindings(school_id,device_id,target_id,id), UNIQUE(school_id,id), UNIQUE(school_id,device_id,id)
);
CREATE TABLE mobile_shared_device_leases (
 device_id uuid PRIMARY KEY REFERENCES execution_devices(id),fence bigint NOT NULL CHECK(fence>0),
 holder_id uuid NOT NULL,purpose text NOT NULL CHECK(purpose IN ('field_check','send')),
 expires_at timestamptz NOT NULL
);
CREATE TABLE mobile_field_checks (
 id uuid PRIMARY KEY,device_id uuid NOT NULL REFERENCES execution_devices(id),opening_id uuid NOT NULL REFERENCES mobile_capability_openings(id),
 school_id uuid NOT NULL,invitation_id uuid NOT NULL UNIQUE,invitation_revision bigint NOT NULL,
 configuration_revision bigint NOT NULL,environment_snapshot_id uuid NOT NULL,consent_revision bigint NOT NULL,credential_generation bigint NOT NULL,
 challenge text NOT NULL CHECK(challenge ~ '^[0-9a-f]{32}$'),state text NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','leased','passed','failed','stale','expired')),
 revision bigint NOT NULL DEFAULT 1 CHECK(revision>0),lease_fence bigint,
 created_by uuid NOT NULL REFERENCES accounts(id),created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 expires_at timestamptz NOT NULL DEFAULT clock_timestamp()+interval '10 minutes',
 FOREIGN KEY(school_id,device_id,invitation_id) REFERENCES mobile_field_check_invitations(school_id,device_id,id),
 FOREIGN KEY(device_id,opening_id) REFERENCES mobile_capability_openings(device_id,id),
 UNIQUE(device_id,id),
 FOREIGN KEY(device_id,environment_snapshot_id) REFERENCES mobile_device_environments(device_id,id)
);
CREATE UNIQUE INDEX mobile_field_check_one_pending ON mobile_field_checks(device_id) WHERE state IN ('pending','leased');
CREATE TABLE mobile_field_check_results (
 id uuid PRIMARY KEY REFERENCES mobile_field_checks(id),device_id uuid NOT NULL REFERENCES execution_devices(id),
 body jsonb NOT NULL,disposition text NOT NULL CHECK(disposition IN ('passed','failed','stale')),
 recorded_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 FOREIGN KEY(device_id,id) REFERENCES mobile_field_checks(device_id,id)
);
CREATE TRIGGER mobile_field_check_result_immutable BEFORE UPDATE OR DELETE ON mobile_field_check_results FOR EACH ROW EXECUTE FUNCTION mobile_capability_evidence_immutable();
CREATE TABLE mobile_field_check_operations (
 device_id uuid NOT NULL REFERENCES execution_devices(id),credential_generation bigint NOT NULL,scope text NOT NULL,idempotency_key text NOT NULL,
 payload_hash text NOT NULL,result jsonb NOT NULL,PRIMARY KEY(device_id,credential_generation,scope,idempotency_key)
);
CREATE FUNCTION mobile_field_invitation_snapshot_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='DELETE' OR (to_jsonb(NEW)-'revoked_at'-'revision') IS DISTINCT FROM (to_jsonb(OLD)-'revoked_at'-'revision') THEN
 RAISE EXCEPTION 'field invitation snapshot immutable' USING ERRCODE='23514'; END IF; RETURN NEW;
END $$;
CREATE TRIGGER mobile_field_invitation_immutable BEFORE UPDATE OR DELETE ON mobile_field_check_invitations FOR EACH ROW EXECUTE FUNCTION mobile_field_invitation_snapshot_immutable();

CREATE FUNCTION mobile_field_check_snapshot_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='DELETE' OR (to_jsonb(NEW)-'state'-'revision'-'lease_fence') IS DISTINCT FROM (to_jsonb(OLD)-'state'-'revision'-'lease_fence') THEN
 RAISE EXCEPTION 'field check snapshot immutable' USING ERRCODE='23514'; END IF; RETURN NEW;
END $$;
CREATE TRIGGER mobile_field_check_immutable BEFORE UPDATE OR DELETE ON mobile_field_checks FOR EACH ROW EXECUTE FUNCTION mobile_field_check_snapshot_immutable();
