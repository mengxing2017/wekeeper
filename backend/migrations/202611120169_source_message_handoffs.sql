-- The source receipt and its pending handoff commit in the same transaction.
CREATE TABLE source_message_handoffs (
    receipt_id uuid PRIMARY KEY REFERENCES source_inbox(receipt_id),
    platform_id uuid NOT NULL REFERENCES source_callers(source_id),
    state text NOT NULL DEFAULT 'pending' CHECK (state IN ('pending','complete')),
    message_id uuid,
    claim_token uuid,
    claim_until timestamptz,
    attempts integer NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    next_attempt_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    last_error text,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    completed_at timestamptz,
    CHECK ((claim_token IS NULL) = (claim_until IS NULL)),
    CHECK ((state = 'complete') = (message_id IS NOT NULL)),
    CHECK ((state = 'complete') = (completed_at IS NOT NULL)),
    CHECK (state <> 'complete' OR claim_token IS NULL)
);
CREATE INDEX source_message_handoffs_due ON source_message_handoffs(next_attempt_at,receipt_id) WHERE state='pending';
CREATE FUNCTION source_message_handoff_enqueue() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    INSERT INTO source_message_handoffs(receipt_id,platform_id)
        SELECT NEW.receipt_id,source_id FROM source_receipts WHERE receipt_id=NEW.receipt_id;
    RETURN NEW;
END $$;
CREATE TRIGGER source_message_handoff_enqueue AFTER INSERT ON source_inbox
    FOR EACH ROW EXECUTE FUNCTION source_message_handoff_enqueue();
