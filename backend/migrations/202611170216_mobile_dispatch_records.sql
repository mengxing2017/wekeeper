-- 手机分发受理、操作与候选核对记录。
-- 空库初始化基线；后续结构变更使用新增递增迁移。

-- function
CREATE FUNCTION mobile_record_attempt() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
 IF (NEW.state,NEW.current) IS DISTINCT FROM (OLD.state,OLD.current) THEN
 INSERT INTO mobile_v9_dispatch_events(intake_id,item_id,attempt_id,device_id,kind,reason,result,evidence)
 SELECT i.id,NEW.item_id,NEW.id,p.device_id,'attempt_state_changed',coalesce(NEW.termination_reason,NEW.invalidation_reason,NEW.state),NEW.state,
 jsonb_build_object('previous_state',OLD.state,'current',NEW.current,'attempt_no',NEW.attempt_no)
 FROM mobile_dispatch_intakes i JOIN mobile_v9_plans p ON p.id=NEW.plan_id WHERE i.item_id=NEW.item_id;
 END IF; RETURN NEW;
END $$;

CREATE FUNCTION mobile_record_dispatch() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
 IF (NEW.state,NEW.reason) IS DISTINCT FROM (OLD.state,OLD.reason) THEN
 INSERT INTO mobile_v9_dispatch_events(item_id,round_no,kind,reason,evidence)
 VALUES(NEW.item_id,least(NEW.completed_rounds+1,3),'dispatch_state_changed',NEW.reason,
 jsonb_build_object('state',NEW.state,'completed_rounds',NEW.completed_rounds,'max_rounds',NEW.max_rounds,'next_retry_at',NEW.next_retry_at,'round_started_at',NEW.round_started_at));
 END IF; RETURN NEW;
END $$;

CREATE FUNCTION mobile_record_event_context() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
 NEW.intake_id=coalesce(NEW.intake_id,(SELECT id FROM mobile_dispatch_intakes WHERE item_id=NEW.item_id));
 NEW.operation_id=coalesce(NEW.operation_id,nullif(current_setting('wekeeper.mobile_operation',true),'')::uuid);
 NEW.delivery_request_id=coalesce(NEW.delivery_request_id,nullif(current_setting('wekeeper.mobile_exchange',true),'')::uuid);
 RETURN NEW;
END $$;

CREATE FUNCTION mobile_record_grant() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
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

CREATE FUNCTION mobile_record_reassignment() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
 INSERT INTO mobile_v9_dispatch_events(intake_id,item_id,attempt_id,device_id,kind,reason,result,evidence)
 SELECT i.id,a.item_id,a.id,p.device_id,'assignment_changed',CASE WHEN NEW.source='automatic' THEN NEW.reason ELSE 'manual_reassignment' END,'assigned',
 jsonb_build_object('reassignment_id',NEW.id,'old_attempt_id',NEW.old_attempt_id,'new_attempt_id',NEW.new_attempt_id,'source',NEW.source)
 FROM mobile_v9_attempts a JOIN mobile_v9_plans p ON p.id=a.plan_id JOIN mobile_dispatch_intakes i ON i.item_id=a.item_id WHERE a.id=NEW.new_attempt_id;
 RETURN NEW;
END $$;

CREATE FUNCTION mobile_record_review() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
 INSERT INTO mobile_v9_dispatch_events(intake_id,item_id,attempt_id,kind,reason,result,evidence)
 SELECT i.id,a.item_id,a.id,'manual_review',NEW.decision,NEW.decision,jsonb_build_object('review_id',NEW.id,'reviewer',NEW.reviewer)
 FROM mobile_v9_attempts a JOIN mobile_dispatch_intakes i ON i.item_id=a.item_id WHERE a.id=NEW.attempt_id;
 RETURN NEW;
END $$;

CREATE FUNCTION mobile_record_stop() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
 IF NEW.stopped AND NOT OLD.stopped THEN
 INSERT INTO mobile_v9_dispatch_events(intake_id,item_id,kind,reason,result,evidence)
 SELECT id,item_id,'task_stopped','task_stopped','stopped',jsonb_build_object('task_id',NEW.id) FROM mobile_dispatch_intakes WHERE task_id=NEW.id;
 END IF; RETURN NEW;
END $$;

-- table
CREATE TABLE mobile_dispatch_checks (
    id uuid NOT NULL,
    operation_id uuid,
    delivery_request_id uuid,
    intake_id uuid,
    device_id uuid,
    check_kind text NOT NULL,
    decision text NOT NULL,
    reason_code text NOT NULL,
    evidence jsonb DEFAULT '{}'::jsonb NOT NULL,
    created_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    CONSTRAINT mobile_dispatch_checks_check CHECK ((num_nonnulls(operation_id, delivery_request_id) = 1))
);

