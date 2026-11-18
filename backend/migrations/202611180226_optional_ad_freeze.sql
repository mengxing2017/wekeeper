-- One authoritative message decision; no legacy business-data conversion.
DROP TABLE ad_message_decisions;
ALTER TABLE orchestration_dispatches ADD COLUMN content_frozen_at timestamptz;
ALTER TABLE orchestration_dispatches ADD CONSTRAINT orchestration_content_frozen CHECK (
    (state NOT IN ('ready','delegated') OR content_frozen_at IS NOT NULL)
    AND (content_frozen_at IS NULL OR (
        decision->>'content' IS NOT NULL
        AND decision->>'content'=content
        AND decision->>'reason' IS NOT NULL
        AND decision ? 'selected_ad'
        AND decision ? 'selection_count'
        AND (decision->>'reason'<>'selected' OR (
            decision->'selected_ad'<>'null'::jsonb
            AND jsonb_typeof(decision->'selection_count')='number'
        ))
    ))
);
CREATE FUNCTION orchestration_content_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF OLD.content_frozen_at IS NOT NULL AND (
        NEW.message_id IS DISTINCT FROM OLD.message_id
        OR NEW.content IS DISTINCT FROM OLD.content
        OR NEW.decision IS DISTINCT FROM OLD.decision
        OR NEW.plans IS DISTINCT FROM OLD.plans
        OR NEW.content_frozen_at IS DISTINCT FROM OLD.content_frozen_at
        OR NEW.state IN ('pending','preparing')
    ) THEN
        RAISE EXCEPTION 'frozen message content and advertising decision are immutable';
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER orchestration_content_immutable BEFORE UPDATE ON orchestration_dispatches
FOR EACH ROW EXECUTE FUNCTION orchestration_content_immutable();
