-- Immutable rule publications and scoped, two-stage execution evidence.
CREATE TABLE keyword_scope_heads (
 id text PRIMARY KEY, kind text NOT NULL CHECK(kind IN ('global','source','unit','route','workspace')),
 object_id text NOT NULL, channel text, label text NOT NULL,
 revision bigint NOT NULL DEFAULT 0, version_id uuid NOT NULL,
 CHECK((kind='workspace')=(channel IS NOT NULL)),CHECK(channel IS NULL OR channel IN ('mobile','wecom'))
);
CREATE TABLE keyword_rule_versions (
 id uuid PRIMARY KEY, scope_id text NOT NULL REFERENCES keyword_scope_heads,
 revision bigint NOT NULL, entries jsonb NOT NULL, actor_id uuid REFERENCES accounts,
 reason text NOT NULL, created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 legacy_revision bigint, UNIQUE(scope_id,revision)
);
ALTER TABLE keyword_scope_heads ADD CONSTRAINT keyword_head_version FOREIGN KEY(version_id) REFERENCES keyword_rule_versions DEFERRABLE INITIALLY DEFERRED;
CREATE TRIGGER keyword_versions_immutable BEFORE UPDATE OR DELETE ON keyword_rule_versions FOR EACH ROW EXECUTE FUNCTION source_routing_immutable();
CREATE FUNCTION keyword_scope_init(scope text, scope_kind text, object_key text, scope_channel text, scope_label text) RETURNS void LANGUAGE plpgsql AS $$
DECLARE version uuid; inserted integer;
BEGIN
 version=gen_random_uuid();
 INSERT INTO keyword_scope_heads(id,kind,object_id,channel,label,version_id) VALUES(scope,scope_kind,object_key,scope_channel,scope_label,version) ON CONFLICT DO NOTHING;
 GET DIAGNOSTICS inserted=ROW_COUNT;
 IF inserted=1 THEN INSERT INTO keyword_rule_versions(id,scope_id,revision,entries,reason) VALUES(version,scope,0,'[]','initial empty rule set'); END IF;
END $$;
SELECT keyword_scope_init('global','global','global',NULL,'全局规则');
SELECT keyword_scope_init('source:'||source_id,'source',source_id::text,NULL,source_id::text) FROM source_callers;
SELECT keyword_scope_init('unit:'||id,'unit',id::text,NULL,name) FROM routing_units;
SELECT keyword_scope_init('route:'||route_id,'route',route_id,NULL,route_name) FROM source_routes;
SELECT keyword_scope_init('mobile:'||id,'workspace',id::text,'mobile',name) FROM mobile_workspaces;
SELECT keyword_scope_init('wecom:'||id,'workspace',id::text,'wecom',name) FROM wecom_workspaces;
DO $$
DECLARE version uuid:=gen_random_uuid(); entries jsonb; legacy bigint;
BEGIN
 SELECT p.revision,COALESCE((SELECT jsonb_agg(jsonb_build_object('word',term,'enabled',true)) FROM unnest(p.terms) term),'[]') INTO legacy,entries FROM source_check_policy p WHERE singleton;
 IF legacy IS NULL THEN RAISE EXCEPTION 'source policy missing; cannot fabricate empty rules'; END IF;
 INSERT INTO keyword_rule_versions(id,scope_id,revision,entries,reason,legacy_revision) VALUES(version,'global',1,entries,'import source_check_policy',legacy);
 UPDATE keyword_scope_heads SET revision=1,version_id=version WHERE id='global';
END $$;
-- Historical policy and check facts remain readable, with no second runtime writer.
CREATE TRIGGER source_policy_retired BEFORE INSERT OR UPDATE OR DELETE ON source_check_policy FOR EACH ROW EXECUTE FUNCTION source_routing_immutable();
CREATE FUNCTION keyword_scope_created() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 CASE TG_TABLE_NAME
 WHEN 'source_callers' THEN PERFORM keyword_scope_init('source:'||NEW.source_id,'source',NEW.source_id::text,NULL,NEW.source_id::text);
 WHEN 'source_routes' THEN PERFORM keyword_scope_init('route:'||NEW.route_id,'route',NEW.route_id,NULL,NEW.route_name);
 WHEN 'routing_units' THEN PERFORM keyword_scope_init('unit:'||NEW.id,'unit',NEW.id::text,NULL,NEW.name);
 WHEN 'mobile_workspaces' THEN PERFORM keyword_scope_init('mobile:'||NEW.id,'workspace',NEW.id::text,'mobile',NEW.name);
 WHEN 'wecom_workspaces' THEN PERFORM keyword_scope_init('wecom:'||NEW.id,'workspace',NEW.id::text,'wecom',NEW.name);
 END CASE;
 RETURN NEW;
