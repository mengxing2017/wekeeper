-- These assertions run against the real view and repository mutations in a
-- disposable single-user PostgreSQL instance. No network/database URL is used.
BEGIN;
INSERT INTO execution_devices(id,name,enabled,credential_generation) VALUES('00000000-0000-4000-8000-000000000201','Control presence',true,1);
INSERT INTO device_credentials(token_hash,device_id,generation,expires_at) VALUES('control-presence-test','00000000-0000-4000-8000-000000000201',1,clock_timestamp()+interval '1 day');
__CONTROL_OPEN__;
DO $$ BEGIN
 IF (SELECT state FROM device_control_presence WHERE id='00000000-0000-4000-8000-000000000201') <> 'connecting' THEN RAISE EXCEPTION 'handshake is not heartbeat evidence'; END IF;
END $$;
DO $$ DECLARE result uuid; evidence_at timestamptz; BEGIN
 __CONTROL_PULSE__ INTO result;
 IF result IS NULL THEN RAISE EXCEPTION 'current pulse rejected'; END IF;
END $$;
DO $$ DECLARE age integer; expected text; actual text; candidate_ready boolean; BEGIN
 FOREACH age IN ARRAY ARRAY[35,70,129,131,181,189,191] LOOP
  UPDATE device_control_sessions SET last_heartbeat_at=clock_timestamp()-make_interval(secs=>age) WHERE device_id='00000000-0000-4000-8000-000000000201';
  expected:=CASE WHEN age<130 THEN 'online' WHEN age<190 THEN 'suspect' ELSE 'offline' END;
  SELECT state INTO actual FROM device_control_presence WHERE id='00000000-0000-4000-8000-000000000201';
  IF actual<>expected THEN RAISE EXCEPTION 'presence at %: expected %, got %',age,expected,actual; END IF;
  __CONTROL_CANDIDATE__ INTO candidate_ready;
  IF candidate_ready IS DISTINCT FROM (age<190) THEN RAISE EXCEPTION 'business candidate used a private heartbeat cutoff at %',age; END IF;
 END LOOP;
END $$;
UPDATE device_control_sessions SET last_heartbeat_at=clock_timestamp()-interval '70 seconds',connected=false WHERE device_id='00000000-0000-4000-8000-000000000201';
CREATE TEMP TABLE control_last_evidence AS SELECT last_heartbeat_at,presence_started_at FROM device_control_sessions WHERE device_id='00000000-0000-4000-8000-000000000201';
DO $$ BEGIN
 IF (SELECT state FROM device_control_presence WHERE id='00000000-0000-4000-8000-000000000201') <> 'online' THEN RAISE EXCEPTION 'transport close erased presence'; END IF;
 IF (SELECT transport_connected FROM device_control_presence WHERE id='00000000-0000-4000-8000-000000000201') THEN RAISE EXCEPTION 'transport close not displayed'; END IF;
END $$;
__CONTROL_OPEN__;
DO $$ BEGIN
 IF EXISTS(SELECT 1 FROM device_control_sessions s CROSS JOIN control_last_evidence e WHERE s.device_id='00000000-0000-4000-8000-000000000201' AND (s.last_heartbeat_at IS DISTINCT FROM e.last_heartbeat_at OR s.presence_started_at IS DISTINCT FROM e.presence_started_at)) THEN RAISE EXCEPTION 'reopen erased or renewed heartbeat evidence'; END IF;
END $$;
UPDATE device_control_sessions SET desired_paused=true WHERE device_id='00000000-0000-4000-8000-000000000201';
DO $$ DECLARE candidate_ready boolean; BEGIN
 __CONTROL_CANDIDATE__ INTO candidate_ready;
 IF candidate_ready OR (SELECT state FROM device_control_presence WHERE id='00000000-0000-4000-8000-000000000201')<>'online' THEN RAISE EXCEPTION 'pause confused with presence'; END IF;
