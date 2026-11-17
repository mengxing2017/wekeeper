-- 跨领域外键、触发器及查询投影；依赖前述所有领域表。
-- 空库初始化基线；后续结构变更使用新增递增迁移。

-- fk constraint
ALTER TABLE ONLY accounts
    ADD CONSTRAINT accounts_wecom_member_id_fkey FOREIGN KEY (wecom_member_id) REFERENCES wecom_members(id);

ALTER TABLE ONLY broadcast_draft_requests
    ADD CONSTRAINT broadcast_draft_requests_source_namespace_fkey FOREIGN KEY (source_namespace) REFERENCES distribution_sources(namespace);

ALTER TABLE ONLY broadcast_task_snapshots
    ADD CONSTRAINT broadcast_task_snapshots_snapshot_id_fkey FOREIGN KEY (snapshot_id) REFERENCES distribution_message_snapshots(id);

ALTER TABLE ONLY broadcast_tasks
    ADD CONSTRAINT broadcast_tasks_source_namespace_fkey FOREIGN KEY (source_namespace) REFERENCES distribution_sources(namespace);

ALTER TABLE ONLY mobile_delivery_exchanges
    ADD CONSTRAINT mobile_delivery_exchanges_intake_id_fkey FOREIGN KEY (intake_id) REFERENCES mobile_dispatch_intakes(id);

ALTER TABLE ONLY mobile_usage_invocation_budgets
    ADD CONSTRAINT mobile_usage_invocation_budgets_invocation_id_fkey FOREIGN KEY (invocation_id) REFERENCES mobile_v9_invocations(id);

ALTER TABLE ONLY mobile_v9_dispatch_events
    ADD CONSTRAINT mobile_v9_dispatch_events_delivery_request_id_fkey FOREIGN KEY (delivery_request_id) REFERENCES mobile_delivery_exchanges(id);

ALTER TABLE ONLY mobile_v9_dispatch_events
    ADD CONSTRAINT mobile_v9_dispatch_events_intake_id_fkey FOREIGN KEY (intake_id) REFERENCES mobile_dispatch_intakes(id);

ALTER TABLE ONLY mobile_v9_dispatch_events
    ADD CONSTRAINT mobile_v9_dispatch_events_operation_id_fkey FOREIGN KEY (operation_id) REFERENCES mobile_dispatch_operations(id);

ALTER TABLE ONLY mobile_v9_plans
    ADD CONSTRAINT mobile_v9_plans_snapshot_id_fkey FOREIGN KEY (snapshot_id) REFERENCES distribution_message_snapshots(id);

ALTER TABLE ONLY mobile_v9_support_requests
    ADD CONSTRAINT mobile_v9_support_requests_task_id_fkey FOREIGN KEY (task_id) REFERENCES mobile_v9_tasks(id);

-- trigger
CREATE TRIGGER ad_message_decision_immutable BEFORE DELETE OR UPDATE ON ad_message_decisions FOR EACH ROW EXECUTE FUNCTION orchestration_message_immutable();

CREATE TRIGGER channel_batch_observation AFTER UPDATE ON broadcast_batches FOR EACH ROW EXECUTE FUNCTION channel_batch_observation();

CREATE TRIGGER channel_mobile_accepted AFTER INSERT ON mobile_v9_tasks FOR EACH ROW EXECUTE FUNCTION channel_task_accepted();

CREATE TRIGGER channel_mobile_observation AFTER UPDATE ON mobile_execution_grants FOR EACH ROW EXECUTE FUNCTION channel_mobile_observation();

CREATE TRIGGER channel_mobile_review_observation AFTER INSERT ON mobile_v9_reviews FOR EACH ROW EXECUTE FUNCTION channel_mobile_review_observation();

CREATE TRIGGER channel_target_observation AFTER UPDATE ON broadcast_targets FOR EACH ROW EXECUTE FUNCTION channel_target_observation();

CREATE TRIGGER channel_wecom_accepted AFTER INSERT ON broadcast_tasks FOR EACH ROW EXECUTE FUNCTION channel_task_accepted();

CREATE TRIGGER message_attempt_progress BEFORE UPDATE ON mobile_v9_attempts FOR EACH ROW EXECUTE FUNCTION message_progress_clock();

