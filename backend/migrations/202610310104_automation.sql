-- Database baseline: multi-application automation, local consent and v5 dispatch.
-- Depends on the phone execution ledger.
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
 state text NOT NULL DEFAULT 'queued' CHECK(state IN ('queued','offered','accepted','running','succeeded','failed','interrupted','cancelled','expired','declined')),
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
CREATE OR REPLACE FUNCTION automation_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN RAISE EXCEPTION 'automation evidence is immutable'; END; $$;
CREATE TRIGGER automation_environment_immutable BEFORE UPDATE OR DELETE ON device_environment_snapshots FOR EACH ROW EXECUTE FUNCTION automation_immutable();
CREATE TRIGGER automation_component_immutable BEFORE UPDATE OR DELETE ON automation_component_versions FOR EACH ROW EXECUTE FUNCTION automation_immutable();
CREATE TRIGGER automation_manifest_immutable BEFORE UPDATE OR DELETE ON automation_execution_manifests FOR EACH ROW EXECUTE FUNCTION automation_immutable();
CREATE TRIGGER automation_event_immutable BEFORE UPDATE OR DELETE ON device_validation_events FOR EACH ROW EXECUTE FUNCTION automation_immutable();

-- Explicit device execution consent
-- Append-only minimal local authorization receipts. A later revoke fences late grants.
CREATE TABLE device_execution_authorizations (
 device_id uuid NOT NULL REFERENCES execution_devices(id), local_revision bigint NOT NULL CHECK(local_revision>0),
 authorization_id uuid NOT NULL, body jsonb NOT NULL, received_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 PRIMARY KEY(device_id,local_revision)
);
ALTER TABLE device_validation_jobs ADD COLUMN authorization_id uuid;
ALTER TABLE device_validation_jobs ADD COLUMN decision_source text CHECK(decision_source IN ('local_action','standing_authorization','local_decline'));
ALTER TABLE automation_readiness_tickets ADD COLUMN authorization_id uuid;

