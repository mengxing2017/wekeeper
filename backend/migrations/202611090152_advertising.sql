-- Unit-owned advertising; the directory is independent of workspace membership.
CREATE TABLE advertisement_unit_managers(unit_id uuid NOT NULL REFERENCES routing_units,account_id uuid NOT NULL REFERENCES accounts,can_edit boolean NOT NULL,can_share boolean NOT NULL,revision bigint NOT NULL DEFAULT 1,granted_by uuid NOT NULL REFERENCES accounts,PRIMARY KEY(unit_id,account_id));
CREATE TABLE advertisements(id uuid PRIMARY KEY,owner_unit_id uuid NOT NULL REFERENCES routing_units,revision bigint NOT NULL DEFAULT 0,content_version uuid,policy_version uuid,archived boolean NOT NULL DEFAULT false,created_by uuid NOT NULL REFERENCES accounts,created_at timestamptz NOT NULL DEFAULT clock_timestamp());
CREATE TABLE advertisement_content_versions(id uuid PRIMARY KEY,ad_id uuid NOT NULL REFERENCES advertisements,revision bigint NOT NULL,name text NOT NULL,body text NOT NULL,link text,actor_id uuid NOT NULL REFERENCES accounts,reason text NOT NULL,created_at timestamptz NOT NULL DEFAULT clock_timestamp(),UNIQUE(ad_id,revision));
CREATE TABLE advertisement_policy_versions(id uuid PRIMARY KEY,ad_id uuid NOT NULL REFERENCES advertisements,revision bigint NOT NULL,policy jsonb NOT NULL,actor_id uuid NOT NULL REFERENCES accounts,reason text NOT NULL,created_at timestamptz NOT NULL DEFAULT clock_timestamp(),UNIQUE(ad_id,revision));
ALTER TABLE advertisements ADD FOREIGN KEY(content_version) REFERENCES advertisement_content_versions,ADD FOREIGN KEY(policy_version) REFERENCES advertisement_policy_versions;
CREATE TRIGGER advertisement_content_immutable BEFORE UPDATE OR DELETE ON advertisement_content_versions FOR EACH ROW EXECUTE FUNCTION source_routing_immutable();
CREATE TRIGGER advertisement_policy_immutable BEFORE UPDATE OR DELETE ON advertisement_policy_versions FOR EACH ROW EXECUTE FUNCTION source_routing_immutable();
CREATE FUNCTION advertisement_owner_fixed() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.owner_unit_id<>OLD.owner_unit_id THEN RAISE EXCEPTION 'advertisement ownership is immutable';END IF;RETURN NEW;END $$;
CREATE TRIGGER advertisement_owner_fixed BEFORE UPDATE ON advertisements FOR EACH ROW EXECUTE FUNCTION advertisement_owner_fixed();
CREATE TABLE source_advertisement_configs(route_id text PRIMARY KEY,revision bigint NOT NULL DEFAULT 0,config jsonb NOT NULL DEFAULT '{"mode":"none","fixed_ad_id":null,"insertion_position":"after","unavailable_policy":"without_ad","auto_recover":true}');
CREATE TABLE source_advertisement_config_versions(route_id text NOT NULL,revision bigint NOT NULL,config jsonb NOT NULL,actor_id uuid NOT NULL REFERENCES accounts,reason text NOT NULL,created_at timestamptz NOT NULL DEFAULT clock_timestamp(),PRIMARY KEY(route_id,revision));
CREATE TRIGGER advertisement_config_immutable BEFORE UPDATE OR DELETE ON source_advertisement_config_versions FOR EACH ROW EXECUTE FUNCTION source_routing_immutable();
INSERT INTO source_advertisement_configs(route_id) SELECT route_id FROM source_routes;
CREATE FUNCTION advertisement_route_created() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN INSERT INTO source_advertisement_configs(route_id) VALUES(NEW.route_id);RETURN NEW;END $$;
CREATE TRIGGER advertisement_route_created AFTER INSERT ON source_routes FOR EACH ROW EXECUTE FUNCTION advertisement_route_created();
CREATE TABLE source_message_compositions(id uuid PRIMARY KEY,receipt_id uuid NOT NULL REFERENCES source_receipts,generation bigint NOT NULL,supersedes uuid REFERENCES source_message_compositions,selection jsonb NOT NULL,content text NOT NULL,content_digest text NOT NULL,composer text NOT NULL,message_snapshot_id uuid NOT NULL REFERENCES distribution_message_snapshots,common_check uuid NOT NULL REFERENCES keyword_check_results,workspace_check uuid NOT NULL REFERENCES keyword_check_results,final_check uuid NOT NULL REFERENCES keyword_check_results,reason text NOT NULL,created_at timestamptz NOT NULL DEFAULT clock_timestamp(),UNIQUE(receipt_id,generation));
CREATE INDEX advertising_composition_receipt ON source_message_compositions(receipt_id,generation DESC);
CREATE INDEX advertising_composition_ad ON source_message_compositions((selection->>'ad_id'));
CREATE TRIGGER advertisement_composition_immutable BEFORE UPDATE OR DELETE ON source_message_compositions FOR EACH ROW EXECUTE FUNCTION source_routing_immutable();
ALTER TABLE source_execution_tasks ADD COLUMN generation bigint NOT NULL DEFAULT 0,ADD COLUMN composition_id uuid REFERENCES source_message_compositions;
CREATE TABLE source_execution_generations(receipt_id uuid NOT NULL REFERENCES source_receipts,generation bigint NOT NULL,task_id uuid NOT NULL UNIQUE,channel text NOT NULL,workspace_id uuid NOT NULL,composition_id uuid REFERENCES source_message_compositions,created_at timestamptz NOT NULL DEFAULT clock_timestamp(),PRIMARY KEY(receipt_id,generation));
INSERT INTO source_execution_generations(receipt_id,generation,task_id,channel,workspace_id) SELECT receipt_id,0,task_id,channel,workspace_id FROM source_execution_tasks;
CREATE TRIGGER execution_generation_immutable BEFORE UPDATE OR DELETE ON source_execution_generations FOR EACH ROW EXECUTE FUNCTION source_routing_immutable();
CREATE FUNCTION source_execution_generation_saved() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF TG_OP='INSERT' OR NEW.task_id<>OLD.task_id THEN INSERT INTO source_execution_generations(receipt_id,generation,task_id,channel,workspace_id,composition_id) VALUES(NEW.receipt_id,NEW.generation,NEW.task_id,NEW.channel,NEW.workspace_id,NEW.composition_id);END IF;RETURN NEW;END $$;
CREATE TRIGGER source_execution_generation_saved AFTER INSERT OR UPDATE ON source_execution_tasks FOR EACH ROW EXECUTE FUNCTION source_execution_generation_saved();
ALTER TABLE keyword_admissions ADD COLUMN composition_id uuid REFERENCES source_message_compositions;
CREATE TABLE advertisement_recheck_jobs(id uuid PRIMARY KEY,ad_id uuid REFERENCES advertisements,route_id text,after_receipt uuid,due timestamptz NOT NULL DEFAULT clock_timestamp(),complete boolean NOT NULL DEFAULT false,last_error text,attempts bigint NOT NULL DEFAULT 0,created_at timestamptz NOT NULL DEFAULT clock_timestamp());
CREATE INDEX advertisement_rechecks_due ON advertisement_recheck_jobs(due,id) WHERE NOT complete;
CREATE TABLE advertisement_target_blocks(receipt_id uuid NOT NULL REFERENCES source_receipts,target_id uuid NOT NULL,reason text NOT NULL,PRIMARY KEY(receipt_id,target_id));
-- Existing generation zero records retain their original snapshots and evidence; no fabricated passed checks.

