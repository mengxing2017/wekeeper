-- Remote commands cannot change the phone owner's local hosting intent.
-- Preserve terminal command history while retiring pending obsolete requests.
UPDATE device_control_commands
SET state='expired', completed_at=coalesce(completed_at,clock_timestamp()),
    result_code='remote_hosting_control_retired'
WHERE kind IN ('pause_execution','resume_execution')
  AND state IN ('queued','delivered','accepted');

UPDATE device_control_sessions
SET desired_paused=false, desired_revision=desired_revision+1
WHERE desired_paused;

-- Wire fields remain for existing device clients, permanently neutral.
ALTER TABLE device_control_sessions ADD CONSTRAINT remote_hosting_pause_retired
CHECK (NOT desired_paused);
-- NOT VALID preserves historical rows; new obsolete commands are rejected.
ALTER TABLE device_control_commands ADD CONSTRAINT remote_hosting_commands_retired
CHECK (kind NOT IN ('pause_execution','resume_execution')) NOT VALID;
