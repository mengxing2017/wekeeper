-- Rollback-only regression on migrated isolated PostgreSQL. No real sending.
BEGIN;
DO $$
DECLARE
 actor uuid:=gen_random_uuid(); school uuid:=gen_random_uuid(); device uuid:=gen_random_uuid();
 target uuid:=gen_random_uuid(); usage uuid:=gen_random_uuid(); successor uuid:=gen_random_uuid();
 root uuid:=gen_random_uuid(); draft uuid:=gen_random_uuid(); preview uuid:=gen_random_uuid();
 intent uuid:=gen_random_uuid(); task uuid:=gen_random_uuid(); item uuid:=gen_random_uuid();
 snapshot uuid:=gen_random_uuid(); plan uuid:=gen_random_uuid(); attempt uuid:=gen_random_uuid();
 claim uuid:=gen_random_uuid(); invocation uuid:=gen_random_uuid(); allowed boolean;
BEGIN
 INSERT INTO accounts(id,username,display_name) VALUES(actor,'budget-attribution-test','Synthetic');
 INSERT INTO schools(id,name) VALUES(school,'Synthetic');
 INSERT INTO execution_devices(id,name) VALUES(device,'Synthetic');
 INSERT INTO mobile_targets(id,school_id,search_text,created_by,updated_by) VALUES(target,school,'Synthetic',actor,actor);
 INSERT INTO mobile_usages(id,school_id,actor_id,approved_by,approval,body,confirmed_until,expires_at,budget_root)
 VALUES(usage,school,actor,actor,'approved',jsonb_build_object('device_ids',jsonb_build_array(device),'targets',jsonb_build_array(jsonb_build_object('id',target)),'max_calls',1),clock_timestamp()+interval '1 day',clock_timestamp()+interval '1 day',root);
 INSERT INTO mobile_v9_task_drafts(id,school_id,intent_id,actor_id,target_ids,message,content_digest,stage,between_targets_seconds)
 VALUES(draft,school,intent,actor,ARRAY[target],'Synthetic','digest','production_allowlist',30);
 INSERT INTO mobile_v9_previews(id,school_id,draft_id,draft_revision,plan,digest) VALUES(preview,school,draft,1,'{}','digest');
 INSERT INTO mobile_v9_tasks(id,school_id,draft_id,intent_id,preview_id,confirmed_by,confirmed_until)
 VALUES(task,school,draft,intent,preview,actor,clock_timestamp()+interval '1 day');
 INSERT INTO mobile_v9_task_items(id,school_id,task_id,intent_id,target_id,position) VALUES(item,school,task,intent,target,0);
 INSERT INTO distribution_message_snapshots(id,school_id,channel,content,content_hash) VALUES(snapshot,school,'mobile','Synthetic','digest');
 INSERT INTO mobile_v9_plans(id,school_id,item_id,target_id,device_id,snapshot_id,plan,digest,content_digest)
 VALUES(plan,school,item,target,device,snapshot,'{}','digest','digest');
 INSERT INTO mobile_v9_attempts(id,school_id,item_id,plan_id,confirmed_by,attempt_no) VALUES(attempt,school,item,plan,actor,1);
 INSERT INTO mobile_v9_send_claims(id,school_id,attempt_id,device_id,credential_generation,runtime_epoch,lease_fence,recovery_credential_hash,state,expires_at)
 VALUES(claim,school,attempt,device,1,gen_random_uuid(),1,'synthetic','closed',clock_timestamp()+interval '1 minute');
 INSERT INTO mobile_v9_invocations(id,school_id,attempt_id,claim_id,device_id,credential_generation,lease_fence,execute_before,state,response,encrypted_response,invocation_secret_hash,result_credential_hash)
 VALUES(invocation,school,attempt,claim,device,1,1,clock_timestamp()+interval '15 seconds','consumed','{}','\x00','synthetic','synthetic');
 -- There is deliberately no grant, usage scope or scope_id on the plan.
 INSERT INTO mobile_usage_invocation_budgets(invocation_id,usage_id,budget_root) VALUES(invocation,usage,root);
 SELECT count(*)<1 INTO allowed FROM mobile_usage_invocation_budgets b JOIN mobile_usages u ON u.id=b.usage_id WHERE u.school_id=school AND b.budget_root=root;
 IF allowed THEN RAISE EXCEPTION 'Missing scope bypassed the cumulative business budget'; END IF;
 INSERT INTO mobile_usages(id,school_id,actor_id,approved_by,approval,body,confirmed_until,expires_at,budget_root,supersedes)
 SELECT successor,school_id,actor_id,approved_by,approval,body,confirmed_until,expires_at+interval '1 day',budget_root,id FROM mobile_usages WHERE id=usage;
 SELECT count(*)<(SELECT (body->>'max_calls')::bigint FROM mobile_usages WHERE id=successor) INTO allowed FROM mobile_usage_invocation_budgets b WHERE b.budget_root=root;
 IF allowed THEN RAISE EXCEPTION 'Renewal reset the business budget'; END IF;
 INSERT INTO mobile_usage_invocation_budgets(invocation_id,usage_id,budget_root) VALUES(invocation,successor,root) ON CONFLICT DO NOTHING;
 IF (SELECT count(*) FROM mobile_usage_invocation_budgets WHERE budget_root=root)<>1 THEN RAISE EXCEPTION 'Idempotent attribution double-charged a root'; END IF;
 BEGIN
  DELETE FROM mobile_usage_invocation_budgets WHERE invocation_id=invocation;
  RAISE EXCEPTION 'Budget attribution was deletable';
 EXCEPTION WHEN raise_exception THEN
  IF SQLERRM<>'mobile capability evidence is immutable' THEN RAISE; END IF;
 END;
END $$;
ROLLBACK;