CREATE TRIGGER message_batch_progress BEFORE UPDATE ON broadcast_batches FOR EACH ROW EXECUTE FUNCTION message_progress_clock();

CREATE TRIGGER message_grant_progress AFTER INSERT OR UPDATE ON mobile_execution_grants FOR EACH ROW EXECUTE FUNCTION message_grant_progress_changed();

CREATE TRIGGER message_mobile_dispatch AFTER INSERT OR UPDATE ON mobile_v9_dispatch_state FOR EACH ROW EXECUTE FUNCTION orchestration_result_changed();

CREATE TRIGGER message_mobile_progress BEFORE UPDATE ON mobile_v9_dispatch_state FOR EACH ROW EXECUTE FUNCTION message_progress_clock();

CREATE TRIGGER message_mobile_result AFTER INSERT OR UPDATE ON mobile_v9_attempts FOR EACH ROW EXECUTE FUNCTION orchestration_result_changed();

CREATE TRIGGER message_mobile_task AFTER UPDATE ON mobile_v9_tasks FOR EACH ROW EXECUTE FUNCTION orchestration_result_changed();

CREATE TRIGGER message_target_progress BEFORE UPDATE ON broadcast_targets FOR EACH ROW EXECUTE FUNCTION message_progress_clock();

CREATE TRIGGER message_wecom_batch AFTER UPDATE ON broadcast_batches FOR EACH ROW EXECUTE FUNCTION orchestration_result_changed();

CREATE TRIGGER message_wecom_result AFTER UPDATE ON broadcast_targets FOR EACH ROW EXECUTE FUNCTION orchestration_result_changed();

CREATE TRIGGER message_wecom_task AFTER UPDATE ON broadcast_tasks FOR EACH ROW EXECUTE FUNCTION orchestration_result_changed();

CREATE TRIGGER mobile_event_context BEFORE INSERT ON mobile_v9_dispatch_events FOR EACH ROW EXECUTE FUNCTION mobile_record_event_context();

CREATE TRIGGER mobile_record_attempt AFTER UPDATE ON mobile_v9_attempts FOR EACH ROW EXECUTE FUNCTION mobile_record_attempt();

CREATE TRIGGER mobile_record_dispatch AFTER UPDATE ON mobile_v9_dispatch_state FOR EACH ROW EXECUTE FUNCTION mobile_record_dispatch();

CREATE TRIGGER mobile_record_grant AFTER INSERT OR UPDATE ON mobile_execution_grants FOR EACH ROW EXECUTE FUNCTION mobile_record_grant();

CREATE TRIGGER mobile_record_reassignment AFTER INSERT ON mobile_v9_reassignments FOR EACH ROW EXECUTE FUNCTION mobile_record_reassignment();

CREATE TRIGGER mobile_record_review AFTER INSERT ON mobile_v9_reviews FOR EACH ROW EXECUTE FUNCTION mobile_record_review();

CREATE TRIGGER mobile_record_stop AFTER UPDATE ON mobile_v9_tasks FOR EACH ROW EXECUTE FUNCTION mobile_record_stop();

CREATE TRIGGER mobile_v9_scope_guard BEFORE INSERT ON mobile_v9_scope_grants FOR EACH ROW EXECUTE FUNCTION mobile_v9_scope_guard();

CREATE TRIGGER source_credential_identity BEFORE UPDATE ON source_credentials FOR EACH ROW EXECUTE FUNCTION source_identity_immutable();

