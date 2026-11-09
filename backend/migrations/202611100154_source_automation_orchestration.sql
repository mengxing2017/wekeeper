-- Proposals may change until the first executable task is committed. Legacy history stays immutable.
CREATE TABLE source_routing_proposals (
 receipt_id uuid PRIMARY KEY REFERENCES source_receipts, source_id uuid NOT NULL, route_id text NOT NULL,
 config_revision bigint NOT NULL, snapshot jsonb NOT NULL, target_digest text NOT NULL
);
CREATE TABLE source_automation_tasks (
 id uuid PRIMARY KEY REFERENCES source_receipts, decision_id uuid NOT NULL UNIQUE,
 source_id uuid NOT NULL, route_id text NOT NULL, unit_id uuid NOT NULL,
 channel text NOT NULL CHECK(channel IN ('mobile','wecom')), workspace_id uuid NOT NULL,
 route_revision bigint NOT NULL, snapshot jsonb NOT NULL, content text NOT NULL,
 composition_id uuid REFERENCES source_message_compositions,
 version_policy jsonb NOT NULL DEFAULT '{"policy":"replace_unsubmitted","revision":0}',
 authorized_by uuid NOT NULL REFERENCES accounts, executed_by text NOT NULL DEFAULT 'source-routing',
 stopped boolean NOT NULL DEFAULT false, stop_reason text, revision bigint NOT NULL DEFAULT 1,
 priority integer NOT NULL DEFAULT 0, created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 reconstructed boolean NOT NULL DEFAULT false
);
CREATE INDEX source_automation_scope_page ON source_automation_tasks(workspace_id,created_at DESC,id);
CREATE INDEX source_automation_route_page ON source_automation_tasks(route_id,created_at DESC,id);
CREATE VIEW source_current_routing_decisions AS
 SELECT r.receipt_id,COALESCE(t.snapshot,p.snapshot,d.snapshot) AS snapshot
 FROM source_receipts r LEFT JOIN source_automation_tasks t ON t.id=r.receipt_id
 LEFT JOIN source_routing_proposals p USING(receipt_id) LEFT JOIN source_routing_decisions d USING(receipt_id)
 WHERE COALESCE(t.snapshot,p.snapshot,d.snapshot) IS NOT NULL;
