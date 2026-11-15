-- Global administrator authorization replaces artificial workspace partitions.
-- Unreleased data is deliberately discarded; no compatibility/default space remains.

DROP FUNCTION distribution_snapshot_workspace_scope() CASCADE;

DROP VIEW distribution_forum_materials;

TRUNCATE TABLE mobile_v9_send_policy_versions, audit_events, broadcast_batches, broadcast_content_references, broadcast_draft_requests, broadcast_events, broadcast_operations, broadcast_preview_intents, broadcast_targets, broadcast_task_posts, broadcast_task_snapshots, broadcast_tasks, channel_execution_observations, distribution_materials, distribution_message_snapshots, distribution_source_organizations, media_access_tokens, media_assets, mobile_device_target_bindings, mobile_targets, mobile_usage_previews, mobile_usages, mobile_v9_attempts, mobile_v9_invocations, mobile_v9_plans, mobile_v9_previews, mobile_v9_reassignments, mobile_v9_reviews, mobile_v9_risk_guards, mobile_v9_scope_grants, mobile_v9_send_events, mobile_v9_send_inspections, mobile_v9_support_requests, mobile_v9_task_drafts, mobile_v9_task_items, mobile_v9_tasks, orchestration_delegates, product_previews, product_requests, shared_mobile_resource_events, template_versions, templates, wecom_groups, wecom_sender_qualifications CASCADE;

ALTER TABLE audit_events DROP COLUMN school_id CASCADE;

ALTER TABLE broadcast_batches DROP COLUMN school_id CASCADE;

ALTER TABLE broadcast_content_references DROP COLUMN school_id CASCADE;

ALTER TABLE broadcast_draft_requests DROP COLUMN school_id CASCADE;

ALTER TABLE broadcast_events DROP COLUMN school_id CASCADE;

ALTER TABLE broadcast_operations DROP COLUMN school_id CASCADE;

ALTER TABLE broadcast_preview_intents DROP COLUMN school_id CASCADE;

ALTER TABLE broadcast_targets DROP COLUMN school_id CASCADE;

ALTER TABLE broadcast_task_posts DROP COLUMN school_id CASCADE;

ALTER TABLE broadcast_task_snapshots DROP COLUMN school_id CASCADE;

ALTER TABLE broadcast_tasks DROP COLUMN school_id CASCADE;

ALTER TABLE channel_execution_observations DROP COLUMN workspace_id CASCADE;

ALTER TABLE distribution_materials DROP COLUMN school_id CASCADE;

ALTER TABLE distribution_message_snapshots DROP COLUMN school_id CASCADE;

ALTER TABLE distribution_source_organizations DROP COLUMN school_id CASCADE;

ALTER TABLE media_access_tokens DROP COLUMN school_id CASCADE;

ALTER TABLE media_assets DROP COLUMN school_id CASCADE;

ALTER TABLE mobile_device_target_bindings DROP COLUMN school_id CASCADE;
ALTER TABLE mobile_device_target_bindings DROP COLUMN source;

ALTER TABLE mobile_targets DROP COLUMN school_id CASCADE;

ALTER TABLE mobile_usage_previews DROP COLUMN school_id CASCADE;

ALTER TABLE mobile_usages DROP COLUMN school_id CASCADE;

ALTER TABLE mobile_v9_attempts DROP COLUMN school_id CASCADE;

ALTER TABLE mobile_v9_invocations DROP COLUMN school_id CASCADE;

ALTER TABLE mobile_v9_plans DROP COLUMN school_id CASCADE;

ALTER TABLE mobile_v9_previews DROP COLUMN school_id CASCADE;

ALTER TABLE mobile_v9_reassignments DROP COLUMN school_id CASCADE;

ALTER TABLE mobile_v9_reviews DROP COLUMN school_id CASCADE;

ALTER TABLE mobile_v9_risk_guards DROP COLUMN school_id CASCADE;

ALTER TABLE mobile_v9_scope_grants DROP COLUMN school_id CASCADE;

