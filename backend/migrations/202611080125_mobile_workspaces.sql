-- Independent mobile business boundary. Existing IDs and signed snapshots stay intact.
CREATE TABLE mobile_workspaces (
 id uuid PRIMARY KEY, name text NOT NULL CHECK(char_length(name) BETWEEN 1 AND 120),
 active boolean NOT NULL DEFAULT true, created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE mobile_workspace_members (
 workspace_id uuid NOT NULL REFERENCES mobile_workspaces(id),
 account_id uuid NOT NULL REFERENCES accounts(id), role text NOT NULL CHECK(role IN ('admin','operator')),
 PRIMARY KEY(workspace_id,account_id)
);
-- One-time snapshot only: future school changes have no effect on mobile authorization.
INSERT INTO mobile_workspaces(id,name,active) SELECT id,name,active FROM schools;
INSERT INTO mobile_workspace_members(workspace_id,account_id,role)
 SELECT school_id,account_id,role FROM school_memberships WHERE role IN ('admin','operator');
INSERT INTO mobile_workspaces(id,name) VALUES(gen_random_uuid(),'移动分发');
-- Keep school_id as a legacy storage/wire name; it now references a mobile workspace.
-- Do not rewrite immutable evidence or historical task payloads.
DO $$
DECLARE c record;
BEGIN
 FOR c IN SELECT con.conrelid::regclass AS tab,con.conname,att.attname
 FROM pg_constraint con JOIN pg_class rel ON rel.oid=con.conrelid
 JOIN pg_attribute att ON att.attrelid=con.conrelid AND att.attnum=con.conkey[1]
 WHERE con.contype='f' AND con.confrelid='schools'::regclass
 AND (rel.relname LIKE 'mobile_%' OR rel.relname LIKE 'shared_mobile_%')
 LOOP
  EXECUTE format('ALTER TABLE %s DROP CONSTRAINT %I',c.tab,c.conname);
  EXECUTE format('ALTER TABLE %s ADD CONSTRAINT %I FOREIGN KEY (%I) REFERENCES mobile_workspaces(id)',c.tab,c.conname,c.attname);
 END LOOP;
END $$;
