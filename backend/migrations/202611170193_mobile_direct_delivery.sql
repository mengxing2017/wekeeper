-- One signing authority and one device transport. No pull/wake compatibility path.
ALTER TABLE device_control_sessions ADD COLUMN runtime_epoch uuid;
ALTER TABLE device_control_sessions DROP CONSTRAINT device_control_sessions_transport_check;
ALTER TABLE device_control_sessions ADD CONSTRAINT device_control_sessions_transport_check CHECK(transport='wss');
ALTER TABLE mobile_execution_grants ADD COLUMN receipt_deadline_at timestamptz NOT NULL DEFAULT (clock_timestamp()+interval '60 seconds');
CREATE UNIQUE INDEX mobile_send_attempt_once ON mobile_execution_grants(attempt_id) WHERE kind='send';
CREATE TABLE mobile_execution_delivery_outbox (
 execution_id uuid PRIMARY KEY REFERENCES mobile_execution_grants(id),
 next_send_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 finished_at timestamptz,
 attempts integer NOT NULL DEFAULT 0
);
CREATE INDEX mobile_delivery_due ON mobile_execution_delivery_outbox(next_send_at) WHERE finished_at IS NULL;
ALTER TABLE mobile_delivery_exchanges ADD COLUMN transport text NOT NULL DEFAULT 'wss',
 ADD COLUMN connection_id uuid, ADD COLUMN connection_epoch bigint;
-- Keep the display inbox revision, remove all wake command production.
CREATE OR REPLACE FUNCTION mobile_v9_inbox_changed() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE r record;
BEGIN
 IF TG_TABLE_NAME='mobile_v9_attempts' THEN
  IF TG_OP='UPDATE' AND NEW.state=OLD.state AND NEW.current=OLD.current THEN RETURN NEW; END IF;
  NEW.inbox_changed_at=clock_timestamp();
  FOR r IN SELECT device_id FROM mobile_v9_plans WHERE id=NEW.plan_id LOOP
   INSERT INTO mobile_v9_device_inbox_versions(device_id,revision) VALUES(r.device_id,1)
   ON CONFLICT(device_id) DO UPDATE SET revision=mobile_v9_device_inbox_versions.revision+1;
  END LOOP;
 ELSE
  IF NEW.stopped=OLD.stopped AND NEW.next_eligible_at=OLD.next_eligible_at AND NEW.confirmed_until=OLD.confirmed_until THEN RETURN NEW; END IF;
  FOR r IN SELECT DISTINCT p.device_id FROM mobile_v9_task_items i JOIN mobile_v9_plans p ON p.item_id=i.id WHERE i.task_id=NEW.id LOOP
   INSERT INTO mobile_v9_device_inbox_versions(device_id,revision) VALUES(r.device_id,1)
   ON CONFLICT(device_id) DO UPDATE SET revision=mobile_v9_device_inbox_versions.revision+1;
  END LOOP;
 END IF;
 RETURN NEW;
END $$;
DROP TABLE mobile_v9_task_wake_outbox;
ALTER TABLE device_control_commands DROP CONSTRAINT device_control_commands_kind_check;
ALTER TABLE device_control_commands ADD CONSTRAINT device_control_commands_kind_check
 CHECK(kind IN ('cancel_execution','configuration_changed','request_status'));
CREATE VIEW mobile_delivery_exchange_view AS
 SELECT x.*,g.accepted_at,g.receipt_deadline_at,g.result_deadline_at,g.result_received_at,g.superseded_reason,
 CASE WHEN g.result_id IS NOT NULL THEN 'result_recorded'
      WHEN g.superseded_reason IN ('receipt_timeout','result_timeout') THEN g.superseded_reason
      WHEN g.accepted_at IS NOT NULL THEN 'awaiting_result'
      WHEN g.id IS NOT NULL THEN 'awaiting_receipt' ELSE 'delivery_failed' END AS reception_state
 FROM mobile_delivery_exchanges x LEFT JOIN mobile_execution_grants g ON g.id=x.execution_id;

CREATE OR REPLACE VIEW mobile_dispatch_record_view AS
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
 WHEN g.state='active' THEN 'awaiting_receipt' ELSE 'awaiting_dispatch' END AS stage,
 CASE WHEN i.creation_state<>'created' THEN coalesce(i.reason_code,'intake_registered')
 WHEN ds.state='waiting_resources' THEN ds.reason ELSE coalesce(v.reason_code,ds.reason,'awaiting_dispatch') END AS reason_code,
 greatest(i.updated_at,ds.progress_at,a.progress_at,g.assigned_at,g.accepted_at,g.result_received_at) AS last_progress_at,
 (SELECT max(o.finished_at) FROM mobile_dispatch_operations o WHERE o.intake_id=i.id AND o.kind='evaluate') AS last_checked_at,
 (coalesce(v.historical_risk,false) OR coalesce(history.risk,false)) AS historical_risk,
 coalesce((SELECT jsonb_agg(jsonb_build_object('device_id',c.device_id,'device_name',d.name,'reason_code',c.reason_code,'checked_at',c.created_at) ORDER BY c.created_at,c.id) FROM mobile_dispatch_checks c LEFT JOIN execution_devices d ON d.id=c.device_id WHERE c.operation_id=(SELECT o.id FROM mobile_dispatch_operations o WHERE o.intake_id=i.id AND o.kind='evaluate' ORDER BY o.started_at DESC,o.id DESC LIMIT 1) AND c.decision<>'passed'),'[]'::jsonb) AS candidate_reasons,
 md5(concat_ws('|',i.creation_state,i.reason_code,i.updated_at,ds.progress_at,a.id,g.id,g.state,g.accepted_at,g.result_id,ds.next_retry_at)) AS version,g.receipt_deadline_at
FROM mobile_dispatch_intakes i
LEFT JOIN mobile_v9_tasks mt ON mt.id=i.task_id
LEFT JOIN LATERAL(SELECT bool_or(a.state IN ('observed_sent','manually_confirmed_sent') OR g.result_body->>'outcome'='observed_sent') AS sent,bool_or(NOT a.current AND (a.state='unknown' OR g.result_body->>'outcome'='unknown')) AS risk FROM mobile_v9_attempts a LEFT JOIN mobile_execution_grants g ON g.attempt_id=a.id WHERE a.item_id=i.item_id)history ON true
LEFT JOIN mobile_v9_dispatch_state ds ON ds.item_id=i.item_id
LEFT JOIN mobile_v9_attempts a ON a.item_id=i.item_id AND a.current
LEFT JOIN mobile_v9_plans p ON p.id=a.plan_id
LEFT JOIN execution_devices device ON device.id=p.device_id
LEFT JOIN mobile_execution_grants g ON g.attempt_id=a.id
LEFT JOIN orchestration_delivery_progress v ON v.message_id=i.message_id AND v.channel='mobile' AND v.target_id=i.target_id;
