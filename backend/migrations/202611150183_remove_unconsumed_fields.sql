-- Retire unused metadata and internal cursors, including their trigger dependencies.
ALTER TABLE execution_devices
 DROP COLUMN remark,
 DROP COLUMN custodian_actor_id,
 DROP COLUMN metadata_revision;

-- A tombstone prevents identifier reuse; the obsolete configuration counter
-- does not participate in identity or current receiving configuration revisions.
CREATE OR REPLACE FUNCTION source_route_lifecycle() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='INSERT' THEN
  IF EXISTS(SELECT 1 FROM source_route_tombstones WHERE route_id=NEW.route_id) THEN RAISE EXCEPTION 'retired route identifier'; END IF;
  RETURN NEW;
 ELSIF TG_OP='DELETE' THEN
  INSERT INTO source_route_tombstones(route_id,source_id,reason) VALUES(OLD.route_id,OLD.source_id,'credential_deleted') ON CONFLICT DO NOTHING;
  RETURN OLD;
 END IF;
 RETURN NEW;
END $$;
ALTER TABLE source_routes DROP COLUMN config_revision;
ALTER TABLE source_route_tombstones DROP COLUMN config_revision;

-- Dispatch now advances per target/attempt. Keep wake hints for real task
-- changes and attempt transitions without retaining the old task cursor.
CREATE OR REPLACE FUNCTION mobile_v9_inbox_changed() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE r record; v bigint;
BEGIN
 IF TG_TABLE_NAME='mobile_v9_attempts' THEN
  IF TG_OP='UPDATE' AND NEW.state=OLD.state AND NEW.current=OLD.current THEN RETURN NEW; END IF;
  NEW.inbox_changed_at=clock_timestamp();
  FOR r IN SELECT p.device_id,NEW.confirmed_by AS actor_id FROM mobile_v9_plans p WHERE p.id=NEW.plan_id LOOP
   INSERT INTO mobile_v9_device_inbox_versions(device_id,revision) VALUES(r.device_id,1) ON CONFLICT(device_id) DO UPDATE SET revision=mobile_v9_device_inbox_versions.revision+1 RETURNING revision INTO v;
   INSERT INTO mobile_v9_task_wake_outbox VALUES(gen_random_uuid(),r.device_id,v,r.actor_id,clock_timestamp());
  END LOOP;
 ELSE
  IF NEW.stopped=OLD.stopped AND NEW.next_eligible_at=OLD.next_eligible_at AND NEW.confirmed_until=OLD.confirmed_until THEN RETURN NEW; END IF;
  FOR r IN SELECT DISTINCT p.device_id,NEW.confirmed_by AS actor_id FROM mobile_v9_task_items i JOIN mobile_v9_plans p ON p.item_id=i.id WHERE i.task_id=NEW.id ORDER BY p.device_id LOOP
   INSERT INTO mobile_v9_device_inbox_versions(device_id,revision) VALUES(r.device_id,1) ON CONFLICT(device_id) DO UPDATE SET revision=mobile_v9_device_inbox_versions.revision+1 RETURNING revision INTO v;
   INSERT INTO mobile_v9_task_wake_outbox VALUES(gen_random_uuid(),r.device_id,v,r.actor_id,clock_timestamp());
  END LOOP;
 END IF;
 RETURN NEW;
END $$;
ALTER TABLE mobile_v9_tasks DROP COLUMN cursor_position;

-- The removed receipt-detail endpoint was the last reader of these retired
-- stores. Retain existing immutable facts in the established evidence archive.
INSERT INTO retired_distribution_evidence(kind,facts)
 SELECT 'source_content_checks',to_jsonb(c) FROM source_content_checks c;
INSERT INTO retired_distribution_evidence(kind,facts)
 SELECT 'source_receipt_relations',to_jsonb(r) FROM source_receipt_relations r;
DROP TABLE source_content_checks;
DROP TABLE source_receipt_relations;
