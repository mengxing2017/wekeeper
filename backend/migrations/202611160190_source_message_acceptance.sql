-- Source messages are complete, independent texts, not versioned content projections.
-- Retain actual acceptance evidence, internal processing revisions and target summaries.
CREATE OR REPLACE FUNCTION source_receipt_transition() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF (NEW.status,NEW.processing_stage,NEW.reason_code)
  IS DISTINCT FROM (OLD.status,OLD.processing_stage,OLD.reason_code) THEN
  NEW.processing_revision=OLD.processing_revision+1;
  NEW.updated_at=clock_timestamp();
 END IF;
 RETURN NEW;
END $$;
ALTER TABLE source_receipts
 DROP COLUMN observed_sequence,
 DROP COLUMN canonical_receipt_id,
 DROP COLUMN keyword_terminal_rejected;
-- The durable handoff owns scheduling/retry state. Inbox only retains evidence and outcome.
DROP TRIGGER source_inbox_event_type ON source_inbox;
DROP FUNCTION source_inbox_event_type();
ALTER TABLE source_inbox
 DROP COLUMN event_type,
 DROP COLUMN reason_class,
 DROP COLUMN projected,
 DROP COLUMN failures,
 DROP COLUMN last_error,
 DROP COLUMN next_attempt_at,
 DROP COLUMN job_state;
