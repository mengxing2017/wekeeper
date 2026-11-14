-- Advertisement materials bind directly to business scopes. There is no
-- separately named or configurable placement-rule entity.
ALTER TABLE ad_placement_rules RENAME TO ad_placements;
ALTER TABLE ad_placements DROP COLUMN name;

-- A scope can contain many advertisements, but the same advertisement cannot
-- be bound to that scope twice (including global/platform NULL components).
ALTER TABLE ad_placements ADD CONSTRAINT ad_placements_material_scope_unique
    UNIQUE NULLS NOT DISTINCT (material_id, scope_kind, platform_id, unit_id);
ALTER INDEX ad_placement_rules_scope RENAME TO ad_placements_scope;
COMMENT ON TABLE ad_placements IS 'Direct advertisement material to business scope bindings; not selection rules';
