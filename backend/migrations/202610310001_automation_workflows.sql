-- New automation state is additive. Existing snapshots, hashes and result ledgers stay intact.
ALTER TABLE execution_devices DROP CONSTRAINT execution_devices_execution_protocol_version_check;
ALTER TABLE execution_devices ADD CHECK(execution_protocol_version IN (1,2,3,4));
ALTER TABLE mobile_execution_claims DROP CONSTRAINT mobile_execution_claims_protocol_version_check;
ALTER TABLE mobile_execution_claims ADD CHECK(protocol_version IN (1,2,3,4));
CREATE TABLE automation_apps (
 id uuid PRIMARY KEY, app_key text NOT NULL UNIQUE, name text NOT NULL, enabled boolean NOT NULL
);
INSERT INTO automation_apps VALUES
 ('00000000-0000-0000-0000-000000000001','wecom','企业微信',true),
 ('00000000-0000-0000-0000-000000000002','wechat','微信',false),
 ('00000000-0000-0000-0000-000000000003','dingtalk','钉钉',false),
 ('00000000-0000-0000-0000-000000000004','douyin','抖音',false);
CREATE TABLE automation_capabilities (
 id uuid PRIMARY KEY, app_id uuid NOT NULL REFERENCES automation_apps(id), capability_key text NOT NULL,
 name text NOT NULL, handler text NOT NULL CHECK(handler='wecom_group_text'),
 minimum_protocol integer NOT NULL CHECK(minimum_protocol=4), UNIQUE(app_id,capability_key)
);
INSERT INTO automation_capabilities VALUES
 ('00000000-0000-0000-0000-000000000011','00000000-0000-0000-0000-000000000001','group_text','向群顺序发送纯文本','wecom_group_text',4);
