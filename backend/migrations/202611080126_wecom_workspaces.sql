-- Official distribution owns its scopes. Never synchronize subsequent school changes.
CREATE TABLE wecom_workspaces (
 id uuid PRIMARY KEY, name text NOT NULL, active boolean NOT NULL DEFAULT true,
 created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE wecom_workspace_members (
 -- Legacy storage name retained to preserve existing task and policy references.
 school_id uuid NOT NULL REFERENCES wecom_workspaces(id), account_id uuid NOT NULL REFERENCES accounts(id),
 role text NOT NULL CHECK(role IN ('member','operator','admin','revoked')), PRIMARY KEY(school_id,account_id)
);
INSERT INTO wecom_workspaces(id,name,active) SELECT id,name,active FROM schools;
INSERT INTO wecom_workspace_members(school_id,account_id,role) SELECT school_id,account_id,role FROM school_memberships;
INSERT INTO wecom_workspaces(id,name) VALUES(gen_random_uuid(),'企业微信分发');
-- Repoint live distribution/media/audit relationships; keep all IDs and evidence intact.
DO $$
DECLARE c record;
BEGIN
 FOR c IN SELECT con.conrelid::regclass AS tab,con.conname,pg_get_constraintdef(con.oid) AS definition
 FROM pg_constraint con JOIN pg_class rel ON rel.oid=con.conrelid
 JOIN pg_attribute att ON att.attrelid=con.conrelid AND att.attnum=con.conkey[1]
 WHERE con.contype='f' AND con.confrelid='schools'::regclass AND
 (rel.relname ~ '^(wecom_|broadcast_|distribution_|official_|product_)' OR
 rel.relname IN ('templates','template_versions','audit_events','media_assets','media_access_tokens','delivery_usage'))
 LOOP
  EXECUTE format('ALTER TABLE %s DROP CONSTRAINT %I',c.tab,c.conname);
  EXECUTE format('ALTER TABLE %s ADD CONSTRAINT %I %s',c.tab,c.conname,replace(c.definition,'REFERENCES schools(id)','REFERENCES wecom_workspaces(id)'));
 END LOOP;
END $$;
