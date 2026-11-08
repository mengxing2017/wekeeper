-- Transport recovery never erases the last authenticated device heartbeat.
ALTER TABLE device_control_policy DROP CONSTRAINT device_control_policy_heartbeat_interval_seconds_check;
ALTER TABLE device_control_policy DROP CONSTRAINT device_control_policy_check;
ALTER TABLE device_control_policy DROP CONSTRAINT device_control_policy_check1;
ALTER TABLE device_control_policy
 ALTER COLUMN heartbeat_interval_seconds SET DEFAULT 60,
 ALTER COLUMN suspect_after_seconds SET DEFAULT 130,
 ALTER COLUMN offline_after_seconds SET DEFAULT 190;
-- Existing longer silence windows remain valid; normalize the old short defaults.
UPDATE device_control_policy SET heartbeat_interval_seconds=60,
 suspect_after_seconds=greatest(suspect_after_seconds,130),
 offline_after_seconds=greatest(offline_after_seconds,suspect_after_seconds+1,190);
ALTER TABLE device_control_policy
 ADD CHECK(heartbeat_interval_seconds BETWEEN 60 AND 300),
 ADD CHECK(suspect_after_seconds >= 2*heartbeat_interval_seconds+10),
 ADD CHECK(offline_after_seconds >= 3*heartbeat_interval_seconds+10 AND offline_after_seconds > suspect_after_seconds AND offline_after_seconds <= 1810);
-- Keep the first unconfirmed-contact deadline separate from the current
-- transport's connected_at; repeated opens cannot defer an initial timeout.
ALTER TABLE device_control_sessions ADD COLUMN presence_started_at timestamptz NOT NULL DEFAULT clock_timestamp();
UPDATE device_control_sessions SET presence_started_at=connected_at;
-- Retain IDs for the entire current session, so replaying a prior pulse cannot
-- extend presence. Opening a replacement session discards the superseded IDs.
CREATE TABLE device_control_pulse_receipts (
 device_id uuid NOT NULL REFERENCES execution_devices(id) ON DELETE CASCADE,
 connection_id uuid NOT NULL,
 heartbeat_id uuid NOT NULL,
 PRIMARY KEY(device_id,connection_id,heartbeat_id)
);
CREATE OR REPLACE VIEW device_control_presence AS
SELECT d.id,
 CASE WHEN NOT d.enabled THEN 'disabled'
 WHEN NOT EXISTS(SELECT 1 FROM device_credentials c WHERE c.device_id=d.id
   AND c.generation=d.credential_generation AND c.revoked_at IS NULL AND c.expires_at>clock_timestamp()) THEN 'unauthorized'
 WHEN s.device_id IS NULL OR s.credential_generation IS DISTINCT FROM d.credential_generation THEN 'offline'
 WHEN coalesce(s.last_heartbeat_at,s.presence_started_at)<=clock_timestamp()-make_interval(secs=>p.offline_after_seconds) THEN 'offline'
 WHEN s.last_heartbeat_at IS NULL THEN 'connecting'
 WHEN s.last_heartbeat_at<=clock_timestamp()-make_interval(secs=>p.suspect_after_seconds) THEN 'suspect'
 ELSE 'online' END AS state,
 s.last_heartbeat_at AS last_control_heartbeat_at, s.transport, s.readiness, s.block_reason,
 coalesce(s.desired_paused,false) AS desired_paused, coalesce(s.desired_revision,0) AS desired_revision,
 s.connected_at, s.last_disconnected_at, s.disconnect_reason,
 p.heartbeat_interval_seconds, p.suspect_after_seconds, p.offline_after_seconds,
 clock_timestamp() AS server_time,
 (SELECT max(c.expires_at) FROM device_credentials c WHERE c.device_id=d.id
   AND c.generation=d.credential_generation AND c.revoked_at IS NULL AND c.expires_at>clock_timestamp()) AS credential_expires_at,
 coalesce(s.connected,false) AS transport_connected
FROM execution_devices d LEFT JOIN device_control_sessions s ON s.device_id=d.id
CROSS JOIN device_control_policy p;
