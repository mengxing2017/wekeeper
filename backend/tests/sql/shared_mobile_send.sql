-- Isolated, rolled-back checks. Never run with production credentials.
BEGIN;
PREPARE active_shared_target(uuid,uuid) AS SELECT EXISTS(
 SELECT 1 FROM mobile_shared_task_items i JOIN mobile_shared_attempts a ON a.item_id=i.id
 WHERE i.school_id=$1 AND i.target_id=$2 AND a.current
 AND a.state IN ('queued','leased','prepared','invocation_issued','unknown')
);
DEALLOCATE active_shared_target;
DO $$
DECLARE s uuid=gen_random_uuid(); other uuid=gen_random_uuid(); actor uuid=gen_random_uuid();
 draft uuid=gen_random_uuid(); preview uuid=gen_random_uuid(); intent uuid=gen_random_uuid(); task uuid=gen_random_uuid(); policy uuid=gen_random_uuid();
 device uuid=gen_random_uuid(); environment uuid=gen_random_uuid(); release uuid=gen_random_uuid(); target uuid=gen_random_uuid(); acceptance uuid=gen_random_uuid();
BEGIN
 IF to_regclass('mobile_execution_tasks') IS NOT NULL OR to_regclass('mobile_execution_task_items') IS NOT NULL OR to_regclass('mobile_execution_plans') IS NOT NULL OR to_regclass('mobile_execution_task_previews') IS NOT NULL OR to_regclass('mobile_execution_device_reservations') IS NOT NULL THEN RAISE EXCEPTION 'retired phone task tables remain'; END IF;
 INSERT INTO schools(id,name) VALUES(s,'shared-test'),(other,'other-test');
 INSERT INTO accounts(id,username,display_name) VALUES(actor,actor::text,'Shared test');
 IF EXISTS(SELECT 1 FROM information_schema.columns WHERE table_name='mobile_shared_tasks' AND column_name='device_id') THEN RAISE EXCEPTION 'shared root must not own a physical device'; END IF;
 INSERT INTO mobile_shared_send_policy_versions(id,revision,mode,limits,timezone,allowed_windows,created_by,reason) VALUES(policy,1,'disabled','{}','Asia/Shanghai','[]',actor,'isolated test');
 BEGIN
  UPDATE mobile_shared_send_policy_versions SET mode='production_allowlist' WHERE id=policy;
  RAISE EXCEPTION 'policy must be immutable' USING ERRCODE='22000';
 EXCEPTION WHEN raise_exception THEN NULL;
 END;
 INSERT INTO mobile_shared_task_drafts(id,school_id,intent_id,actor_id,target_ids,message,content_digest) VALUES(draft,s,intent,actor,ARRAY[gen_random_uuid()],' exact text ','digest');
 BEGIN
  INSERT INTO mobile_shared_previews(id,school_id,draft_id,draft_revision,plan,digest) VALUES(gen_random_uuid(),other,draft,1,'{}','digest');
  RAISE EXCEPTION 'cross-school preview accepted';
 EXCEPTION WHEN foreign_key_violation THEN NULL;
 END;
 INSERT INTO mobile_shared_previews(id,school_id,draft_id,draft_revision,plan,digest) VALUES(preview,s,draft,1,'{}','digest');
 INSERT INTO mobile_shared_tasks(id,school_id,draft_id,intent_id,preview_id,confirmed_by) VALUES(task,s,draft,intent,preview,actor);
 BEGIN
  INSERT INTO mobile_shared_tasks(id,school_id,draft_id,intent_id,preview_id,confirmed_by) VALUES(gen_random_uuid(),s,draft,intent,preview,actor);
  RAISE EXCEPTION 'same intent confirmed twice';
 EXCEPTION WHEN unique_violation THEN NULL;
 END;
 INSERT INTO mobile_shared_operations(principal_id,generation,scope,idempotency_key,request_hash,response) VALUES(actor,0,'test','stable','first','{}');
 BEGIN
  INSERT INTO mobile_shared_operations(principal_id,generation,scope,idempotency_key,request_hash,response) VALUES(actor,0,'test','stable','different','{}');
  RAISE EXCEPTION 'idempotency identity duplicated';
 EXCEPTION WHEN unique_violation THEN NULL;
 END;
 INSERT INTO execution_devices(id,name,credential_generation) VALUES(device,'synthetic isolated device',1);
 INSERT INTO mobile_targets(id,school_id,search_text,created_by,updated_by) VALUES(target,s,'synthetic target',actor,actor);
 INSERT INTO mobile_device_environments(id,device_id,credential_generation,snapshot,digest) VALUES(environment,device,1,'{}',repeat('a',64));
 INSERT INTO mobile_send_adapter_releases(id,name,version,environment,flow,content_digest,source_revision,evidence) VALUES(release,'synthetic-rollback-only','test','{}','{}',repeat('a',64),'test','[{"isolated_fixture":true}]');
 INSERT INTO mobile_shared_acceptance_runs(id,device_id,school_id,target_id,release_id,environment_snapshot_id,stage,actual_apk_sha256,evidence_digest,evidence,reviewer_id) VALUES(acceptance,device,s,target,release,environment,'production_allowlist',repeat('a',64),repeat('b',64),'{"preflight_passed":true,"soak_passed":true,"historical_debt_closed":true,"cases":{}}',actor);
 BEGIN
  INSERT INTO mobile_shared_release_scopes(id,device_id,school_id,target_id,release_id,environment_snapshot_id,stage,acceptance_id,expires_at) VALUES(gen_random_uuid(),device,s,target,release,environment,'production_allowlist',acceptance,now()+interval '1 hour');
  RAISE EXCEPTION 'missing soak hours bypassed production gate' USING ERRCODE='22000';
 EXCEPTION WHEN raise_exception THEN NULL;
 END;
 BEGIN
  INSERT INTO mobile_shared_release_scopes(id,device_id,school_id,target_id,release_id,environment_snapshot_id,stage,acceptance_id,expires_at) VALUES(gen_random_uuid(),device,other,target,release,environment,'production_allowlist',acceptance,now()+interval '1 hour');
  RAISE EXCEPTION 'cross-school acceptance accepted' USING ERRCODE='22000';
 EXCEPTION WHEN raise_exception THEN NULL;
 END;
 IF NOT EXISTS(SELECT 1 FROM pg_trigger WHERE tgrelid='mobile_shared_release_scopes'::regclass AND tgname='mobile_shared_scope_acceptance') THEN RAISE EXCEPTION 'scope bypasses acceptance gate'; END IF;
 IF NOT EXISTS(SELECT 1 FROM pg_attribute WHERE attrelid='mobile_shared_invocations'::regclass AND attname='encrypted_response' AND atttypid='bytea'::regtype) THEN RAISE EXCEPTION 'invocation response must be encrypted'; END IF;
END $$;
ROLLBACK;
