-- Rollback-only synthetic regression, executed in a temporary single-user database.
BEGIN;
DO $$ BEGIN
 IF NOT EXISTS(SELECT 1 FROM mobile_workspaces WHERE id='20000000-0000-4000-8000-000000000001' AND name='Legacy school') OR NOT EXISTS(SELECT 1 FROM mobile_workspace_members WHERE workspace_id='20000000-0000-4000-8000-000000000001' AND account_id='20000000-0000-4000-8000-000000000002' AND role='admin') OR NOT EXISTS(SELECT 1 FROM mobile_targets WHERE id='20000000-0000-4000-8000-000000000003' AND search_text='Legacy target') THEN RAISE EXCEPTION 'migration lost legacy scope, authorization or target'; END IF;
END $$;
UPDATE schools SET active=false WHERE id='20000000-0000-4000-8000-000000000001';
DELETE FROM school_memberships WHERE school_id='20000000-0000-4000-8000-000000000001';
DO $$ BEGIN
 IF NOT EXISTS(SELECT 1 FROM mobile_workspaces WHERE id='20000000-0000-4000-8000-000000000001' AND active) OR NOT EXISTS(SELECT 1 FROM mobile_workspace_members WHERE workspace_id='20000000-0000-4000-8000-000000000001') THEN RAISE EXCEPTION 'school change affected independent mobile authorization'; END IF;
END $$;
INSERT INTO accounts(id,username,display_name) VALUES('10000000-0000-4000-8000-000000000001','mobile-only','Mobile only');
INSERT INTO mobile_workspaces(id,name) VALUES('10000000-0000-4000-8000-000000000002','No school'),('10000000-0000-4000-8000-000000000003','Other workspace');
INSERT INTO mobile_workspace_members(workspace_id,account_id,role) VALUES('10000000-0000-4000-8000-000000000002','10000000-0000-4000-8000-000000000001','operator');
INSERT INTO mobile_targets(id,school_id,search_text,created_by,updated_by) VALUES('10000000-0000-4000-8000-000000000004','10000000-0000-4000-8000-000000000002','Independent group','10000000-0000-4000-8000-000000000001','10000000-0000-4000-8000-000000000001');
INSERT INTO execution_devices(id,name,enabled) VALUES('10000000-0000-4000-8000-000000000005','Independent phone',true);
INSERT INTO mobile_device_target_bindings(id,school_id,device_id,target_id,created_by,updated_by) VALUES('10000000-0000-4000-8000-000000000006','10000000-0000-4000-8000-000000000002','10000000-0000-4000-8000-000000000005','10000000-0000-4000-8000-000000000004','10000000-0000-4000-8000-000000000001','10000000-0000-4000-8000-000000000001');
DO $$ BEGIN
 IF EXISTS(SELECT 1 FROM schools WHERE id='10000000-0000-4000-8000-000000000002') THEN RAISE EXCEPTION 'independent workspace fabricated school'; END IF;
 IF EXISTS(SELECT 1 FROM pg_constraint c JOIN pg_class r ON r.oid=c.conrelid WHERE c.contype='f' AND c.confrelid='schools'::regclass AND (r.relname LIKE 'mobile_%' OR r.relname LIKE 'shared_mobile_%')) THEN RAISE EXCEPTION 'mobile foreign key still references school'; END IF;
 IF NOT EXISTS(SELECT a.id FROM accounts a JOIN mobile_workspaces s ON s.id='10000000-0000-4000-8000-000000000002' WHERE a.id='10000000-0000-4000-8000-000000000001' AND a.active AND s.active AND (a.platform_admin OR EXISTS(SELECT 1 FROM mobile_workspace_members m WHERE m.account_id=a.id AND m.workspace_id=s.id AND (m.role='admin' OR m.role='operator')))) THEN RAISE EXCEPTION 'mobile operator needs school membership'; END IF;
 IF EXISTS(SELECT 1 FROM mobile_workspace_members WHERE account_id='10000000-0000-4000-8000-000000000001' AND workspace_id='10000000-0000-4000-8000-000000000003') THEN RAISE EXCEPTION 'cross workspace permission'; END IF;
 BEGIN
  UPDATE mobile_device_target_bindings SET school_id='10000000-0000-4000-8000-000000000003' WHERE id='10000000-0000-4000-8000-000000000006';
  RAISE EXCEPTION 'cross workspace binding accepted';
 EXCEPTION WHEN foreign_key_violation THEN NULL; END;
END $$;
DELETE FROM mobile_workspace_members WHERE account_id='10000000-0000-4000-8000-000000000001';
DO $$ BEGIN
 IF EXISTS(SELECT a.id FROM accounts a JOIN mobile_workspaces s ON s.id='10000000-0000-4000-8000-000000000002' WHERE a.id='10000000-0000-4000-8000-000000000001' AND a.active AND s.active AND (a.platform_admin OR EXISTS(SELECT 1 FROM mobile_workspace_members m WHERE m.account_id=a.id AND m.workspace_id=s.id AND (m.role='admin' OR m.role='operator')))) THEN RAISE EXCEPTION 'revoked permission remained effective'; END IF;
END $$;
ROLLBACK;
