-- Forward-only: old task snapshots/outboxes and single-invocation facts are untouched.
ALTER TABLE execution_devices DROP CONSTRAINT execution_devices_execution_protocol_version_check;
ALTER TABLE execution_devices ADD CHECK(execution_protocol_version IN (1,2,3));
ALTER TABLE execution_devices ADD COLUMN supported_protocol_versions integer[] NOT NULL DEFAULT ARRAY[1];
UPDATE execution_devices SET supported_protocol_versions=ARRAY[execution_protocol_version];
ALTER TABLE mobile_execution_claims DROP CONSTRAINT mobile_execution_claims_protocol_version_check;
ALTER TABLE mobile_execution_claims ADD CHECK(protocol_version IN (1,2,3));
CREATE TABLE client_rule_bundle_versions (
 id uuid PRIMARY KEY, name text NOT NULL, revision bigint NOT NULL DEFAULT 1,
 configuration jsonb NOT NULL, content_digest text NOT NULL, created_by uuid NOT NULL REFERENCES accounts(id),
 created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE client_rule_releases (
 id uuid PRIMARY KEY, configuration_id uuid NOT NULL REFERENCES client_rule_bundle_versions(id),
 school_id uuid NOT NULL REFERENCES schools(id), device_id uuid REFERENCES execution_devices(id),
 revision bigint NOT NULL DEFAULT 1, active boolean NOT NULL DEFAULT true, revoked boolean NOT NULL DEFAULT false,
 evidence_ref text NOT NULL, created_by uuid NOT NULL REFERENCES accounts(id), created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 UNIQUE(school_id,id)
);
CREATE INDEX rule_release_resolution ON client_rule_releases(school_id,device_id) WHERE active AND NOT revoked;
-- Visible execution quotas are policy data; retain non-negative integer capacity in storage.
ALTER TABLE sending_accounts DROP CONSTRAINT sending_accounts_daily_limit_check;
ALTER TABLE sending_accounts ADD CHECK(daily_limit>=0);
ALTER TABLE delivery_groups DROP CONSTRAINT delivery_groups_daily_limit_check;
ALTER TABLE delivery_groups ADD CHECK(daily_limit>=0);
-- Legacy execution defaults live in the database, not hidden Rust/SQL constants.
CREATE TABLE delivery_execution_defaults (
 id boolean PRIMARY KEY DEFAULT true CHECK(id), account_cooldown_ms bigint NOT NULL,
 content_cooldown_ms bigint NOT NULL, quota_window_ms bigint NOT NULL
);
INSERT INTO delivery_execution_defaults VALUES(true,1800000,600000,86400000);
ALTER TABLE mobile_delivery_strategies ALTER COLUMN group_id DROP NOT NULL,
 ADD COLUMN delivery_mode text NOT NULL DEFAULT 'single_group' CHECK(delivery_mode IN ('single_group','ordered_groups')),
 ADD COLUMN targets jsonb NOT NULL DEFAULT '[]',
 ADD COLUMN inter_group_interval_seconds bigint CHECK(inter_group_interval_seconds>=0),
 ADD COLUMN configuration_release_id uuid,
 ADD FOREIGN KEY(school_id,configuration_release_id) REFERENCES client_rule_releases(school_id,id),
 ADD CHECK((delivery_mode='single_group' AND group_id IS NOT NULL AND targets='[]') OR (delivery_mode='ordered_groups' AND group_id IS NULL AND jsonb_array_length(targets)>=2 AND configuration_release_id IS NOT NULL));
CREATE TABLE mobile_delivery_strategy_targets (
 strategy_id uuid NOT NULL, school_id uuid NOT NULL, position integer NOT NULL CHECK(position>0), group_id uuid NOT NULL,
 PRIMARY KEY(strategy_id,position), UNIQUE(strategy_id,group_id),
 FOREIGN KEY(school_id,strategy_id) REFERENCES mobile_delivery_strategies(school_id,id),
 FOREIGN KEY(school_id,group_id) REFERENCES delivery_groups(school_id,id)
);
INSERT INTO mobile_delivery_strategy_targets SELECT id,school_id,1,group_id FROM mobile_delivery_strategies;
ALTER TABLE mobile_delivery_strategy_runs ADD UNIQUE(school_id,id);
CREATE TABLE mobile_delivery_run_items (
 item_id uuid PRIMARY KEY, school_id uuid NOT NULL, run_id uuid NOT NULL, position integer NOT NULL CHECK(position>0),
 UNIQUE(run_id,position), FOREIGN KEY(school_id,item_id) REFERENCES mobile_delivery_items(school_id,id),
 FOREIGN KEY(school_id,run_id) REFERENCES mobile_delivery_strategy_runs(school_id,id)
);
CREATE TABLE mobile_delivery_run_progress (
 id uuid PRIMARY KEY REFERENCES mobile_delivery_strategy_runs(id), current_position integer NOT NULL DEFAULT 1 CHECK(current_position>0),
 confirmed_at timestamptz, last_completed_at timestamptz, next_eligible_at timestamptz, pause_reason text,
 configuration_release_id uuid NOT NULL REFERENCES client_rule_releases(id),
 revision bigint NOT NULL DEFAULT 1, execution_revision bigint NOT NULL DEFAULT 1,
 inter_group_interval_ms bigint NOT NULL CHECK(inter_group_interval_ms>=0)
);
CREATE TABLE mobile_delivery_run_execution_revisions (
 id uuid PRIMARY KEY, run_id uuid NOT NULL REFERENCES mobile_delivery_strategy_runs(id), revision bigint NOT NULL,
 configuration_release_id uuid NOT NULL REFERENCES client_rule_releases(id), preview_digest text NOT NULL, previous_progress jsonb NOT NULL,
 evidence_ref text NOT NULL, created_by uuid NOT NULL REFERENCES accounts(id), created_at timestamptz NOT NULL DEFAULT clock_timestamp(), UNIQUE(run_id,revision)
);
CREATE TABLE delivery_content_scope_owners (
 id uuid PRIMARY KEY, school_id uuid NOT NULL REFERENCES schools(id), content_digest text NOT NULL,
 owner_id uuid NOT NULL, active boolean NOT NULL DEFAULT true,
 UNIQUE(id,school_id,content_digest)
);
CREATE UNIQUE INDEX content_scope_assignment ON delivery_content_scope_owners(school_id,content_digest) WHERE active;
ALTER TABLE delivery_dedup_reservations ADD COLUMN content_scope_id uuid;
INSERT INTO delivery_content_scope_owners(id,school_id,content_digest,owner_id,active)
 SELECT item_id,school_id,content_digest,item_id,active FROM delivery_dedup_reservations;
UPDATE delivery_dedup_reservations SET content_scope_id=item_id;
ALTER TABLE delivery_dedup_reservations ALTER COLUMN content_scope_id SET NOT NULL,
 ADD FOREIGN KEY(content_scope_id,school_id,content_digest) REFERENCES delivery_content_scope_owners(id,school_id,content_digest);
CREATE OR REPLACE FUNCTION maintain_delivery_content_scope() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE intended uuid; scope uuid; existing_owner uuid;
BEGIN
 IF TG_OP='INSERT' THEN
  SELECT COALESCE((SELECT r.run_id FROM mobile_delivery_run_items r JOIN mobile_delivery_run_progress p ON p.id=r.run_id WHERE r.item_id=NEW.item_id),NEW.item_id) INTO intended;
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
DROP INDEX mobile_school_content_assignment;
