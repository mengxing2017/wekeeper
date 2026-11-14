-- Immediate messages do not require a version delivery policy.
CREATE OR REPLACE FUNCTION source_automation_freeze() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE s jsonb; body text;
BEGIN
 SELECT snapshot INTO s FROM source_current_routing_decisions WHERE receipt_id=NEW.receipt_id;
 IF (s->>'revision')::bigint IS DISTINCT FROM NEW.config_revision OR s #>> '{execution,channel}' IS DISTINCT FROM NEW.channel OR (s #>> '{execution,workspace_id}')::uuid IS DISTINCT FROM NEW.workspace_id THEN RAISE EXCEPTION 'source decision changed before task creation'; END IF;
 SELECT content INTO body FROM source_message_compositions WHERE id=NEW.composition_id;
 IF body IS NULL THEN
  IF NEW.channel='mobile' THEN SELECT ms.content INTO body FROM mobile_v9_task_items i JOIN mobile_v9_plans p ON p.item_id=i.id JOIN distribution_message_snapshots ms ON ms.id=p.snapshot_id WHERE i.task_id=NEW.task_id LIMIT 1;
  ELSE SELECT content INTO body FROM broadcast_tasks WHERE id=NEW.task_id; END IF;
 END IF;
 INSERT INTO source_automation_tasks(id,decision_id,source_id,route_id,unit_id,channel,workspace_id,route_revision,snapshot,content,composition_id,authorized_by)
 VALUES(NEW.receipt_id,gen_random_uuid(),(s->>'source_id')::uuid,s->>'route_id',(s->>'unit_id')::uuid,NEW.channel,NEW.workspace_id,NEW.config_revision,s,body,NEW.composition_id,NEW.authorization_actor_id)
 ON CONFLICT(id) DO NOTHING;
 INSERT INTO source_automation_task_events(task_id,kind,detail,created_at) SELECT NEW.receipt_id,h.stage,jsonb_build_object('reason_code',h.reason_code,'processing_revision',h.revision),h.created_at FROM source_receipt_history h WHERE h.receipt_id=NEW.receipt_id ORDER BY h.id;
 INSERT INTO source_automation_task_events(task_id,kind,detail) VALUES(NEW.receipt_id,'task.created',jsonb_build_object('channel_task_id',NEW.task_id,'generation',NEW.generation,'route_revision',NEW.config_revision));
 RETURN NEW;
END $$;

CREATE OR REPLACE FUNCTION source_automation_snapshot_fixed() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF (NEW.id,NEW.decision_id,NEW.source_id,NEW.route_id,NEW.unit_id,NEW.channel,NEW.workspace_id,NEW.route_revision,NEW.snapshot,NEW.content,NEW.composition_id,NEW.authorized_by,NEW.executed_by,NEW.created_at,NEW.reconstructed)
 IS DISTINCT FROM (OLD.id,OLD.decision_id,OLD.source_id,OLD.route_id,OLD.unit_id,OLD.channel,OLD.workspace_id,OLD.route_revision,OLD.snapshot,OLD.content,OLD.composition_id,OLD.authorized_by,OLD.executed_by,OLD.created_at,OLD.reconstructed)
 OR (OLD.stopped AND NOT NEW.stopped) THEN RAISE EXCEPTION 'automation decision is immutable'; END IF;
 RETURN NEW;
END $$;

DROP VIEW source_automation_summaries;
ALTER TABLE source_automation_tasks DROP COLUMN version_policy;
DROP TABLE source_route_version_policies;

CREATE VIEW source_automation_summaries AS
 SELECT t.*,r.event_id,r.received_at,r.processing_stage,r.reason_code,
 convert_from(i.request_body,'UTF8')::jsonb #>> '{data,attributes,external_content_id}' AS external_content_id,
 COALESCE(c.counts,'{}'::jsonb) AS counts,
 CASE WHEN COALESCE(c.pending,0)>0 THEN CASE WHEN c.running>0 OR c.sent>0 THEN 'running' WHEN c.scheduled=c.pending THEN 'scheduled' ELSE 'queued' END
 WHEN c.unknown>0 THEN 'unknown' WHEN c.sent=c.total AND c.total>0 THEN 'succeeded'
 WHEN c.sent>0 THEN 'partially_succeeded' WHEN c.stopped=c.total OR t.stopped THEN 'stopped' ELSE 'failed' END AS state,
 COALESCE(c.has_unknown,false) AS has_unknown
 FROM source_automation_tasks t JOIN source_receipts r ON r.receipt_id=t.id JOIN source_inbox i ON i.receipt_id=t.id
 LEFT JOIN LATERAL (
 SELECT count(*) AS total,count(*) FILTER(WHERE state IN ('queued','scheduled','dispatching','running','retry_wait')) AS pending,
 count(*) FILTER(WHERE state IN ('running','dispatching')) AS running,count(*) FILTER(WHERE state='scheduled') AS scheduled,
 count(*) FILTER(WHERE state='sent') AS sent,count(*) FILTER(WHERE state='unknown') AS unknown,
 count(*) FILTER(WHERE state IN ('stopped','cancelled')) AS stopped,bool_or(has_unknown) AS has_unknown,
 jsonb_build_object('total',count(*),'pending',count(*) FILTER(WHERE state IN ('queued','scheduled','dispatching','running','retry_wait')),'sent',count(*) FILTER(WHERE state='sent'),'failed',count(*) FILTER(WHERE state='failed'),'unknown',count(*) FILTER(WHERE state='unknown'),'stopped',count(*) FILTER(WHERE state='stopped'),'cancelled',count(*) FILTER(WHERE state='cancelled')) AS counts
 FROM source_automation_target_facts f WHERE f.receipt_id=t.id
 ) c ON true;
