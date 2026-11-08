-- Isolated migration/constraint smoke test; no credentials or real messages leave PostgreSQL.
BEGIN;
DO $$ BEGIN
 IF EXISTS(SELECT 1 FROM information_schema.columns WHERE table_name='source_callers' AND column_name IN ('origins','path_prefix','name','status')) THEN RAISE EXCEPTION 'obsolete caller settings remain'; END IF;
 IF to_regclass('source_platforms') IS NOT NULL THEN RAISE EXCEPTION 'separate platform model remains'; END IF;
END $$;
INSERT INTO accounts(id,username,display_name,platform_admin) VALUES('10000000-0000-0000-0000-000000000001','source-test','system',true);
INSERT INTO source_callers(source_id) VALUES
('10000000-0000-0000-0000-000000000011'),
('10000000-0000-0000-0000-000000000012');
INSERT INTO source_credentials(key_id,source_id,unit_name,remark,encryption_version,status,valid_from,valid_until,created_by) VALUES
('k1','10000000-0000-0000-0000-000000000011','unit','credential remark','v1','active',now()-interval '1 day',now()+interval '1 day','10000000-0000-0000-0000-000000000001'),
('k2','10000000-0000-0000-0000-000000000012','unit','other platform','v1','active',now()-interval '1 day',now()+interval '1 day','10000000-0000-0000-0000-000000000001');
INSERT INTO source_routes(route_id,source_id,route_name,remark,status) VALUES
('m1','10000000-0000-0000-0000-000000000011','route','route remark','active'),
('m2','10000000-0000-0000-0000-000000000012','route','other platform','active');
INSERT INTO source_key_routes(key_id,route_id,source_id,status) VALUES('k1','m1','10000000-0000-0000-0000-000000000011','active');
DO $$ BEGIN
 BEGIN INSERT INTO source_key_routes(key_id,route_id,source_id,status) VALUES('k1','m2','10000000-0000-0000-0000-000000000011','active'); RAISE EXCEPTION 'cross-platform association accepted'; EXCEPTION WHEN foreign_key_violation THEN NULL; END;
 BEGIN INSERT INTO source_key_routes(key_id,route_id,source_id,status) VALUES('k1','m1','10000000-0000-0000-0000-000000000011','active'); RAISE EXCEPTION 'duplicate association accepted'; EXCEPTION WHEN unique_violation THEN NULL; END;
 BEGIN UPDATE source_credentials SET source_id='10000000-0000-0000-0000-000000000012' WHERE key_id='k1'; RAISE EXCEPTION 'credential moved'; EXCEPTION WHEN raise_exception THEN IF SQLERRM<>'credential identity immutable' THEN RAISE; END IF; END;
 BEGIN UPDATE source_routes SET source_id='10000000-0000-0000-0000-000000000012' WHERE route_id='m1'; RAISE EXCEPTION 'route moved'; EXCEPTION WHEN raise_exception THEN IF SQLERRM<>'route identity immutable' THEN RAISE; END IF; END;
END $$;
INSERT INTO source_request_nonces VALUES('k1','10000000-0000-0000-0000-000000000011','10000000-0000-0000-0000-000000000101',now()+interval '6 minutes');
INSERT INTO source_request_nonces(key_id,source_id,nonce,expires_at) VALUES('k1','10000000-0000-0000-0000-000000000011','10000000-0000-0000-0000-000000000101',now()) ON CONFLICT DO NOTHING;
DO $$ BEGIN IF (SELECT count(*) FROM source_request_nonces WHERE key_id='k1')<>1 THEN RAISE EXCEPTION 'nonce not unique'; END IF; END $$;
INSERT INTO source_receipts(receipt_id,source_id,event_id,route_id,body_sha256,schema_version) VALUES('10000000-0000-0000-0000-000000000201','10000000-0000-0000-0000-000000000011','10000000-0000-0000-0000-000000000301','m1','hash',2);
INSERT INTO source_inbox(receipt_id,request_body) VALUES('10000000-0000-0000-0000-000000000201','body');
-- Real NULL expiry, rather than a sentinel date; invalid bounded periods still fail.
UPDATE source_credentials SET valid_until=NULL WHERE key_id='k1';
DO $$ BEGIN
 IF (SELECT valid_until FROM source_credentials WHERE key_id='k1') IS NOT NULL THEN RAISE EXCEPTION 'perpetual credential not stored'; END IF;
 BEGIN UPDATE source_credentials SET valid_until=valid_from WHERE key_id='k2'; RAISE EXCEPTION 'invalid period accepted'; EXCEPTION WHEN check_violation THEN NULL; END;
END $$;
DELETE FROM source_key_routes WHERE key_id='k1';
DELETE FROM source_request_nonces WHERE key_id='k1';
DELETE FROM source_routes WHERE route_id='m1';
DELETE FROM source_credentials WHERE key_id='k1';
DO $$ BEGIN
 IF EXISTS(SELECT 1 FROM source_credentials WHERE key_id='k1') OR EXISTS(SELECT 1 FROM source_routes WHERE route_id='m1') THEN RAISE EXCEPTION 'live configuration remains'; END IF;
 IF NOT EXISTS(SELECT 1 FROM source_receipts WHERE route_id='m1') OR NOT EXISTS(SELECT 1 FROM source_inbox) THEN RAISE EXCEPTION 'history removed'; END IF;
END $$;
ROLLBACK;
