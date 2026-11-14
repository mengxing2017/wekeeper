-- Keep immutable historical evidence; retire all old mutable control stores.
CREATE TABLE retired_distribution_evidence (
 kind text NOT NULL, facts jsonb NOT NULL, recorded_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TRIGGER retired_distribution_evidence_immutable BEFORE UPDATE OR DELETE ON retired_distribution_evidence FOR EACH ROW EXECUTE FUNCTION source_routing_immutable();
DROP FUNCTION advertisement_route_created() CASCADE;
DROP FUNCTION advertisement_owner_fixed() CASCADE;
DROP FUNCTION source_automation_freeze() CASCADE;
DROP FUNCTION source_receive_order_allocate() CASCADE;
DROP FUNCTION keyword_scope_created() CASCADE;
DO $$ DECLARE object_name text; BEGIN
 FOREACH object_name IN ARRAY ARRAY[
 'keyword_recheck_batches','keyword_target_blocks','source_route_receive_counters','advertisement_pool_consumptions','advertisement_pool_transitions','advertisement_pool_queue',
 'advertisement_pool_slots','advertisement_pool_cursors','advertisement_pool_revisions','advertisement_pools',
 'source_advertisement_configs','source_advertisement_config_versions','advertisement_recheck_jobs',
 'advertisement_target_blocks','advertisement_content_versions','advertisement_policy_versions',
 'advertisements','advertisement_unit_managers','source_content_versions','source_recheck_requests',
 'source_routing_waits','source_orchestration_recovery_jobs','source_automation_commands','source_routing_proposals',
 'source_route_targets','routing_entity_history','routing_groups','routing_units'
 ] LOOP
  EXECUTE format('INSERT INTO retired_distribution_evidence(kind,facts) SELECT %L,to_jsonb(t) FROM %I t',object_name,object_name);
  EXECUTE format('DROP TABLE %I CASCADE',object_name);
 END LOOP;
END $$;
INSERT INTO retired_distribution_evidence(kind,facts)
 SELECT 'source_receipt_order',jsonb_build_object('receipt_id',receipt_id,'receive_order',receive_order,'reconstructed',receive_order_reconstructed) FROM source_receipts;
ALTER TABLE source_receipts DROP COLUMN receive_order,DROP COLUMN receive_order_reconstructed;
ALTER TABLE source_routes DROP COLUMN unit_id,DROP COLUMN group_id;
-- Route lifecycle protects source identity only. It no longer controls delivery.
CREATE OR REPLACE FUNCTION source_route_lifecycle() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='INSERT' THEN
  IF EXISTS(SELECT 1 FROM source_route_tombstones WHERE route_id=NEW.route_id) THEN RAISE EXCEPTION 'retired route identifier'; END IF;
  RETURN NEW;
 ELSIF TG_OP='DELETE' THEN
  INSERT INTO source_route_tombstones(route_id,source_id,reason,config_revision) VALUES(OLD.route_id,OLD.source_id,'credential_deleted',OLD.config_revision) ON CONFLICT DO NOTHING;
  RETURN OLD;
 END IF;
 RETURN NEW;
END $$;
-- Historical withdrawal events are terminal evidence, never work to execute.
INSERT INTO orchestration_dispatches(message_id,content,decision,plans,state,reason_code)
SELECT id,'','{}','[]','historical','historical_event' FROM orchestration_messages
WHERE original->>'event_kind'='content.withdrawn' ON CONFLICT DO NOTHING;
-- Pending unsupported handoffs have no valid business message. Retire them as
-- receipt evidence instead of perpetually retrying an obsolete control command.
CREATE TABLE retired_message_handoffs AS SELECT h.* FROM source_message_handoffs h JOIN source_inbox i USING(receipt_id) WHERE h.state='pending' AND i.event_type='content.withdrawn';
DELETE FROM source_message_handoffs WHERE receipt_id IN (SELECT receipt_id FROM retired_message_handoffs);
