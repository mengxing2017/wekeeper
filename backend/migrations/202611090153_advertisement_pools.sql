-- Forward-only ordered advertising. Existing snapshots and migrations remain immutable.
LOCK TABLE source_receipts IN ACCESS EXCLUSIVE MODE;
CREATE TABLE source_route_receive_counters(route_id text PRIMARY KEY, last_order bigint NOT NULL CHECK(last_order>=0));
ALTER TABLE source_receipts ADD COLUMN receive_order bigint, ADD COLUMN receive_order_reconstructed boolean NOT NULL DEFAULT false;
WITH numbered AS (SELECT receipt_id,row_number() OVER(PARTITION BY route_id ORDER BY received_at,receipt_id) AS n FROM source_receipts)
UPDATE source_receipts r SET receive_order=n.n,receive_order_reconstructed=true FROM numbered n WHERE r.receipt_id=n.receipt_id;
INSERT INTO source_route_receive_counters SELECT route_id,max(receive_order) FROM source_receipts GROUP BY route_id;
ALTER TABLE source_receipts ALTER COLUMN receive_order SET NOT NULL;
CREATE UNIQUE INDEX source_receipt_receive_order ON source_receipts(route_id,receive_order);
CREATE FUNCTION source_receive_order_allocate() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
  INSERT INTO source_route_receive_counters(route_id,last_order) VALUES(NEW.route_id,1)
    ON CONFLICT(route_id) DO UPDATE SET last_order=source_route_receive_counters.last_order+1 RETURNING last_order INTO NEW.receive_order;
  NEW.receive_order_reconstructed=false;
  RETURN NEW;
END $$;
CREATE TRIGGER source_receive_order_allocate BEFORE INSERT ON source_receipts FOR EACH ROW EXECUTE FUNCTION source_receive_order_allocate();
CREATE TABLE advertisement_pools(id uuid PRIMARY KEY,route_id text NOT NULL UNIQUE,current_revision bigint NOT NULL DEFAULT 0,created_by uuid NOT NULL REFERENCES accounts,created_at timestamptz NOT NULL DEFAULT clock_timestamp());
CREATE TABLE advertisement_pool_revisions(pool_id uuid NOT NULL REFERENCES advertisement_pools,revision bigint NOT NULL,actor_id uuid NOT NULL REFERENCES accounts,reason text NOT NULL,created_at timestamptz NOT NULL DEFAULT clock_timestamp(),PRIMARY KEY(pool_id,revision));
CREATE TABLE advertisement_pool_slots(pool_id uuid NOT NULL,revision bigint NOT NULL,slot_id uuid NOT NULL,position integer NOT NULL CHECK(position>=1 AND position<=1000),ad_id uuid NOT NULL REFERENCES advertisements,PRIMARY KEY(pool_id,revision,slot_id),UNIQUE(pool_id,revision,position),FOREIGN KEY(pool_id,revision) REFERENCES advertisement_pool_revisions);
CREATE INDEX advertisement_pool_slot_ad ON advertisement_pool_slots(ad_id,pool_id,revision);
CREATE TABLE advertisement_pool_cursors(pool_id uuid PRIMARY KEY REFERENCES advertisement_pools,pool_revision bigint NOT NULL DEFAULT 0,cursor_version bigint NOT NULL DEFAULT 0,reset_epoch bigint NOT NULL DEFAULT 0,next_slot_id uuid,last_consumed_slot_id uuid,cycle bigint NOT NULL DEFAULT 0 CHECK(cycle>=0),selection_order bigint NOT NULL DEFAULT 0 CHECK(selection_order>=0));
-- Queue all receipts before any content matching so a slow earlier check cannot be overtaken.
-- receipt_id is also the stable chain_decision_id for the single-route event protocol.
CREATE TABLE advertisement_pool_queue(receipt_id uuid PRIMARY KEY REFERENCES source_receipts,chain_decision_id uuid NOT NULL UNIQUE,route_id text NOT NULL,receive_order bigint NOT NULL,pool_id uuid REFERENCES advertisement_pools,state text NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','deferred','done')),reservation_version bigint NOT NULL DEFAULT 0,reservation_expiry timestamptz,reason text,updated_at timestamptz NOT NULL DEFAULT clock_timestamp());
CREATE INDEX advertisement_pool_queue_head ON advertisement_pool_queue(route_id,state,receive_order,receipt_id);
INSERT INTO advertisement_pool_queue(receipt_id,chain_decision_id,route_id,receive_order,state) SELECT r.receipt_id,r.receipt_id,r.route_id,r.receive_order,CASE WHEN i.state IN ('received','waiting') THEN 'pending' ELSE 'done' END FROM source_receipts r JOIN source_inbox i USING(receipt_id);
CREATE FUNCTION advertisement_pool_queue_received() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
 INSERT INTO advertisement_pool_queue(receipt_id,chain_decision_id,route_id,receive_order) SELECT receipt_id,receipt_id,route_id,receive_order FROM source_receipts WHERE receipt_id=NEW.receipt_id;
 RETURN NEW;
END $$;
CREATE TRIGGER advertisement_pool_queue_received AFTER INSERT ON source_inbox FOR EACH ROW EXECUTE FUNCTION advertisement_pool_queue_received();
CREATE TABLE advertisement_pool_consumptions(id uuid PRIMARY KEY,pool_id uuid NOT NULL REFERENCES advertisement_pools,pool_revision bigint NOT NULL,receipt_id uuid NOT NULL REFERENCES source_receipts,chain_decision_id uuid NOT NULL,selection_order bigint NOT NULL,slot_id uuid NOT NULL,composition_id uuid NOT NULL REFERENCES source_message_compositions,selection jsonb NOT NULL,created_at timestamptz NOT NULL DEFAULT clock_timestamp(),UNIQUE(receipt_id,chain_decision_id),UNIQUE(pool_id,selection_order),FOREIGN KEY(pool_id,pool_revision,slot_id) REFERENCES advertisement_pool_slots(pool_id,revision,slot_id));
CREATE TABLE advertisement_pool_transitions(id uuid PRIMARY KEY,pool_id uuid NOT NULL REFERENCES advertisement_pools,kind text NOT NULL,actor_id uuid NOT NULL REFERENCES accounts,reason text NOT NULL,before_cursor jsonb NOT NULL,after_cursor jsonb NOT NULL,created_at timestamptz NOT NULL DEFAULT clock_timestamp());
CREATE TRIGGER advertisement_pool_revisions_immutable BEFORE UPDATE OR DELETE ON advertisement_pool_revisions FOR EACH ROW EXECUTE FUNCTION source_routing_immutable();
CREATE TRIGGER advertisement_pool_slots_immutable BEFORE UPDATE OR DELETE ON advertisement_pool_slots FOR EACH ROW EXECUTE FUNCTION source_routing_immutable();
CREATE TRIGGER advertisement_pool_consumptions_immutable BEFORE UPDATE OR DELETE ON advertisement_pool_consumptions FOR EACH ROW EXECUTE FUNCTION source_routing_immutable();
CREATE TRIGGER advertisement_pool_transitions_immutable BEFORE UPDATE OR DELETE ON advertisement_pool_transitions FOR EACH ROW EXECUTE FUNCTION source_routing_immutable();
