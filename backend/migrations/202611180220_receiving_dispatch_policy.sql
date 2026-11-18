-- Project-owned dispatch policy. External source/device wire contracts are unchanged.
ALTER TABLE channel_receiving_configurations ADD COLUMN revision bigint NOT NULL DEFAULT 1;
ALTER TABLE channel_receiving_configurations ADD COLUMN next_target_id uuid;
ALTER TABLE channel_receiving_configurations ADD COLUMN next_device_id uuid;
CREATE TABLE receiving_dispatch_decisions (
 id uuid PRIMARY KEY, message_id uuid NOT NULL REFERENCES orchestration_messages(id),
 configuration_id uuid NOT NULL REFERENCES channel_receiving_configurations(id), channel text NOT NULL CHECK(channel IN ('mobile','wecom')),
 configuration jsonb NOT NULL, plan jsonb NOT NULL, created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 UNIQUE(message_id,configuration_id,channel)
);
CREATE TABLE receiving_dispatch_targets (
 id uuid PRIMARY KEY, decision_id uuid NOT NULL REFERENCES receiving_dispatch_decisions(id),
 message_id uuid NOT NULL REFERENCES orchestration_messages(id), channel text NOT NULL,
 target_id uuid NOT NULL, target jsonb NOT NULL, plan jsonb NOT NULL,
 expected_count integer NOT NULL CHECK(expected_count>=0), materialized_count integer NOT NULL DEFAULT 0,
 stopped boolean NOT NULL DEFAULT false, not_before timestamptz NOT NULL,
 UNIQUE(message_id,channel,target_id), CHECK(materialized_count BETWEEN 0 AND expected_count)
);
CREATE INDEX receiving_materialization_due ON receiving_dispatch_targets(message_id,id) WHERE channel='mobile' AND materialized_count<expected_count;
CREATE TABLE receiving_dispatch_deliveries (
 id uuid PRIMARY KEY, dispatch_target_id uuid NOT NULL REFERENCES receiving_dispatch_targets(id),
 device_id uuid, position integer NOT NULL, created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 UNIQUE(dispatch_target_id,position)
);
CREATE UNIQUE INDEX receiving_delivery_pool ON receiving_dispatch_deliveries(dispatch_target_id) WHERE device_id IS NULL;
CREATE UNIQUE INDEX receiving_delivery_device ON receiving_dispatch_deliveries(dispatch_target_id,device_id) WHERE device_id IS NOT NULL;
ALTER TABLE mobile_dispatch_intakes ADD COLUMN delivery_id uuid UNIQUE REFERENCES receiving_dispatch_deliveries(id);
DROP INDEX mobile_auto_intake_key;
CREATE UNIQUE INDEX mobile_auto_intake_key ON mobile_dispatch_intakes(delivery_id) WHERE source_kind='source_route';
ALTER TABLE mobile_dispatch_intakes ADD CONSTRAINT mobile_auto_delivery_required CHECK(source_kind<>'source_route' OR delivery_id IS NOT NULL);
CREATE VIEW receiving_task_progress AS
 WITH planned AS (
         SELECT d.message_id,
            (plan.value ->> 'channel'::text) AS channel,
            target.value AS target,
            ((plan.value ->> 'id'::text))::uuid AS delegate_id,
            NULL::uuid AS task_id,
                CASE
                    WHEN (d.state = 'failed'::text) THEN 'failed'::text
                    ELSE 'planning'::text
                END AS delegate_state,
            COALESCE(d.reason_code, d.last_error_code) AS delegate_reason,
            d.stage_entered_at AS created_at,
            d.next_attempt_at AS process_retry_at,
            d.last_error_code AS process_error
           FROM ((orchestration_dispatches d
             CROSS JOIN LATERAL jsonb_array_elements(d.plans) plan(value))
             CROSS JOIN LATERAL jsonb_array_elements((plan.value -> 'targets'::text)) target(value))
          WHERE (d.state = ANY (ARRAY['preparing'::text, 'ready'::text, 'failed'::text]))
        UNION ALL
         SELECT d.message_id,
            d.channel,
            target.value AS target,
            d.id,
            d.task_id,
            d.state,
            d.reason_code,
            d.created_at,
            NULL::timestamp with time zone AS timestamptz,
            NULL::text AS text
           FROM (orchestration_delegates d
             CROSS JOIN LATERAL jsonb_array_elements((d.plan -> 'targets'::text)) target(value))
        ), frozen AS (
         SELECT DISTINCT ON (planned.message_id, planned.channel, (planned.target ->> 'id'::text), planned.delegate_id) planned.message_id,
            planned.channel,
            ((planned.target ->> 'id'::text))::uuid AS target_id,
            (planned.target ->> 'name'::text) AS target_name,
            planned.delegate_id,
            planned.task_id,
            planned.delegate_state,
            planned.delegate_reason,
            planned.created_at,
            planned.process_retry_at,
            planned.process_error,
            planned.target
           FROM planned
          ORDER BY planned.message_id, planned.channel, (planned.target ->> 'id'::text), planned.delegate_id,
                CASE planned.delegate_state
                    WHEN 'delegated'::text THEN 0
                    WHEN 'failed'::text THEN 1
                    ELSE 2
                END, planned.created_at, planned.delegate_id
        ), facts AS (
         SELECT f.message_id,
            f.channel,
            f.target_id,
            f.target_name,
            f.delegate_id,
            f.task_id,
            f.delegate_state,
            f.delegate_reason,
            f.created_at,
            f.process_retry_at,
            f.process_error,
            f.target,
            i.id AS item_id,
            ds.state AS dispatch_state,
            ds.reason AS dispatch_reason,
            COALESCE(ds.next_retry_at, f.process_retry_at) AS next_retry_at,
            a.id AS attempt_id,
            a.state AS attempt_state,
            a.attempt_no,
            p.device_id,
            presence.state AS device_presence,
            device.enabled AS device_enabled,
            (EXISTS ( SELECT 1
                   FROM mobile_execution_grants lock
                  WHERE ((lock.device_id = p.device_id) AND (NOT lock.session_closed) AND (lock.attempt_id IS DISTINCT FROM a.id)))) AS device_busy,
            g.assigned_at,
            g.accepted_at,
            g.result_deadline_at,
            g.state AS grant_state,
            COALESCE(mt.stopped, false) AS mobile_stopped,
            COALESCE(history.sent, false) AS sent,
            COALESCE(history.risk, false) AS historical_risk,
            x.id AS official_target_id,
            x.official_status,
            x.rejected,
            b.state AS batch_state,
            b.stop_state,
            b.error_code,
            b.sync_error,
            b.claimed_at,
            b.last_synced_at,
            b.next_sync_at,
            b.sender,
            GREATEST(f.created_at, ds.progress_at, a.progress_at, g.assigned_at, g.accepted_at, g.result_received_at, x.progress_at, b.progress_at) AS last_progress_at
           FROM (((((((((((frozen f
             LEFT JOIN mobile_v9_task_items i ON (((f.channel = 'mobile'::text) AND (i.task_id = f.task_id) AND (i.target_id = f.target_id))))
             LEFT JOIN mobile_v9_tasks mt ON ((mt.id = i.task_id)))
             LEFT JOIN mobile_v9_dispatch_state ds ON ((ds.item_id = i.id)))
             LEFT JOIN mobile_v9_attempts a ON (((a.item_id = i.id) AND a.current)))
             LEFT JOIN mobile_v9_plans p ON ((p.id = a.plan_id)))
             LEFT JOIN execution_devices device ON ((device.id = p.device_id)))
             LEFT JOIN device_control_presence presence ON ((presence.id = p.device_id)))
             LEFT JOIN mobile_execution_grants g ON ((g.attempt_id = a.id)))
             LEFT JOIN LATERAL ( SELECT bool_or(((h.state = ANY (ARRAY['observed_sent'::text, 'manually_confirmed_sent'::text])) OR ((hg.result_body ->> 'outcome'::text) = 'observed_sent'::text))) AS sent,
                    bool_or(((NOT h.current) AND ((h.state = 'unknown'::text) OR ((hg.result_body ->> 'outcome'::text) = 'unknown'::text)))) AS risk
                   FROM (mobile_v9_attempts h
                     LEFT JOIN mobile_execution_grants hg ON ((hg.attempt_id = h.id)))
                  WHERE (h.item_id = i.id)) history ON ((f.channel = 'mobile'::text)))
             LEFT JOIN broadcast_targets x ON (((f.channel = 'wecom'::text) AND (x.task_id = f.task_id) AND (x.group_id = f.target_id))))
             LEFT JOIN broadcast_batches b ON ((b.id = x.batch_id)))
        ), outcomes AS (
         SELECT facts.message_id,
            facts.channel,
            facts.target_id,
            facts.target_name,
            facts.delegate_id,
            facts.task_id,
            facts.delegate_state,
            facts.delegate_reason,
            facts.created_at,
            facts.process_retry_at,
            facts.process_error,
            facts.target,
            facts.item_id,
            facts.dispatch_state,
            facts.dispatch_reason,
            facts.next_retry_at,
            facts.attempt_id,
            facts.attempt_state,
            facts.attempt_no,
            facts.device_id,
            facts.device_presence,
            facts.device_enabled,
            facts.device_busy,
            facts.assigned_at,
            facts.accepted_at,
            facts.result_deadline_at,
            facts.grant_state,
            facts.mobile_stopped,
            facts.sent,
            facts.historical_risk,
            facts.official_target_id,
            facts.official_status,
            facts.rejected,
            facts.batch_state,
            facts.stop_state,
            facts.error_code,
            facts.sync_error,
            facts.claimed_at,
            facts.last_synced_at,
            facts.next_sync_at,
            facts.sender,
            facts.last_progress_at,
                CASE
                    WHEN (((facts.channel = 'mobile'::text) AND (facts.sent OR (facts.dispatch_state = 'completed'::text))) OR ((facts.channel = 'wecom'::text) AND (facts.official_status = 1))) THEN 'sent'::text
                    WHEN (facts.delegate_state = 'failed'::text) THEN 'failed'::text
                    WHEN (facts.channel = 'mobile'::text) THEN
                    CASE
                        WHEN (facts.mobile_stopped OR (facts.dispatch_state = ANY (ARRAY['dispatch_failed'::text, 'stopped'::text, 'confirmation_expired'::text]))) THEN 'failed'::text
                        WHEN (facts.dispatch_state = 'needs_review'::text) THEN 'unknown'::text
                        WHEN ((facts.dispatch_state IS NULL) AND (facts.attempt_state = 'unknown'::text)) THEN 'unknown'::text
                        WHEN ((facts.dispatch_state IS NULL) AND (facts.attempt_state = 'cancelled'::text)) THEN 'failed'::text
                        ELSE 'pending'::text
                    END
                    WHEN ((facts.batch_state = 'unknown'::text) OR (facts.stop_state = 'unknown'::text) OR ((facts.official_status IS NOT NULL) AND (facts.official_status <> ALL (ARRAY[0, 1, 2, 3])))) THEN 'unknown'::text
                    WHEN ((facts.batch_state = ANY (ARRAY['rejected'::text, 'cancelled'::text])) OR (facts.stop_state = 'stopped'::text) OR facts.rejected OR (facts.official_status = ANY (ARRAY[2, 3]))) THEN 'failed'::text
                    ELSE 'pending'::text
                END AS outcome
           FROM facts
        ), phases AS (
         SELECT outcomes.message_id,
            outcomes.channel,
            outcomes.target_id,
            outcomes.target_name,
            outcomes.delegate_id,
            outcomes.task_id,
            outcomes.delegate_state,
            outcomes.delegate_reason,
            outcomes.created_at,
            outcomes.process_retry_at,
            outcomes.process_error,
            outcomes.target,
            outcomes.item_id,
            outcomes.dispatch_state,
            outcomes.dispatch_reason,
            outcomes.next_retry_at,
            outcomes.attempt_id,
            outcomes.attempt_state,
            outcomes.attempt_no,
            outcomes.device_id,
            outcomes.device_presence,
            outcomes.device_enabled,
            outcomes.device_busy,
            outcomes.assigned_at,
            outcomes.accepted_at,
            outcomes.result_deadline_at,
            outcomes.grant_state,
            outcomes.mobile_stopped,
            outcomes.sent,
            outcomes.historical_risk,
            outcomes.official_target_id,
            outcomes.official_status,
            outcomes.rejected,
            outcomes.batch_state,
            outcomes.stop_state,
            outcomes.error_code,
            outcomes.sync_error,
            outcomes.claimed_at,
            outcomes.last_synced_at,
            outcomes.next_sync_at,
            outcomes.sender,
            outcomes.last_progress_at,
            outcomes.outcome,
                CASE
                    WHEN (outcomes.delegate_state = 'planning'::text) THEN 'creating_tasks'::text
                    WHEN (outcomes.outcome <> 'pending'::text) THEN outcomes.outcome
                    WHEN (((outcomes.channel = 'mobile'::text) AND ((outcomes.item_id IS NULL) OR (outcomes.attempt_id IS NULL))) OR ((outcomes.channel = 'wecom'::text) AND (outcomes.official_target_id IS NULL))) THEN 'missing_evidence'::text
                    WHEN ((outcomes.channel = 'mobile'::text) AND (outcomes.grant_state = 'active'::text) AND (outcomes.accepted_at IS NOT NULL)) THEN 'device_execution'::text
                    WHEN ((outcomes.channel = 'mobile'::text) AND (outcomes.grant_state = 'active'::text)) THEN 'device_receipt'::text
                    WHEN (outcomes.channel = 'mobile'::text) THEN 'scheduling'::text
                    WHEN (outcomes.batch_state = 'created'::text) THEN 'official_result'::text
                    WHEN (outcomes.batch_state = 'submitting'::text) THEN 'official_call'::text
                    ELSE 'official_queue'::text
                END AS stage
           FROM outcomes
        )
 SELECT message_id,
    channel,
    target_id,
    target_name,
    delegate_id,
    task_id,
    delegate_state,
    delegate_reason,
    created_at,
    process_retry_at,
    process_error,
    target,
    item_id,
    dispatch_state,
    dispatch_reason,
    next_retry_at,
    attempt_id,
    attempt_state,
    attempt_no,
    device_id,
    device_presence,
    device_enabled,
    device_busy,
    assigned_at,
    accepted_at,
    result_deadline_at,
    grant_state,
    mobile_stopped,
    sent,
    historical_risk,
    official_target_id,
    official_status,
    rejected,
    batch_state,
    stop_state,
    error_code,
    sync_error,
    claimed_at,
    last_synced_at,
    next_sync_at,
    sender,
    last_progress_at,
    outcome,
    stage,
    COALESCE(delegate_reason,
        CASE
            WHEN (stage = ANY (ARRAY['device_receipt'::text, 'device_execution'::text, 'sent'::text, 'unknown'::text])) THEN stage
            WHEN (((channel = 'mobile'::text) AND (outcome = 'failed'::text) AND mobile_stopped) OR ((channel = 'wecom'::text) AND (outcome = 'failed'::text) AND (stop_state = 'stopped'::text))) THEN 'task_stopped'::text
            WHEN ((channel = 'mobile'::text) AND (stage = 'scheduling'::text) AND (dispatch_reason = 'awaiting_round_deadline'::text)) THEN dispatch_reason
            WHEN ((channel = 'mobile'::text) AND (stage = 'scheduling'::text) AND (device_presence = 'offline'::text)) THEN 'device_offline'::text
            WHEN ((channel = 'mobile'::text) AND (stage = 'scheduling'::text) AND (device_enabled = false)) THEN 'device_disabled'::text
            WHEN ((channel = 'mobile'::text) AND (stage = 'scheduling'::text) AND device_busy) THEN 'device_busy'::text
            WHEN (channel = 'mobile'::text) THEN dispatch_reason
            ELSE error_code
        END, stage) AS reason_code,
        CASE
            WHEN (stage = 'missing_evidence'::text) THEN 'unobserved'::text
            WHEN ((stage = 'creating_tasks'::text) AND (process_error IS NOT NULL)) THEN 'retrying'::text
            WHEN (outcome = 'unknown'::text) THEN 'attention'::text
            WHEN (outcome = ANY (ARRAY['sent'::text, 'failed'::text])) THEN 'terminal'::text
            WHEN ((channel = 'mobile'::text) AND (grant_state = 'active'::text) AND ((result_deadline_at + make_interval(secs => (COALESCE((NULLIF(current_setting('wekeeper.progress_stall_seconds'::text, true), ''::text))::integer, 60))::double precision)) < transaction_timestamp())) THEN 'attention'::text
            WHEN ((stage = 'scheduling'::text) AND ((next_retry_at + make_interval(secs => (COALESCE((NULLIF(current_setting('wekeeper.progress_stall_seconds'::text, true), ''::text))::integer, 60))::double precision)) < transaction_timestamp())) THEN 'attention'::text
            WHEN ((stage = 'official_result'::text) AND ((next_sync_at + make_interval(secs => (COALESCE((NULLIF(current_setting('wekeeper.progress_stall_seconds'::text, true), ''::text))::integer, 60))::double precision)) < transaction_timestamp())) THEN 'attention'::text
            WHEN ((channel = 'wecom'::text) AND (sync_error IS NOT NULL)) THEN 'retrying'::text
            WHEN ((stage = ANY (ARRAY['creating_tasks'::text, 'scheduling'::text, 'official_queue'::text, 'official_call'::text])) AND ((last_progress_at + make_interval(secs => (COALESCE((NULLIF(current_setting('wekeeper.progress_stall_seconds'::text, true), ''::text))::integer, 60))::double precision)) < transaction_timestamp()) AND (COALESCE(next_retry_at, next_sync_at, last_progress_at) <= transaction_timestamp())) THEN 'attention'::text
            ELSE 'normal'::text
        END AS severity
   FROM phases;
