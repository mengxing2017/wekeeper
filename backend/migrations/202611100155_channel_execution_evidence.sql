-- Evidence is append-only observation, never a second executor or a send permission.
ALTER TABLE broadcast_tasks ADD COLUMN execution_mode text NOT NULL DEFAULT 'official_customer_group_broadcast'
 CHECK(execution_mode='official_customer_group_broadcast');
ALTER TABLE broadcast_batches ADD COLUMN sync_failures integer NOT NULL DEFAULT 0 CHECK(sync_failures>=0);
ALTER TABLE broadcast_batches ADD COLUMN request_digest text;
ALTER TABLE broadcast_batches ADD COLUMN submitted_at timestamptz;
ALTER TABLE mobile_execution_grants ADD COLUMN device_started_at timestamptz;
ALTER TABLE mobile_execution_grants ADD COLUMN device_finished_at timestamptz;
CREATE TABLE channel_execution_observations (
 id bigserial PRIMARY KEY, channel text NOT NULL CHECK(channel IN ('wecom','mobile')),
 workspace_id uuid NOT NULL, task_id uuid NOT NULL, target_id uuid,
 attempt_id uuid, batch_id uuid, execution_id uuid, invocation_id uuid,
 evidence_type text NOT NULL, observation_source text NOT NULL,
 observed_at timestamptz NOT NULL DEFAULT clock_timestamp(), device_occurred_at timestamptz,
 external_status text, normalized_result text NOT NULL,
 detail jsonb NOT NULL DEFAULT '{}', reconstructed boolean NOT NULL DEFAULT false
);
CREATE INDEX channel_observation_task_page ON channel_execution_observations(channel,workspace_id,task_id,id);
CREATE INDEX channel_observation_attempt ON channel_execution_observations(attempt_id,id);
CREATE INDEX channel_observation_batch ON channel_execution_observations(batch_id,id);
CREATE TRIGGER channel_observations_immutable BEFORE UPDATE OR DELETE ON channel_execution_observations
 FOR EACH ROW EXECUTE FUNCTION source_routing_immutable();

CREATE FUNCTION channel_batch_observation() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF (NEW.state,NEW.msgid,NEW.member_status,NEW.last_synced_at,NEW.sync_error,NEW.stop_state,NEW.reconciliation,NEW.error_code,NEW.sync_failures)
 IS DISTINCT FROM (OLD.state,OLD.msgid,OLD.member_status,OLD.last_synced_at,OLD.sync_error,OLD.stop_state,OLD.reconciliation,OLD.error_code,OLD.sync_failures) THEN
 INSERT INTO channel_execution_observations(channel,workspace_id,task_id,batch_id,invocation_id,evidence_type,observation_source,external_status,normalized_result,detail)
 VALUES('wecom',NEW.school_id,NEW.task_id,NEW.id,NEW.invocation_id,
 CASE WHEN NEW.state='unknown' OR NEW.stop_state='unknown' THEN 'unknown'
 WHEN NEW.reconciliation IS DISTINCT FROM OLD.reconciliation THEN 'operator_reconciled'
 WHEN NEW.sync_error IS NOT NULL THEN 'official_read_failed'
 WHEN NEW.last_synced_at IS DISTINCT FROM OLD.last_synced_at THEN CASE WHEN NEW.member_status=2 THEN 'external_member_executed' WHEN NEW.member_status=0 THEN 'external_member_pending' ELSE 'external_member_unknown' END
 WHEN NEW.state='created' THEN 'official_task_created' ELSE 'official_submission' END,
 CASE WHEN NEW.reconciliation IS DISTINCT FROM OLD.reconciliation THEN 'operator' ELSE 'wecom_api' END,
 NEW.member_status::text,CASE WHEN NEW.state='unknown' OR NEW.stop_state='unknown' OR (NEW.last_synced_at IS DISTINCT FROM OLD.last_synced_at AND (NEW.member_status IS NULL OR NEW.member_status NOT IN (0,2))) THEN 'unknown' ELSE 'pending' END,
 jsonb_build_object('msgid',NEW.msgid,'state',NEW.state,'member_status',NEW.member_status,'sync_error',NEW.sync_error,'stop_state',NEW.stop_state,'reconciliation',NEW.reconciliation,'error_code',NEW.error_code,'request_digest',NEW.request_digest));
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER channel_batch_observation AFTER UPDATE ON broadcast_batches FOR EACH ROW EXECUTE FUNCTION channel_batch_observation();

CREATE FUNCTION channel_target_observation() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF (NEW.official_status,NEW.send_time,NEW.rejected) IS DISTINCT FROM (OLD.official_status,OLD.send_time,OLD.rejected) THEN
 INSERT INTO channel_execution_observations(channel,workspace_id,task_id,target_id,batch_id,invocation_id,evidence_type,observation_source,external_status,normalized_result,detail)
 SELECT 'wecom',NEW.school_id,NEW.task_id,NEW.group_id,NEW.batch_id,b.invocation_id,
 CASE WHEN NEW.official_status=1 THEN 'official_group_sent' ELSE 'official_group_result' END,'wecom_api',NEW.official_status::text,
 CASE WHEN NEW.official_status=1 THEN 'sent' WHEN NEW.rejected OR NEW.official_status IN (2,3) THEN 'failed' WHEN NEW.official_status=0 THEN 'pending' ELSE 'unknown' END,
 jsonb_build_object('msgid',b.msgid,'chat_id',NEW.chat_id,'send_time',NEW.send_time,'rejected',NEW.rejected)
 FROM broadcast_batches b WHERE b.id=NEW.batch_id;
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER channel_target_observation AFTER UPDATE ON broadcast_targets FOR EACH ROW EXECUTE FUNCTION channel_target_observation();