CREATE TABLE source_automation_task_events (
 id bigserial PRIMARY KEY,task_id uuid NOT NULL REFERENCES source_automation_tasks,
 kind text NOT NULL,detail jsonb NOT NULL DEFAULT '{}',actor_id uuid REFERENCES accounts,
 executed_by text NOT NULL DEFAULT 'source-routing',created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE INDEX source_automation_event_page ON source_automation_task_events(task_id,id);
CREATE TRIGGER source_automation_events_immutable BEFORE UPDATE OR DELETE ON source_automation_task_events FOR EACH ROW EXECUTE FUNCTION source_routing_immutable();
CREATE TABLE source_automation_commands (
 task_id uuid NOT NULL REFERENCES source_automation_tasks, actor_id uuid NOT NULL REFERENCES accounts,
 idempotency_key uuid NOT NULL, request_hash text NOT NULL,response jsonb NOT NULL,
 PRIMARY KEY(task_id,actor_id,idempotency_key)
);
CREATE TABLE source_orchestration_recovery_jobs (
 id bigserial PRIMARY KEY,route_id text,scope_kind text,scope_id uuid,after_receipt uuid,complete boolean NOT NULL DEFAULT false,
 created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE INDEX source_orchestration_recovery_due ON source_orchestration_recovery_jobs(id) WHERE NOT complete;
CREATE TABLE source_route_version_policies (
 route_id text PRIMARY KEY,policy text NOT NULL CHECK(policy IN ('replace_unsubmitted','resend_new_version')),
 revision bigint NOT NULL DEFAULT 1,authorized_by uuid NOT NULL REFERENCES accounts,
 effective_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE FUNCTION source_automation_freeze() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE s jsonb; body text;
BEGIN
 SELECT snapshot INTO s FROM source_current_routing_decisions WHERE receipt_id=NEW.receipt_id;
 IF (s->>'revision')::bigint IS DISTINCT FROM NEW.config_revision OR s #>> '{execution,channel}' IS DISTINCT FROM NEW.channel OR (s #>> '{execution,workspace_id}')::uuid IS DISTINCT FROM NEW.workspace_id THEN RAISE EXCEPTION 'source decision changed before task creation'; END IF;
 SELECT content INTO body FROM source_message_compositions WHERE id=NEW.composition_id;
 IF body IS NULL THEN
  IF NEW.channel='mobile' THEN SELECT ms.content INTO body FROM mobile_v9_task_items i JOIN mobile_v9_plans p ON p.item_id=i.id JOIN distribution_message_snapshots ms ON ms.id=p.snapshot_id WHERE i.task_id=NEW.task_id LIMIT 1;
  ELSE SELECT content INTO body FROM broadcast_tasks WHERE id=NEW.task_id; END IF;
 END IF;
 INSERT INTO source_automation_tasks(id,decision_id,source_id,route_id,unit_id,channel,workspace_id,route_revision,snapshot,content,composition_id,authorized_by,version_policy)
 VALUES(NEW.receipt_id,gen_random_uuid(),(s->>'source_id')::uuid,s->>'route_id',(s->>'unit_id')::uuid,NEW.channel,NEW.workspace_id,NEW.config_revision,s,body,NEW.composition_id,NEW.authorization_actor_id,COALESCE((SELECT to_jsonb(p) FROM source_route_version_policies p WHERE p.route_id=s->>'route_id'),'{"policy":"replace_unsubmitted","revision":0}'::jsonb))
 ON CONFLICT(id) DO NOTHING;
 INSERT INTO source_automation_task_events(task_id,kind,detail,created_at) SELECT NEW.receipt_id,h.stage,jsonb_build_object('reason_code',h.reason_code,'processing_revision',h.revision),h.created_at FROM source_receipt_history h WHERE h.receipt_id=NEW.receipt_id ORDER BY h.id;
 INSERT INTO source_automation_task_events(task_id,kind,detail) VALUES(NEW.receipt_id,'task.created',jsonb_build_object('channel_task_id',NEW.task_id,'generation',NEW.generation,'route_revision',NEW.config_revision));
 RETURN NEW;
END $$;
CREATE TRIGGER source_automation_freeze AFTER INSERT ON source_execution_tasks FOR EACH ROW EXECUTE FUNCTION source_automation_freeze();
-- Stop is sticky, including stops issued through the existing channel UI.
CREATE FUNCTION source_automation_channel_stopped() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE receipt uuid;
BEGIN
 IF NEW.stopped AND NOT OLD.stopped AND COALESCE(current_setting('wekeeper.composition_replacement',true),'off')<>'on' THEN
  SELECT receipt_id INTO receipt FROM source_execution_tasks WHERE task_id=NEW.id;
  IF receipt IS NOT NULL THEN
   UPDATE source_automation_tasks SET stopped=true,stop_reason=COALESCE(stop_reason,'channel_task_stopped'),revision=revision+1 WHERE id=receipt AND NOT stopped;
   INSERT INTO source_automation_task_events(task_id,kind) SELECT receipt,'channel.stopped' WHERE EXISTS(SELECT 1 FROM source_automation_tasks WHERE id=receipt);
  END IF;
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER source_mobile_automation_stop AFTER UPDATE ON mobile_v9_tasks FOR EACH ROW EXECUTE FUNCTION source_automation_channel_stopped();
CREATE TRIGGER source_wecom_automation_stop AFTER UPDATE ON broadcast_tasks FOR EACH ROW EXECUTE FUNCTION source_automation_channel_stopped();
CREATE FUNCTION source_automation_snapshot_fixed() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF (NEW.id,NEW.decision_id,NEW.source_id,NEW.route_id,NEW.unit_id,NEW.channel,NEW.workspace_id,NEW.route_revision,NEW.snapshot,NEW.content,NEW.composition_id,NEW.version_policy,NEW.authorized_by,NEW.executed_by,NEW.created_at,NEW.reconstructed)
 IS DISTINCT FROM (OLD.id,OLD.decision_id,OLD.source_id,OLD.route_id,OLD.unit_id,OLD.channel,OLD.workspace_id,OLD.route_revision,OLD.snapshot,OLD.content,OLD.composition_id,OLD.version_policy,OLD.authorized_by,OLD.executed_by,OLD.created_at,OLD.reconstructed)
 OR (OLD.stopped AND NOT NEW.stopped) THEN RAISE EXCEPTION 'automation decision is immutable'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER source_automation_snapshot_fixed BEFORE UPDATE ON source_automation_tasks FOR EACH ROW EXECUTE FUNCTION source_automation_snapshot_fixed();
-- Logical targets are derived across generations; attempts remain owned by the channel.
CREATE VIEW source_automation_target_facts AS
 WITH facts AS (
 SELECT g.receipt_id,i.target_id,
 CASE WHEN EXISTS(SELECT 1 FROM mobile_v9_attempts a WHERE a.item_id=i.id AND a.state IN ('observed_sent','manually_confirmed_sent')) THEN 'sent'
 WHEN NOT (EXISTS(SELECT 1 FROM keyword_target_blocks kb WHERE kb.receipt_id=g.receipt_id AND kb.target_id=i.target_id) OR EXISTS(SELECT 1 FROM advertisement_target_blocks ab WHERE ab.receipt_id=g.receipt_id AND ab.target_id=i.target_id)) AND ds.state NOT IN ('completed','dispatch_failed','stopped','confirmation_expired') AND NOT t.stopped AND g.task_id=cur.task_id THEN CASE WHEN ds.state='executing' THEN 'running' WHEN ds.state='assigned' THEN 'dispatching' WHEN ds.next_retry_at>now() THEN 'scheduled' ELSE 'queued' END
 WHEN EXISTS(SELECT 1 FROM mobile_v9_attempts a LEFT JOIN mobile_execution_grants x ON x.attempt_id=a.id WHERE a.item_id=i.id AND (a.state='unknown' OR (x.id IS NOT NULL AND x.result_body IS NULL AND a.state NOT IN ('verified_not_sent','failed_before_call','invalidated','cancelled')))) THEN 'unknown'
 WHEN EXISTS(SELECT 1 FROM keyword_target_blocks kb WHERE kb.receipt_id=g.receipt_id AND kb.target_id=i.target_id) OR EXISTS(SELECT 1 FROM advertisement_target_blocks ab WHERE ab.receipt_id=g.receipt_id AND ab.target_id=i.target_id) THEN 'failed'
 WHEN t.stopped OR ds.state='stopped' THEN 'stopped' ELSE 'failed' END AS state,
 ds.reason,ds.next_retry_at AS next_attempt_at,
 EXISTS(SELECT 1 FROM mobile_v9_attempts a LEFT JOIN mobile_execution_grants x ON x.attempt_id=a.id WHERE a.item_id=i.id AND (a.state='unknown' OR (x.id IS NOT NULL AND x.result_body IS NULL AND a.state NOT IN ('observed_sent','manually_confirmed_sent','verified_not_sent','failed_before_call','invalidated','cancelled')))) AS has_unknown
 FROM source_execution_generations g JOIN source_execution_tasks cur USING(receipt_id)
 JOIN mobile_v9_tasks t ON t.id=g.task_id JOIN mobile_v9_task_items i ON i.task_id=t.id
 JOIN mobile_v9_dispatch_state ds ON ds.item_id=i.id WHERE g.channel='mobile'
 UNION ALL
 SELECT g.receipt_id,x.group_id,
 CASE WHEN x.official_status=1 THEN 'sent' WHEN b.state='unknown' OR b.stop_state='unknown' THEN 'unknown'
 WHEN EXISTS(SELECT 1 FROM keyword_target_blocks kb WHERE kb.receipt_id=g.receipt_id AND kb.target_id=x.group_id) OR EXISTS(SELECT 1 FROM advertisement_target_blocks ab WHERE ab.receipt_id=g.receipt_id AND ab.target_id=x.group_id) THEN 'failed'
 WHEN b.state='cancelled' OR b.stop_state='stopped' THEN 'cancelled'
 WHEN x.rejected OR x.official_status IN (2,3) OR b.state='rejected' THEN 'failed'
 WHEN b.state='submitting' THEN 'dispatching' WHEN b.state='created' THEN 'running'
 WHEN t.stopped THEN 'stopped' WHEN b.next_sync_at>now() THEN 'scheduled' ELSE 'queued' END,
 COALESCE(b.error_code,b.sync_error,b.stop_state),b.next_sync_at,b.state='unknown' OR b.stop_state='unknown'
 FROM source_execution_generations g JOIN broadcast_tasks t ON t.id=g.task_id JOIN broadcast_targets x ON x.task_id=t.id JOIN broadcast_batches b ON b.id=x.batch_id WHERE g.channel='wecom'
 ), logical AS (
 SELECT receipt_id,target_id,
 CASE WHEN bool_or(state='sent') THEN 'sent'
 WHEN bool_or(state='running') THEN 'running' WHEN bool_or(state='dispatching') THEN 'dispatching'
 WHEN bool_or(state='queued') THEN 'queued' WHEN bool_or(state='scheduled') THEN 'scheduled'
 WHEN bool_or(state='unknown') THEN 'unknown' WHEN bool_or(state='failed') THEN 'failed'
 WHEN bool_or(state='stopped') THEN 'stopped' ELSE 'cancelled' END AS state,
 bool_or(has_unknown) AS has_unknown,min(next_attempt_at) AS next_attempt_at,
 string_agg(DISTINCT reason,',') AS reason FROM facts GROUP BY receipt_id,target_id
 ) SELECT l.*,s.value->>'name' AS name,s.value->'identity' AS identity
 FROM logical l JOIN source_automation_tasks t ON t.id=l.receipt_id
 JOIN LATERAL jsonb_array_elements(t.snapshot->'targets') s(value) ON (s.value->>'id')::uuid=l.target_id;
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
ALTER TABLE source_inbox ADD COLUMN reason_class text,
 ADD COLUMN job_state text NOT NULL DEFAULT 'ready' CHECK(job_state IN ('ready','processing','configuration_error','filter_blocked','retry_wait','applied','no_resend','withdrawn','ignored_stale','stopped'));
CREATE FUNCTION source_automation_receipt_event() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.sending IS DISTINCT FROM OLD.sending THEN
  INSERT INTO source_automation_task_events(task_id,kind,detail) SELECT NEW.receipt_id,'targets.updated',jsonb_build_object('sending',NEW.sending,'summary_revision',NEW.summary_revision) WHERE EXISTS(SELECT 1 FROM source_automation_tasks WHERE id=NEW.receipt_id);
 END IF;
 IF (NEW.processing_stage,NEW.reason_code,NEW.status) IS DISTINCT FROM (OLD.processing_stage,OLD.reason_code,OLD.status) THEN
  UPDATE source_inbox SET job_state=CASE WHEN NEW.processing_stage='no_resend' THEN 'no_resend' WHEN NEW.processing_stage='withdrawn' THEN 'withdrawn' WHEN NEW.processing_stage='ignored_stale' THEN 'ignored_stale' WHEN NEW.processing_stage='execution_stopped' THEN 'stopped' WHEN NEW.reason_code IN ('keyword_blocked','workspace_keyword_blocked','final_keyword_blocked','ad_final_keyword_blocked') THEN 'filter_blocked' WHEN NEW.processing_stage='task_created' THEN 'applied' WHEN NEW.processing_stage='execution_blocked' THEN 'configuration_error' WHEN NEW.processing_stage='projection_duplicate' THEN 'applied' ELSE job_state END WHERE receipt_id=NEW.receipt_id;
  INSERT INTO source_automation_task_events(task_id,kind,detail)
   SELECT NEW.receipt_id,NEW.processing_stage,jsonb_build_object('reason_code',NEW.reason_code,'processing_revision',NEW.processing_revision)
   WHERE EXISTS(SELECT 1 FROM source_automation_tasks WHERE id=NEW.receipt_id);
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER source_automation_receipt_event AFTER UPDATE ON source_receipts FOR EACH ROW EXECUTE FUNCTION source_automation_receipt_event();
CREATE FUNCTION source_automation_generation_event() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.task_id<>OLD.task_id THEN
 INSERT INTO source_automation_task_events(task_id,kind,detail) VALUES(NEW.receipt_id,'composition.replaced',jsonb_build_object('old_channel_task_id',OLD.task_id,'channel_task_id',NEW.task_id,'generation',NEW.generation,'composition_id',NEW.composition_id));
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER source_automation_generation_event AFTER UPDATE ON source_execution_tasks FOR EACH ROW EXECUTE FUNCTION source_automation_generation_event();
