-- Phone-domain intake survives task creation rollback; observations never authorize execution.
CREATE TABLE mobile_dispatch_intakes (
 id uuid PRIMARY KEY, source_kind text NOT NULL CHECK(source_kind IN ('source_route','manual')),
 message_id uuid, receipt_id uuid, configuration_id uuid, target_id uuid, target_name text NOT NULL,
 delegate_id uuid UNIQUE, snapshot_id uuid NOT NULL, plan jsonb NOT NULL, content text NOT NULL,
 payload_hash text NOT NULL, task_id uuid, item_id uuid UNIQUE,
 creation_state text NOT NULL DEFAULT 'registered', reason_code text,
 next_retry_at timestamptz NOT NULL DEFAULT clock_timestamp(), created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 updated_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE UNIQUE INDEX mobile_auto_intake_key ON mobile_dispatch_intakes(message_id,configuration_id,target_id) NULLS NOT DISTINCT WHERE source_kind='source_route';
CREATE INDEX mobile_intake_time ON mobile_dispatch_intakes(created_at DESC,id DESC);
CREATE INDEX mobile_intake_source_time ON mobile_dispatch_intakes(source_kind,created_at DESC,id DESC);
CREATE INDEX mobile_intake_message ON mobile_dispatch_intakes(message_id,created_at DESC,id DESC);
CREATE INDEX mobile_intake_configuration ON mobile_dispatch_intakes(configuration_id,created_at DESC,id DESC);
CREATE INDEX mobile_intake_target ON mobile_dispatch_intakes(target_id,created_at DESC,id DESC);
CREATE INDEX mobile_intake_task ON mobile_dispatch_intakes(task_id);
CREATE INDEX mobile_intake_receipt ON mobile_dispatch_intakes(receipt_id);
CREATE INDEX mobile_intake_due ON mobile_dispatch_intakes(next_retry_at,id) WHERE creation_state IN ('registered','processing','retrying');
CREATE TABLE mobile_dispatch_operations (
 id uuid PRIMARY KEY, intake_id uuid NOT NULL REFERENCES mobile_dispatch_intakes(id), kind text NOT NULL CHECK(kind IN ('create','evaluate')),
 round_no integer, attempt_id uuid, result text NOT NULL DEFAULT 'started', reason_code text,
 started_at timestamptz NOT NULL DEFAULT clock_timestamp(), finished_at timestamptz, retry_at timestamptz
);
CREATE INDEX mobile_operation_intake ON mobile_dispatch_operations(intake_id,started_at DESC,id DESC);
CREATE INDEX mobile_operation_started ON mobile_dispatch_operations(started_at,id) WHERE result='started';
CREATE TABLE mobile_delivery_exchanges (
 id uuid PRIMARY KEY, device_id uuid NOT NULL, generation bigint NOT NULL, key_digest text NOT NULL,
 replay_of uuid REFERENCES mobile_delivery_exchanges(id), intake_id uuid REFERENCES mobile_dispatch_intakes(id),
 attempt_id uuid, execution_id uuid, result text NOT NULL DEFAULT 'started', reason_code text,
 response_state text NOT NULL DEFAULT 'not_started', created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 finished_at timestamptz, response_at timestamptz
);
CREATE INDEX mobile_exchange_device ON mobile_delivery_exchanges(device_id,created_at DESC,id DESC);
CREATE INDEX mobile_exchange_intake ON mobile_delivery_exchanges(intake_id,created_at DESC,id DESC);
CREATE INDEX mobile_exchange_key ON mobile_delivery_exchanges(device_id,generation,key_digest,created_at DESC);
CREATE TABLE mobile_dispatch_checks (
 id uuid PRIMARY KEY, operation_id uuid REFERENCES mobile_dispatch_operations(id), delivery_request_id uuid REFERENCES mobile_delivery_exchanges(id),
 intake_id uuid REFERENCES mobile_dispatch_intakes(id), device_id uuid, check_kind text NOT NULL,
 decision text NOT NULL, reason_code text NOT NULL, evidence jsonb NOT NULL DEFAULT '{}',
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(), CHECK(num_nonnulls(operation_id,delivery_request_id)=1)
);
CREATE INDEX mobile_check_operation ON mobile_dispatch_checks(operation_id,created_at,id);
CREATE INDEX mobile_check_exchange ON mobile_dispatch_checks(delivery_request_id,created_at,id);
CREATE INDEX mobile_check_intake_reason ON mobile_dispatch_checks(intake_id,reason_code,created_at DESC,id DESC);
CREATE INDEX mobile_check_device ON mobile_dispatch_checks(device_id,created_at DESC,id DESC);
ALTER TABLE mobile_v9_dispatch_events ALTER COLUMN item_id DROP NOT NULL;
ALTER TABLE mobile_v9_dispatch_events ALTER COLUMN round_no DROP NOT NULL;
ALTER TABLE mobile_v9_dispatch_events ADD COLUMN intake_id uuid REFERENCES mobile_dispatch_intakes(id),
 ADD COLUMN operation_id uuid REFERENCES mobile_dispatch_operations(id),
 ADD COLUMN delivery_request_id uuid REFERENCES mobile_delivery_exchanges(id),
 ADD COLUMN execution_id uuid, ADD COLUMN result text NOT NULL DEFAULT 'recorded',
 ADD COLUMN evidence jsonb NOT NULL DEFAULT '{}';
CREATE INDEX mobile_event_intake ON mobile_v9_dispatch_events(intake_id,created_at DESC,id DESC);
CREATE INDEX mobile_event_reason ON mobile_v9_dispatch_events(reason,created_at DESC,id DESC);
CREATE FUNCTION mobile_record_event_context() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 NEW.intake_id=coalesce(NEW.intake_id,(SELECT id FROM mobile_dispatch_intakes WHERE item_id=NEW.item_id));
 NEW.operation_id=coalesce(NEW.operation_id,nullif(current_setting('wekeeper.mobile_operation',true),'')::uuid);
 NEW.delivery_request_id=coalesce(NEW.delivery_request_id,nullif(current_setting('wekeeper.mobile_exchange',true),'')::uuid);
 RETURN NEW;
END $$;
CREATE TRIGGER mobile_event_context BEFORE INSERT ON mobile_v9_dispatch_events FOR EACH ROW EXECUTE FUNCTION mobile_record_event_context();
CREATE FUNCTION mobile_record_grant() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE iid uuid; tid uuid; ev text; why text;
BEGIN
 SELECT a.item_id,i.id INTO iid,tid FROM mobile_v9_attempts a JOIN mobile_dispatch_intakes i ON i.item_id=a.item_id WHERE a.id=NEW.attempt_id;
 IF tid IS NULL THEN RETURN NEW; END IF;
 IF TG_OP='INSERT' THEN ev='execution_generated'; why='execution_generated';
 ELSIF NEW.result_id IS DISTINCT FROM OLD.result_id AND NEW.result_id IS NOT NULL THEN ev='execution_result'; why=coalesce(NEW.result_body->>'reason',NEW.result_body->>'outcome','unknown');
 ELSIF NEW.accepted_at IS DISTINCT FROM OLD.accepted_at AND NEW.accepted_at IS NOT NULL THEN ev='device_received'; why='device_received';
 ELSIF NEW.state IS DISTINCT FROM OLD.state THEN ev='execution_'||NEW.state; why=coalesce(NEW.superseded_reason,NEW.state);
 ELSE RETURN NEW; END IF;
 INSERT INTO mobile_v9_dispatch_events(intake_id,item_id,attempt_id,device_id,execution_id,kind,reason,result,evidence)
 VALUES(tid,iid,NEW.attempt_id,NEW.device_id,NEW.id,ev,why,coalesce(NEW.result_body->>'outcome','recorded'),jsonb_build_object('accepted_at',NEW.accepted_at,'result_deadline_at',NEW.result_deadline_at,'execute_before',NEW.execute_before));
 RETURN NEW;
END $$;
CREATE TRIGGER mobile_record_grant AFTER INSERT OR UPDATE ON mobile_execution_grants FOR EACH ROW EXECUTE FUNCTION mobile_record_grant();
CREATE FUNCTION mobile_record_dispatch() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF (NEW.state,NEW.reason) IS DISTINCT FROM (OLD.state,OLD.reason) THEN
 INSERT INTO mobile_v9_dispatch_events(item_id,round_no,kind,reason,evidence)
 VALUES(NEW.item_id,least(NEW.completed_rounds+1,3),'dispatch_state_changed',NEW.reason,
 jsonb_build_object('state',NEW.state,'completed_rounds',NEW.completed_rounds,'max_rounds',NEW.max_rounds,'next_retry_at',NEW.next_retry_at,'round_started_at',NEW.round_started_at));
 END IF; RETURN NEW;
END $$;
CREATE TRIGGER mobile_record_dispatch AFTER UPDATE ON mobile_v9_dispatch_state FOR EACH ROW EXECUTE FUNCTION mobile_record_dispatch();
CREATE VIEW mobile_dispatch_record_view AS
SELECT i.id,i.source_kind,i.message_id,i.receipt_id,i.configuration_id,i.target_id,i.target_name,
 i.task_id,i.item_id,i.created_at,i.creation_state,left(i.content,80) AS content_summary,
 p.device_id,device.name AS device_name,a.id AS attempt_id,a.attempt_no,g.id AS execution_id,
 g.assigned_at AS generated_at,g.accepted_at AS received_at,g.result_deadline_at,
 ds.round_started_at+interval '5 minutes' AS round_deadline_at,ds.completed_rounds,ds.max_rounds,
 CASE WHEN i.creation_state IN ('registered','processing','retrying') THEN i.next_retry_at ELSE ds.next_retry_at END AS next_evaluation_at,
 CASE WHEN history.sent THEN 'sent' WHEN i.creation_state='rejected' THEN 'failed' WHEN i.creation_state<>'created' THEN 'pending'
 WHEN mt.stopped THEN 'failed' ELSE coalesce(v.outcome,CASE WHEN ds.state='completed' THEN 'sent' WHEN ds.state IN ('stopped','confirmation_expired','dispatch_failed') THEN 'failed' WHEN ds.state='needs_review' THEN 'unknown' ELSE 'pending' END) END AS outcome,
 CASE WHEN history.sent THEN 'sent' WHEN i.creation_state<>'created' THEN i.creation_state
 WHEN mt.stopped THEN 'stopped' WHEN ds.state='waiting_resources' THEN 'round_wait' WHEN coalesce(v.outcome,'pending')<>'pending' THEN v.outcome
 WHEN ds.state IN ('completed','stopped','confirmation_expired','dispatch_failed','needs_review') THEN ds.state
 WHEN g.state='active' AND g.accepted_at IS NOT NULL THEN 'executing'
 WHEN g.state='active' THEN 'awaiting_receipt' ELSE 'awaiting_pull' END AS stage,
 CASE WHEN i.creation_state<>'created' THEN coalesce(i.reason_code,'intake_registered')
 WHEN ds.state='waiting_resources' THEN ds.reason ELSE coalesce(v.reason_code,ds.reason,'awaiting_dispatch') END AS reason_code,
 greatest(i.updated_at,ds.progress_at,a.progress_at,g.assigned_at,g.accepted_at,g.result_received_at) AS last_progress_at,
 (SELECT max(o.finished_at) FROM mobile_dispatch_operations o WHERE o.intake_id=i.id AND o.kind='evaluate') AS last_checked_at,
 (coalesce(v.historical_risk,false) OR coalesce(history.risk,false)) AS historical_risk,
 coalesce((SELECT jsonb_agg(jsonb_build_object('device_id',c.device_id,'device_name',d.name,'reason_code',c.reason_code,'checked_at',c.created_at) ORDER BY c.created_at,c.id) FROM mobile_dispatch_checks c LEFT JOIN execution_devices d ON d.id=c.device_id WHERE c.operation_id=(SELECT o.id FROM mobile_dispatch_operations o WHERE o.intake_id=i.id AND o.kind='evaluate' ORDER BY o.started_at DESC,o.id DESC LIMIT 1) AND c.decision<>'passed'),'[]'::jsonb) AS candidate_reasons,
 md5(concat_ws('|',i.creation_state,i.reason_code,i.updated_at,ds.progress_at,a.id,g.id,g.state,g.accepted_at,g.result_id,ds.next_retry_at)) AS version
FROM mobile_dispatch_intakes i
LEFT JOIN mobile_v9_tasks mt ON mt.id=i.task_id
LEFT JOIN LATERAL(SELECT bool_or(a.state IN ('observed_sent','manually_confirmed_sent') OR g.result_body->>'outcome'='observed_sent') AS sent,bool_or(NOT a.current AND (a.state='unknown' OR g.result_body->>'outcome'='unknown')) AS risk FROM mobile_v9_attempts a LEFT JOIN mobile_execution_grants g ON g.attempt_id=a.id WHERE a.item_id=i.item_id)history ON true
LEFT JOIN mobile_v9_dispatch_state ds ON ds.item_id=i.item_id
LEFT JOIN mobile_v9_attempts a ON a.item_id=i.item_id AND a.current
LEFT JOIN mobile_v9_plans p ON p.id=a.plan_id
LEFT JOIN execution_devices device ON device.id=p.device_id
LEFT JOIN mobile_execution_grants g ON g.attempt_id=a.id
LEFT JOIN orchestration_delivery_progress v ON v.message_id=i.message_id AND v.channel='mobile' AND v.target_id=i.target_id;

-- Waiting for the round is a scheduling decision, independent of current phone presence.
CREATE OR REPLACE VIEW orchestration_delivery_progress AS
WITH planned AS (
 SELECT d.message_id,plan->>'channel' AS channel,target,
 (plan->>'id')::uuid AS delegate_id,NULL::uuid AS task_id,
 CASE WHEN d.state='failed' THEN 'failed' ELSE 'planning' END AS delegate_state,
 coalesce(d.reason_code,d.last_error_code) AS delegate_reason,d.stage_entered_at AS created_at,
 d.next_attempt_at AS process_retry_at,d.last_error_code AS process_error
 FROM orchestration_dispatches d CROSS JOIN LATERAL jsonb_array_elements(d.plans) plan
 CROSS JOIN LATERAL jsonb_array_elements(plan->'targets') target
 WHERE d.state IN ('preparing','ready','failed')
 UNION ALL
 SELECT d.message_id,d.channel,target,d.id,d.task_id,d.state,d.reason_code,d.created_at,NULL::timestamptz,NULL::text
 FROM orchestration_delegates d CROSS JOIN LATERAL jsonb_array_elements(d.plan->'targets') target
), frozen AS (
 SELECT DISTINCT ON (message_id,channel,target->>'id')
 message_id,channel,(target->>'id')::uuid AS target_id,target->>'name' AS target_name,
 delegate_id,task_id,delegate_state,delegate_reason,created_at,process_retry_at,process_error,target
 FROM planned
 ORDER BY message_id,channel,target->>'id',CASE delegate_state WHEN 'delegated' THEN 0 WHEN 'failed' THEN 1 ELSE 2 END,created_at,delegate_id
), facts AS (
 SELECT f.*,
 i.id AS item_id,ds.state AS dispatch_state,ds.reason AS dispatch_reason,coalesce(ds.next_retry_at,f.process_retry_at) AS next_retry_at,
 a.id AS attempt_id,a.state AS attempt_state,a.attempt_no,p.device_id,
 presence.state AS device_presence,device.enabled AS device_enabled,
 EXISTS(SELECT 1 FROM mobile_execution_grants lock WHERE lock.device_id=p.device_id AND NOT lock.session_closed AND lock.attempt_id IS DISTINCT FROM a.id) AS device_busy,
 g.assigned_at,g.accepted_at,g.result_deadline_at,g.state AS grant_state,
 COALESCE(mt.stopped,false) AS mobile_stopped,
 COALESCE(history.sent,false) AS sent,COALESCE(history.risk,false) AS historical_risk,
 x.id AS official_target_id,x.official_status,x.rejected,b.state AS batch_state,
 b.stop_state,b.error_code,b.sync_error,b.claimed_at,b.last_synced_at,b.next_sync_at,b.sender,
 greatest(f.created_at,ds.progress_at,a.progress_at,g.assigned_at,g.accepted_at,g.result_received_at,x.progress_at,b.progress_at) AS last_progress_at
 FROM frozen f
 LEFT JOIN mobile_v9_task_items i ON f.channel='mobile' AND i.task_id=f.task_id AND i.target_id=f.target_id
 LEFT JOIN mobile_v9_tasks mt ON mt.id=i.task_id
 LEFT JOIN mobile_v9_dispatch_state ds ON ds.item_id=i.id
 LEFT JOIN mobile_v9_attempts a ON a.item_id=i.id AND a.current
 LEFT JOIN mobile_v9_plans p ON p.id=a.plan_id
 LEFT JOIN execution_devices device ON device.id=p.device_id
 LEFT JOIN device_control_presence presence ON presence.id=p.device_id
 LEFT JOIN mobile_execution_grants g ON g.attempt_id=a.id
 LEFT JOIN LATERAL (
  SELECT bool_or(h.state IN ('observed_sent','manually_confirmed_sent') OR hg.result_body->>'outcome'='observed_sent') AS sent,
  bool_or(NOT h.current AND (h.state='unknown' OR hg.result_body->>'outcome'='unknown')) AS risk
  FROM mobile_v9_attempts h LEFT JOIN mobile_execution_grants hg ON hg.attempt_id=h.id WHERE h.item_id=i.id
 ) history ON f.channel='mobile'
 LEFT JOIN broadcast_targets x ON f.channel='wecom' AND x.task_id=f.task_id AND x.group_id=f.target_id
 LEFT JOIN broadcast_batches b ON b.id=x.batch_id
), outcomes AS (
 SELECT *,CASE
 WHEN channel='mobile' AND (sent OR dispatch_state='completed') OR channel='wecom' AND official_status=1 THEN 'sent'
 WHEN delegate_state='failed' THEN 'failed'
 WHEN channel='mobile' THEN CASE
  WHEN mobile_stopped OR dispatch_state IN ('dispatch_failed','stopped','confirmation_expired') THEN 'failed'
  WHEN dispatch_state='needs_review' THEN 'unknown'
  WHEN dispatch_state IS NULL AND attempt_state='unknown' THEN 'unknown'
  WHEN dispatch_state IS NULL AND attempt_state='cancelled' THEN 'failed'
  ELSE 'pending' END
 WHEN batch_state='unknown' OR stop_state='unknown' OR (official_status IS NOT NULL AND official_status NOT IN (0,1,2,3)) THEN 'unknown'
 WHEN batch_state IN ('rejected','cancelled') OR stop_state='stopped' OR rejected OR official_status IN (2,3) THEN 'failed'
 ELSE 'pending' END AS outcome
 FROM facts
), phases AS (
 SELECT *,CASE
 WHEN delegate_state='planning' THEN 'creating_tasks'
 WHEN outcome<>'pending' THEN outcome
 WHEN channel='mobile' AND (item_id IS NULL OR attempt_id IS NULL) OR channel='wecom' AND official_target_id IS NULL THEN 'missing_evidence'
 WHEN channel='mobile' AND grant_state='active' AND accepted_at IS NOT NULL THEN 'device_execution'
 WHEN channel='mobile' AND grant_state='active' THEN 'device_receipt'
 WHEN channel='mobile' THEN 'scheduling'
 WHEN batch_state='created' THEN 'official_result'
 WHEN batch_state='submitting' THEN 'official_call'
 ELSE 'official_queue' END AS stage
 FROM outcomes
)
SELECT *,coalesce(delegate_reason,CASE
 WHEN stage IN ('device_receipt','device_execution','sent','unknown') THEN stage
 WHEN channel='mobile' AND outcome='failed' AND mobile_stopped OR channel='wecom' AND outcome='failed' AND stop_state='stopped' THEN 'task_stopped'
 WHEN channel='mobile' AND stage='scheduling' AND dispatch_reason='awaiting_round_deadline' THEN dispatch_reason
 WHEN channel='mobile' AND stage='scheduling' AND device_presence='offline' THEN 'device_offline'
 WHEN channel='mobile' AND stage='scheduling' AND device_enabled=false THEN 'device_disabled'
 WHEN channel='mobile' AND stage='scheduling' AND device_busy THEN 'device_busy'
 WHEN channel='mobile' THEN dispatch_reason ELSE error_code END,stage) AS reason_code,
 CASE
 WHEN stage='missing_evidence' THEN 'unobserved'
 WHEN stage='creating_tasks' AND process_error IS NOT NULL THEN 'retrying'
 WHEN outcome='unknown' THEN 'attention'
 WHEN outcome IN ('sent','failed') THEN 'terminal'
 WHEN channel='mobile' AND grant_state='active' AND result_deadline_at+make_interval(secs=>coalesce(nullif(current_setting('wekeeper.progress_stall_seconds',true),'')::integer,60))<transaction_timestamp() THEN 'attention'
 WHEN stage='scheduling' AND next_retry_at+make_interval(secs=>coalesce(nullif(current_setting('wekeeper.progress_stall_seconds',true),'')::integer,60))<transaction_timestamp() THEN 'attention'
 WHEN stage='official_result' AND next_sync_at+make_interval(secs=>coalesce(nullif(current_setting('wekeeper.progress_stall_seconds',true),'')::integer,60))<transaction_timestamp() THEN 'attention'
 WHEN channel='wecom' AND sync_error IS NOT NULL THEN 'retrying'
 WHEN stage IN ('creating_tasks','scheduling','official_queue','official_call') AND last_progress_at+make_interval(secs=>coalesce(nullif(current_setting('wekeeper.progress_stall_seconds',true),'')::integer,60))<transaction_timestamp() AND coalesce(next_retry_at,next_sync_at,last_progress_at)<=transaction_timestamp() THEN 'attention'
 ELSE 'normal' END AS severity
FROM phases;

CREATE FUNCTION mobile_record_attempt() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF (NEW.state,NEW.current) IS DISTINCT FROM (OLD.state,OLD.current) THEN
 INSERT INTO mobile_v9_dispatch_events(intake_id,item_id,attempt_id,device_id,kind,reason,result,evidence)
 SELECT i.id,NEW.item_id,NEW.id,p.device_id,'attempt_state_changed',coalesce(NEW.termination_reason,NEW.invalidation_reason,NEW.state),NEW.state,
 jsonb_build_object('previous_state',OLD.state,'current',NEW.current,'attempt_no',NEW.attempt_no)
 FROM mobile_dispatch_intakes i JOIN mobile_v9_plans p ON p.id=NEW.plan_id WHERE i.item_id=NEW.item_id;
 END IF; RETURN NEW;
END $$;
CREATE TRIGGER mobile_record_attempt AFTER UPDATE ON mobile_v9_attempts FOR EACH ROW EXECUTE FUNCTION mobile_record_attempt();
CREATE FUNCTION mobile_record_review() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 INSERT INTO mobile_v9_dispatch_events(intake_id,item_id,attempt_id,kind,reason,result,evidence)
 SELECT i.id,a.item_id,a.id,'manual_review',NEW.decision,NEW.decision,jsonb_build_object('review_id',NEW.id,'reviewer',NEW.reviewer)
 FROM mobile_v9_attempts a JOIN mobile_dispatch_intakes i ON i.item_id=a.item_id WHERE a.id=NEW.attempt_id;
 RETURN NEW;
END $$;
CREATE TRIGGER mobile_record_review AFTER INSERT ON mobile_v9_reviews FOR EACH ROW EXECUTE FUNCTION mobile_record_review();
CREATE FUNCTION mobile_record_stop() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.stopped AND NOT OLD.stopped THEN
 INSERT INTO mobile_v9_dispatch_events(intake_id,item_id,kind,reason,result,evidence)
 SELECT id,item_id,'task_stopped','task_stopped','stopped',jsonb_build_object('task_id',NEW.id) FROM mobile_dispatch_intakes WHERE task_id=NEW.id;
 END IF; RETURN NEW;
END $$;
CREATE TRIGGER mobile_record_stop AFTER UPDATE ON mobile_v9_tasks FOR EACH ROW EXECUTE FUNCTION mobile_record_stop();
CREATE FUNCTION mobile_record_reassignment() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 INSERT INTO mobile_v9_dispatch_events(intake_id,item_id,attempt_id,device_id,kind,reason,result,evidence)
 SELECT i.id,a.item_id,a.id,p.device_id,'assignment_changed',CASE WHEN NEW.source='automatic' THEN NEW.reason ELSE 'manual_reassignment' END,'assigned',
 jsonb_build_object('reassignment_id',NEW.id,'old_attempt_id',NEW.old_attempt_id,'new_attempt_id',NEW.new_attempt_id,'source',NEW.source)
 FROM mobile_v9_attempts a JOIN mobile_v9_plans p ON p.id=a.plan_id JOIN mobile_dispatch_intakes i ON i.item_id=a.item_id WHERE a.id=NEW.new_attempt_id;
 RETURN NEW;
END $$;
CREATE TRIGGER mobile_record_reassignment AFTER INSERT ON mobile_v9_reassignments FOR EACH ROW EXECUTE FUNCTION mobile_record_reassignment();
CREATE TRIGGER mobile_dispatch_event_immutable BEFORE UPDATE OR DELETE ON mobile_v9_dispatch_events FOR EACH ROW EXECUTE FUNCTION mobile_capability_evidence_immutable();
