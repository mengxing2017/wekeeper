-- Existing tasks keep receiving result projections after the worker cutover.
-- This creates evidence links only; it never creates or resubmits channel tasks.
INSERT INTO orchestration_delegates(id,message_id,configuration_id,channel,storage_scope,task_id,snapshot_id,plan,executed_by)
SELECT gen_random_uuid(),m.id,gen_random_uuid(),t.channel,t.workspace_id,t.task_id,
 COALESCE(c.message_snapshot_id,
   CASE WHEN t.channel='mobile' THEN (SELECT p.snapshot_id FROM mobile_v9_task_items i JOIN mobile_v9_plans p ON p.item_id=i.id WHERE i.task_id=t.task_id ORDER BY i.position LIMIT 1)
        ELSE (SELECT s.snapshot_id FROM broadcast_task_snapshots s WHERE s.task_id=t.task_id ORDER BY s.task_revision DESC LIMIT 1) END),
 jsonb_build_object('targets',CASE WHEN t.channel='mobile' THEN
   COALESCE((SELECT jsonb_agg(jsonb_build_object('id',i.target_id)) FROM mobile_v9_task_items i WHERE i.task_id=t.task_id),'[]')
 ELSE COALESCE((SELECT jsonb_agg(jsonb_build_object('id',x.group_id)) FROM broadcast_targets x WHERE x.task_id=t.task_id),'[]') END,
 'historical',true,'authorized_by',t.authorization_actor_id),t.execution_actor
FROM source_execution_tasks t JOIN orchestration_messages m ON m.evidence_ref=t.receipt_id
LEFT JOIN source_message_compositions c ON c.id=t.composition_id
WHERE NOT EXISTS(SELECT 1 FROM orchestration_delegates d WHERE d.task_id=t.task_id)
AND (c.message_snapshot_id IS NOT NULL OR
 CASE WHEN t.channel='mobile' THEN EXISTS(SELECT 1 FROM mobile_v9_task_items i JOIN mobile_v9_plans p ON p.item_id=i.id WHERE i.task_id=t.task_id)
 ELSE EXISTS(SELECT 1 FROM broadcast_task_snapshots s WHERE s.task_id=t.task_id) END);
