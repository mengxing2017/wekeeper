-- Persist retry state before the initial dependency reads, without freezing placeholder input.
ALTER TABLE orchestration_dispatches DROP CONSTRAINT orchestration_dispatches_state_check;
ALTER TABLE orchestration_dispatches ADD CONSTRAINT orchestration_dispatches_state_check
 CHECK(state IN ('pending','preparing','ready','delegated','failed','historical'));
CREATE OR REPLACE FUNCTION orchestration_dispatch_fixed() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.message_id<>OLD.message_id
 OR (NEW.plans IS DISTINCT FROM OLD.plans AND OLD.state<>'pending')
 OR ((NEW.content,NEW.decision) IS DISTINCT FROM (OLD.content,OLD.decision)
     AND NOT (OLD.state='pending' OR (OLD.state='preparing' AND NEW.state='ready')))
 OR (OLD.state IN ('delegated','failed','historical') AND NEW.state<>OLD.state)
 OR (OLD.state='ready' AND NEW.state IN ('pending','preparing'))
 OR (OLD.state='preparing' AND NEW.state='pending') THEN
  RAISE EXCEPTION 'frozen message input is immutable';
 END IF;
 RETURN NEW;
END $$;
