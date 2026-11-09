-- Projection evidence is independent of execution and of live route credentials.
ALTER TABLE source_receipts ADD COLUMN observed_sequence bigint,
 ADD COLUMN processing_revision bigint NOT NULL DEFAULT 0,
 ADD COLUMN canonical_receipt_id uuid REFERENCES source_receipts,
 ADD COLUMN summary_version text,
 ADD COLUMN summary_revision bigint NOT NULL DEFAULT 0,
 ADD COLUMN summary_updated_at timestamptz,
 ADD COLUMN summary_total bigint;
ALTER TABLE source_inbox ADD COLUMN projected boolean NOT NULL DEFAULT false,
 ADD COLUMN failures integer NOT NULL DEFAULT 0, ADD COLUMN last_error text;
ALTER TABLE source_content_versions ADD COLUMN fingerprint text,
 ADD COLUMN check_state text NOT NULL DEFAULT 'legacy' CHECK(check_state IN ('legacy','pending','passed','blocked','withdrawn')),
 ADD COLUMN check_revision text, ADD COLUMN no_resend boolean NOT NULL DEFAULT false;
CREATE TABLE source_content_history (
 source_id uuid NOT NULL REFERENCES source_callers, route_id text NOT NULL, external_content_id text NOT NULL,
 sequence bigint NOT NULL, receipt_id uuid NOT NULL UNIQUE REFERENCES source_receipts,
 fingerprint text NOT NULL, fingerprint_version text NOT NULL DEFAULT 'source-payload/1',
 created_at timestamptz NOT NULL DEFAULT now(), PRIMARY KEY(source_id,route_id,external_content_id,sequence)
);
CREATE TRIGGER source_content_history_immutable BEFORE UPDATE OR DELETE ON source_content_history FOR EACH ROW EXECUTE FUNCTION source_routing_immutable();
CREATE TABLE source_receipt_history (
 id bigserial PRIMARY KEY, receipt_id uuid NOT NULL REFERENCES source_receipts,
 revision bigint NOT NULL, status text NOT NULL, stage text NOT NULL, reason_code text,
 observed_sequence bigint, created_at timestamptz NOT NULL DEFAULT clock_timestamp(), UNIQUE(receipt_id,revision)
);
CREATE TRIGGER source_receipt_history_immutable BEFORE UPDATE OR DELETE ON source_receipt_history FOR EACH ROW EXECUTE FUNCTION source_routing_immutable();
CREATE FUNCTION source_receipt_transition() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF (NEW.status,NEW.processing_stage,NEW.reason_code,NEW.observed_sequence) IS DISTINCT FROM (OLD.status,OLD.processing_stage,OLD.reason_code,OLD.observed_sequence) THEN
  NEW.processing_revision=OLD.processing_revision+1; NEW.updated_at=clock_timestamp();
  INSERT INTO source_receipt_history(receipt_id,revision,status,stage,reason_code,observed_sequence) VALUES(NEW.receipt_id,NEW.processing_revision,NEW.status,NEW.processing_stage,NEW.reason_code,NEW.observed_sequence);
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER source_receipt_transition BEFORE UPDATE ON source_receipts FOR EACH ROW EXECUTE FUNCTION source_receipt_transition();
-- A successfully loaded empty policy is real data; load errors never imply an empty policy.
CREATE TABLE source_check_policy (
 singleton boolean PRIMARY KEY DEFAULT true CHECK(singleton), revision bigint NOT NULL DEFAULT 1,
 terms text[] NOT NULL DEFAULT '{}', updated_at timestamptz NOT NULL DEFAULT now()
);
INSERT INTO source_check_policy(singleton) VALUES(true);
CREATE FUNCTION source_check_policy_revision() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN NEW.revision=OLD.revision+1; NEW.updated_at=clock_timestamp(); RETURN NEW; END $$;
CREATE TRIGGER source_check_policy_revision BEFORE UPDATE ON source_check_policy FOR EACH ROW EXECUTE FUNCTION source_check_policy_revision();
CREATE TABLE source_content_checks (
 receipt_id uuid NOT NULL REFERENCES source_receipts, rule_revision bigint NOT NULL,
 fingerprint text NOT NULL, outcome text NOT NULL CHECK(outcome IN ('passed','blocked')),
 created_at timestamptz NOT NULL DEFAULT now(), PRIMARY KEY(receipt_id,rule_revision,fingerprint)
);
CREATE TRIGGER source_content_checks_immutable BEFORE UPDATE OR DELETE ON source_content_checks FOR EACH ROW EXECUTE FUNCTION source_routing_immutable();
CREATE TABLE source_recheck_requests (
 receipt_id uuid PRIMARY KEY REFERENCES source_receipts, actor_id uuid NOT NULL REFERENCES accounts,
 reason text NOT NULL, expected_revision bigint NOT NULL, created_at timestamptz NOT NULL DEFAULT now()
);
ALTER TABLE source_route_managers ADD COLUMN can_recheck boolean NOT NULL DEFAULT false;
CREATE TABLE source_receipt_relations (
 receipt_id uuid NOT NULL REFERENCES source_receipts, related_receipt_id uuid NOT NULL REFERENCES source_receipts,
 kind text NOT NULL CHECK(kind IN ('duplicate','update','withdrawal')), PRIMARY KEY(receipt_id,related_receipt_id,kind),
 CHECK(receipt_id<>related_receipt_id)
);
CREATE TRIGGER source_receipt_relations_immutable BEFORE UPDATE OR DELETE ON source_receipt_relations FOR EACH ROW EXECUTE FUNCTION source_routing_immutable();
CREATE TABLE source_query_usage (
 source_id uuid NOT NULL REFERENCES source_callers, minute timestamptz NOT NULL,
 queries bigint NOT NULL DEFAULT 0, last_queried_at timestamptz NOT NULL DEFAULT now(), PRIMARY KEY(source_id,minute)
);
CREATE INDEX source_receipt_route_page ON source_receipts(route_id,received_at DESC,receipt_id DESC);
CREATE TABLE source_summary_queue(receipt_id uuid PRIMARY KEY REFERENCES source_receipts);
CREATE FUNCTION source_summary_dirty() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE obj jsonb; item uuid; task uuid; receipt uuid;
BEGIN
 obj=to_jsonb(NEW);
 IF TG_TABLE_NAME='source_execution_tasks' THEN receipt=(obj->>'receipt_id')::uuid;
 ELSIF TG_TABLE_NAME IN ('mobile_v9_attempts','mobile_v9_dispatch_state') THEN item=(obj->>'item_id')::uuid;
 ELSIF TG_TABLE_NAME='mobile_execution_grants' THEN SELECT item_id INTO item FROM mobile_v9_attempts WHERE id=(obj->>'attempt_id')::uuid;
 ELSE task=COALESCE((obj->>'task_id')::uuid,(obj->>'id')::uuid);
 END IF;
 IF item IS NOT NULL THEN SELECT task_id INTO task FROM mobile_v9_task_items WHERE id=item; END IF;
 IF task IS NOT NULL THEN SELECT receipt_id INTO receipt FROM source_execution_tasks WHERE task_id=task; END IF;
 IF receipt IS NOT NULL THEN INSERT INTO source_summary_queue VALUES(receipt) ON CONFLICT DO NOTHING; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER source_task_summary AFTER INSERT ON source_execution_tasks FOR EACH ROW EXECUTE FUNCTION source_summary_dirty();
