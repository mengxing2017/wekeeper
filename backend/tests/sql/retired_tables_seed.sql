-- Runs immediately before final retirement; old data must not block an upgrade.
UPDATE accounts SET bound_school_id='20000000-0000-4000-8000-000000000001'
 WHERE id='20000000-0000-4000-8000-000000000002';
INSERT INTO posts(id,school_id,author_id,title,content,audit_status)
 VALUES('20000000-0000-4000-8000-000000000010','20000000-0000-4000-8000-000000000001','20000000-0000-4000-8000-000000000002','Retired forum material','Frozen material body','approved');
INSERT INTO comments(id,school_id,post_id,author_id,content)
 VALUES('20000000-0000-4000-8000-000000000011','20000000-0000-4000-8000-000000000001','20000000-0000-4000-8000-000000000010','20000000-0000-4000-8000-000000000002','Obsolete comment');
INSERT INTO distribution_message_snapshots(id,school_id,channel,content,content_hash)
 VALUES('20000000-0000-4000-8000-000000000012','20000000-0000-4000-8000-000000000001','wecom','Preserved immutable message','preserved-hash');
INSERT INTO audit_events(id,school_id,actor_id,action,object_id,detail)
 VALUES('20000000-0000-4000-8000-000000000013',NULL,'20000000-0000-4000-8000-000000000002','retirement.test.history','20000000-0000-4000-8000-000000000010','{"preserve":true}');
INSERT INTO execution_devices(id,name,enabled)
 VALUES('20000000-0000-4000-8000-000000000014','Preserved phone',false);
INSERT INTO retired_mobile_incident_archive(source_table,source_id,record)
 VALUES('old-test','20000000-0000-4000-8000-000000000015','{"retired":true}');