-- view
CREATE VIEW device_control_presence AS
 SELECT d.id,
        CASE
            WHEN (NOT d.enabled) THEN 'disabled'::text
            WHEN (NOT (EXISTS ( SELECT 1
               FROM device_credentials c
              WHERE ((c.device_id = d.id) AND (c.generation = d.credential_generation) AND (c.revoked_at IS NULL) AND (c.expires_at > clock_timestamp()))))) THEN 'unauthorized'::text
            WHEN ((s.device_id IS NULL) OR (s.credential_generation IS DISTINCT FROM d.credential_generation)) THEN 'offline'::text
            WHEN (COALESCE(s.last_heartbeat_at, s.presence_started_at) <= (clock_timestamp() - make_interval(secs => (p.offline_after_seconds)::double precision))) THEN 'offline'::text
            WHEN (s.last_heartbeat_at IS NULL) THEN 'connecting'::text
            WHEN (s.last_heartbeat_at <= (clock_timestamp() - make_interval(secs => (p.suspect_after_seconds)::double precision))) THEN 'suspect'::text
            ELSE 'online'::text
        END AS state,
    s.last_heartbeat_at AS last_control_heartbeat_at,
    s.transport,
    s.readiness,
    s.block_reason,
    COALESCE(s.desired_paused, false) AS desired_paused,
    COALESCE(s.desired_revision, (0)::bigint) AS desired_revision,
    s.connected_at,
    s.last_disconnected_at,
    s.disconnect_reason,
    p.heartbeat_interval_seconds,
    p.suspect_after_seconds,
    p.offline_after_seconds,
    clock_timestamp() AS server_time,
    ( SELECT max(c.expires_at) AS max
           FROM device_credentials c
          WHERE ((c.device_id = d.id) AND (c.generation = d.credential_generation) AND (c.revoked_at IS NULL) AND (c.expires_at > clock_timestamp()))) AS credential_expires_at,
    COALESCE(s.connected, false) AS transport_connected
   FROM ((execution_devices d
     LEFT JOIN device_control_sessions s ON ((s.device_id = d.id)))
     CROSS JOIN device_control_policy p);

CREATE VIEW mobile_delivery_exchange_view AS
 SELECT x.id,
    x.device_id,
    x.generation,
    x.key_digest,
    x.replay_of,
    x.intake_id,
    x.attempt_id,
    x.execution_id,
    x.result,
    x.reason_code,
    x.response_state,
    x.created_at,
    x.finished_at,
    x.response_at,
    x.transport,
    x.connection_id,
    x.connection_epoch,
    g.accepted_at,
    g.receipt_deadline_at,
    g.result_deadline_at,
    g.result_received_at,
    g.superseded_reason,
        CASE
            WHEN (g.result_id IS NOT NULL) THEN 'result_recorded'::text
            WHEN (g.superseded_reason = ANY (ARRAY['receipt_timeout'::text, 'result_timeout'::text])) THEN g.superseded_reason
            WHEN (g.accepted_at IS NOT NULL) THEN 'awaiting_result'::text
            WHEN (g.id IS NOT NULL) THEN 'awaiting_receipt'::text
            ELSE 'delivery_failed'::text
        END AS reception_state
   FROM (mobile_delivery_exchanges x
     LEFT JOIN mobile_execution_grants g ON ((g.id = x.execution_id)));

CREATE VIEW orchestration_delivery_progress AS
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
         SELECT DISTINCT ON (planned.message_id, planned.channel, (planned.target ->> 'id'::text)) planned.message_id,
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
          ORDER BY planned.message_id, planned.channel, (planned.target ->> 'id'::text),
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

CREATE VIEW mobile_dispatch_record_view AS
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
     LEFT JOIN orchestration_delivery_progress v ON (((v.message_id = i.message_id) AND (v.channel = 'mobile'::text) AND (v.target_id = i.target_id))));

CREATE VIEW wecom_group_directory AS
 SELECT id,
    kind,
    corp_id,
    key,
    parent_key,
    member_key,
    member_type,
    data,
    last_seen_run_id,
    checked_at,
    credential_generation,
    visibility,
    verification,
    source
   FROM wecom_asset_objects
  WHERE (kind = 'groups'::text);

CREATE VIEW wecom_group_directory_members AS
 SELECT o.id,
    o.kind,
    o.corp_id,
    o.key,
    o.parent_key,
    o.member_key,
    o.member_type,
    o.data,
    o.last_seen_run_id,
    o.checked_at,
    o.credential_generation,
    o.visibility,
    o.verification,
    o.source,
    g.id AS directory_group_id,
    m.id AS member_id
   FROM ((wecom_asset_objects o
     LEFT JOIN wecom_asset_objects g ON (((g.kind = 'groups'::text) AND (g.corp_id = o.corp_id) AND (g.key = o.parent_key))))
     LEFT JOIN wecom_members m ON (((o.member_type = 1) AND (m.userid_key = o.member_key))))
  WHERE (o.kind = 'members'::text);

