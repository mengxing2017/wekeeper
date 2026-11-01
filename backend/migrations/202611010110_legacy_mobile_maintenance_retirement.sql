-- Keep originals and accepted-job credentials/revisions for bounded terminal reports.
-- queued work never started; accepted/running work must stop on empty inbox, not fake success.
CREATE TABLE legacy_mobile_maintenance_retirement (
 id boolean PRIMARY KEY DEFAULT true CHECK (id),
 retired_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 reason text NOT NULL DEFAULT 'legacy_mobile_maintenance_retired',
 inventory jsonb NOT NULL
);
INSERT INTO legacy_mobile_maintenance_retirement(inventory)
SELECT jsonb_build_object(
 'v5_validations', (SELECT coalesce(jsonb_object_agg(state,n),'{}') FROM (SELECT state,count(*) n FROM automation_flow_validations GROUP BY state) v),
 'v4_validations', (SELECT coalesce(jsonb_object_agg(state,n),'{}') FROM (SELECT state,count(*) n FROM device_validation_jobs GROUP BY state) v),
 'send_items', (SELECT coalesce(jsonb_object_agg(state,n),'{}') FROM (SELECT state,count(*) n FROM mobile_delivery_items GROUP BY state) v)
);
ALTER TABLE automation_flow_validations ADD COLUMN retired_at timestamptz;
ALTER TABLE device_validation_jobs ADD COLUMN retired_at timestamptz;
UPDATE automation_flow_validations SET retired_at=clock_timestamp();
UPDATE device_validation_jobs SET retired_at=clock_timestamp();
UPDATE automation_flow_validations SET state='cancelled',revision=revision+1
 WHERE state='queued';
UPDATE device_validation_jobs SET state='cancelled',revision=revision+1
 WHERE state IN ('queued','offered');
-- Do not change accepted revisions, invocation permits, unknown items, or evidence.
-- The unchanged expires_at bounds original credential/generation-checked terminal reporting.

-- A Web/API rollback must not reopen retired work. These are legacy-only tables;
-- v7 openings/field checks and official WeCom broadcasts use separate tables.
CREATE FUNCTION reject_retired_mobile_creation() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 RAISE EXCEPTION 'legacy_mobile_maintenance_retired' USING ERRCODE='23514';
END $$;
DO $$ DECLARE t text; BEGIN
 FOREACH t IN ARRAY ARRAY[
  'device_validation_jobs','automation_flow_validations','automation_component_versions',
  'automation_component_releases','automation_pacing_profiles','automation_flow_versions',
  'automation_flow_releases','automation_execution_assignments','automation_dispatch_batches',
  'automation_dispatch_plans','automation_dispatch_preparations','automation_readiness_tickets',
  'mobile_execution_claims','mobile_send_permits'
 ] LOOP
  EXECUTE format('CREATE TRIGGER retired_mobile_no_creation BEFORE INSERT ON %I FOR EACH ROW EXECUTE FUNCTION reject_retired_mobile_creation()',t);
 END LOOP;
END $$;
CREATE FUNCTION reject_retired_mobile_activation() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF (to_jsonb(NEW)->TG_ARGV[0]) IS DISTINCT FROM (to_jsonb(OLD)->TG_ARGV[0])
 AND (to_jsonb(NEW)->>TG_ARGV[0])=ANY(string_to_array(TG_ARGV[1],',')) THEN
  RAISE EXCEPTION 'legacy_mobile_maintenance_retired' USING ERRCODE='23514';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER retired_mobile_no_activation BEFORE UPDATE ON automation_flow_validations FOR EACH ROW EXECUTE FUNCTION reject_retired_mobile_activation('state','queued,accepted');
CREATE TRIGGER retired_mobile_no_activation BEFORE UPDATE ON device_validation_jobs FOR EACH ROW EXECUTE FUNCTION reject_retired_mobile_activation('state','queued,offered,accepted,running');
CREATE TRIGGER retired_mobile_no_activation BEFORE UPDATE ON automation_execution_assignments FOR EACH ROW EXECUTE FUNCTION reject_retired_mobile_activation('enabled','true');
CREATE TRIGGER retired_mobile_no_activation BEFORE UPDATE ON automation_flow_releases FOR EACH ROW EXECUTE FUNCTION reject_retired_mobile_activation('active','true');
CREATE TRIGGER retired_mobile_no_activation BEFORE UPDATE ON automation_dispatch_batches FOR EACH ROW EXECUTE FUNCTION reject_retired_mobile_activation('state','confirmed');
CREATE TRIGGER retired_mobile_no_activation BEFORE UPDATE ON mobile_execution_claims FOR EACH ROW EXECUTE FUNCTION reject_retired_mobile_activation('state','claimed,prepared,permit_issued');
