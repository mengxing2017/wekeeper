-- Rollback-only regression for a single business group name and budget lineage.
BEGIN;
INSERT INTO accounts(id,username,display_name,platform_admin) VALUES('10000000-0000-4000-8000-000000000001','locator-version-test','Synthetic',true);
INSERT INTO mobile_workspaces(id,name) VALUES('10000000-0000-4000-8000-000000000002','Synthetic locator school');
INSERT INTO execution_devices(id,name) VALUES('10000000-0000-4000-8000-000000000003','Phone A'),('10000000-0000-4000-8000-000000000004','Phone B');
INSERT INTO mobile_targets(id,school_id,search_text,created_by,updated_by) VALUES('10000000-0000-4000-8000-000000000005','10000000-0000-4000-8000-000000000002','业务群名','10000000-0000-4000-8000-000000000001','10000000-0000-4000-8000-000000000001');
INSERT INTO mobile_device_target_bindings(id,school_id,device_id,target_id,created_by,updated_by) SELECT id,'10000000-0000-4000-8000-000000000002',device,'10000000-0000-4000-8000-000000000005','10000000-0000-4000-8000-000000000001','10000000-0000-4000-8000-000000000001' FROM (VALUES ('10000000-0000-4000-8000-000000000006'::uuid,'10000000-0000-4000-8000-000000000003'::uuid),('10000000-0000-4000-8000-000000000007'::uuid,'10000000-0000-4000-8000-000000000004'::uuid)) v(id,device);
CREATE TEMP TABLE original_locators ON COMMIT DROP AS SELECT b.id,t.search_text,t.locator_revision FROM mobile_device_target_bindings b JOIN mobile_targets t ON t.id=b.target_id WHERE b.school_id='10000000-0000-4000-8000-000000000002';
DO $$ BEGIN
 IF (SELECT count(DISTINCT search_text) FROM original_locators)<>1 THEN RAISE EXCEPTION 'Phones must share one business group name'; END IF;
 UPDATE mobile_targets SET search_text='业务群新名',locator_revision=locator_revision+1 WHERE id='10000000-0000-4000-8000-000000000005';
 IF EXISTS(SELECT 1 FROM original_locators o JOIN mobile_device_target_bindings b ON b.id=o.id JOIN mobile_targets t ON t.id=b.target_id WHERE o.search_text=t.search_text OR o.locator_revision=t.locator_revision) THEN RAISE EXCEPTION 'Business group rename failed to invalidate old target'; END IF;
END $$;
INSERT INTO mobile_usages(id,school_id,actor_id,approval,state,body,confirmed_until,expires_at,budget_root) VALUES('10000000-0000-4000-8000-000000000008','10000000-0000-4000-8000-000000000002','10000000-0000-4000-8000-000000000001','approved','paused','{}',clock_timestamp()+interval '1 day',clock_timestamp()+interval '10 days','10000000-0000-4000-8000-000000000008');
INSERT INTO mobile_usages(id,school_id,actor_id,approval,state,body,confirmed_until,expires_at,budget_root,supersedes) SELECT '10000000-0000-4000-8000-000000000009',school_id,actor_id,'pending',state,'{}',clock_timestamp()+interval '1 day',clock_timestamp()+interval '40 days',budget_root,id FROM mobile_usages WHERE id='10000000-0000-4000-8000-000000000008';
DO $$ BEGIN
 IF (SELECT count(DISTINCT budget_root) FROM mobile_usages WHERE school_id='10000000-0000-4000-8000-000000000002')<>1 THEN RAISE EXCEPTION 'Renewal reset business family'; END IF;
 IF NOT EXISTS(SELECT 1 FROM mobile_usages WHERE supersedes='10000000-0000-4000-8000-000000000008' AND state='paused') THEN RAISE EXCEPTION 'Paused renewal resumed implicitly'; END IF;
END $$;
ROLLBACK;
