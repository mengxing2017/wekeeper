-- Group-level target directories are retired; channel targets have their own owners.
CREATE OR REPLACE FUNCTION routing_group_scope() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.unit_id IS DISTINCT FROM OLD.unit_id AND EXISTS(SELECT 1 FROM source_routes WHERE group_id=OLD.id) THEN
  RAISE EXCEPTION 'referenced group cannot move';
 END IF;
 RETURN NEW;
END $$;
DROP TABLE routing_group_targets;
