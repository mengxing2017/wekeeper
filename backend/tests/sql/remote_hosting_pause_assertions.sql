DO $$ DECLARE obsolete_kind text; BEGIN
 IF NOT EXISTS(SELECT 1 FROM device_control_sessions WHERE device_id='00000000-0000-4000-8000-000000000301' AND NOT desired_paused AND desired_revision=8) THEN RAISE EXCEPTION 'historical remote pause not cleared'; END IF;
 IF (SELECT count(*) FROM device_control_commands WHERE device_id='00000000-0000-4000-8000-000000000301' AND state='expired' AND result_code='remote_hosting_control_retired')<>6 THEN RAISE EXCEPTION 'obsolete pending commands not retired'; END IF;
 IF (SELECT count(*) FROM device_control_commands WHERE device_id='00000000-0000-4000-8000-000000000301' AND state IN ('completed','rejected'))<>4 THEN RAISE EXCEPTION 'terminal history changed'; END IF;
 IF NOT EXISTS(SELECT 1 FROM device_control_commands WHERE device_id='00000000-0000-4000-8000-000000000301' AND kind='request_status' AND state='queued') THEN RAISE EXCEPTION 'status request removed'; END IF;
 FOREACH obsolete_kind IN ARRAY ARRAY['pause_execution','resume_execution'] LOOP
  BEGIN
   INSERT INTO device_control_commands(id,device_id,kind) VALUES(gen_random_uuid(),'00000000-0000-4000-8000-000000000301',obsolete_kind);
   RAISE EXCEPTION 'obsolete command accepted';
  EXCEPTION WHEN check_violation THEN NULL; END;
 END LOOP;
END $$;
