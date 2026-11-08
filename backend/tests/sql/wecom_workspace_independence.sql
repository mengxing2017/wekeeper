BEGIN;
DO $$ BEGIN
 IF NOT EXISTS(SELECT 1 FROM broadcast_tasks t JOIN broadcast_batches b ON b.task_id=t.id JOIN broadcast_targets x ON x.batch_id=b.id JOIN wecom_groups g ON g.id=x.group_id WHERE t.id='20000000-0000-4000-8000-000000000005' AND t.content='Legacy frozen content' AND t.content_hash='legacy-hash' AND b.msgid='legacy-msgid' AND x.official_status=1 AND g.chat_id='legacy-chat') THEN RAISE EXCEPTION 'migration changed official task snapshot, group or result'; END IF;
 IF NOT EXISTS(SELECT 1 FROM wecom_workspaces WHERE id='20000000-0000-4000-8000-000000000001' AND name='Legacy school') OR NOT EXISTS(SELECT 1 FROM wecom_workspace_members WHERE school_id='20000000-0000-4000-8000-000000000001' AND account_id='20000000-0000-4000-8000-000000000002' AND role='admin') THEN RAISE EXCEPTION 'wecom migration lost original scope or authorization'; END IF;
 IF EXISTS(SELECT 1 FROM pg_constraint c JOIN pg_class r ON r.oid=c.conrelid WHERE c.contype='f' AND c.confrelid='schools'::regclass AND (r.relname ~ '^(wecom_|broadcast_|distribution_|official_|product_)' OR r.relname IN ('templates','template_versions','audit_events','media_assets','media_access_tokens','delivery_usage'))) THEN RAISE EXCEPTION 'live wecom foreign key still references schools'; END IF;
END $$;
UPDATE schools SET active=false,name='Retired' WHERE id='20000000-0000-4000-8000-000000000001';
DELETE FROM school_memberships WHERE school_id='20000000-0000-4000-8000-000000000001';
DO $$ BEGIN
 IF NOT EXISTS(SELECT 1 FROM wecom_workspaces WHERE id='20000000-0000-4000-8000-000000000001' AND active AND name='Legacy school') OR NOT EXISTS(SELECT 1 FROM wecom_workspace_members WHERE school_id='20000000-0000-4000-8000-000000000001' AND role='admin') THEN RAISE EXCEPTION 'school changes affected wecom authorization'; END IF;
END $$;
INSERT INTO accounts(id,username,display_name) VALUES('30000000-0000-4000-8000-000000000001','wecom-only','Wecom only');
INSERT INTO wecom_workspaces(id,name) VALUES('30000000-0000-4000-8000-000000000002','Independent wecom');
INSERT INTO wecom_workspace_members(school_id,account_id,role) VALUES('30000000-0000-4000-8000-000000000002','30000000-0000-4000-8000-000000000001','operator');
INSERT INTO wecom_groups(id,school_id,corp_id,chat_id,name) VALUES('30000000-0000-4000-8000-000000000003','30000000-0000-4000-8000-000000000002','test','independent','Independent group');
DO $$ BEGIN
 IF EXISTS(SELECT 1 FROM schools WHERE id='30000000-0000-4000-8000-000000000002') THEN RAISE EXCEPTION 'independent scope created a school'; END IF;
 IF EXISTS(SELECT 1 FROM mobile_workspace_members WHERE account_id='30000000-0000-4000-8000-000000000001') THEN RAISE EXCEPTION 'wecom grant leaked to mobile'; END IF;
 IF NOT EXISTS(SELECT a.id FROM accounts a JOIN wecom_workspaces s ON s.id='30000000-0000-4000-8000-000000000002' WHERE a.id='30000000-0000-4000-8000-000000000001' AND a.active AND s.active AND EXISTS(SELECT 1 FROM wecom_workspace_members m WHERE m.school_id=s.id AND m.account_id=a.id AND m.role IN ('admin','operator'))) THEN RAISE EXCEPTION 'wecom operator requires school'; END IF;
END $$;
DELETE FROM wecom_workspace_members WHERE account_id='30000000-0000-4000-8000-000000000001';
DO $$ BEGIN
 IF EXISTS(SELECT 1 FROM wecom_workspace_members WHERE account_id='30000000-0000-4000-8000-000000000001') THEN RAISE EXCEPTION 'revoked wecom authorization remains'; END IF;
END $$;
ROLLBACK;
