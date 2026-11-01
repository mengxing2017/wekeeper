-- Shared devices are authorized per target, never by a school-device grant.
-- Existing bindings require explicit maintainer verification; do not infer scope.
DO $$
DECLARE obsolete_fk text;
BEGIN
 FOR obsolete_fk IN SELECT conname FROM pg_constraint WHERE conrelid='mobile_device_target_bindings'::regclass AND confrelid='mobile_device_school_grants'::regclass LOOP
  EXECUTE format('ALTER TABLE mobile_device_target_bindings DROP CONSTRAINT %I',obsolete_fk);
 END LOOP;
END $$;
ALTER TABLE mobile_device_target_bindings DROP COLUMN grant_id;
ALTER TABLE mobile_device_target_bindings ADD CONSTRAINT mobile_binding_device_fk FOREIGN KEY(device_id) REFERENCES execution_devices(id);
ALTER TABLE mobile_device_target_bindings ADD CONSTRAINT mobile_binding_device_target_unique UNIQUE(device_id,target_id);
ALTER TABLE mobile_device_target_bindings ADD COLUMN search_text_override text;
ALTER TABLE mobile_device_target_bindings ADD COLUMN locator_revision bigint NOT NULL DEFAULT 1 CHECK(locator_revision>0);
ALTER TABLE mobile_device_target_bindings ADD CONSTRAINT mobile_binding_search_text CHECK (
 search_text_override IS NULL OR (
 length(search_text_override) BETWEEN 1 AND 256 AND octet_length(search_text_override)<=1024
 AND search_text_override=btrim(search_text_override)
 AND search_text_override !~ '[[:cntrl:]]'
 ));
-- Keep original snapshots and calls intact; old bindings must not become global grants.
INSERT INTO mobile_execution_audit_events(id,school_id,resource_id,actor_id,action,detail)
SELECT gen_random_uuid(),school_id,id,updated_by,'shared-cutover-disable-binding',
 jsonb_build_object('reason','shared binding requires explicit maintainer verification','previous_enabled',enabled,'previous_revision',revision)
FROM mobile_device_target_bindings;
UPDATE mobile_device_target_bindings SET enabled=false,revision=revision+1,updated_at=clock_timestamp();
CREATE INDEX mobile_binding_target_candidates ON mobile_device_target_bindings(target_id,device_id) WHERE enabled;
