-- New messages use business-unit advertising decisions, never route-pool reservations.
DROP TRIGGER advertisement_pool_queue_received ON source_inbox;
DROP FUNCTION advertisement_pool_queue_received();
UPDATE advertisement_pool_queue SET state='done',reservation_expiry=NULL,updated_at=clock_timestamp();
-- Former workspace gates must not leave accepted messages parked indefinitely.
UPDATE source_inbox SET state='received',job_state='ready',next_attempt_at=clock_timestamp(),last_error=NULL
WHERE state='waiting' AND last_error IN ('workspace_authorization_revoked','ad_queue_ownership_lost','ad_queue_head_changed');
