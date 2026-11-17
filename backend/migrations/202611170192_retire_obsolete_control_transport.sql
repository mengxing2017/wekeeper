-- Prerequisite for 202611170193: CHECK constraints validate terminal rows too.
-- Retire obsolete transport state before direct delivery tightens its constraints.
-- Task snapshots, execution grants, results and audit evidence are untouched.
DELETE FROM device_control_commands
WHERE kind IN ('task_available','pause_execution','resume_execution');

-- HTTPS presence cannot represent an authenticated WSS connection. Discard it
-- rather than relabel it as WSS; the next real WSS connection recreates presence.
DELETE FROM device_control_sessions WHERE transport='https';
