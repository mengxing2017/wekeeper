-- Approval covers business resources, not a technical release or an acceptance run.
-- Preserve historical scopes, plans and invocation ledgers without rewriting intent.
ALTER TABLE mobile_v9_scope_grants ALTER COLUMN release_id DROP NOT NULL;
ALTER TABLE mobile_v9_scope_grants ALTER COLUMN acceptance_id DROP NOT NULL;
ALTER TABLE mobile_usage_scopes ALTER COLUMN release_id DROP NOT NULL;
CREATE UNIQUE INDEX mobile_usage_business_scope ON mobile_usage_scopes(usage_id,target_id,device_id) WHERE release_id IS NULL;
-- The former trigger also enforced acceptance, even after dropping NOT NULL.
-- Keep tenant/resource checks; technical fields are absent on business scopes.
ALTER TABLE mobile_v9_scope_grants ADD CONSTRAINT mobile_scope_technical_pair
 CHECK ((release_id IS NULL) = (acceptance_id IS NULL));
CREATE OR REPLACE FUNCTION mobile_v9_scope_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 PERFORM pg_advisory_xact_lock(78234610243);
 IF NEW.acceptance_id IS NOT NULL AND NOT EXISTS(
  SELECT 1 FROM mobile_v9_acceptance_runs a
  WHERE a.id=NEW.acceptance_id AND a.release_id=NEW.release_id AND a.stage=NEW.stage
 ) THEN RAISE EXCEPTION 'v9 acceptance scope mismatch'; END IF;
 IF cardinality(NEW.device_ids)=0 OR EXISTS(
  SELECT 1 FROM unnest(NEW.device_ids) d
  WHERE NOT EXISTS(SELECT 1 FROM execution_devices WHERE id=d)
 ) OR EXISTS(
  SELECT 1 FROM unnest(NEW.target_ids) t
  WHERE NOT EXISTS(SELECT 1 FROM mobile_targets WHERE id=t AND school_id=NEW.school_id)
 ) THEN RAISE EXCEPTION 'v9 resource scope mismatch'; END IF;
 RETURN NEW;
END $$;