CREATE TABLE mobile_dispatch_intakes (
    id uuid NOT NULL,
    source_kind text NOT NULL,
    message_id uuid,
    receipt_id uuid,
    configuration_id uuid,
    target_id uuid,
    target_name text NOT NULL,
    delegate_id uuid,
    snapshot_id uuid NOT NULL,
    plan jsonb NOT NULL,
    content text NOT NULL,
    payload_hash text NOT NULL,
    task_id uuid,
    item_id uuid,
    creation_state text DEFAULT 'registered'::text NOT NULL,
    reason_code text,
    next_retry_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    created_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    updated_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    CONSTRAINT mobile_dispatch_intakes_source_kind_check CHECK ((source_kind = ANY (ARRAY['source_route'::text, 'manual'::text])))
);

CREATE TABLE mobile_dispatch_operations (
    id uuid NOT NULL,
    intake_id uuid NOT NULL,
    kind text NOT NULL,
    round_no integer,
    attempt_id uuid,
    result text DEFAULT 'started'::text NOT NULL,
    reason_code text,
    started_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    finished_at timestamp with time zone,
    retry_at timestamp with time zone,
    CONSTRAINT mobile_dispatch_operations_kind_check CHECK ((kind = ANY (ARRAY['create'::text, 'evaluate'::text])))
);

-- constraint
ALTER TABLE ONLY mobile_dispatch_checks
    ADD CONSTRAINT mobile_dispatch_checks_pkey PRIMARY KEY (id);

ALTER TABLE ONLY mobile_dispatch_intakes
    ADD CONSTRAINT mobile_dispatch_intakes_delegate_id_key UNIQUE (delegate_id);

ALTER TABLE ONLY mobile_dispatch_intakes
    ADD CONSTRAINT mobile_dispatch_intakes_item_id_key UNIQUE (item_id);

ALTER TABLE ONLY mobile_dispatch_intakes
    ADD CONSTRAINT mobile_dispatch_intakes_pkey PRIMARY KEY (id);

ALTER TABLE ONLY mobile_dispatch_operations
    ADD CONSTRAINT mobile_dispatch_operations_pkey PRIMARY KEY (id);

-- index
CREATE UNIQUE INDEX mobile_auto_intake_key ON mobile_dispatch_intakes USING btree (message_id, configuration_id, target_id) NULLS NOT DISTINCT WHERE (source_kind = 'source_route'::text);

CREATE INDEX mobile_check_device ON mobile_dispatch_checks USING btree (device_id, created_at DESC, id DESC);

CREATE INDEX mobile_check_exchange ON mobile_dispatch_checks USING btree (delivery_request_id, created_at, id);

CREATE INDEX mobile_check_intake_reason ON mobile_dispatch_checks USING btree (intake_id, reason_code, created_at DESC, id DESC);

CREATE INDEX mobile_check_operation ON mobile_dispatch_checks USING btree (operation_id, created_at, id);

CREATE INDEX mobile_intake_configuration ON mobile_dispatch_intakes USING btree (configuration_id, created_at DESC, id DESC);

CREATE INDEX mobile_intake_due ON mobile_dispatch_intakes USING btree (next_retry_at, id) WHERE (creation_state = ANY (ARRAY['registered'::text, 'processing'::text, 'retrying'::text]));

CREATE INDEX mobile_intake_message ON mobile_dispatch_intakes USING btree (message_id, created_at DESC, id DESC);

CREATE INDEX mobile_intake_receipt ON mobile_dispatch_intakes USING btree (receipt_id);

CREATE INDEX mobile_intake_source_time ON mobile_dispatch_intakes USING btree (source_kind, created_at DESC, id DESC);

CREATE INDEX mobile_intake_target ON mobile_dispatch_intakes USING btree (target_id, created_at DESC, id DESC);

CREATE INDEX mobile_intake_task ON mobile_dispatch_intakes USING btree (task_id);

CREATE INDEX mobile_intake_time ON mobile_dispatch_intakes USING btree (created_at DESC, id DESC);

CREATE INDEX mobile_operation_intake ON mobile_dispatch_operations USING btree (intake_id, started_at DESC, id DESC);

CREATE INDEX mobile_operation_started ON mobile_dispatch_operations USING btree (started_at, id) WHERE (result = 'started'::text);

-- fk constraint
ALTER TABLE ONLY mobile_dispatch_checks
    ADD CONSTRAINT mobile_dispatch_checks_delivery_request_id_fkey FOREIGN KEY (delivery_request_id) REFERENCES mobile_delivery_exchanges(id);

ALTER TABLE ONLY mobile_dispatch_checks
    ADD CONSTRAINT mobile_dispatch_checks_intake_id_fkey FOREIGN KEY (intake_id) REFERENCES mobile_dispatch_intakes(id);

ALTER TABLE ONLY mobile_dispatch_checks
    ADD CONSTRAINT mobile_dispatch_checks_operation_id_fkey FOREIGN KEY (operation_id) REFERENCES mobile_dispatch_operations(id);

ALTER TABLE ONLY mobile_dispatch_operations
    ADD CONSTRAINT mobile_dispatch_operations_intake_id_fkey FOREIGN KEY (intake_id) REFERENCES mobile_dispatch_intakes(id);