ALTER TABLE mobile_v9_send_events DROP COLUMN school_id CASCADE;

ALTER TABLE mobile_v9_send_inspections DROP COLUMN school_id CASCADE;

ALTER TABLE mobile_v9_support_requests DROP COLUMN school_id CASCADE;

ALTER TABLE mobile_v9_task_drafts DROP COLUMN school_id CASCADE;

ALTER TABLE mobile_v9_task_items DROP COLUMN school_id CASCADE;

ALTER TABLE mobile_v9_tasks DROP COLUMN school_id CASCADE;

ALTER TABLE orchestration_delegates DROP COLUMN storage_scope CASCADE;

ALTER TABLE product_previews DROP COLUMN school_id CASCADE;

ALTER TABLE product_requests DROP COLUMN school_id CASCADE;

ALTER TABLE shared_mobile_resource_events DROP COLUMN school_id CASCADE;

ALTER TABLE template_versions DROP COLUMN school_id CASCADE;

ALTER TABLE templates DROP COLUMN school_id CASCADE;

ALTER TABLE wecom_groups DROP COLUMN school_id CASCADE;

ALTER TABLE wecom_sender_qualifications DROP COLUMN school_id CASCADE;

DROP TABLE wecom_workspace_members, mobile_workspace_members, unit_receiving_storage, mobile_shared_access_events, mobile_shared_school_access, wecom_workspaces, mobile_workspaces CASCADE;

-- Rebuild standalone indexes as well as constraints dropped with partition columns.
CREATE INDEX audit_created_page ON audit_events(created_at DESC,id DESC);
CREATE INDEX broadcast_task_created_page ON broadcast_tasks(created_at DESC,id);
CREATE INDEX channel_observation_task_page ON channel_execution_observations(channel,task_id,id);
CREATE INDEX mobile_targets_page ON mobile_targets(created_at,id) WHERE archived_at IS NULL;
CREATE INDEX mobile_usages_created_page ON mobile_usages(created_at DESC);
CREATE UNIQUE INDEX product_requests_open ON product_requests(kind,context_key) WHERE state IN ('pending','investigating','waiting_user');

ALTER TABLE broadcast_batches ADD CONSTRAINT broadcast_batches_task_id_id_key UNIQUE (task_id, id);

ALTER TABLE broadcast_draft_requests ADD CONSTRAINT broadcast_draft_requests_pkey PRIMARY KEY (source_namespace, actor_id, request_key);

ALTER TABLE broadcast_operations ADD CONSTRAINT broadcast_operations_actor_id_operation_idempoten_key UNIQUE (actor_id, operation, idempotency_key);

ALTER TABLE broadcast_preview_intents ADD CONSTRAINT broadcast_preview_intents_pkey PRIMARY KEY (actor_id, request_key);

ALTER TABLE broadcast_tasks ADD CONSTRAINT broadcast_tasks_id_key UNIQUE (id);

ALTER TABLE distribution_message_snapshots ADD CONSTRAINT distribution_message_snapshots_id_key UNIQUE (id);

ALTER TABLE distribution_source_organizations ADD CONSTRAINT distribution_source_organizat_namespace_external_organizati_key UNIQUE (namespace, external_organization_id);

ALTER TABLE mobile_device_target_bindings ADD CONSTRAINT mobile_binding_scope_identity UNIQUE (device_id, target_id, id);

ALTER TABLE mobile_device_target_bindings ADD CONSTRAINT mobile_device_target_bindings_device_id_target_id_key UNIQUE (device_id, target_id);

ALTER TABLE mobile_device_target_bindings ADD CONSTRAINT mobile_device_target_bindings_id_key UNIQUE (id);

ALTER TABLE mobile_targets ADD CONSTRAINT mobile_targets_id_key UNIQUE (id);

ALTER TABLE mobile_v9_attempts ADD CONSTRAINT mobile_v9_attempts_id_key UNIQUE (id);

