-- Heartbeat evidence is current state, never an append-only history.
-- Retain the session's latest timestamp; discard the historical ID collection.
DELETE FROM device_control_pulse_receipts;
ALTER TABLE device_control_pulse_receipts DROP CONSTRAINT device_control_pulse_receipts_pkey;
ALTER TABLE device_control_pulse_receipts ADD PRIMARY KEY(device_id);
ALTER TABLE device_control_pulse_receipts ADD COLUMN heartbeat_sequence bigint NOT NULL DEFAULT 0 CHECK(heartbeat_sequence>=0);
