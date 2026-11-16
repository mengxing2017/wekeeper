-- Read-only progress evidence; never participates in execution eligibility.
ALTER TABLE orchestration_dispatches ADD COLUMN stage_entered_at timestamptz NOT NULL DEFAULT clock_timestamp(), ADD COLUMN last_error_code text;
ALTER TABLE mobile_v9_dispatch_state ADD COLUMN progress_at timestamptz NOT NULL DEFAULT clock_timestamp();
ALTER TABLE mobile_v9_attempts ADD COLUMN progress_at timestamptz NOT NULL DEFAULT clock_timestamp();
ALTER TABLE broadcast_batches ADD COLUMN progress_at timestamptz NOT NULL DEFAULT clock_timestamp();
ALTER TABLE broadcast_targets ADD COLUMN progress_at timestamptz NOT NULL DEFAULT clock_timestamp();
CREATE FUNCTION message_progress_clock() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_TABLE_NAME='orchestration_dispatches' THEN
  IF NEW.state IS DISTINCT FROM OLD.state THEN NEW.stage_entered_at=clock_timestamp(); END IF;
 ELSIF TG_TABLE_NAME='mobile_v9_dispatch_state' THEN
  IF (NEW.state,NEW.reason,NEW.candidate_cursor) IS DISTINCT FROM (OLD.state,OLD.reason,OLD.candidate_cursor) THEN NEW.progress_at=clock_timestamp(); END IF;
 ELSIF TG_TABLE_NAME='mobile_v9_attempts' THEN
  IF (NEW.state,NEW.current) IS DISTINCT FROM (OLD.state,OLD.current) THEN NEW.progress_at=clock_timestamp(); END IF;
 ELSIF TG_TABLE_NAME='broadcast_batches' THEN
  IF (NEW.state,NEW.error_code,NEW.stop_state,NEW.msgid) IS DISTINCT FROM (OLD.state,OLD.error_code,OLD.stop_state,OLD.msgid) THEN NEW.progress_at=clock_timestamp(); END IF;
 ELSIF TG_TABLE_NAME='broadcast_targets' THEN
  IF (NEW.rejected,NEW.official_status) IS DISTINCT FROM (OLD.rejected,OLD.official_status) THEN NEW.progress_at=clock_timestamp(); END IF;
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER message_dispatch_progress BEFORE UPDATE ON orchestration_dispatches FOR EACH ROW EXECUTE FUNCTION message_progress_clock();
CREATE TRIGGER message_mobile_progress BEFORE UPDATE ON mobile_v9_dispatch_state FOR EACH ROW EXECUTE FUNCTION message_progress_clock();
CREATE TRIGGER message_attempt_progress BEFORE UPDATE ON mobile_v9_attempts FOR EACH ROW EXECUTE FUNCTION message_progress_clock();
CREATE TRIGGER message_batch_progress BEFORE UPDATE ON broadcast_batches FOR EACH ROW EXECUTE FUNCTION message_progress_clock();
CREATE TRIGGER message_target_progress BEFORE UPDATE ON broadcast_targets FOR EACH ROW EXECUTE FUNCTION message_progress_clock();

-- Stable unit: message + channel + frozen target. Current attempt and historical
-- uncertainty are separate, so an old unknown cannot conceal an active successor.
CREATE VIEW orchestration_delivery_progress AS
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
CREATE INDEX orchestration_receipt_lookup ON orchestration_messages(evidence_ref);
CREATE FUNCTION message_grant_progress_changed() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 INSERT INTO orchestration_result_queue
 SELECT d.message_id FROM mobile_v9_task_items i JOIN orchestration_delegates d ON d.task_id=i.task_id
 WHERE i.id=(SELECT item_id FROM mobile_v9_attempts WHERE id=NEW.attempt_id)
 ON CONFLICT DO NOTHING;
 RETURN NEW;
END $$;
CREATE TRIGGER message_grant_progress AFTER INSERT OR UPDATE ON mobile_execution_grants FOR EACH ROW EXECUTE FUNCTION message_grant_progress_changed();

CREATE INDEX orchestration_delegate_message ON orchestration_delegates(message_id);
CREATE INDEX message_mobile_target_lookup ON mobile_v9_task_items(task_id,target_id);
CREATE INDEX message_summary_reconcile ON source_receipts(summary_updated_at,receipt_id);
