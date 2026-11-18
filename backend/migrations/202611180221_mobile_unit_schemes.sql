-- Unit business schemes replace the retired range-opening workflow. No data migration.
CREATE TABLE mobile_business_scheme_versions (
 id uuid PRIMARY KEY,
 business_unit_id uuid REFERENCES business_units(id),
 revision bigint NOT NULL CHECK (revision>0),
 action text NOT NULL CHECK (action IN ('save','inherit')),
 body jsonb NOT NULL,
 created_by uuid NOT NULL REFERENCES accounts(id),
 reason text NOT NULL CHECK (length(trim(reason)) BETWEEN 1 AND 2000),
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 CHECK ((action='save' AND jsonb_typeof(body)='object') OR (action='inherit' AND business_unit_id IS NOT NULL AND body='null'::jsonb))
);
CREATE UNIQUE INDEX mobile_scheme_default_revision ON mobile_business_scheme_versions(revision) WHERE business_unit_id IS NULL;
CREATE UNIQUE INDEX mobile_scheme_unit_revision ON mobile_business_scheme_versions(business_unit_id,revision) WHERE business_unit_id IS NOT NULL;
CREATE TRIGGER mobile_scheme_immutable BEFORE UPDATE OR DELETE ON mobile_business_scheme_versions FOR EACH ROW EXECUTE FUNCTION mobile_capability_evidence_immutable();
ALTER TABLE mobile_v9_task_drafts ADD COLUMN business_unit_id uuid REFERENCES business_units(id);
ALTER TABLE mobile_v9_tasks ADD COLUMN business_unit_id uuid REFERENCES business_units(id);
ALTER TABLE mobile_v9_tasks ADD COLUMN scheme_snapshot jsonb NOT NULL;
ALTER TABLE mobile_v9_previews DROP CONSTRAINT mobile_v9_previews_policy_id_fkey;
ALTER TABLE mobile_v9_previews ADD CONSTRAINT mobile_preview_scheme_version_fk FOREIGN KEY(policy_id) REFERENCES mobile_business_scheme_versions(id);
ALTER TABLE mobile_device_target_bindings ADD COLUMN max_calls bigint CHECK(max_calls>0);
ALTER TABLE mobile_device_target_bindings ADD COLUMN valid_until timestamptz;
ALTER TABLE mobile_device_target_bindings ADD COLUMN budget_root uuid NOT NULL DEFAULT gen_random_uuid();
CREATE TABLE mobile_execution_resource_charges (
 execution_id uuid NOT NULL REFERENCES mobile_execution_grants(id),
 binding_id uuid NOT NULL REFERENCES mobile_device_target_bindings(id),
 budget_root uuid NOT NULL,
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 PRIMARY KEY(execution_id,budget_root)
);
CREATE INDEX mobile_resource_budget_charges ON mobile_execution_resource_charges(budget_root);
DROP TABLE mobile_execution_usage_budgets;
DROP TABLE mobile_usage_invocation_budgets;
DROP TABLE mobile_usage_confirmations;
DROP TABLE mobile_usage_events;
DROP TABLE mobile_usage_issues;
DROP TABLE mobile_usage_jobs;
DROP TABLE mobile_usage_previews;
DROP TABLE mobile_usage_scopes;
DROP TABLE mobile_v9_scope_revocations;
DROP TABLE mobile_v9_scope_grants;
DROP FUNCTION mobile_v9_scope_guard();
DROP TABLE mobile_usages;
DROP TABLE mobile_v9_send_policy_versions;

-- Frozen task intent and real resource budget identities cannot be silently replaced.
CREATE FUNCTION mobile_task_scheme_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.business_unit_id IS DISTINCT FROM OLD.business_unit_id OR NEW.scheme_snapshot IS DISTINCT FROM OLD.scheme_snapshot THEN
  RAISE EXCEPTION 'mobile task business scheme is immutable';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER mobile_task_scheme_immutable BEFORE UPDATE ON mobile_v9_tasks FOR EACH ROW EXECUTE FUNCTION mobile_task_scheme_immutable();
CREATE FUNCTION mobile_binding_budget_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.budget_root IS DISTINCT FROM OLD.budget_root OR NEW.device_id IS DISTINCT FROM OLD.device_id OR NEW.target_id IS DISTINCT FROM OLD.target_id THEN
  RAISE EXCEPTION 'mobile resource budget identity is immutable';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER mobile_binding_budget_immutable BEFORE UPDATE ON mobile_device_target_bindings FOR EACH ROW EXECUTE FUNCTION mobile_binding_budget_immutable();
