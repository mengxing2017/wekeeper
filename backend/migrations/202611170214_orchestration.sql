-- 逐消息编排、执行证据与结果队列。
-- 空库初始化基线；后续结构变更使用新增递增迁移。

-- function
CREATE FUNCTION channel_batch_observation() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
 IF (NEW.state,NEW.msgid,NEW.member_status,NEW.last_synced_at,NEW.sync_error,NEW.stop_state,NEW.reconciliation,NEW.error_code,NEW.sync_failures)
 IS DISTINCT FROM (OLD.state,OLD.msgid,OLD.member_status,OLD.last_synced_at,OLD.sync_error,OLD.stop_state,OLD.reconciliation,OLD.error_code,OLD.sync_failures) THEN
 INSERT INTO channel_execution_observations(channel,task_id,batch_id,invocation_id,evidence_type,observation_source,external_status,normalized_result,detail)
 VALUES('wecom',NEW.task_id,NEW.id,NEW.invocation_id,
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

CREATE FUNCTION channel_mobile_observation() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
DECLARE kind text; result text;
BEGIN
 IF NEW.kind<>'send' THEN RETURN NEW; END IF;
 IF NEW.result_id IS DISTINCT FROM OLD.result_id AND NEW.result_id IS NOT NULL THEN
 result:=NEW.result_body->>'outcome';
 kind:=CASE WHEN result='observed_sent' THEN 'device_observed_sent' WHEN result='unknown' THEN 'unknown' ELSE 'device_execution_result' END;
 ELSIF NEW.accepted_at IS DISTINCT FROM OLD.accepted_at AND NEW.accepted_at IS NOT NULL THEN
 result:='pending';kind:='device_received';
 ELSE RETURN NEW; END IF;
 INSERT INTO channel_execution_observations(channel,task_id,target_id,attempt_id,execution_id,evidence_type,observation_source,observed_at,device_occurred_at,normalized_result,detail)
 SELECT 'mobile',i.task_id,i.target_id,a.id,NEW.id,kind,'device',COALESCE(NEW.result_received_at,NEW.accepted_at,clock_timestamp()),NEW.device_finished_at,
 CASE WHEN result='observed_sent' THEN 'sent' WHEN result IN ('not_called','failed') THEN 'failed' ELSE result END,
 jsonb_build_object('delivery_id',NEW.id,'result_id',NEW.result_id,'outcome',NEW.result_body->>'outcome','reason',NEW.result_body->>'reason','device_started_at',NEW.device_started_at,'device_finished_at',NEW.device_finished_at,'device_clock_trusted',false)
  FROM mobile_v9_attempts a JOIN mobile_v9_task_items i ON i.id=a.item_id WHERE a.id=NEW.attempt_id;
 RETURN NEW;
END $$;

CREATE FUNCTION channel_mobile_review_observation() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
 INSERT INTO channel_execution_observations(channel,task_id,target_id,attempt_id,evidence_type,observation_source,normalized_result,detail)
 SELECT 'mobile',i.task_id,i.target_id,a.id,'operator_reconciled','operator',CASE WHEN NEW.decision='manually_confirmed_sent' THEN 'sent' WHEN NEW.decision='verified_not_sent' THEN 'failed' ELSE 'unknown' END,jsonb_build_object('review_id',NEW.id,'decision',NEW.decision,'actor_id',NEW.reviewer)
  FROM mobile_v9_attempts a JOIN mobile_v9_task_items i ON i.id=a.item_id WHERE a.id=NEW.attempt_id;
 RETURN NEW;
END $$;

CREATE FUNCTION channel_target_observation() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
 IF (NEW.official_status,NEW.send_time,NEW.rejected) IS DISTINCT FROM (OLD.official_status,OLD.send_time,OLD.rejected) THEN
 INSERT INTO channel_execution_observations(channel,task_id,target_id,batch_id,invocation_id,evidence_type,observation_source,external_status,normalized_result,detail)
 SELECT 'wecom',NEW.task_id,NEW.group_id,NEW.batch_id,b.invocation_id,
 CASE WHEN NEW.official_status=1 THEN 'official_group_sent' ELSE 'official_group_result' END,'wecom_api',NEW.official_status::text,
 CASE WHEN NEW.official_status=1 THEN 'sent' WHEN NEW.rejected OR NEW.official_status IN (2,3) THEN 'failed' WHEN NEW.official_status=0 THEN 'pending' ELSE 'unknown' END,
 jsonb_build_object('msgid',b.msgid,'chat_id',NEW.chat_id,'send_time',NEW.send_time,'rejected',NEW.rejected)
  FROM broadcast_batches b WHERE b.id=NEW.batch_id;
 END IF;
 RETURN NEW;
END $$;

CREATE FUNCTION channel_task_accepted() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
 INSERT INTO channel_execution_observations(channel,task_id,evidence_type,observation_source,normalized_result)
 VALUES(CASE WHEN TG_TABLE_NAME='broadcast_tasks' THEN 'wecom' ELSE 'mobile' END,NEW.id,'platform_accepted','platform','pending');
 RETURN NEW;
END $$;

CREATE FUNCTION distribution_snapshot_immutable() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN RAISE EXCEPTION 'distribution snapshots are immutable'; END $$;

CREATE FUNCTION message_grant_progress_changed() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
 INSERT INTO orchestration_result_queue
 SELECT d.message_id FROM mobile_v9_task_items i JOIN orchestration_delegates d ON d.task_id=i.task_id
 WHERE i.id=(SELECT item_id FROM mobile_v9_attempts WHERE id=NEW.attempt_id)
 ON CONFLICT DO NOTHING;
 RETURN NEW;
END $$;

CREATE FUNCTION message_progress_clock() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
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

CREATE FUNCTION orchestration_dispatch_fixed() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
 IF NEW.message_id<>OLD.message_id
 OR (NEW.plans IS DISTINCT FROM OLD.plans AND OLD.state<>'pending')
 OR ((NEW.content,NEW.decision) IS DISTINCT FROM (OLD.content,OLD.decision)
     AND NOT (OLD.state='pending' OR (OLD.state='preparing' AND NEW.state='ready')))
 OR (OLD.state IN ('delegated','failed','historical') AND NEW.state<>OLD.state)
 OR (OLD.state='ready' AND NEW.state IN ('pending','preparing'))
 OR (OLD.state='preparing' AND NEW.state='pending') THEN
  RAISE EXCEPTION 'frozen message input is immutable';
 END IF;
 RETURN NEW;
END $$;

CREATE FUNCTION orchestration_message_immutable() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN RAISE EXCEPTION 'accepted orchestration message is immutable'; END $$;

CREATE FUNCTION orchestration_result_changed() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
DECLARE task uuid;
BEGIN
 IF TG_TABLE_NAME='orchestration_delegates' THEN
  INSERT INTO orchestration_result_queue VALUES(NEW.message_id) ON CONFLICT DO NOTHING;
  RETURN NEW;
 ELSIF TG_TABLE_NAME='mobile_v9_attempts' THEN
  SELECT task_id INTO task FROM mobile_v9_task_items WHERE id=NEW.item_id;
 ELSIF TG_TABLE_NAME='mobile_v9_dispatch_state' THEN
  SELECT task_id INTO task FROM mobile_v9_task_items WHERE id=NEW.item_id;
 ELSIF TG_TABLE_NAME IN ('mobile_v9_tasks','broadcast_tasks') THEN task=NEW.id;
 ELSE task=NEW.task_id;
 END IF;
 INSERT INTO orchestration_result_queue SELECT message_id FROM orchestration_delegates WHERE task_id=task ON CONFLICT DO NOTHING;
 RETURN NEW;
END $$;

-- table
CREATE TABLE channel_execution_observations (
    id bigint NOT NULL,
    channel text NOT NULL,
    task_id uuid NOT NULL,
    target_id uuid,
    attempt_id uuid,
    batch_id uuid,
    execution_id uuid,
    invocation_id uuid,
    evidence_type text NOT NULL,
    observation_source text NOT NULL,
    observed_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    device_occurred_at timestamp with time zone,
    external_status text,
    normalized_result text NOT NULL,
    detail jsonb DEFAULT '{}'::jsonb NOT NULL,
    reconstructed boolean DEFAULT false NOT NULL,
    CONSTRAINT channel_execution_observations_channel_check CHECK ((channel = ANY (ARRAY['wecom'::text, 'mobile'::text])))
);

CREATE TABLE channel_receiving_configurations (
    id uuid NOT NULL,
    business_unit_id uuid NOT NULL,
    channel text NOT NULL,
    configuration jsonb NOT NULL,
    authorized_by uuid NOT NULL,
    updated_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    CONSTRAINT channel_receiving_configurations_channel_check CHECK ((channel = ANY (ARRAY['mobile'::text, 'wecom'::text])))
);

CREATE TABLE distribution_message_snapshots (
    id uuid NOT NULL,
    channel text NOT NULL,
    content text NOT NULL,
    content_hash text NOT NULL,
    algorithm_version text DEFAULT 'sha256-text/1'::text NOT NULL,
    media jsonb DEFAULT '[]'::jsonb NOT NULL,
    source_references jsonb DEFAULT '[]'::jsonb NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT distribution_message_snapshots_channel_check CHECK ((channel = ANY (ARRAY['wecom'::text, 'mobile'::text])))
);

CREATE TABLE distribution_resource_policies (
    resource_key text NOT NULL,
    window_seconds integer NOT NULL,
    max_calls integer NOT NULL,
    enabled boolean DEFAULT true NOT NULL,
    revision bigint DEFAULT 1 NOT NULL,
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    CONSTRAINT distribution_resource_policies_max_calls_check CHECK ((max_calls > 0)),
    CONSTRAINT distribution_resource_policies_window_seconds_check CHECK ((window_seconds > 0))
);

CREATE TABLE distribution_resource_usage (
    batch_id uuid NOT NULL,
    resource_key text NOT NULL,
    source_namespace text NOT NULL,
    called_at timestamp with time zone DEFAULT now() NOT NULL
);

CREATE TABLE distribution_source_turns (
    source_namespace text NOT NULL,
    last_claimed_at timestamp with time zone DEFAULT '-infinity'::timestamp with time zone NOT NULL
);

CREATE TABLE distribution_sources (
    namespace text NOT NULL,
    enabled boolean DEFAULT true NOT NULL,
    max_pending integer DEFAULT 10000 NOT NULL,
    revision bigint DEFAULT 1 NOT NULL,
    CONSTRAINT distribution_sources_max_pending_check CHECK ((max_pending > 0)),
    CONSTRAINT distribution_sources_namespace_check CHECK (((length(namespace) >= 1) AND (length(namespace) <= 100)))
);

CREATE TABLE orchestration_delegates (
    id uuid NOT NULL,
    message_id uuid NOT NULL,
    configuration_id uuid NOT NULL,
    channel text NOT NULL,
    task_id uuid,
    snapshot_id uuid,
    plan jsonb NOT NULL,
    state text DEFAULT 'delegated'::text NOT NULL,
    reason_code text,
    created_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    executed_by text DEFAULT 'message-orchestration'::text NOT NULL,
    CONSTRAINT orchestration_delegates_channel_check CHECK ((channel = ANY (ARRAY['mobile'::text, 'wecom'::text]))),
    CONSTRAINT orchestration_delegates_check CHECK (((state = 'delegated'::text) = ((task_id IS NOT NULL) AND (snapshot_id IS NOT NULL)))),
    CONSTRAINT orchestration_delegates_state_check CHECK ((state = ANY (ARRAY['delegated'::text, 'failed'::text])))
);

CREATE TABLE orchestration_dispatches (
    message_id uuid NOT NULL,
    content text NOT NULL,
    decision jsonb NOT NULL,
    plans jsonb NOT NULL,
    state text NOT NULL,
    reason_code text,
    failures integer DEFAULT 0 NOT NULL,
    next_attempt_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    created_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    updated_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    keyword_decision jsonb,
    stage_entered_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    last_error_code text,
    CONSTRAINT orchestration_dispatches_state_check CHECK ((state = ANY (ARRAY['pending'::text, 'preparing'::text, 'ready'::text, 'delegated'::text, 'failed'::text, 'historical'::text])))
);

CREATE TABLE orchestration_messages (
    id uuid NOT NULL,
    platform_id uuid NOT NULL,
    unit_id uuid NOT NULL,
    source_event_id uuid NOT NULL,
    evidence_ref uuid NOT NULL,
    original jsonb NOT NULL,
    accepted_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    CONSTRAINT orchestration_messages_evidence_ref_check CHECK ((evidence_ref <> '00000000-0000-0000-0000-000000000000'::uuid)),
    CONSTRAINT orchestration_messages_id_check CHECK ((id <> '00000000-0000-0000-0000-000000000000'::uuid)),
    CONSTRAINT orchestration_messages_original_check CHECK ((jsonb_typeof(original) = 'object'::text)),
    CONSTRAINT orchestration_messages_source_event_id_check CHECK ((source_event_id <> '00000000-0000-0000-0000-000000000000'::uuid))
);

CREATE TABLE orchestration_result_queue (
    message_id uuid NOT NULL
);

-- sequence
CREATE SEQUENCE channel_execution_observations_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;

-- sequence owned by
ALTER SEQUENCE channel_execution_observations_id_seq OWNED BY channel_execution_observations.id;

-- default
ALTER TABLE ONLY channel_execution_observations ALTER COLUMN id SET DEFAULT nextval('channel_execution_observations_id_seq'::regclass);

-- constraint
ALTER TABLE ONLY channel_execution_observations
    ADD CONSTRAINT channel_execution_observations_pkey PRIMARY KEY (id);

ALTER TABLE ONLY channel_receiving_configurations
    ADD CONSTRAINT channel_receiving_configurations_pkey PRIMARY KEY (id);

ALTER TABLE ONLY distribution_message_snapshots
    ADD CONSTRAINT distribution_message_snapshots_id_key UNIQUE (id);

ALTER TABLE ONLY distribution_message_snapshots
    ADD CONSTRAINT distribution_message_snapshots_pkey PRIMARY KEY (id);

ALTER TABLE ONLY distribution_resource_policies
    ADD CONSTRAINT distribution_resource_policies_id_key UNIQUE (id);

ALTER TABLE ONLY distribution_resource_policies
    ADD CONSTRAINT distribution_resource_policies_pkey PRIMARY KEY (resource_key);

ALTER TABLE ONLY distribution_resource_usage
    ADD CONSTRAINT distribution_resource_usage_pkey PRIMARY KEY (batch_id);

ALTER TABLE ONLY distribution_source_turns
    ADD CONSTRAINT distribution_source_turns_pkey PRIMARY KEY (source_namespace);

ALTER TABLE ONLY distribution_sources
    ADD CONSTRAINT distribution_sources_pkey PRIMARY KEY (namespace);

ALTER TABLE ONLY orchestration_delegates
    ADD CONSTRAINT orchestration_delegates_pkey PRIMARY KEY (id);

ALTER TABLE ONLY orchestration_delegates
    ADD CONSTRAINT orchestration_delegates_task_id_key UNIQUE (task_id);

ALTER TABLE ONLY orchestration_dispatches
    ADD CONSTRAINT orchestration_dispatches_pkey PRIMARY KEY (message_id);

ALTER TABLE ONLY orchestration_messages
    ADD CONSTRAINT orchestration_messages_pkey PRIMARY KEY (id);

ALTER TABLE ONLY orchestration_messages
    ADD CONSTRAINT orchestration_messages_platform_id_evidence_ref_key UNIQUE (platform_id, evidence_ref);

ALTER TABLE ONLY orchestration_messages
    ADD CONSTRAINT orchestration_messages_platform_id_source_event_id_key UNIQUE (platform_id, source_event_id);

ALTER TABLE ONLY orchestration_result_queue
    ADD CONSTRAINT orchestration_result_queue_pkey PRIMARY KEY (message_id);

-- index
CREATE INDEX channel_observation_attempt ON channel_execution_observations USING btree (attempt_id, id);

CREATE INDEX channel_observation_batch ON channel_execution_observations USING btree (batch_id, id);

CREATE INDEX channel_observation_task_page ON channel_execution_observations USING btree (channel, task_id, id);

CREATE INDEX channel_receiving_unit ON channel_receiving_configurations USING btree (business_unit_id, channel, id);

CREATE INDEX distribution_resource_usage_window ON distribution_resource_usage USING btree (resource_key, called_at);

CREATE INDEX orchestration_delegate_message ON orchestration_delegates USING btree (message_id);

CREATE INDEX orchestration_receipt_lookup ON orchestration_messages USING btree (evidence_ref);

-- fk constraint
ALTER TABLE ONLY channel_receiving_configurations
    ADD CONSTRAINT channel_receiving_configurations_authorized_by_fkey FOREIGN KEY (authorized_by) REFERENCES accounts(id);

ALTER TABLE ONLY channel_receiving_configurations
    ADD CONSTRAINT channel_receiving_configurations_business_unit_id_fkey FOREIGN KEY (business_unit_id) REFERENCES business_units(id);

ALTER TABLE ONLY distribution_resource_usage
    ADD CONSTRAINT distribution_resource_usage_batch_id_fkey FOREIGN KEY (batch_id) REFERENCES broadcast_batches(id);

ALTER TABLE ONLY distribution_resource_usage
    ADD CONSTRAINT distribution_resource_usage_source_namespace_fkey FOREIGN KEY (source_namespace) REFERENCES distribution_sources(namespace);

ALTER TABLE ONLY distribution_source_turns
    ADD CONSTRAINT distribution_source_turns_source_namespace_fkey FOREIGN KEY (source_namespace) REFERENCES distribution_sources(namespace);

ALTER TABLE ONLY orchestration_delegates
    ADD CONSTRAINT orchestration_delegates_message_id_fkey FOREIGN KEY (message_id) REFERENCES orchestration_messages(id);

ALTER TABLE ONLY orchestration_delegates
    ADD CONSTRAINT orchestration_delegates_snapshot_id_fkey FOREIGN KEY (snapshot_id) REFERENCES distribution_message_snapshots(id);

ALTER TABLE ONLY orchestration_dispatches
    ADD CONSTRAINT orchestration_dispatches_message_id_fkey FOREIGN KEY (message_id) REFERENCES orchestration_messages(id);

ALTER TABLE ONLY orchestration_messages
    ADD CONSTRAINT orchestration_messages_platform_id_unit_id_fkey FOREIGN KEY (platform_id, unit_id) REFERENCES business_units(platform_id, id);

ALTER TABLE ONLY orchestration_result_queue
    ADD CONSTRAINT orchestration_result_queue_message_id_fkey FOREIGN KEY (message_id) REFERENCES orchestration_messages(id);

-- trigger
CREATE TRIGGER channel_observations_immutable BEFORE DELETE OR UPDATE ON channel_execution_observations FOR EACH ROW EXECUTE FUNCTION source_routing_immutable();

CREATE TRIGGER distribution_snapshot_immutable BEFORE DELETE OR UPDATE ON distribution_message_snapshots FOR EACH ROW EXECUTE FUNCTION distribution_snapshot_immutable();

CREATE TRIGGER message_delegate_created AFTER INSERT ON orchestration_delegates FOR EACH ROW EXECUTE FUNCTION orchestration_result_changed();

CREATE TRIGGER message_dispatch_progress BEFORE UPDATE ON orchestration_dispatches FOR EACH ROW EXECUTE FUNCTION message_progress_clock();

CREATE TRIGGER orchestration_delegate_immutable BEFORE DELETE OR UPDATE ON orchestration_delegates FOR EACH ROW EXECUTE FUNCTION orchestration_message_immutable();

CREATE TRIGGER orchestration_dispatch_fixed BEFORE UPDATE ON orchestration_dispatches FOR EACH ROW EXECUTE FUNCTION orchestration_dispatch_fixed();

CREATE TRIGGER orchestration_message_immutable BEFORE DELETE OR UPDATE ON orchestration_messages FOR EACH ROW EXECUTE FUNCTION orchestration_message_immutable();

