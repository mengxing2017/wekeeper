-- Observations never authorize execution. Queue changes and wake hints commit with business writes.
CREATE TABLE mobile_v9_device_inbox_versions(device_id uuid PRIMARY KEY REFERENCES execution_devices(id), revision bigint NOT NULL DEFAULT 0);
CREATE TABLE mobile_v9_task_receipts(attempt_id uuid NOT NULL REFERENCES mobile_v9_attempts(id),device_id uuid NOT NULL REFERENCES execution_devices(id),credential_generation bigint NOT NULL,assignment_revision bigint NOT NULL CHECK(assignment_revision=1),received_at timestamptz NOT NULL DEFAULT clock_timestamp(),PRIMARY KEY(attempt_id,device_id,credential_generation,assignment_revision));
CREATE TABLE mobile_v9_inbox_receipt_operations(device_id uuid NOT NULL,generation bigint NOT NULL,idempotency_key text NOT NULL,request_hash text NOT NULL,response jsonb NOT NULL,created_at timestamptz NOT NULL DEFAULT clock_timestamp(),PRIMARY KEY(device_id,generation,idempotency_key));
CREATE TABLE mobile_v9_task_wake_outbox(id uuid PRIMARY KEY,device_id uuid NOT NULL REFERENCES execution_devices(id),revision bigint NOT NULL,actor_id uuid NOT NULL REFERENCES accounts(id),created_at timestamptz NOT NULL DEFAULT clock_timestamp(),UNIQUE(device_id,revision));
CREATE INDEX mobile_v9_wake_pending ON mobile_v9_task_wake_outbox(created_at);
CREATE INDEX mobile_v9_plan_device ON mobile_v9_plans(device_id,item_id);
CREATE INDEX mobile_v9_receipt_device ON mobile_v9_task_receipts(device_id,credential_generation);
ALTER TABLE mobile_v9_attempts ADD COLUMN inbox_assigned_at timestamptz NOT NULL DEFAULT clock_timestamp(), ADD COLUMN inbox_changed_at timestamptz NOT NULL DEFAULT clock_timestamp();
UPDATE mobile_v9_attempts a SET inbox_assigned_at=t.created_at FROM mobile_v9_task_items i JOIN mobile_v9_tasks t ON t.id=i.task_id WHERE i.id=a.item_id;
CREATE FUNCTION mobile_v9_inbox_changed() RETURNS trigger LANGUAGE plpgsql AS $$
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
  IF NEW.stopped=OLD.stopped AND NEW.cursor_position=OLD.cursor_position AND NEW.next_eligible_at=OLD.next_eligible_at AND NEW.confirmed_until=OLD.confirmed_until THEN RETURN NEW; END IF;
  FOR r IN SELECT DISTINCT p.device_id,NEW.confirmed_by AS actor_id FROM mobile_v9_task_items i JOIN mobile_v9_plans p ON p.item_id=i.id WHERE i.task_id=NEW.id ORDER BY p.device_id LOOP
   INSERT INTO mobile_v9_device_inbox_versions(device_id,revision) VALUES(r.device_id,1) ON CONFLICT(device_id) DO UPDATE SET revision=mobile_v9_device_inbox_versions.revision+1 RETURNING revision INTO v;
   INSERT INTO mobile_v9_task_wake_outbox VALUES(gen_random_uuid(),r.device_id,v,r.actor_id,clock_timestamp());
  END LOOP;
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER mobile_v9_attempt_inbox BEFORE INSERT OR UPDATE ON mobile_v9_attempts FOR EACH ROW EXECUTE FUNCTION mobile_v9_inbox_changed();
CREATE TRIGGER mobile_v9_task_inbox AFTER UPDATE ON mobile_v9_tasks FOR EACH ROW EXECUTE FUNCTION mobile_v9_inbox_changed();