ALTER TABLE mobile_v9_attempts ADD CONSTRAINT mobile_v9_attempts_item_id_id_key UNIQUE (item_id, id);

ALTER TABLE mobile_v9_invocations ADD CONSTRAINT mobile_v9_invocations_id_key UNIQUE (id);

ALTER TABLE mobile_v9_plans ADD CONSTRAINT mobile_v9_plans_id_key UNIQUE (id);

ALTER TABLE mobile_v9_plans ADD CONSTRAINT mobile_v9_plans_item_id_id_key UNIQUE (item_id, id);

ALTER TABLE mobile_v9_previews ADD CONSTRAINT mobile_v9_previews_id_key UNIQUE (id);

ALTER TABLE mobile_v9_risk_guards ADD CONSTRAINT mobile_v9_risk_guards_pkey PRIMARY KEY (intent_id, target_id);

ALTER TABLE mobile_v9_task_drafts ADD CONSTRAINT mobile_v9_task_drafts_id_key UNIQUE (id);

ALTER TABLE mobile_v9_task_items ADD CONSTRAINT mobile_v9_task_items_id_key UNIQUE (id);

ALTER TABLE mobile_v9_task_items ADD CONSTRAINT mobile_v9_task_items_id_target_id_key UNIQUE (id, target_id);

ALTER TABLE mobile_v9_task_items ADD CONSTRAINT mobile_v9_task_items_intent_id_target_id_key UNIQUE (intent_id, target_id);

ALTER TABLE mobile_v9_tasks ADD CONSTRAINT mobile_v9_tasks_id_key UNIQUE (id);

ALTER TABLE template_versions ADD CONSTRAINT template_versions_id_key UNIQUE (id);

ALTER TABLE templates ADD CONSTRAINT templates_id_key UNIQUE (id);

ALTER TABLE wecom_groups ADD CONSTRAINT wecom_groups_id_key UNIQUE (id);

ALTER TABLE wecom_sender_qualifications ADD CONSTRAINT wecom_sender_qualifications_pkey PRIMARY KEY (account_id);

ALTER TABLE broadcast_batches ADD CONSTRAINT broadcast_batches_task_id_fkey FOREIGN KEY (task_id) REFERENCES broadcast_tasks(id);

ALTER TABLE broadcast_content_references ADD CONSTRAINT broadcast_content_references_task_id_fkey FOREIGN KEY (task_id) REFERENCES broadcast_tasks(id);

ALTER TABLE broadcast_draft_requests ADD CONSTRAINT broadcast_draft_requests_task_id_fkey FOREIGN KEY (task_id) REFERENCES broadcast_tasks(id);

ALTER TABLE broadcast_events ADD CONSTRAINT broadcast_events_task_id_fkey FOREIGN KEY (task_id) REFERENCES broadcast_tasks(id);

ALTER TABLE broadcast_operations ADD CONSTRAINT broadcast_operations_task_id_batch_id_fkey FOREIGN KEY (task_id, batch_id) REFERENCES broadcast_batches(task_id, id);

ALTER TABLE broadcast_operations ADD CONSTRAINT broadcast_operations_task_id_fkey FOREIGN KEY (task_id) REFERENCES broadcast_tasks(id);

ALTER TABLE broadcast_targets ADD CONSTRAINT broadcast_targets_group_id_fkey FOREIGN KEY (group_id) REFERENCES wecom_groups(id);

ALTER TABLE broadcast_targets ADD CONSTRAINT broadcast_targets_task_id_batch_id_fkey FOREIGN KEY (task_id, batch_id) REFERENCES broadcast_batches(task_id, id);

ALTER TABLE broadcast_task_posts ADD CONSTRAINT broadcast_task_posts_task_id_fkey FOREIGN KEY (task_id) REFERENCES broadcast_tasks(id);

ALTER TABLE broadcast_task_snapshots ADD CONSTRAINT broadcast_task_snapshots_snapshot_id_fkey FOREIGN KEY (snapshot_id) REFERENCES distribution_message_snapshots(id);