CREATE FUNCTION channel_mobile_observation() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE kind text; result text;
BEGIN
 IF NEW.kind<>'send' THEN RETURN NEW; END IF;
 IF NEW.result_id IS DISTINCT FROM OLD.result_id AND NEW.result_id IS NOT NULL THEN
 result:=NEW.result_body->>'outcome';
 kind:=CASE WHEN result='observed_sent' THEN 'device_observed_sent' WHEN result='unknown' THEN 'unknown' ELSE 'device_execution_result' END;
 ELSIF NEW.accepted_at IS DISTINCT FROM OLD.accepted_at AND NEW.accepted_at IS NOT NULL THEN
 result:='pending';kind:='device_received';
 ELSE RETURN NEW; END IF;
 INSERT INTO channel_execution_observations(channel,workspace_id,task_id,target_id,attempt_id,execution_id,evidence_type,observation_source,observed_at,device_occurred_at,normalized_result,detail)
 SELECT 'mobile',i.school_id,i.task_id,i.target_id,a.id,NEW.id,kind,'device',COALESCE(NEW.result_received_at,NEW.accepted_at,clock_timestamp()),NEW.device_finished_at,
 CASE WHEN result='observed_sent' THEN 'sent' WHEN result IN ('not_called','failed') THEN 'failed' ELSE result END,
 jsonb_build_object('delivery_id',NEW.id,'result_id',NEW.result_id,'outcome',NEW.result_body->>'outcome','reason',NEW.result_body->>'reason','device_started_at',NEW.device_started_at,'device_finished_at',NEW.device_finished_at,'device_clock_trusted',false)
 FROM mobile_v9_attempts a JOIN mobile_v9_task_items i ON i.id=a.item_id WHERE a.id=NEW.attempt_id;
 RETURN NEW;
END $$;
CREATE TRIGGER channel_mobile_observation AFTER UPDATE ON mobile_execution_grants FOR EACH ROW EXECUTE FUNCTION channel_mobile_observation();

CREATE FUNCTION channel_task_accepted() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 INSERT INTO channel_execution_observations(channel,workspace_id,task_id,evidence_type,observation_source,normalized_result)
 VALUES(CASE WHEN TG_TABLE_NAME='broadcast_tasks' THEN 'wecom' ELSE 'mobile' END,NEW.school_id,NEW.id,'platform_accepted','platform','pending');
 RETURN NEW;
END $$;
CREATE TRIGGER channel_wecom_accepted AFTER INSERT ON broadcast_tasks FOR EACH ROW EXECUTE FUNCTION channel_task_accepted();
CREATE TRIGGER channel_mobile_accepted AFTER INSERT ON mobile_v9_tasks FOR EACH ROW EXECUTE FUNCTION channel_task_accepted();

CREATE FUNCTION channel_mobile_review_observation() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 INSERT INTO channel_execution_observations(channel,workspace_id,task_id,target_id,attempt_id,evidence_type,observation_source,normalized_result,detail)
 SELECT 'mobile',i.school_id,i.task_id,i.target_id,a.id,'operator_reconciled','operator',CASE WHEN NEW.decision='manually_confirmed_sent' THEN 'sent' WHEN NEW.decision='verified_not_sent' THEN 'failed' ELSE 'unknown' END,jsonb_build_object('review_id',NEW.id,'decision',NEW.decision,'actor_id',NEW.reviewer)
 FROM mobile_v9_attempts a JOIN mobile_v9_task_items i ON i.id=a.item_id WHERE a.id=NEW.attempt_id;
 RETURN NEW;
END $$;
CREATE TRIGGER channel_mobile_review_observation AFTER INSERT ON mobile_v9_reviews FOR EACH ROW EXECUTE FUNCTION channel_mobile_review_observation();
-- Historical evidence remains in channel tables; do not fabricate observations or successful delivery.

CREATE OR REPLACE VIEW source_automation_target_facts AS
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
 CASE WHEN x.official_status=1 THEN 'sent' WHEN b.state='unknown' OR b.stop_state='unknown' OR (x.official_status IS NOT NULL AND x.official_status NOT IN (0,1,2,3)) THEN 'unknown'
 WHEN EXISTS(SELECT 1 FROM keyword_target_blocks kb WHERE kb.receipt_id=g.receipt_id AND kb.target_id=x.group_id) OR EXISTS(SELECT 1 FROM advertisement_target_blocks ab WHERE ab.receipt_id=g.receipt_id AND ab.target_id=x.group_id) THEN 'failed'
 WHEN b.state='cancelled' OR b.stop_state='stopped' THEN 'cancelled'
 WHEN x.rejected OR x.official_status IN (2,3) OR b.state='rejected' THEN 'failed'
 WHEN b.state='submitting' THEN 'dispatching' WHEN b.state='created' THEN 'running'
 WHEN t.stopped THEN 'stopped' WHEN b.next_sync_at>now() THEN 'scheduled' ELSE 'queued' END,
 COALESCE(b.error_code,b.sync_error,b.stop_state),b.next_sync_at,b.state='unknown' OR b.stop_state='unknown' OR (x.official_status IS NOT NULL AND x.official_status NOT IN (0,1,2,3))
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
