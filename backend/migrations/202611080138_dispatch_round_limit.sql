-- Bound both candidate pools and historical singleton scopes without changing authorization.
ALTER TABLE mobile_v9_dispatch_state
 ADD COLUMN infrastructure_failures integer NOT NULL DEFAULT 0 CHECK(infrastructure_failures BETWEEN 0 AND 4),
 ADD COLUMN max_rounds integer NOT NULL DEFAULT 3 CHECK(max_rounds=3),
 ADD COLUMN completed_rounds integer NOT NULL DEFAULT 0 CHECK(completed_rounds BETWEEN 0 AND 3),
 ADD COLUMN round_origin integer NOT NULL DEFAULT 0 CHECK(round_origin>=0),
 ADD COLUMN assigned_at timestamptz,
 ADD COLUMN finished_at timestamptz;
ALTER TABLE mobile_v9_dispatch_state ALTER COLUMN next_retry_at DROP NOT NULL;
ALTER TABLE mobile_v9_dispatch_state DROP CONSTRAINT mobile_v9_dispatch_state_state_check;
ALTER TABLE mobile_v9_dispatch_state ADD CONSTRAINT mobile_v9_dispatch_state_state_check
 CHECK(state IN ('scheduling','assigned','waiting_resources','executing','completed','needs_review','stopped','confirmation_expired','dispatch_failed'));
CREATE TABLE mobile_v9_dispatch_events (
 id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
 item_id uuid NOT NULL REFERENCES mobile_v9_dispatch_state(item_id),
 round_no integer NOT NULL CHECK(round_no BETWEEN 1 AND 3),
 kind text NOT NULL,
 device_id uuid REFERENCES execution_devices(id),
 attempt_id uuid REFERENCES mobile_v9_attempts(id),
 reason text NOT NULL,
 created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE UNIQUE INDEX mobile_v9_dispatch_failed_once ON mobile_v9_dispatch_events(item_id) WHERE kind='dispatch_failed';
-- Start a documented budget at migration time; do not infer old rounds from attempt numbers.
UPDATE mobile_v9_dispatch_state ds SET
 round_origin=coalesce((SELECT (c.ordinality-1)::int FROM mobile_v9_dispatch_scopes sc
 CROSS JOIN LATERAL jsonb_array_elements(sc.candidates) WITH ORDINALITY c(value,ordinality)
 JOIN mobile_v9_attempts a ON a.item_id=sc.item_id AND a.current
 JOIN mobile_v9_plans p ON p.id=a.plan_id
 WHERE sc.item_id=ds.item_id AND c.value->>'device_id'=p.plan->>'device_id' LIMIT 1),0),
 candidate_cursor=CASE WHEN state='waiting_resources' THEN 0 ELSE 1 END,
 assigned_at=NULL,
 finished_at=CASE WHEN state IN ('completed','stopped','confirmation_expired') THEN clock_timestamp() END,
 next_retry_at=CASE WHEN state IN ('completed','stopped','confirmation_expired') THEN NULL ELSE clock_timestamp() END;
INSERT INTO mobile_v9_dispatch_events(item_id,round_no,kind,reason)
 SELECT item_id,1,'retry_budget_initialized','migration_budget_start' FROM mobile_v9_dispatch_state
 WHERE state NOT IN ('completed','stopped','confirmation_expired');
DROP INDEX mobile_v9_dispatch_due;
CREATE INDEX mobile_v9_dispatch_due ON mobile_v9_dispatch_state(next_retry_at,item_id)
 WHERE state NOT IN ('completed','stopped','confirmation_expired','dispatch_failed');