CREATE OR REPLACE VIEW mobile_dispatch_record_view AS
 SELECT i.id,
    i.source_kind,
    i.message_id,
    i.receipt_id,
    i.configuration_id,
    i.target_id,
    i.target_name,
    i.task_id,
    i.item_id,
    i.created_at,
    i.creation_state,
    "left"(i.content, 80) AS content_summary,
    p.device_id,
    device.name AS device_name,
    a.id AS attempt_id,
    a.attempt_no,
    g.id AS execution_id,
    g.assigned_at AS generated_at,
    g.accepted_at AS received_at,
    g.result_deadline_at,
    (ds.round_started_at + '00:05:00'::interval) AS round_deadline_at,
    ds.completed_rounds,
    ds.max_rounds,
        CASE
            WHEN (i.creation_state = ANY (ARRAY['registered'::text, 'processing'::text, 'retrying'::text])) THEN i.next_retry_at
            ELSE ds.next_retry_at
        END AS next_evaluation_at,
        CASE
            WHEN history.sent THEN 'sent'::text
            WHEN (i.creation_state = 'rejected'::text) THEN 'failed'::text
            WHEN (i.creation_state <> 'created'::text) THEN 'pending'::text
            WHEN mt.stopped THEN 'failed'::text
            ELSE COALESCE(v.outcome,
            CASE
                WHEN (ds.state = 'completed'::text) THEN 'sent'::text
                WHEN (ds.state = ANY (ARRAY['stopped'::text, 'confirmation_expired'::text, 'dispatch_failed'::text])) THEN 'failed'::text
                WHEN (ds.state = 'needs_review'::text) THEN 'unknown'::text
                ELSE 'pending'::text
            END)
        END AS outcome,
        CASE
            WHEN history.sent THEN 'sent'::text
            WHEN (i.creation_state <> 'created'::text) THEN i.creation_state
            WHEN mt.stopped THEN 'stopped'::text
            WHEN (ds.state = 'waiting_resources'::text) THEN 'round_wait'::text
            WHEN (COALESCE(v.outcome, 'pending'::text) <> 'pending'::text) THEN v.outcome
            WHEN (ds.state = ANY (ARRAY['completed'::text, 'stopped'::text, 'confirmation_expired'::text, 'dispatch_failed'::text, 'needs_review'::text])) THEN ds.state
            WHEN ((g.state = 'active'::text) AND (g.accepted_at IS NOT NULL)) THEN 'executing'::text
            WHEN (g.state = 'active'::text) THEN 'awaiting_receipt'::text
            ELSE 'awaiting_dispatch'::text
        END AS stage,
        CASE
            WHEN (i.creation_state <> 'created'::text) THEN COALESCE(i.reason_code, 'intake_registered'::text)
            WHEN (ds.state = 'waiting_resources'::text) THEN ds.reason
            ELSE COALESCE(v.reason_code, ds.reason, 'awaiting_dispatch'::text)
        END AS reason_code,
    GREATEST(i.updated_at, ds.progress_at, a.progress_at, g.assigned_at, g.accepted_at, g.result_received_at) AS last_progress_at,
    ( SELECT max(o.finished_at) AS max
           FROM mobile_dispatch_operations o
          WHERE ((o.intake_id = i.id) AND (o.kind = 'evaluate'::text))) AS last_checked_at,
    (COALESCE(v.historical_risk, false) OR COALESCE(history.risk, false)) AS historical_risk,
    COALESCE(( SELECT jsonb_agg(jsonb_build_object('device_id', c.device_id, 'device_name', d.name, 'reason_code', c.reason_code, 'checked_at', c.created_at) ORDER BY c.created_at, c.id) AS jsonb_agg
           FROM (mobile_dispatch_checks c
             LEFT JOIN execution_devices d ON ((d.id = c.device_id)))
          WHERE ((c.operation_id = ( SELECT o.id
                   FROM mobile_dispatch_operations o
                  WHERE ((o.intake_id = i.id) AND (o.kind = 'evaluate'::text))
                  ORDER BY o.started_at DESC, o.id DESC
                 LIMIT 1)) AND (c.decision <> 'passed'::text))), '[]'::jsonb) AS candidate_reasons,
    md5(concat_ws('|'::text, i.creation_state, i.reason_code, i.updated_at, ds.progress_at, a.id, g.id, g.state, g.accepted_at, g.result_id, ds.next_retry_at)) AS version,
    g.receipt_deadline_at
   FROM ((((((((mobile_dispatch_intakes i
     LEFT JOIN mobile_v9_tasks mt ON ((mt.id = i.task_id)))
     LEFT JOIN LATERAL ( SELECT bool_or(((a_1.state = ANY (ARRAY['observed_sent'::text, 'manually_confirmed_sent'::text])) OR ((g_1.result_body ->> 'outcome'::text) = 'observed_sent'::text))) AS sent,
            bool_or(((NOT a_1.current) AND ((a_1.state = 'unknown'::text) OR ((g_1.result_body ->> 'outcome'::text) = 'unknown'::text)))) AS risk
           FROM (mobile_v9_attempts a_1
             LEFT JOIN mobile_execution_grants g_1 ON ((g_1.attempt_id = a_1.id)))
          WHERE (a_1.item_id = i.item_id)) history ON (true))
     LEFT JOIN mobile_v9_dispatch_state ds ON ((ds.item_id = i.item_id)))
     LEFT JOIN mobile_v9_attempts a ON (((a.item_id = i.item_id) AND a.current)))
     LEFT JOIN mobile_v9_plans p ON ((p.id = a.plan_id)))
     LEFT JOIN execution_devices device ON ((device.id = p.device_id)))
     LEFT JOIN mobile_execution_grants g ON ((g.attempt_id = a.id)))
     LEFT JOIN receiving_task_progress v ON (((v.message_id = i.message_id) AND (v.channel = 'mobile'::text) AND (v.target_id = i.target_id) AND (v.task_id = i.task_id))));