-- Old children remain attributable after replacement, including late observations.
CREATE OR REPLACE FUNCTION source_summary_dirty() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE obj jsonb; item uuid; task uuid; receipt uuid;
BEGIN
 obj=to_jsonb(NEW);
 IF TG_TABLE_NAME='source_execution_tasks' THEN receipt=(obj->>'receipt_id')::uuid;
 ELSIF TG_TABLE_NAME IN ('mobile_v9_attempts','mobile_v9_dispatch_state') THEN item=(obj->>'item_id')::uuid;
 ELSIF TG_TABLE_NAME='mobile_execution_grants' THEN SELECT item_id INTO item FROM mobile_v9_attempts WHERE id=(obj->>'attempt_id')::uuid;
 ELSE task=COALESCE((obj->>'task_id')::uuid,(obj->>'id')::uuid);
 END IF;
 IF item IS NOT NULL THEN SELECT task_id INTO task FROM mobile_v9_task_items WHERE id=item; END IF;
 IF task IS NOT NULL THEN SELECT receipt_id INTO receipt FROM source_execution_generations WHERE task_id=task; END IF;
 IF receipt IS NOT NULL THEN INSERT INTO source_summary_queue VALUES(receipt) ON CONFLICT DO NOTHING; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER source_task_update_summary AFTER UPDATE ON source_execution_tasks FOR EACH ROW EXECUTE FUNCTION source_summary_dirty();
CREATE TABLE source_advertisement_decisions(id uuid PRIMARY KEY,receipt_id uuid NOT NULL REFERENCES source_receipts,selection jsonb NOT NULL,reason text NOT NULL,decision_digest text NOT NULL,composition_id uuid REFERENCES source_message_compositions,created_at timestamptz NOT NULL DEFAULT clock_timestamp(),UNIQUE(receipt_id,decision_digest));
CREATE INDEX advertisement_decisions_receipt ON source_advertisement_decisions(receipt_id,created_at DESC,id DESC);
CREATE TRIGGER advertisement_decision_immutable BEFORE UPDATE OR DELETE ON source_advertisement_decisions FOR EACH ROW EXECUTE FUNCTION source_routing_immutable();
