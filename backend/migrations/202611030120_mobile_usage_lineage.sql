-- Versions retain their frozen task references; lineage is indexed, never deleted.
CREATE INDEX mobile_usages_predecessor ON mobile_usages(supersedes) WHERE supersedes IS NOT NULL;