ALTER TABLE broadcast_task_snapshots ADD CONSTRAINT broadcast_task_snapshots_task_id_fkey FOREIGN KEY (task_id) REFERENCES broadcast_tasks(id);

ALTER TABLE broadcast_tasks ADD CONSTRAINT broadcast_tasks_source_task_id_fkey FOREIGN KEY (source_task_id) REFERENCES broadcast_tasks(id);

ALTER TABLE broadcast_tasks ADD CONSTRAINT broadcast_tasks_template_version_id_fkey FOREIGN KEY (template_version_id) REFERENCES template_versions(id);

ALTER TABLE distribution_materials ADD CONSTRAINT distribution_materials_namespace_external_organization_id__fkey FOREIGN KEY (namespace, external_organization_id) REFERENCES distribution_source_organizations(namespace, external_organization_id);

ALTER TABLE mobile_device_target_bindings ADD CONSTRAINT mobile_device_target_bindings_target_id_fkey FOREIGN KEY (target_id) REFERENCES mobile_targets(id);

ALTER TABLE mobile_v9_attempts ADD CONSTRAINT mobile_v9_attempts_item_id_plan_id_fkey FOREIGN KEY (item_id, plan_id) REFERENCES mobile_v9_plans(item_id, id);

ALTER TABLE mobile_v9_invocations ADD CONSTRAINT mobile_v9_invocations_attempt_id_fkey FOREIGN KEY (attempt_id) REFERENCES mobile_v9_attempts(id);

ALTER TABLE mobile_v9_plans ADD CONSTRAINT mobile_v9_plans_item_id_target_id_fkey FOREIGN KEY (item_id, target_id) REFERENCES mobile_v9_task_items(id, target_id);

ALTER TABLE mobile_v9_plans ADD CONSTRAINT mobile_v9_plans_snapshot_id_fkey FOREIGN KEY (snapshot_id) REFERENCES distribution_message_snapshots(id);

ALTER TABLE mobile_v9_previews ADD CONSTRAINT mobile_v9_previews_draft_id_fkey FOREIGN KEY (draft_id) REFERENCES mobile_v9_task_drafts(id);

ALTER TABLE mobile_v9_reassignments ADD CONSTRAINT mobile_v9_reassignments_new_attempt_id_fkey FOREIGN KEY (new_attempt_id) REFERENCES mobile_v9_attempts(id);

ALTER TABLE mobile_v9_reassignments ADD CONSTRAINT mobile_v9_reassignments_old_attempt_id_fkey FOREIGN KEY (old_attempt_id) REFERENCES mobile_v9_attempts(id);

ALTER TABLE mobile_v9_reviews ADD CONSTRAINT mobile_v9_reviews_attempt_id_fkey FOREIGN KEY (attempt_id) REFERENCES mobile_v9_attempts(id);

ALTER TABLE mobile_v9_risk_guards ADD CONSTRAINT mobile_v9_risk_guards_attempt_id_fkey FOREIGN KEY (attempt_id) REFERENCES mobile_v9_attempts(id);

ALTER TABLE mobile_v9_risk_guards ADD CONSTRAINT mobile_v9_risk_guards_invocation_id_fkey FOREIGN KEY (invocation_id) REFERENCES mobile_v9_invocations(id);

ALTER TABLE mobile_v9_send_events ADD CONSTRAINT mobile_v9_send_events_attempt_id_fkey FOREIGN KEY (attempt_id) REFERENCES mobile_v9_attempts(id);

ALTER TABLE mobile_v9_send_events ADD CONSTRAINT mobile_v9_send_events_invocation_id_fkey FOREIGN KEY (invocation_id) REFERENCES mobile_v9_invocations(id);

ALTER TABLE mobile_v9_task_items ADD CONSTRAINT mobile_v9_task_items_target_id_fkey FOREIGN KEY (target_id) REFERENCES mobile_targets(id);

