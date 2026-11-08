INSERT INTO execution_devices(id,name) VALUES('00000000-0000-4000-8000-000000000301','Retired remote hosting control');
INSERT INTO device_control_sessions(device_id,connection_id,transport,desired_paused,desired_revision)
VALUES('00000000-0000-4000-8000-000000000301',gen_random_uuid(),'https',true,7);
INSERT INTO device_control_commands(id,device_id,kind,state)
SELECT gen_random_uuid(),'00000000-0000-4000-8000-000000000301',kind,state
FROM unnest(ARRAY['pause_execution','resume_execution']) kind,
     unnest(ARRAY['queued','delivered','accepted','completed','rejected']) state;
INSERT INTO device_control_commands(id,device_id,kind)
VALUES(gen_random_uuid(),'00000000-0000-4000-8000-000000000301','request_status');
