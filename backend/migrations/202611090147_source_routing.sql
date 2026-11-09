-- Routing organization is independent of callers, schools and execution workspaces.
CREATE TABLE routing_units (
 id uuid PRIMARY KEY, name text NOT NULL CHECK(length(trim(name)) BETWEEN 1 AND 120),
 description text NOT NULL DEFAULT '' CHECK(length(description)<=2000), active boolean NOT NULL DEFAULT true,
 revision bigint NOT NULL DEFAULT 1, created_by uuid NOT NULL REFERENCES accounts,
 updated_by uuid NOT NULL REFERENCES accounts, created_at timestamptz NOT NULL DEFAULT now(), updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE routing_groups (
 id uuid PRIMARY KEY, unit_id uuid NOT NULL REFERENCES routing_units, name text NOT NULL CHECK(length(trim(name)) BETWEEN 1 AND 120),
 description text NOT NULL DEFAULT '' CHECK(length(description)<=2000), active boolean NOT NULL DEFAULT true,
 revision bigint NOT NULL DEFAULT 1, created_by uuid NOT NULL REFERENCES accounts,
 updated_by uuid NOT NULL REFERENCES accounts, created_at timestamptz NOT NULL DEFAULT now(), updated_at timestamptz NOT NULL DEFAULT now(), UNIQUE(unit_id,id)
);
ALTER TABLE source_routes ADD COLUMN unit_id uuid REFERENCES routing_units,
 ADD COLUMN group_id uuid, ADD COLUMN config_revision bigint NOT NULL DEFAULT 0,
 ADD COLUMN execution_state text NOT NULL DEFAULT 'draft' CHECK(execution_state IN ('draft','running','paused')),
 ADD COLUMN retired_at timestamptz,
 ADD FOREIGN KEY(unit_id,group_id) REFERENCES routing_groups(unit_id,id),
 ADD CHECK((unit_id IS NULL)=(group_id IS NULL));
CREATE TABLE source_route_tombstones (
 route_id text PRIMARY KEY, source_id uuid NOT NULL REFERENCES source_callers,
 reason text NOT NULL, retired_at timestamptz NOT NULL DEFAULT now(), config_revision bigint NOT NULL
);
-- History deliberately has no FK to live routes/credentials.
CREATE TABLE source_route_revisions (
 route_id text NOT NULL, revision bigint NOT NULL, source_id uuid NOT NULL REFERENCES source_callers,
 snapshot jsonb NOT NULL, actor_id uuid NOT NULL REFERENCES accounts, created_at timestamptz NOT NULL DEFAULT now(),
 PRIMARY KEY(route_id,revision)
);
CREATE TABLE source_route_managers (
 route_id text NOT NULL REFERENCES source_routes ON DELETE CASCADE, account_id uuid NOT NULL REFERENCES accounts,
 revision bigint NOT NULL DEFAULT 1, granted_by uuid NOT NULL REFERENCES accounts, created_at timestamptz NOT NULL DEFAULT now(),
 PRIMARY KEY(route_id,account_id)
);
CREATE TABLE routing_group_targets (
 id uuid PRIMARY KEY, group_id uuid NOT NULL REFERENCES routing_groups,
 channel text NOT NULL CHECK(channel IN ('wecom','mobile')), workspace_id uuid NOT NULL, target_id uuid NOT NULL,
 added_by uuid NOT NULL REFERENCES accounts, added_at timestamptz NOT NULL DEFAULT now(), removed_at timestamptz
);
CREATE UNIQUE INDEX routing_group_target_current ON routing_group_targets(group_id,channel,workspace_id,target_id) WHERE removed_at IS NULL;
CREATE TABLE source_route_targets (
 id uuid PRIMARY KEY, route_id text NOT NULL REFERENCES source_routes ON DELETE CASCADE,
 channel text NOT NULL CHECK(channel IN ('wecom','mobile')), workspace_id uuid NOT NULL, target_id uuid NOT NULL,
 added_by uuid NOT NULL REFERENCES accounts, added_at timestamptz NOT NULL DEFAULT now(), removed_at timestamptz
);
CREATE UNIQUE INDEX source_route_target_current ON source_route_targets(route_id,channel,workspace_id,target_id) WHERE removed_at IS NULL;
CREATE TABLE routing_entity_history (
 id uuid PRIMARY KEY, kind text NOT NULL CHECK(kind IN ('unit','group')), entity_id uuid NOT NULL,
 revision bigint NOT NULL, snapshot jsonb NOT NULL, actor_id uuid NOT NULL REFERENCES accounts,
 created_at timestamptz NOT NULL DEFAULT now(), UNIQUE(kind,entity_id,revision)
);
CREATE TABLE source_routing_decisions (
 receipt_id uuid PRIMARY KEY REFERENCES source_receipts, source_id uuid NOT NULL REFERENCES source_callers,
 route_id text NOT NULL, config_revision bigint NOT NULL, snapshot jsonb NOT NULL, target_digest text NOT NULL,
 created_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE source_routing_waits (
 receipt_id uuid PRIMARY KEY REFERENCES source_receipts, route_id text NOT NULL, reason_code text NOT NULL,
 wake_key text NOT NULL, next_attempt_at timestamptz NOT NULL DEFAULT now(), stopped boolean NOT NULL DEFAULT false,
 updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX source_routing_wait_due ON source_routing_waits(next_attempt_at) WHERE NOT stopped;
CREATE FUNCTION source_routing_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN RAISE EXCEPTION 'routing history is immutable'; END $$;
CREATE TRIGGER source_route_revision_immutable BEFORE UPDATE OR DELETE ON source_route_revisions FOR EACH ROW EXECUTE FUNCTION source_routing_immutable();
CREATE TRIGGER source_routing_decision_immutable BEFORE UPDATE OR DELETE ON source_routing_decisions FOR EACH ROW EXECUTE FUNCTION source_routing_immutable();
CREATE TRIGGER routing_entity_history_immutable BEFORE UPDATE OR DELETE ON routing_entity_history FOR EACH ROW EXECUTE FUNCTION source_routing_immutable();
CREATE FUNCTION source_route_lifecycle() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='INSERT' THEN
  IF EXISTS(SELECT 1 FROM source_route_tombstones WHERE route_id=NEW.route_id) THEN RAISE EXCEPTION 'retired route identifier'; END IF;
  RETURN NEW;
 END IF;
 IF TG_OP='DELETE' THEN
  INSERT INTO source_route_tombstones(route_id,source_id,reason,config_revision) VALUES(OLD.route_id,OLD.source_id,'credential_deleted',OLD.config_revision) ON CONFLICT DO NOTHING;
  RETURN OLD;
 END IF;
 IF OLD.retired_at IS NOT NULL AND (NEW.retired_at IS DISTINCT FROM OLD.retired_at OR NEW.status<>'disabled' OR NEW.execution_state<>'paused') THEN RAISE EXCEPTION 'retired route is irreversible'; END IF;
 IF OLD.unit_id IS NOT NULL AND (NEW.unit_id IS DISTINCT FROM OLD.unit_id OR NEW.group_id IS DISTINCT FROM OLD.group_id) THEN RAISE EXCEPTION 'route scope is immutable'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER source_route_lifecycle BEFORE INSERT OR UPDATE OR DELETE ON source_routes FOR EACH ROW EXECUTE FUNCTION source_route_lifecycle();

CREATE FUNCTION routing_group_scope() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.unit_id IS DISTINCT FROM OLD.unit_id AND (
  EXISTS(SELECT 1 FROM source_routes WHERE group_id=OLD.id) OR
  EXISTS(SELECT 1 FROM routing_group_targets WHERE group_id=OLD.id AND removed_at IS NULL)
 ) THEN RAISE EXCEPTION 'referenced group cannot move'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER routing_group_scope BEFORE UPDATE ON routing_groups FOR EACH ROW EXECUTE FUNCTION routing_group_scope();