CREATE VIEW receiving_delivery_progress AS
SELECT d.id,d.dispatch_target_id,d.device_id,d.position,t.message_id,t.target_id,t.target->>'name' AS target_name,
 coalesce(device.name,d.device_id::text) AS device_name,i.id AS intake_id,i.task_id,i.item_id,
 CASE WHEN coalesce(v.outcome,'pending')='sent' THEN 'sent'
      WHEN i.creation_state='rejected' THEN 'failed'
      WHEN t.stopped AND i.task_id IS NULL THEN 'failed'
      WHEN ds.state IN ('completed','stopped','confirmation_expired','dispatch_failed') AND risk.unknown THEN 'unknown'
      ELSE coalesce(v.outcome,'pending') END AS outcome,
 coalesce(v.stage,CASE WHEN t.stopped THEN 'stopped' ELSE 'creating_tasks' END) AS stage,
 coalesce(v.reason_code,CASE WHEN t.stopped THEN 'task_stopped' ELSE 'intake_registered' END) AS reason_code,
 coalesce(v.historical_risk,false) OR coalesce(risk.unknown,false) AS historical_risk,
 coalesce(v.last_progress_at,d.created_at) AS last_progress_at
FROM receiving_dispatch_deliveries d JOIN receiving_dispatch_targets t ON t.id=d.dispatch_target_id
LEFT JOIN execution_devices device ON device.id=d.device_id
LEFT JOIN mobile_dispatch_intakes i ON i.delivery_id=d.id
LEFT JOIN mobile_dispatch_record_view v ON v.id=i.id
LEFT JOIN mobile_v9_dispatch_state ds ON ds.item_id=i.item_id
LEFT JOIN LATERAL (SELECT bool_or(a.state NOT IN ('verified_not_sent','failed_before_call','observed_sent','manually_confirmed_sent') AND (a.state='unknown' OR g.result_body->>'outcome'='unknown' OR g.result_id IS NULL AND g.id IS NOT NULL)) AS unknown
 FROM mobile_v9_attempts a LEFT JOIN mobile_execution_grants g ON g.attempt_id=a.id WHERE a.item_id=i.item_id) risk ON true;
