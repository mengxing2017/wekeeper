-- One policy and one projection for admin lists, control details and school assignments.
CREATE TABLE device_control_policy (
 singleton boolean PRIMARY KEY DEFAULT true CHECK(singleton),
 heartbeat_interval_seconds smallint NOT NULL DEFAULT 20 CHECK(heartbeat_interval_seconds BETWEEN 10 AND 60),
 suspect_after_seconds smallint NOT NULL DEFAULT 45,
 offline_after_seconds smallint NOT NULL DEFAULT 90,
 CHECK(suspect_after_seconds > heartbeat_interval_seconds + 10),
 CHECK(offline_after_seconds > suspect_after_seconds AND offline_after_seconds BETWEEN 45 AND 300)
);
INSERT INTO device_control_policy(singleton) VALUES(true);
ALTER TABLE device_control_sessions
 ALTER COLUMN last_heartbeat_at DROP NOT NULL,
 ALTER COLUMN last_heartbeat_at DROP DEFAULT,
 ADD COLUMN credential_generation bigint,
 ADD COLUMN connected_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 ADD COLUMN last_disconnected_at timestamptz,
 ADD COLUMN disconnect_reason text;
-- No historical handshake may masquerade as a confirmed heartbeat after upgrade.
UPDATE device_control_sessions SET last_heartbeat_at=NULL,connected=false,
 last_disconnected_at=clock_timestamp(),disconnect_reason='presence_upgrade';
CREATE VIEW device_control_presence AS
SELECT d.id,
 CASE WHEN NOT d.enabled THEN 'disabled'
 WHEN NOT EXISTS(SELECT 1 FROM device_credentials c WHERE c.device_id=d.id
   AND c.generation=d.credential_generation AND c.revoked_at IS NULL AND c.expires_at>clock_timestamp()) THEN 'unauthorized'
 WHEN s.device_id IS NULL OR NOT s.connected OR s.credential_generation IS DISTINCT FROM d.credential_generation THEN 'offline'
 WHEN coalesce(s.last_heartbeat_at,s.connected_at)<=clock_timestamp()-make_interval(secs=>p.offline_after_seconds) THEN 'offline'
 WHEN s.last_heartbeat_at IS NULL THEN 'connecting'
 WHEN s.last_heartbeat_at<=clock_timestamp()-make_interval(secs=>p.suspect_after_seconds) THEN 'suspect'
 ELSE 'online' END AS state,
 s.last_heartbeat_at AS last_control_heartbeat_at, s.transport, s.readiness, s.block_reason,
 coalesce(s.desired_paused,false) AS desired_paused, coalesce(s.desired_revision,0) AS desired_revision,
 s.connected_at, s.last_disconnected_at, s.disconnect_reason,
 p.heartbeat_interval_seconds, p.suspect_after_seconds, p.offline_after_seconds,
 clock_timestamp() AS server_time,
 (SELECT max(c.expires_at) FROM device_credentials c WHERE c.device_id=d.id
   AND c.generation=d.credential_generation AND c.revoked_at IS NULL AND c.expires_at>clock_timestamp()) AS credential_expires_at
FROM execution_devices d LEFT JOIN device_control_sessions s ON s.device_id=d.id
CROSS JOIN device_control_policy p;
