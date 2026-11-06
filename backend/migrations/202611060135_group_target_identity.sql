-- Do not infer a real group name from a display alias or a search query.
ALTER TABLE mobile_targets ADD COLUMN exact_name text;
ALTER TABLE mobile_targets ADD COLUMN identity_revision bigint NOT NULL DEFAULT 1;
ALTER TABLE mobile_targets ADD CONSTRAINT mobile_target_exact_name CHECK (
 exact_name IS NULL OR (char_length(exact_name) BETWEEN 1 AND 256
 AND exact_name = btrim(exact_name) AND exact_name !~ '[[:cntrl:]]'));
