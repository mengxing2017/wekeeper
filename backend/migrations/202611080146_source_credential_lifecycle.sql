-- NULL expiry represents a credential that remains valid until disabled/revoked/deleted.
ALTER TABLE source_credentials ALTER COLUMN valid_until DROP NOT NULL;
-- Receipts retain immutable route identifiers after live configuration is removed.
ALTER TABLE source_receipts DROP CONSTRAINT source_receipts_route_id_source_id_fkey;
ALTER TABLE source_credentials DROP COLUMN rotated_from;

-- Complete prior soft deletions, including their child routes and nonce records.
DO $$
DECLARE credential record; routes text[];
BEGIN
 FOR credential IN SELECT key_id,source_id FROM source_credentials WHERE status='deleted' LOOP
  SELECT array_agg(r.route_id) INTO routes FROM source_routes r
  WHERE r.source_id=credential.source_id AND (
   EXISTS(SELECT 1 FROM source_key_routes k WHERE k.route_id=r.route_id AND k.key_id=credential.key_id)
   OR NOT EXISTS(SELECT 1 FROM source_credentials c WHERE c.source_id=credential.source_id AND c.status<>'deleted'));
  DELETE FROM source_key_routes WHERE key_id=credential.key_id OR route_id=ANY(routes);
  DELETE FROM source_routes WHERE route_id=ANY(routes);
  DELETE FROM source_request_nonces WHERE key_id=credential.key_id;
  DELETE FROM source_credentials WHERE key_id=credential.key_id;
 END LOOP;
END $$;
ALTER TABLE source_credentials DROP COLUMN deleted_by, DROP COLUMN deleted_at;
ALTER TABLE source_credentials DROP CONSTRAINT source_credentials_status_check;
ALTER TABLE source_credentials ADD CONSTRAINT source_credentials_status_check CHECK(status IN ('active','disabled','revoked'));
COMMENT ON TABLE source_callers IS 'Internal caller identity retained for historical receipts and audit after credential deletion';