CREATE TABLE device_environment_snapshots (
 id uuid PRIMARY KEY, device_id uuid NOT NULL REFERENCES execution_devices(id), generation bigint NOT NULL CHECK(generation>0),
 credential_generation bigint NOT NULL, body jsonb NOT NULL, content_digest text NOT NULL,
 received_at timestamptz NOT NULL DEFAULT clock_timestamp(), UNIQUE(device_id,generation)
);
CREATE TABLE device_app_bindings (
 id uuid PRIMARY KEY, school_id uuid NOT NULL REFERENCES schools(id), device_id uuid NOT NULL REFERENCES execution_devices(id),
 app_id uuid NOT NULL REFERENCES automation_apps(id), sender_account_id uuid NOT NULL REFERENCES sending_accounts(id),
 account_binding_revision bigint NOT NULL, instance_ref text NOT NULL, environment_id uuid NOT NULL REFERENCES device_environment_snapshots(id),
 revision bigint NOT NULL DEFAULT 1, created_by uuid NOT NULL REFERENCES accounts(id),
 UNIQUE(school_id,device_id,app_id), UNIQUE(school_id,id)
);
CREATE TABLE automation_component_versions (
 id uuid PRIMARY KEY, name text NOT NULL, kind text NOT NULL CHECK(kind IN ('launcher','workflow','policy','baseline')),
 revision bigint NOT NULL DEFAULT 1, body jsonb NOT NULL, content_digest text NOT NULL,
 created_by uuid NOT NULL REFERENCES accounts(id), created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE device_validation_jobs (
 id uuid PRIMARY KEY, school_id uuid NOT NULL REFERENCES schools(id), device_id uuid NOT NULL REFERENCES execution_devices(id),
 app_binding_id uuid REFERENCES device_app_bindings(id), kind text NOT NULL CHECK(kind IN ('inspect_environment','open_application','inspect_roles','measure_manual')),
 component_ids jsonb NOT NULL, environment_id uuid REFERENCES device_environment_snapshots(id), binding_revision bigint,
 state text NOT NULL DEFAULT 'queued' CHECK(state IN ('queued','offered','accepted','running','succeeded','failed','interrupted','cancelled','expired')),
 generation uuid NOT NULL, revision bigint NOT NULL DEFAULT 1, credential_generation bigint NOT NULL,
 created_by uuid NOT NULL REFERENCES accounts(id), created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 expires_at timestamptz NOT NULL, accepted_at timestamptz, result jsonb, UNIQUE(school_id,id)
);
ALTER TABLE device_validation_jobs ADD FOREIGN KEY(school_id,app_binding_id) REFERENCES device_app_bindings(school_id,id);
CREATE UNIQUE INDEX automation_one_validation_per_device ON device_validation_jobs(device_id)
 WHERE state IN ('queued','offered','accepted','running');
CREATE TABLE device_validation_events (
 id uuid PRIMARY KEY, job_id uuid NOT NULL REFERENCES device_validation_jobs(id), event_id uuid NOT NULL,
 body jsonb NOT NULL, applied boolean NOT NULL, created_at timestamptz NOT NULL DEFAULT clock_timestamp(), UNIQUE(job_id,event_id)
);
CREATE TABLE automation_component_releases (
 id uuid PRIMARY KEY, component_id uuid NOT NULL REFERENCES automation_component_versions(id), kind text NOT NULL,
 school_id uuid NOT NULL REFERENCES schools(id), device_id uuid REFERENCES execution_devices(id),
 environment_digest text NOT NULL, revision bigint NOT NULL DEFAULT 1, active boolean NOT NULL DEFAULT true,
 revoked boolean NOT NULL DEFAULT false, validation_id uuid NOT NULL REFERENCES device_validation_jobs(id),
 created_by uuid NOT NULL REFERENCES accounts(id), created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE UNIQUE INDEX automation_unique_release ON automation_component_releases(school_id,COALESCE(device_id,'00000000-0000-0000-0000-000000000000'::uuid),environment_digest,kind) WHERE active AND NOT revoked;
CREATE TABLE automation_execution_manifests (
 id uuid PRIMARY KEY, school_id uuid NOT NULL REFERENCES schools(id), app_binding_id uuid NOT NULL REFERENCES device_app_bindings(id),
 body jsonb NOT NULL, content_digest text NOT NULL, created_at timestamptz NOT NULL DEFAULT clock_timestamp(), UNIQUE(school_id,id)
);
ALTER TABLE automation_execution_manifests ADD FOREIGN KEY(school_id,app_binding_id) REFERENCES device_app_bindings(school_id,id);
ALTER TABLE mobile_delivery_strategies ADD COLUMN automation_binding_id uuid REFERENCES device_app_bindings(id),
 ADD COLUMN automation_capability_id uuid REFERENCES automation_capabilities(id);
ALTER TABLE mobile_delivery_strategy_runs ADD COLUMN automation_manifest_id uuid REFERENCES automation_execution_manifests(id);
CREATE TABLE automation_business_previews (
 id uuid PRIMARY KEY, school_id uuid NOT NULL REFERENCES schools(id), created_by uuid NOT NULL REFERENCES accounts(id),
 revision bigint NOT NULL DEFAULT 1, input jsonb NOT NULL, resolution jsonb NOT NULL, preview_digest text NOT NULL,
 run_id uuid NOT NULL REFERENCES mobile_delivery_strategy_runs(id), task_preview jsonb NOT NULL,
 expires_at timestamptz NOT NULL DEFAULT clock_timestamp()+interval '10 minutes', confirmed_at timestamptz, UNIQUE(school_id,id)
);
ALTER TABLE automation_business_previews ADD FOREIGN KEY(school_id,run_id) REFERENCES mobile_delivery_strategy_runs(school_id,id);
CREATE TABLE automation_readiness_tickets (
 id uuid PRIMARY KEY, device_id uuid NOT NULL REFERENCES execution_devices(id), item_id uuid NOT NULL REFERENCES mobile_delivery_items(id),
 manifest_id uuid NOT NULL REFERENCES automation_execution_manifests(id), environment_id uuid NOT NULL REFERENCES device_environment_snapshots(id),
 credential_generation bigint NOT NULL, generation uuid NOT NULL, ready boolean NOT NULL DEFAULT false,
 stage text NOT NULL DEFAULT 'checking_environment' CHECK(stage IN ('checking_environment','returning_to_launcher','matching_launcher','searching_application','browsing_all_applications','opening_application','verifying_application','ready')),
 expires_at timestamptz NOT NULL DEFAULT clock_timestamp()+interval '2 minutes'
);
-- Immutable evidence can never be edited in place, including by later application code.
CREATE FUNCTION automation_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN RAISE EXCEPTION 'automation evidence is immutable'; END; $$;
CREATE TRIGGER automation_environment_immutable BEFORE UPDATE OR DELETE ON device_environment_snapshots FOR EACH ROW EXECUTE FUNCTION automation_immutable();
CREATE TRIGGER automation_component_immutable BEFORE UPDATE OR DELETE ON automation_component_versions FOR EACH ROW EXECUTE FUNCTION automation_immutable();
CREATE TRIGGER automation_manifest_immutable BEFORE UPDATE OR DELETE ON automation_execution_manifests FOR EACH ROW EXECUTE FUNCTION automation_immutable();
CREATE TRIGGER automation_event_immutable BEFORE UPDATE OR DELETE ON device_validation_events FOR EACH ROW EXECUTE FUNCTION automation_immutable();
