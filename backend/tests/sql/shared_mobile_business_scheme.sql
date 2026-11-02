-- Synthetic transaction only. Does not create acceptance or grant real execution.
BEGIN;
INSERT INTO accounts(id,username,display_name,platform_admin) VALUES('00000000-0000-4000-8000-000000000001','scheme-test','Synthetic',true);
INSERT INTO mobile_v9_send_policy_versions(id,revision,mode,limits,created_by,reason,timezone,allowed_windows,allowed_stages,confirmation_seconds,inspection_seconds,retry_seconds,retry_limit)
VALUES('00000000-0000-4000-8000-000000000002',1,'disabled','{"global":{"interval_seconds":5,"window_seconds":3600,"max_calls":120,"daily_max_calls":1000},"school":{"interval_seconds":10,"window_seconds":3600,"max_calls":60,"daily_max_calls":300},"device":{"interval_seconds":30,"window_seconds":3600,"max_calls":30,"daily_max_calls":150},"target":{"interval_seconds":60,"window_seconds":3600,"max_calls":10,"daily_max_calls":30}}','00000000-0000-4000-8000-000000000001','Synthetic','Asia/Shanghai','[{"weekdays":[1,2,3,4,5],"start_minute":540,"end_minute":1080}]','["production_allowlist"]',86400,3600,300,3);
DO $$
DECLARE p record; snapshot jsonb; changed jsonb; rejected boolean:=false;
BEGIN
 SELECT * INTO p FROM mobile_v9_send_policy_versions ORDER BY revision DESC LIMIT 1;
 snapshot:=jsonb_build_object('id',p.id,'limits',p.limits,'timezone',p.timezone,'allowed_windows',p.allowed_windows,'daily_max_calls',p.limits->'school'->'daily_max_calls','interval_seconds',p.limits->'target'->'interval_seconds');
 changed:=jsonb_set(snapshot,'{limits,global,daily_max_calls}','999');
 IF (snapshot-'id')=(changed-'id') THEN RAISE EXCEPTION 'Global quota change must invalidate old scheme'; END IF;
 changed:=jsonb_set(snapshot,'{allowed_windows,0,start_minute}','600');
 IF (snapshot-'id')=(changed-'id') THEN RAISE EXCEPTION 'Schedule change must invalidate old scheme'; END IF;
 changed:=jsonb_set(snapshot,'{id}','"another-version"');
 IF (snapshot-'id')<>(changed-'id') THEN RAISE EXCEPTION 'Same business scheme must not require reapproval merely for a new version'; END IF;
 IF EXISTS(SELECT 1 FROM information_schema.columns WHERE table_schema=current_schema() AND table_name='mobile_capability_deliveries' AND column_name='policy') THEN RAISE EXCEPTION 'Delivery still owns policy'; END IF;
 BEGIN
  UPDATE mobile_v9_send_policy_versions SET reason='overwrite' WHERE id=p.id;
 EXCEPTION WHEN OTHERS THEN rejected:=true;
 END;
 IF NOT rejected THEN RAISE EXCEPTION 'Business scheme audit must be immutable'; END IF;
END $$;
ROLLBACK;
