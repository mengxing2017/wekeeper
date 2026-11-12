-- Rules have their own operator-facing name, independent of their material.
ALTER TABLE ad_placement_rules ADD COLUMN name text NOT NULL
    CHECK (length(trim(name)) BETWEEN 1 AND 120);