CREATE TRIGGER source_mobile_summary AFTER INSERT OR UPDATE ON mobile_v9_attempts FOR EACH ROW EXECUTE FUNCTION source_summary_dirty();
CREATE TRIGGER source_dispatch_summary AFTER INSERT OR UPDATE ON mobile_v9_dispatch_state FOR EACH ROW EXECUTE FUNCTION source_summary_dirty();
CREATE TRIGGER source_grant_summary AFTER INSERT OR UPDATE ON mobile_execution_grants FOR EACH ROW EXECUTE FUNCTION source_summary_dirty();
CREATE TRIGGER source_mobile_task_summary AFTER UPDATE ON mobile_v9_tasks FOR EACH ROW EXECUTE FUNCTION source_summary_dirty();
CREATE TRIGGER source_official_summary AFTER INSERT OR UPDATE ON broadcast_targets FOR EACH ROW EXECUTE FUNCTION source_summary_dirty();
CREATE TRIGGER source_batch_summary AFTER INSERT OR UPDATE ON broadcast_batches FOR EACH ROW EXECUTE FUNCTION source_summary_dirty();
-- Bounded initial summary population is performed by the worker, not in this migration.

-- A policy revision walks current projections in bounded indexed pages, once per revision.
CREATE UNIQUE INDEX source_content_versions_receipt ON source_content_versions(receipt_id);
CREATE INDEX source_content_versions_legacy ON source_content_versions(updated_at) WHERE fingerprint IS NULL;
CREATE TABLE source_check_scan (
 singleton boolean PRIMARY KEY DEFAULT true CHECK(singleton), rule_revision bigint NOT NULL DEFAULT 0,
 after_receipt uuid, complete boolean NOT NULL DEFAULT false
);
INSERT INTO source_check_scan(singleton) VALUES(true);
