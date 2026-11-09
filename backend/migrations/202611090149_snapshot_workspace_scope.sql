-- Shared immutable snapshots belong to the workspace of their actual channel.
-- The earlier single FK to wecom_workspaces incorrectly rejected new mobile scopes.
ALTER TABLE distribution_message_snapshots DROP CONSTRAINT distribution_message_snapshots_school_id_fkey;
CREATE FUNCTION distribution_snapshot_workspace_scope() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.channel='mobile' THEN
  PERFORM id FROM mobile_workspaces WHERE id=NEW.school_id FOR KEY SHARE;
 ELSE
  PERFORM id FROM wecom_workspaces WHERE id=NEW.school_id FOR KEY SHARE;
 END IF;
 IF NOT FOUND THEN RAISE EXCEPTION 'snapshot workspace missing' USING ERRCODE='23503'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER distribution_snapshot_workspace_scope BEFORE INSERT ON distribution_message_snapshots
 FOR EACH ROW EXECUTE FUNCTION distribution_snapshot_workspace_scope();