-- Environment-independent v5 dispatch
ALTER TABLE mobile_delivery_settings ADD COLUMN dispatch_enabled boolean NOT NULL DEFAULT false;
CREATE TABLE automation_pacing_profiles (
 id uuid PRIMARY KEY, name text NOT NULL, school_id uuid NOT NULL REFERENCES schools(id),
 validation_ids jsonb NOT NULL, baseline jsonb NOT NULL, created_by uuid NOT NULL REFERENCES accounts(id), created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE automation_flow_versions (
 id uuid PRIMARY KEY, name text NOT NULL, body jsonb NOT NULL, content_digest text NOT NULL,
 pacing_profile_id uuid NOT NULL REFERENCES automation_pacing_profiles(id), created_by uuid NOT NULL REFERENCES accounts(id), created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE automation_flow_validations (
 id uuid PRIMARY KEY,school_id uuid NOT NULL REFERENCES schools(id),device_id uuid NOT NULL REFERENCES execution_devices(id),
 flow_id uuid REFERENCES automation_flow_versions(id),manual_application jsonb,kind text NOT NULL CHECK(kind IN ('inspect_roles','open_application','measure_manual')),
 path_key text NOT NULL DEFAULT 'generic',launcher_index integer,revision bigint NOT NULL DEFAULT 1,generation uuid NOT NULL,
 credential_generation bigint NOT NULL,state text NOT NULL DEFAULT 'queued' CHECK(state IN ('queued','accepted','succeeded','failed','interrupted','cancelled')),
 authorization_id uuid, result jsonb, created_by uuid NOT NULL REFERENCES accounts(id),created_at timestamptz NOT NULL DEFAULT clock_timestamp(),expires_at timestamptz NOT NULL DEFAULT clock_timestamp()+interval '10 minutes'
);
CREATE TABLE automation_flow_validation_events (job_id uuid NOT NULL REFERENCES automation_flow_validations(id),event_id uuid NOT NULL,body jsonb NOT NULL,PRIMARY KEY(job_id,event_id));
CREATE TRIGGER dispatch_validation_event_immutable BEFORE UPDATE OR DELETE ON automation_flow_validation_events FOR EACH ROW EXECUTE FUNCTION automation_immutable();
CREATE TABLE automation_flow_releases (
 id uuid PRIMARY KEY, flow_id uuid NOT NULL REFERENCES automation_flow_versions(id), school_id uuid NOT NULL REFERENCES schools(id),
 validation_id uuid REFERENCES device_validation_jobs(id),flow_validation_id uuid REFERENCES automation_flow_validations(id),CHECK((validation_id IS NOT NULL)<>(flow_validation_id IS NOT NULL)), revision bigint NOT NULL DEFAULT 1,
 active boolean NOT NULL DEFAULT true, revoked boolean NOT NULL DEFAULT false, created_by uuid NOT NULL REFERENCES accounts(id), created_at timestamptz NOT NULL DEFAULT clock_timestamp(), UNIQUE(school_id,id)
);
CREATE TABLE automation_device_groups (id uuid PRIMARY KEY, school_id uuid NOT NULL REFERENCES schools(id), name text NOT NULL, revision bigint NOT NULL DEFAULT 1, UNIQUE(school_id,id));
CREATE TABLE automation_device_group_members (group_id uuid NOT NULL REFERENCES automation_device_groups(id), device_id uuid NOT NULL REFERENCES execution_devices(id), PRIMARY KEY(group_id,device_id));
CREATE TABLE automation_execution_assignments (
 id uuid PRIMARY KEY, school_id uuid NOT NULL REFERENCES schools(id), device_id uuid NOT NULL REFERENCES execution_devices(id),
 sender_account_id uuid NOT NULL REFERENCES sending_accounts(id), account_binding_revision bigint NOT NULL,
 release_id uuid NOT NULL REFERENCES automation_flow_releases(id), revision bigint NOT NULL DEFAULT 1, enabled boolean NOT NULL DEFAULT true,
 UNIQUE(school_id,device_id), UNIQUE(school_id,id)
);
CREATE TABLE automation_dispatch_batches (
 id uuid PRIMARY KEY, school_id uuid NOT NULL REFERENCES schools(id), name text NOT NULL,
 state text NOT NULL CHECK(state IN ('draft','confirmed','paused','cancelled')), revision bigint NOT NULL DEFAULT 1,
 input jsonb NOT NULL, preview jsonb NOT NULL, preview_digest text NOT NULL, created_by uuid NOT NULL REFERENCES accounts(id),
 scope_owner_id uuid REFERENCES automation_dispatch_batches(id), confirmed_at timestamptz, created_at timestamptz NOT NULL DEFAULT clock_timestamp(), UNIQUE(school_id,id)
);
CREATE TABLE automation_dispatch_plans (
 id uuid PRIMARY KEY, batch_id uuid NOT NULL REFERENCES automation_dispatch_batches(id), school_id uuid NOT NULL REFERENCES schools(id),
 assignment_id uuid NOT NULL REFERENCES automation_execution_assignments(id), release_id uuid NOT NULL REFERENCES automation_flow_releases(id),
 device_id uuid NOT NULL REFERENCES execution_devices(id), sender_account_id uuid NOT NULL REFERENCES sending_accounts(id),
 run_id uuid NOT NULL UNIQUE REFERENCES mobile_delivery_strategy_runs(id), body jsonb NOT NULL, content_digest text NOT NULL,
 replaces_id uuid REFERENCES automation_dispatch_plans(id), created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE automation_task_deliveries (
 id uuid PRIMARY KEY, plan_id uuid NOT NULL UNIQUE REFERENCES automation_dispatch_plans(id), device_id uuid NOT NULL REFERENCES execution_devices(id),
 received_at timestamptz, generation uuid NOT NULL, cancelled boolean NOT NULL DEFAULT false
);
CREATE TABLE automation_dispatch_preparations (
 id uuid PRIMARY KEY, delivery_id uuid NOT NULL REFERENCES automation_task_deliveries(id), item_id uuid NOT NULL REFERENCES mobile_delivery_items(id),
 credential_generation bigint NOT NULL, generation uuid NOT NULL, authorization_id uuid NOT NULL, fence bigint NOT NULL,
 stage text NOT NULL DEFAULT 'preparing_app', path_key text NOT NULL DEFAULT 'generic', path_digest text,
 ready boolean NOT NULL DEFAULT false, expires_at timestamptz NOT NULL DEFAULT clock_timestamp()+interval '2 minutes', UNIQUE(delivery_id,item_id)
);
CREATE TABLE automation_dispatch_events (
 id uuid PRIMARY KEY, delivery_id uuid NOT NULL REFERENCES automation_task_deliveries(id), event_id uuid NOT NULL,
 body jsonb NOT NULL, created_at timestamptz NOT NULL DEFAULT clock_timestamp(), UNIQUE(delivery_id,event_id)
);
CREATE TABLE automation_execution_failures (
 id uuid PRIMARY KEY, delivery_id uuid NOT NULL REFERENCES automation_task_deliveries(id), school_id uuid NOT NULL REFERENCES schools(id),
 event_id uuid NOT NULL, body jsonb NOT NULL, fingerprint text NOT NULL, created_at timestamptz NOT NULL DEFAULT clock_timestamp(), UNIQUE(delivery_id,event_id)
);
CREATE TABLE automation_business_authorizations (
 device_id uuid NOT NULL REFERENCES execution_devices(id), revision bigint NOT NULL CHECK(revision>0),
 authorization_id uuid NOT NULL, body jsonb NOT NULL, PRIMARY KEY(device_id,revision)
);
ALTER TABLE mobile_delivery_run_progress ADD COLUMN dispatch_plan_id uuid UNIQUE REFERENCES automation_dispatch_plans(id),
 ADD CHECK((configuration_release_id IS NOT NULL) <> (dispatch_plan_id IS NOT NULL));
CREATE INDEX automation_dispatch_device ON automation_dispatch_plans(device_id,created_at,id);
CREATE INDEX automation_failures_fingerprint ON automation_execution_failures(fingerprint,created_at);
CREATE TRIGGER dispatch_flow_immutable BEFORE UPDATE OR DELETE ON automation_flow_versions FOR EACH ROW EXECUTE FUNCTION automation_immutable();
CREATE TRIGGER dispatch_plan_immutable BEFORE UPDATE OR DELETE ON automation_dispatch_plans FOR EACH ROW EXECUTE FUNCTION automation_immutable();
CREATE TRIGGER dispatch_pacing_immutable BEFORE UPDATE OR DELETE ON automation_pacing_profiles FOR EACH ROW EXECUTE FUNCTION automation_immutable();
CREATE TRIGGER dispatch_failure_immutable BEFORE UPDATE OR DELETE ON automation_execution_failures FOR EACH ROW EXECUTE FUNCTION automation_immutable();
CREATE TRIGGER dispatch_event_immutable BEFORE UPDATE OR DELETE ON automation_dispatch_events FOR EACH ROW EXECUTE FUNCTION automation_immutable();

CREATE OR REPLACE FUNCTION delivery_scope_owner(run uuid) RETURNS uuid LANGUAGE sql STABLE AS $$
 SELECT coalesce((SELECT coalesce(b.scope_owner_id,b.id) FROM automation_dispatch_plans p JOIN automation_dispatch_batches b ON b.id=p.batch_id WHERE p.run_id=run),run)
$$;
CREATE OR REPLACE FUNCTION maintain_delivery_content_scope() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE intended uuid; scope uuid; existing_owner uuid;
BEGIN
 IF TG_OP='INSERT' THEN
  SELECT COALESCE((SELECT delivery_scope_owner(r.run_id) FROM mobile_delivery_run_items r JOIN mobile_delivery_run_progress p ON p.id=r.run_id WHERE r.item_id=NEW.item_id),NEW.item_id) INTO intended;
  SELECT id,owner_id INTO scope,existing_owner FROM delivery_content_scope_owners WHERE school_id=NEW.school_id AND content_digest=NEW.content_digest AND active;
  IF scope IS NULL THEN
   scope:=NEW.item_id;
   INSERT INTO delivery_content_scope_owners(id,school_id,content_digest,owner_id) VALUES(scope,NEW.school_id,NEW.content_digest,intended);
  ELSIF existing_owner<>intended THEN
   RAISE EXCEPTION 'school_content_conflict' USING ERRCODE='23514';
  END IF;
  NEW.content_scope_id:=scope;
  RETURN NEW;
 END IF;
 IF OLD.active AND NOT NEW.active THEN
  UPDATE delivery_content_scope_owners o SET active=false WHERE o.id=OLD.content_scope_id
   AND NOT EXISTS(SELECT 1 FROM delivery_dedup_reservations r WHERE r.content_scope_id=o.id AND r.active AND r.item_id<>OLD.item_id)
   AND NOT EXISTS(SELECT 1 FROM delivery_usage u JOIN mobile_delivery_items i ON u.source_channel='mobile' AND u.source_id=i.id JOIN delivery_dedup_reservations r ON r.item_id=i.id WHERE r.content_scope_id=o.id AND u.state<>'not_sent');
 END IF;
 RETURN NEW;
END $$;

CREATE TRIGGER content_scope_insert BEFORE INSERT ON delivery_dedup_reservations FOR EACH ROW EXECUTE FUNCTION maintain_delivery_content_scope();
CREATE TRIGGER content_scope_release AFTER UPDATE OF active ON delivery_dedup_reservations FOR EACH ROW EXECUTE FUNCTION maintain_delivery_content_scope();
