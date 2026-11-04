-- UI preparation belongs to the device after claiming a business task.
-- Leave leased work and physical owners intact for legitimate finalization/recovery.
UPDATE mobile_v9_send_inspections
SET state='stale', termination_reason='device_owned_execution', finished_at=clock_timestamp()
WHERE purpose='automatic' AND state='pending';