ALTER TABLE mobile_v9_task_items ADD CONSTRAINT mobile_v9_task_items_task_id_fkey FOREIGN KEY (task_id) REFERENCES mobile_v9_tasks(id);

ALTER TABLE mobile_v9_tasks ADD CONSTRAINT mobile_v9_tasks_draft_id_fkey FOREIGN KEY (draft_id) REFERENCES mobile_v9_task_drafts(id);

ALTER TABLE mobile_v9_tasks ADD CONSTRAINT mobile_v9_tasks_preview_id_fkey FOREIGN KEY (preview_id) REFERENCES mobile_v9_previews(id);

ALTER TABLE template_versions ADD CONSTRAINT template_versions_template_id_fkey FOREIGN KEY (template_id) REFERENCES templates(id);

ALTER TABLE templates ADD CONSTRAINT templates_current_version_id_fkey FOREIGN KEY (current_version_id) REFERENCES template_versions(id);

CREATE OR REPLACE FUNCTION channel_batch_observation()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
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
END $function$
;

CREATE OR REPLACE FUNCTION channel_mobile_observation()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
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
END $function$
;

CREATE OR REPLACE FUNCTION channel_mobile_review_observation()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
BEGIN
 INSERT INTO channel_execution_observations(channel,task_id,target_id,attempt_id,evidence_type,observation_source,normalized_result,detail)
 SELECT 'mobile',i.task_id,i.target_id,a.id,'operator_reconciled','operator',CASE WHEN NEW.decision='manually_confirmed_sent' THEN 'sent' WHEN NEW.decision='verified_not_sent' THEN 'failed' ELSE 'unknown' END,jsonb_build_object('review_id',NEW.id,'decision',NEW.decision,'actor_id',NEW.reviewer)
  FROM mobile_v9_attempts a JOIN mobile_v9_task_items i ON i.id=a.item_id WHERE a.id=NEW.attempt_id;
 RETURN NEW;
END $function$
;

CREATE OR REPLACE FUNCTION channel_target_observation()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
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
END $function$
;

CREATE OR REPLACE FUNCTION channel_task_accepted()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
BEGIN
 INSERT INTO channel_execution_observations(channel,task_id,evidence_type,observation_source,normalized_result)
 VALUES(CASE WHEN TG_TABLE_NAME='broadcast_tasks' THEN 'wecom' ELSE 'mobile' END,NEW.id,'platform_accepted','platform','pending');
 RETURN NEW;
END $function$
;

CREATE OR REPLACE FUNCTION mobile_v9_scope_guard()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
BEGIN
 PERFORM pg_advisory_xact_lock(78234610243);
 IF NEW.acceptance_id IS NOT NULL AND NOT EXISTS(
  SELECT 1 FROM mobile_v9_acceptance_runs a
  WHERE a.id=NEW.acceptance_id AND a.release_id=NEW.release_id AND a.stage=NEW.stage
 ) THEN RAISE EXCEPTION 'v9 acceptance scope mismatch'; END IF;
 IF cardinality(NEW.device_ids)=0 OR EXISTS(
  SELECT 1 FROM unnest(NEW.device_ids) d
  WHERE NOT EXISTS(SELECT 1 FROM execution_devices WHERE id=d)
 ) OR EXISTS(
  SELECT 1 FROM unnest(NEW.target_ids) t
  WHERE NOT EXISTS(SELECT 1 FROM mobile_targets WHERE id=t)
 ) THEN RAISE EXCEPTION 'v9 resource scope mismatch'; END IF;
 RETURN NEW;
END $function$
;

CREATE VIEW distribution_forum_materials AS
 SELECT external_content_id::uuid AS id,external_organization_id,title,content,version::bigint AS revision,
 CASE WHEN allowed THEN 'approved' ELSE 'rejected' END AS audit_status,
 CASE WHEN allowed THEN 'visible' ELSE 'hidden' END AS visibility
 FROM distribution_materials WHERE namespace='builtin.forum';
