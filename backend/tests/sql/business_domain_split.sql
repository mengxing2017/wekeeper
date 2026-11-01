-- Executable after all migrations, on an isolated database only.
BEGIN;
DO $$
DECLARE s uuid=gen_random_uuid(); a uuid=gen_random_uuid(); p uuid=gen_random_uuid(); t uuid=gen_random_uuid(); snap uuid=gen_random_uuid(); n bigint;
BEGIN
 INSERT INTO schools(id,name) VALUES(s,'domain-split-test');
 INSERT INTO accounts(id,username,display_name) VALUES(a,a::text,'Test operator');
 INSERT INTO distribution_sources(namespace) VALUES('test.one'),('test.two');
 INSERT INTO distribution_source_organizations VALUES('test.one','1',s,true),('test.two','1',s,true);
 INSERT INTO distribution_materials VALUES('test.one','1','1',s,'v1','notice','message',true,now()),('test.two','1','1',s,'v1','notice','message',true,now());
 SELECT count(*) INTO n FROM distribution_materials WHERE external_content_id='1';
 IF n<>2 THEN RAISE EXCEPTION 'source namespaces merged'; END IF;
 INSERT INTO broadcast_tasks(id,school_id,creator_id,content,content_hash,corp_id,generation,content_source,source_namespace)
 VALUES(t,s,a,'message','frozen-hash','corp',1,'materials','test.one');
 INSERT INTO distribution_message_snapshots(id,school_id,channel,content,content_hash,source_references)
 VALUES(snap,s,'wecom','message','frozen-hash','[{"namespace":"test.one","external_organization_id":"1","external_content_id":"1","version":"v1"}]');
 INSERT INTO broadcast_task_snapshots VALUES(s,t,1,snap);
 INSERT INTO broadcast_content_references VALUES(s,t,0,'test.one','1','1','v1');
 DELETE FROM distribution_materials WHERE namespace='test.one';
 SELECT count(*) INTO n FROM broadcast_tasks b JOIN broadcast_task_snapshots l ON l.task_id=b.id JOIN distribution_message_snapshots m ON m.id=l.snapshot_id WHERE b.id=t AND m.content=b.content;
 IF n<>1 THEN RAISE EXCEPTION 'source deletion lost task history'; END IF;
 BEGIN
  UPDATE distribution_message_snapshots SET content='rewritten' WHERE id=snap;
  RAISE EXCEPTION 'snapshot was mutable' USING ERRCODE='22000';
 EXCEPTION WHEN raise_exception THEN NULL;
 END;
 INSERT INTO broadcast_draft_requests VALUES(s,'test.one',a,'key','request-hash',t);
 BEGIN
  INSERT INTO broadcast_draft_requests VALUES(s,'test.one',a,'key','different-hash',t);
  RAISE EXCEPTION 'idempotency duplicate accepted';
 EXCEPTION WHEN unique_violation THEN NULL;
 END;
 INSERT INTO posts(id,school_id,author_id,title,content,audit_status) VALUES(p,s,a,'Notice','Body','approved');
 IF NOT EXISTS(SELECT 1 FROM distribution_materials WHERE namespace='builtin.forum' AND external_content_id=p::text AND allowed) THEN RAISE EXCEPTION 'forum qualification not projected'; END IF;
 UPDATE posts SET visibility='deleted',revision=revision+1 WHERE id=p;
 IF EXISTS(SELECT 1 FROM distribution_materials WHERE namespace='builtin.forum' AND external_content_id=p::text AND allowed) THEN RAISE EXCEPTION 'withdrawal not transactional'; END IF;
 IF NOT EXISTS(SELECT 1 FROM distribution_source_events WHERE external_content_id=p::text AND NOT allowed AND processed_at IS NULL) THEN RAISE EXCEPTION 'withdrawal event not durable'; END IF;
 IF EXISTS(SELECT 1 FROM pg_constraint WHERE conrelid='broadcast_task_posts'::regclass AND confrelid='posts'::regclass) THEN RAISE EXCEPTION 'cross domain FK still exists'; END IF;
 IF EXISTS(SELECT 1 FROM pg_constraint WHERE conrelid='delivery_draft_posts'::regclass AND confrelid='posts'::regclass) THEN RAISE EXCEPTION 'legacy cross domain FK still exists'; END IF;
END $$;
ROLLBACK;
