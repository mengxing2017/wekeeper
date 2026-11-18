-- Read-only analytics identities. No dispatch authorization or workflow is introduced.
CREATE TABLE statistics_query_snapshots (
 id uuid PRIMARY KEY,
 actor_id uuid NOT NULL REFERENCES accounts(id),
 filter jsonb NOT NULL,
 fingerprint text NOT NULL,
 payload jsonb NOT NULL,
 generated_at timestamptz NOT NULL,
 expires_at timestamptz NOT NULL
);
CREATE INDEX statistics_snapshot_expiry ON statistics_query_snapshots(expires_at);
CREATE INDEX statistics_receipt_time ON source_receipts(received_at,receipt_id);
CREATE INDEX statistics_receipt_unit_time ON source_receipts(business_unit_id,received_at,receipt_id);

CREATE VIEW statistics_targets AS
WITH automatic AS (
 SELECT coalesce(p.message_id,t.message_id) AS message_id,coalesce(p.channel,t.channel) AS channel,
 coalesce(p.target_id,t.target_id) AS target_id,coalesce(p.target_name,t.target->>'name') AS target_name,
 coalesce(p.task_id,(SELECT i.task_id FROM mobile_dispatch_intakes i WHERE i.delivery_id IN
 (SELECT id FROM receiving_dispatch_deliveries WHERE dispatch_target_id=t.id) ORDER BY i.id LIMIT 1)) AS task_id,
 coalesce(p.target,t.target) AS target,t.id AS dispatch_target_id,
 coalesce(rr.outcome,p.outcome,'pending') AS outcome,
 coalesce(p.reason_code,CASE WHEN t.stopped THEN 'task_stopped' ELSE 'creating_tasks' END) AS reason_code,
 coalesce(rr.historical_risk,p.historical_risk,false) AS historical_risk,
 coalesce(rr.expected_count,1)::bigint AS expected,
 coalesce(rr.sent,CASE WHEN p.outcome='sent' THEN 1 ELSE 0 END)::bigint AS sent,
 coalesce(rr.failed,CASE WHEN p.outcome='failed' THEN 1 ELSE 0 END)::bigint AS failed,
 coalesce(rr.unknown,CASE WHEN p.outcome='unknown' THEN 1 ELSE 0 END)::bigint AS unknown,
 coalesce(rr.pending,CASE WHEN coalesce(p.outcome,'pending')='pending' THEN 1 ELSE 0 END)::bigint AS pending
 FROM orchestration_delivery_progress p FULL JOIN receiving_dispatch_targets t
 ON (p.message_id,p.channel,p.target_id)=(t.message_id,t.channel,t.target_id)
 LEFT JOIN receiving_target_results rr ON rr.id=t.id
), manual_mobile AS (
 SELECT i.id,i.task_id,i.target_id,t.business_unit_id,t.created_at,
 coalesce(v.target_name,plan.plan->>'target_name',i.target_id::text) AS target_name,
 coalesce(plan.plan->>'client_kind','unclassified') AS client_kind,
 CASE WHEN history.sent THEN 'sent' WHEN (t.stopped OR ds.state IN ('stopped','confirmation_expired','dispatch_failed')) AND history.unresolved THEN 'unknown'
 WHEN t.stopped THEN 'failed' ELSE coalesce(v.outcome,CASE WHEN ds.state='completed' THEN 'sent' WHEN ds.state='needs_review' THEN 'unknown'
 WHEN ds.state IN ('stopped','confirmation_expired','dispatch_failed') THEN 'failed' ELSE 'pending' END) END AS outcome,
 coalesce(v.reason_code,ds.reason,'awaiting_dispatch') AS reason_code,coalesce(history.risk,false) AS historical_risk
 FROM mobile_v9_task_items i JOIN mobile_v9_tasks t ON t.id=i.task_id
 LEFT JOIN mobile_dispatch_record_view v ON v.item_id=i.id AND v.source_kind='manual'
 LEFT JOIN mobile_v9_dispatch_state ds ON ds.item_id=i.id
 LEFT JOIN LATERAL (SELECT p.plan FROM mobile_v9_plans p WHERE p.item_id=i.id ORDER BY p.id LIMIT 1) plan ON true
 LEFT JOIN LATERAL (SELECT bool_or(a.state IN ('observed_sent','manually_confirmed_sent') OR g.result_body->>'outcome'='observed_sent') AS sent,
 bool_or(NOT a.current AND (a.state='unknown' OR g.result_body->>'outcome'='unknown')) AS risk,
 bool_or(a.state='unknown' OR g.result_body->>'outcome'='unknown' OR (g.id IS NOT NULL AND g.result_id IS NULL AND a.state NOT IN ('verified_not_sent','failed_before_call','observed_sent','manually_confirmed_sent'))) AS unresolved
 FROM mobile_v9_attempts a LEFT JOIN mobile_execution_grants g ON g.attempt_id=a.id WHERE a.item_id=i.id) history ON true
 WHERE t.authorization_kind='manual' AND NOT EXISTS(SELECT 1 FROM mobile_dispatch_intakes x WHERE x.item_id=i.id AND x.source_kind='source_route')
), manual_official AS (
 SELECT x.id,x.task_id,x.group_id AS target_id,x.group_name AS target_name,t.confirmed_at AS created_at,
 CASE WHEN x.official_status=1 OR b.reconciliation='sent' THEN 'sent'
 WHEN b.state='unknown' OR b.stop_state='unknown' OR x.official_status NOT IN (0,1,2,3) THEN 'unknown'
 WHEN x.rejected OR x.official_status IN (2,3) OR b.state IN ('rejected','cancelled') OR b.stop_state='stopped' THEN 'failed' ELSE 'pending' END AS outcome,
 coalesce(b.error_code,CASE WHEN b.stop_state='stopped' THEN 'task_stopped' ELSE b.state END) AS reason_code
 FROM broadcast_targets x JOIN broadcast_tasks t ON t.id=x.task_id JOIN broadcast_batches b ON b.id=x.batch_id
 WHERE t.state='confirmed' AND t.authorization_kind='manual'
 AND NOT EXISTS(SELECT 1 FROM orchestration_delegates d WHERE d.task_id=t.id AND d.channel='wecom')
), facts AS (
 SELECT concat('source:',a.message_id,':',a.channel,':',a.target_id) AS id,'source_route'::text AS source_kind,
 r.receipt_id,a.message_id,r.source_id AS platform_id,r.business_unit_id,r.route_id,r.received_at AS cohort_at,
 a.channel,CASE WHEN a.channel='wecom' THEN 'wecom' ELSE coalesce(a.target->>'client_kind',a.target#>>'{identity,client_kind}','unclassified') END AS client_kind,
 a.target_id,a.target_name,a.task_id,a.dispatch_target_id,a.outcome,a.reason_code,a.historical_risk,a.expected,a.sent,a.failed,a.unknown,a.pending
 FROM automatic a JOIN orchestration_messages m ON m.id=a.message_id JOIN source_receipts r ON r.receipt_id=m.evidence_ref
 UNION ALL
 SELECT 'manual:mobile:'||m.id,'manual',NULL::uuid,NULL::uuid,u.platform_id,m.business_unit_id,NULL::text,m.created_at,
 'mobile',m.client_kind,m.target_id,m.target_name,m.task_id,NULL::uuid,m.outcome,m.reason_code,m.historical_risk,1,
 CASE WHEN m.outcome='sent' THEN 1 ELSE 0 END,CASE WHEN m.outcome='failed' THEN 1 ELSE 0 END,
 CASE WHEN m.outcome='unknown' THEN 1 ELSE 0 END,CASE WHEN m.outcome='pending' THEN 1 ELSE 0 END
 FROM manual_mobile m LEFT JOIN business_units u ON u.id=m.business_unit_id
 UNION ALL
 SELECT 'manual:wecom:'||m.id,'manual',NULL::uuid,NULL::uuid,NULL::uuid,NULL::uuid,NULL::text,m.created_at,
 'wecom','wecom',m.target_id,m.target_name,m.task_id,NULL::uuid,m.outcome,m.reason_code,false,1,
 CASE WHEN m.outcome='sent' THEN 1 ELSE 0 END,CASE WHEN m.outcome='failed' THEN 1 ELSE 0 END,
 CASE WHEN m.outcome='unknown' THEN 1 ELSE 0 END,CASE WHEN m.outcome='pending' THEN 1 ELSE 0 END FROM manual_official m
)
SELECT f.*,coalesce(u.name,CASE WHEN f.business_unit_id IS NULL THEN '未关联业务单位（手动任务）' ELSE f.business_unit_id::text END) AS unit_name,
 coalesce(c.unit_name,f.platform_id::text,'未关联来源平台') AS platform_name
FROM facts f LEFT JOIN business_units u ON u.id=f.business_unit_id LEFT JOIN source_credentials c ON c.source_id=f.platform_id;

CREATE VIEW statistics_messages AS
SELECT r.receipt_id AS id,r.event_id,r.source_id AS platform_id,r.business_unit_id,r.route_id,r.received_at AS cohort_at,
 coalesce(u.name,r.business_unit_id::text) AS unit_name,coalesce(c.unit_name,r.source_id::text) AS platform_name,
 coalesce(d.reason_code,r.reason_code) AS reason_code,
 coalesce(d.state,'handoff') AS processing_state,
 (coalesce(d.state,'pending') IN ('failed','historical') OR r.processing_stage='not_dispatched') AS processing_terminal,
 (r.processing_stage='not_dispatched' OR coalesce(d.state,'pending') IN ('delegated','failed','historical')) AS scope_complete,
 EXISTS(SELECT 1 FROM orchestration_delegates x WHERE x.message_id=m.id AND x.state='failed') AS channel_failure,
 coalesce(m.id,h.message_id) AS message_id
FROM source_receipts r LEFT JOIN business_units u ON u.id=r.business_unit_id
LEFT JOIN source_credentials c ON c.source_id=r.source_id
LEFT JOIN orchestration_messages m ON m.evidence_ref=r.receipt_id AND m.platform_id=r.source_id
LEFT JOIN source_message_handoffs h ON h.receipt_id=r.receipt_id
LEFT JOIN orchestration_dispatches d ON d.message_id=m.id;