CREATE VIEW receiving_target_results AS
SELECT t.id, t.message_id,t.channel,t.target_id,t.expected_count,t.materialized_count,t.stopped,
 max(d.last_progress_at) AS last_progress_at,bool_or(d.historical_risk) AS historical_risk,
 count(d.id) FILTER(WHERE d.outcome='sent') AS sent,
 count(d.id) FILTER(WHERE d.outcome='failed')+CASE WHEN t.stopped THEN t.expected_count-count(d.id) ELSE 0 END AS failed,
 count(d.id) FILTER(WHERE d.outcome='unknown') AS unknown,
 CASE WHEN t.stopped THEN 0 ELSE t.expected_count-count(d.id) END+count(d.id) FILTER(WHERE d.outcome='pending') AS pending,
 CASE WHEN t.expected_count=0 THEN 'failed'
 WHEN NOT t.stopped AND (t.materialized_count<t.expected_count OR count(d.id) FILTER(WHERE d.outcome='pending')>0) THEN 'pending'
 WHEN count(d.id) FILTER(WHERE d.outcome='sent')=t.expected_count THEN 'sent'
 WHEN count(d.id) FILTER(WHERE d.outcome='unknown')>0 THEN 'unknown'
 ELSE 'failed' END AS outcome
FROM receiving_dispatch_targets t LEFT JOIN receiving_delivery_progress d ON d.dispatch_target_id=t.id
WHERE t.channel='mobile' GROUP BY t.id;
CREATE OR REPLACE VIEW orchestration_delivery_progress AS
SELECT r.message_id,
r.channel,
r.target_id,
r.target_name,
r.delegate_id,
r.task_id,
r.delegate_state,
r.delegate_reason,
r.created_at,
r.process_retry_at,
r.process_error,
r.target,
r.item_id,
r.dispatch_state,
r.dispatch_reason,
r.next_retry_at,
r.attempt_id,
r.attempt_state,
r.attempt_no,
r.device_id,
r.device_presence,
r.device_enabled,
r.device_busy,
r.assigned_at,
r.accepted_at,
r.result_deadline_at,
r.grant_state,
r.mobile_stopped,
r.sent,
(r.historical_risk OR coalesce(parent.historical_risk,false)) AS historical_risk,
r.official_target_id,
r.official_status,
r.rejected,
r.batch_state,
r.stop_state,
r.error_code,
r.sync_error,
r.claimed_at,
r.last_synced_at,
r.next_sync_at,
r.sender,
greatest(r.last_progress_at,parent.last_progress_at) AS last_progress_at,
coalesce(parent.outcome,r.outcome) AS outcome,
CASE WHEN parent.outcome='pending' AND r.outcome<>'pending' THEN 'scheduling' WHEN parent.outcome='pending' THEN r.stage WHEN parent.outcome IS NOT NULL THEN parent.outcome ELSE r.stage END AS stage,
CASE WHEN parent.outcome='pending' AND r.outcome<>'pending' THEN 'awaiting_phone_responsibilities' WHEN parent.outcome IS NOT NULL AND parent.outcome<>r.outcome THEN parent.outcome ELSE r.reason_code END AS reason_code,
CASE WHEN parent.outcome='pending' AND r.outcome<>'pending' THEN 'normal' WHEN parent.outcome='unknown' THEN 'attention' WHEN parent.outcome IN ('sent','failed') THEN 'terminal' ELSE r.severity END AS severity FROM (
 SELECT DISTINCT ON(message_id,channel,target_id) * FROM receiving_task_progress
 ORDER BY message_id,channel,target_id,CASE delegate_state WHEN 'delegated' THEN 0 WHEN 'failed' THEN 1 ELSE 2 END,created_at,delegate_id
) r LEFT JOIN receiving_target_results parent ON parent.message_id=r.message_id AND parent.channel=r.channel AND parent.target_id=r.target_id;
