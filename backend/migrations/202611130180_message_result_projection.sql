ALTER TABLE orchestration_delegates ADD COLUMN executed_by text NOT NULL DEFAULT 'message-orchestration';
CREATE TABLE orchestration_result_queue(message_id uuid PRIMARY KEY REFERENCES orchestration_messages(id));
CREATE FUNCTION orchestration_result_changed() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE task uuid;
BEGIN
 IF TG_TABLE_NAME='orchestration_delegates' THEN
  INSERT INTO orchestration_result_queue VALUES(NEW.message_id) ON CONFLICT DO NOTHING;
  RETURN NEW;
 ELSIF TG_TABLE_NAME='mobile_v9_attempts' THEN
  SELECT task_id INTO task FROM mobile_v9_task_items WHERE id=NEW.item_id;
 ELSIF TG_TABLE_NAME='mobile_v9_dispatch_state' THEN
  SELECT task_id INTO task FROM mobile_v9_task_items WHERE id=NEW.item_id;
 ELSIF TG_TABLE_NAME IN ('mobile_v9_tasks','broadcast_tasks') THEN task=NEW.id;
 ELSE task=NEW.task_id;
 END IF;
 INSERT INTO orchestration_result_queue SELECT message_id FROM orchestration_delegates WHERE task_id=task ON CONFLICT DO NOTHING;
 RETURN NEW;
END $$;
CREATE TRIGGER message_delegate_created AFTER INSERT ON orchestration_delegates FOR EACH ROW EXECUTE FUNCTION orchestration_result_changed();
CREATE TRIGGER message_mobile_result AFTER INSERT OR UPDATE ON mobile_v9_attempts FOR EACH ROW EXECUTE FUNCTION orchestration_result_changed();
CREATE TRIGGER message_mobile_dispatch AFTER INSERT OR UPDATE ON mobile_v9_dispatch_state FOR EACH ROW EXECUTE FUNCTION orchestration_result_changed();
CREATE TRIGGER message_mobile_task AFTER UPDATE ON mobile_v9_tasks FOR EACH ROW EXECUTE FUNCTION orchestration_result_changed();
CREATE TRIGGER message_wecom_result AFTER UPDATE ON broadcast_targets FOR EACH ROW EXECUTE FUNCTION orchestration_result_changed();
CREATE TRIGGER message_wecom_batch AFTER UPDATE ON broadcast_batches FOR EACH ROW EXECUTE FUNCTION orchestration_result_changed();
CREATE TRIGGER message_wecom_task AFTER UPDATE ON broadcast_tasks FOR EACH ROW EXECUTE FUNCTION orchestration_result_changed();
INSERT INTO orchestration_result_queue SELECT DISTINCT message_id FROM orchestration_delegates;
-- Business input is immutable after intake/freeze. Retries only change retry state.
CREATE FUNCTION orchestration_dispatch_fixed() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.message_id<>OLD.message_id OR NEW.plans IS DISTINCT FROM OLD.plans
 OR ((NEW.content,NEW.decision) IS DISTINCT FROM (OLD.content,OLD.decision) AND NOT (OLD.state='preparing' AND NEW.state='ready'))
 OR (OLD.state IN ('delegated','failed','historical') AND NEW.state<>OLD.state)
 OR (OLD.state='ready' AND NEW.state='preparing') THEN
  RAISE EXCEPTION 'frozen message input is immutable';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER orchestration_dispatch_fixed BEFORE UPDATE ON orchestration_dispatches FOR EACH ROW EXECUTE FUNCTION orchestration_dispatch_fixed();
CREATE TRIGGER ad_message_decision_immutable BEFORE UPDATE OR DELETE ON ad_message_decisions FOR EACH ROW EXECUTE FUNCTION orchestration_message_immutable();
