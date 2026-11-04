ALTER TABLE mobile_v9_task_drafts DROP COLUMN stage;
ALTER TABLE mobile_v9_send_policy_versions DROP COLUMN allowed_stages;
-- Frozen historical plan JSON is immutable and retains its original digest.
-- Current code neither projects nor interprets those historical phase annotations.
