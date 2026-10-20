-- Infrastructure metadata only. Business tables require reviewed follow-up migrations.
CREATE TABLE application_metadata (
    singleton BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (singleton),
    schema_generation INTEGER NOT NULL CHECK (schema_generation > 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO application_metadata (singleton, schema_generation) VALUES (TRUE, 1);