END $$;
UPDATE device_control_sessions SET desired_paused=false WHERE device_id='00000000-0000-4000-8000-000000000201';
UPDATE device_control_sessions SET last_heartbeat_at=NULL,presence_started_at=clock_timestamp()-interval '191 seconds' WHERE device_id='00000000-0000-4000-8000-000000000201';
__CONTROL_OPEN__;
DO $$ DECLARE result uuid; evidence_at timestamptz; BEGIN
 IF (SELECT state FROM device_control_presence WHERE id='00000000-0000-4000-8000-000000000201') <> 'offline' THEN RAISE EXCEPTION 'reopen postponed initial timeout'; END IF;
 -- Fresh receipt restores an expired current session immediately.
 __CONTROL_PULSE__ INTO result;
 IF (SELECT state FROM device_control_presence WHERE id='00000000-0000-4000-8000-000000000201') <> 'online' THEN RAISE EXCEPTION 'valid heartbeat did not restore presence'; END IF;
 -- A duplicate receipt cannot extend its timestamp.
 SELECT last_heartbeat_at INTO evidence_at FROM device_control_sessions WHERE device_id='00000000-0000-4000-8000-000000000201';
 __CONTROL_DUPLICATE_PULSE__ INTO result;
 IF (SELECT last_heartbeat_at FROM device_control_sessions WHERE device_id='00000000-0000-4000-8000-000000000201') IS DISTINCT FROM evidence_at THEN RAISE EXCEPTION 'duplicate renewed presence'; END IF;
END $$;
DO $$ DECLARE inserted bigint; BEGIN
 __CONTROL_RECEIPT__;
 GET DIAGNOSTICS inserted = ROW_COUNT;
 IF inserted<>1 THEN RAISE EXCEPTION 'first heartbeat ID was not stored'; END IF;
 __CONTROL_RECEIPT__;
 GET DIAGNOSTICS inserted = ROW_COUNT;
 IF inserted<>0 THEN RAISE EXCEPTION 'duplicate heartbeat ID accepted as new'; END IF;
 __CONTROL_RECEIPT_NEWER__;
 GET DIAGNOSTICS inserted = ROW_COUNT;
 IF inserted<>1 THEN RAISE EXCEPTION 'new heartbeat did not replace latest'; END IF;
 __CONTROL_RECEIPT_OLDER__;
 GET DIAGNOSTICS inserted = ROW_COUNT;
 IF inserted<>0 THEN RAISE EXCEPTION 'old sequence renewed presence'; END IF;
 IF (SELECT count(*) FROM device_control_pulse_receipts WHERE device_id='00000000-0000-4000-8000-000000000201')<>1 THEN RAISE EXCEPTION 'heartbeat history retained'; END IF;
 IF (SELECT heartbeat_sequence FROM device_control_pulse_receipts WHERE device_id='00000000-0000-4000-8000-000000000201')<>2 THEN RAISE EXCEPTION 'latest heartbeat overwritten by replay'; END IF;
END $$;
UPDATE device_control_sessions SET last_heartbeat_at=clock_timestamp()-interval '191 seconds' WHERE device_id='00000000-0000-4000-8000-000000000201';
DO $$ DECLARE confirmed bigint; BEGIN
 __CONTROL_CONFIRM_OLD__;
 GET DIAGNOSTICS confirmed = ROW_COUNT;
 IF confirmed<>0 THEN RAISE EXCEPTION 'stale reply renewed heartbeat'; END IF;
 __CONTROL_CONFIRM__;
 GET DIAGNOSTICS confirmed = ROW_COUNT;
 IF confirmed<>1 THEN RAISE EXCEPTION 'successful latest WSS reply did not update heartbeat'; END IF;
 IF (SELECT state FROM device_control_presence WHERE id='00000000-0000-4000-8000-000000000201')<>'online' THEN RAISE EXCEPTION 'reply completion did not restore online'; END IF;
END $$;
UPDATE execution_devices SET credential_generation=2 WHERE id='00000000-0000-4000-8000-000000000201';
INSERT INTO device_credentials(token_hash,device_id,generation,expires_at) VALUES('control-presence-test-2','00000000-0000-4000-8000-000000000201',2,clock_timestamp()+interval '1 day');
__CONTROL_OPEN_GENERATION__;
DO $$ BEGIN
 IF (SELECT state FROM device_control_presence WHERE id='00000000-0000-4000-8000-000000000201') <> 'connecting' THEN RAISE EXCEPTION 'new identity reused old evidence'; END IF;
 BEGIN
  UPDATE device_control_policy SET heartbeat_interval_seconds=60,suspect_after_seconds=130,offline_after_seconds=189;
  RAISE EXCEPTION 'less than three cycles accepted';
 EXCEPTION WHEN check_violation THEN NULL; END;
END $$;
ROLLBACK;
