CREATE TABLE source_platforms (
 source_id uuid PRIMARY KEY, name text NOT NULL CHECK(length(trim(name))>0), status text NOT NULL CHECK(status IN ('active','disabled')),
 origins jsonb NOT NULL DEFAULT '[]', path_prefix text NOT NULL CHECK(path_prefix LIKE '/%'), revision bigint NOT NULL DEFAULT 1,
 created_at timestamptz NOT NULL DEFAULT now(), updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE source_credentials (
 key_id text PRIMARY KEY, source_id uuid NOT NULL REFERENCES source_platforms, unit_name text NOT NULL CHECK(length(trim(unit_name))>0), remark text NOT NULL DEFAULT '',
 encrypted_secret bytea, encryption_version text NOT NULL, status text NOT NULL CHECK(status IN ('active','disabled','revoked','deleted')),
 valid_from timestamptz NOT NULL, valid_until timestamptz NOT NULL, revision bigint NOT NULL DEFAULT 1,
 created_by uuid NOT NULL REFERENCES accounts, created_at timestamptz NOT NULL DEFAULT now(), updated_at timestamptz NOT NULL DEFAULT now(), last_used_at timestamptz,
 deleted_by uuid REFERENCES accounts, deleted_at timestamptz, rotated_from text REFERENCES source_credentials,
 delivery_pending boolean NOT NULL DEFAULT true, UNIQUE(key_id,source_id), CHECK(valid_until>valid_from)
);
CREATE TABLE source_routes (
 route_id text PRIMARY KEY, source_id uuid NOT NULL REFERENCES source_platforms, route_name text NOT NULL CHECK(length(trim(route_name))>0), remark text NOT NULL DEFAULT '',
 status text NOT NULL CHECK(status IN ('active','disabled')), revision bigint NOT NULL DEFAULT 1,
 created_at timestamptz NOT NULL DEFAULT now(), updated_at timestamptz NOT NULL DEFAULT now(), UNIQUE(route_id,source_id)
);
CREATE TABLE source_key_routes (
 key_id text NOT NULL, route_id text NOT NULL, source_id uuid NOT NULL, status text NOT NULL CHECK(status IN ('active','revoked')),
 revision bigint NOT NULL DEFAULT 1, created_at timestamptz NOT NULL DEFAULT now(), updated_at timestamptz NOT NULL DEFAULT now(),
 PRIMARY KEY(key_id,route_id), FOREIGN KEY(key_id,source_id) REFERENCES source_credentials(key_id,source_id), FOREIGN KEY(route_id,source_id) REFERENCES source_routes(route_id,source_id)
);
CREATE TABLE source_request_nonces (
 key_id text NOT NULL, source_id uuid NOT NULL, nonce uuid NOT NULL, expires_at timestamptz NOT NULL, created_at timestamptz NOT NULL DEFAULT now(),
 PRIMARY KEY(key_id,nonce), FOREIGN KEY(key_id,source_id) REFERENCES source_credentials(key_id,source_id)
);
CREATE INDEX source_nonce_expiry ON source_request_nonces(expires_at);
CREATE INDEX source_nonce_rate ON source_request_nonces(key_id,created_at);
CREATE TABLE source_receipts (
 receipt_id uuid PRIMARY KEY, source_id uuid NOT NULL REFERENCES source_platforms, event_id uuid NOT NULL, route_id text NOT NULL,
 body_sha256 text NOT NULL, schema_version integer NOT NULL CHECK(schema_version=2), signature_version integer NOT NULL DEFAULT 2 CHECK(signature_version=2), status text NOT NULL DEFAULT 'received',
 reason_code text, processing_stage text NOT NULL DEFAULT 'received', sending jsonb,
 received_at timestamptz NOT NULL DEFAULT now(), updated_at timestamptz NOT NULL DEFAULT now(), UNIQUE(source_id,event_id),
 FOREIGN KEY(route_id,source_id) REFERENCES source_routes(route_id,source_id)
);
CREATE TABLE source_inbox (
 receipt_id uuid PRIMARY KEY REFERENCES source_receipts, request_body bytea NOT NULL, state text NOT NULL DEFAULT 'received',
 next_attempt_at timestamptz NOT NULL DEFAULT now(), created_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE source_access_audit (
 id uuid PRIMARY KEY, source_id uuid REFERENCES source_platforms, key_id text, actor_id uuid REFERENCES accounts,
 action text NOT NULL, result_code text NOT NULL, request_id text NOT NULL, detail jsonb NOT NULL DEFAULT '{}', created_at timestamptz NOT NULL DEFAULT now()
);
-- Defense in depth: identity cannot be reassigned, including direct SQL updates.
CREATE FUNCTION source_identity_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_TABLE_NAME='source_credentials' THEN
 IF (NEW.key_id IS DISTINCT FROM OLD.key_id OR NEW.source_id IS DISTINCT FROM OLD.source_id OR (OLD.status='deleted' AND NEW.status<>'deleted')) THEN RAISE EXCEPTION 'credential identity immutable'; END IF;
 END IF;
 IF TG_TABLE_NAME='source_routes' THEN
 IF (NEW.route_id IS DISTINCT FROM OLD.route_id OR NEW.source_id IS DISTINCT FROM OLD.source_id) THEN RAISE EXCEPTION 'route identity immutable'; END IF;
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER source_credential_identity BEFORE UPDATE ON source_credentials FOR EACH ROW EXECUTE FUNCTION source_identity_immutable();
CREATE TRIGGER source_route_identity BEFORE UPDATE ON source_routes FOR EACH ROW EXECUTE FUNCTION source_identity_immutable();
