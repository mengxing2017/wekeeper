-- An accepted business message has its own identity and immutable source facts.
CREATE TABLE orchestration_messages (
    id uuid PRIMARY KEY,
    platform_id uuid NOT NULL,
    unit_id uuid NOT NULL,
    source_event_id uuid NOT NULL,
    evidence_ref uuid NOT NULL,
    original jsonb NOT NULL,
    accepted_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    FOREIGN KEY (platform_id, unit_id) REFERENCES business_units(platform_id, id),
    UNIQUE (platform_id, source_event_id),
    UNIQUE (platform_id, evidence_ref),
    CHECK (id <> '00000000-0000-0000-0000-000000000000'::uuid),
    CHECK (source_event_id <> '00000000-0000-0000-0000-000000000000'::uuid),
    CHECK (evidence_ref <> '00000000-0000-0000-0000-000000000000'::uuid),
    CHECK (jsonb_typeof(original) = 'object')
);
CREATE FUNCTION orchestration_message_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN RAISE EXCEPTION 'accepted orchestration message is immutable'; END $$;
CREATE TRIGGER orchestration_message_immutable BEFORE UPDATE OR DELETE ON orchestration_messages
    FOR EACH ROW EXECUTE FUNCTION orchestration_message_immutable();