END $$;
CREATE TRIGGER keyword_source_created AFTER INSERT ON source_callers FOR EACH ROW EXECUTE FUNCTION keyword_scope_created();
CREATE TRIGGER keyword_route_created AFTER INSERT ON source_routes FOR EACH ROW EXECUTE FUNCTION keyword_scope_created();
CREATE TRIGGER keyword_unit_created AFTER INSERT ON routing_units FOR EACH ROW EXECUTE FUNCTION keyword_scope_created();
CREATE TRIGGER keyword_mobile_created AFTER INSERT ON mobile_workspaces FOR EACH ROW EXECUTE FUNCTION keyword_scope_created();
CREATE TRIGGER keyword_wecom_created AFTER INSERT ON wecom_workspaces FOR EACH ROW EXECUTE FUNCTION keyword_scope_created();
CREATE TABLE keyword_check_results (
 id uuid PRIMARY KEY, receipt_id uuid NOT NULL REFERENCES source_receipts,
 stage text NOT NULL CHECK(stage IN ('original_common','original_workspace','final')),
 payload_fingerprint text NOT NULL, context jsonb NOT NULL, effective_fingerprint text NOT NULL,
 input_digest text NOT NULL, normalized_digest text NOT NULL, algorithm text NOT NULL, unicode_version text NOT NULL,
 outcome text NOT NULL CHECK(outcome IN ('passed','blocked')), total_hits bigint NOT NULL,
 matched_rules jsonb NOT NULL, samples jsonb NOT NULL, truncated boolean NOT NULL,
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 UNIQUE(receipt_id,stage,payload_fingerprint,effective_fingerprint,input_digest,algorithm)
);
CREATE INDEX keyword_checks_receipt ON keyword_check_results(receipt_id,created_at DESC,id);
CREATE TRIGGER keyword_checks_immutable BEFORE UPDATE OR DELETE ON keyword_check_results FOR EACH ROW EXECUTE FUNCTION source_routing_immutable();
CREATE TABLE keyword_admissions (
 receipt_id uuid PRIMARY KEY REFERENCES source_receipts, common_check uuid NOT NULL REFERENCES keyword_check_results,
 workspace_check uuid NOT NULL REFERENCES keyword_check_results, final_check uuid NOT NULL REFERENCES keyword_check_results,
 effective_fingerprint text NOT NULL, content_digest text NOT NULL,
 composer text NOT NULL, context jsonb NOT NULL, updated_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE keyword_recheck_batches (
 id uuid PRIMARY KEY, scope_id text REFERENCES keyword_scope_heads, version_id uuid REFERENCES keyword_rule_versions,
 after_receipt uuid, complete boolean NOT NULL DEFAULT false, created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
INSERT INTO keyword_recheck_batches(id) VALUES(gen_random_uuid());
ALTER TABLE source_receipts ADD COLUMN execution_check_state text;
ALTER TABLE source_inbox ADD COLUMN event_type text;
UPDATE source_inbox SET event_type=convert_from(request_body,'UTF8')::jsonb #>> '{data,attributes,event_type}';
CREATE FUNCTION source_inbox_event_type() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN NEW.event_type=convert_from(NEW.request_body,'UTF8')::jsonb #>> '{data,attributes,event_type}'; RETURN NEW; END $$;
CREATE TRIGGER source_inbox_event_type BEFORE INSERT OR UPDATE OF request_body ON source_inbox FOR EACH ROW EXECUTE FUNCTION source_inbox_event_type();
CREATE INDEX source_withdrawals_due ON source_inbox(next_attempt_at,receipt_id) WHERE event_type='content.withdrawn' AND state IN ('received','waiting');
CREATE TABLE keyword_target_blocks(receipt_id uuid NOT NULL REFERENCES source_receipts,target_id uuid NOT NULL,check_id uuid NOT NULL REFERENCES keyword_check_results,PRIMARY KEY(receipt_id,target_id));
