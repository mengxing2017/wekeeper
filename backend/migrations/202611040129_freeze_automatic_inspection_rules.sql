-- Freeze real existing rules; never replace them with contract fixtures.
ALTER TABLE mobile_v9_send_inspections ADD COLUMN flow jsonb;
ALTER TABLE mobile_v9_send_inspections ADD COLUMN flow_digest text;
UPDATE mobile_v9_send_inspections i
 SET flow=r.flow,flow_digest=r.content_digest
 FROM mobile_v9_adapter_releases r WHERE i.release_id=r.id AND i.purpose='automatic';
-- Unclaimed missing configuration is not an executable waiting queue.
UPDATE mobile_v9_send_inspections SET state='stale',finished_at=clock_timestamp(),termination_reason='configuration_invalid'
 WHERE purpose='automatic' AND state='pending' AND lease_fence IS NULL AND (flow IS NULL OR flow_digest IS NULL);
-- Claimed inspections retain their original challenge, dependencies, digest and fence.
