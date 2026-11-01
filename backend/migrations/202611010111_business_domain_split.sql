-- Logical split only: one database/login/deployment. No historical task is re-created.
CREATE TABLE distribution_sources (
 namespace text PRIMARY KEY CHECK(length(namespace) BETWEEN 1 AND 100),
 enabled boolean NOT NULL DEFAULT true,
 max_pending integer NOT NULL DEFAULT 10000 CHECK(max_pending>0)
);
INSERT INTO distribution_sources(namespace) VALUES('builtin.forum'),('builtin.direct');
CREATE TABLE distribution_source_organizations (
 namespace text NOT NULL REFERENCES distribution_sources(namespace),
 external_organization_id text NOT NULL CHECK(length(external_organization_id) BETWEEN 1 AND 200),
 school_id uuid NOT NULL REFERENCES schools(id), enabled boolean NOT NULL DEFAULT true,
 PRIMARY KEY(namespace,external_organization_id), UNIQUE(namespace,external_organization_id,school_id)
);
-- Platform-owned qualification projection. The forum is one transactional producer,
-- replaceable by another port/event consumer; no FK points back to a forum entity.
CREATE TABLE distribution_materials (
 namespace text NOT NULL, external_organization_id text NOT NULL,
 external_content_id text NOT NULL CHECK(length(external_content_id) BETWEEN 1 AND 200),
 school_id uuid NOT NULL, version text NOT NULL, title text NOT NULL, content text NOT NULL,
 allowed boolean NOT NULL, updated_at timestamptz NOT NULL DEFAULT now(),
 PRIMARY KEY(namespace,external_organization_id,external_content_id),
 FOREIGN KEY(namespace,external_organization_id,school_id) REFERENCES distribution_source_organizations(namespace,external_organization_id,school_id)
);
CREATE TABLE distribution_source_events (
 id uuid PRIMARY KEY DEFAULT gen_random_uuid(), namespace text NOT NULL,
 external_organization_id text NOT NULL, external_content_id text NOT NULL,
 version text NOT NULL, allowed boolean NOT NULL, created_at timestamptz NOT NULL DEFAULT now(),
 processed_at timestamptz
);
CREATE INDEX distribution_source_events_pending ON distribution_source_events(created_at,id) WHERE processed_at IS NULL;
ALTER TABLE broadcast_tasks ADD COLUMN source_namespace text NOT NULL DEFAULT 'builtin.forum' REFERENCES distribution_sources(namespace);
UPDATE broadcast_tasks SET source_namespace='builtin.direct' WHERE content_source='text';
ALTER TABLE broadcast_tasks DROP CONSTRAINT broadcast_content_source;
ALTER TABLE broadcast_tasks ADD CONSTRAINT broadcast_content_source CHECK (
 (content_source='posts' AND template_version_id IS NOT NULL) OR
 (content_source IN ('text','materials') AND template_version_id IS NULL)
);
CREATE TABLE distribution_message_snapshots (
 id uuid PRIMARY KEY, school_id uuid NOT NULL REFERENCES schools(id), channel text NOT NULL CHECK(channel IN ('wecom','mobile')),
 content text NOT NULL, content_hash text NOT NULL, algorithm_version text NOT NULL DEFAULT 'sha256-text/1',
 media jsonb NOT NULL DEFAULT '[]', source_references jsonb NOT NULL DEFAULT '[]', created_at timestamptz NOT NULL DEFAULT now(), UNIQUE(school_id,id)
);
CREATE TABLE broadcast_task_snapshots (
 school_id uuid NOT NULL, task_id uuid NOT NULL, task_revision bigint NOT NULL,
 snapshot_id uuid NOT NULL, PRIMARY KEY(task_id,task_revision),
 FOREIGN KEY(school_id,task_id) REFERENCES broadcast_tasks(school_id,id),
 FOREIGN KEY(school_id,snapshot_id) REFERENCES distribution_message_snapshots(school_id,id)
);
CREATE TABLE broadcast_content_references (
 school_id uuid NOT NULL, task_id uuid NOT NULL, position integer NOT NULL,
 namespace text NOT NULL, external_organization_id text NOT NULL, external_content_id text NOT NULL,
 version text NOT NULL, PRIMARY KEY(task_id,position),
 UNIQUE(task_id,namespace,external_organization_id,external_content_id),
 FOREIGN KEY(school_id,task_id) REFERENCES broadcast_tasks(school_id,id)
);
INSERT INTO distribution_source_organizations(namespace,external_organization_id,school_id)
 SELECT n.namespace,s.id::text,s.id FROM schools s CROSS JOIN distribution_sources n;
INSERT INTO distribution_materials(namespace,external_organization_id,external_content_id,school_id,version,title,content,allowed)
 SELECT 'builtin.forum',school_id::text,id::text,school_id,revision::text,title,content,audit_status='approved' AND visibility='visible' FROM posts;
INSERT INTO broadcast_content_references
 SELECT school_id,task_id,position,'builtin.forum',school_id::text,post_id::text,post_revision::text FROM broadcast_task_posts;
INSERT INTO distribution_message_snapshots(id,school_id,channel,content,content_hash,source_references)
 SELECT t.id,t.school_id,'wecom',t.content,t.content_hash,coalesce((SELECT jsonb_agg(jsonb_build_object('namespace',r.namespace,'external_organization_id',r.external_organization_id,'external_content_id',r.external_content_id,'version',r.version) ORDER BY r.position) FROM broadcast_content_references r WHERE r.task_id=t.id),'[]'::jsonb) FROM broadcast_tasks t;
INSERT INTO broadcast_task_snapshots SELECT school_id,id,revision,id FROM broadcast_tasks;
-- Preserve immutable evidence after forum removal. Old relation records remain for history.
ALTER TABLE broadcast_task_posts DROP CONSTRAINT broadcast_task_posts_school_id_post_id_fkey;
ALTER TABLE delivery_draft_posts DROP CONSTRAINT delivery_draft_posts_school_id_post_id_fkey;
CREATE FUNCTION distribution_snapshot_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN RAISE EXCEPTION 'distribution snapshots are immutable'; END $$;
CREATE TRIGGER distribution_snapshot_immutable BEFORE UPDATE OR DELETE ON distribution_message_snapshots
 FOR EACH ROW EXECUTE FUNCTION distribution_snapshot_immutable();
-- The forum producer updates its qualification projection and durable outbox in
-- the SAME transaction, including non-HTTP writes. Deleted content is a tombstone.
CREATE FUNCTION forum_distribution_material_changed() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE r posts%ROWTYPE; ok boolean;
BEGIN
 IF TG_OP='DELETE' THEN r=OLD; ok=false; ELSE r=NEW; ok=r.audit_status='approved' AND r.visibility='visible'; END IF;
 INSERT INTO distribution_source_organizations VALUES('builtin.forum',r.school_id::text,r.school_id,true) ON CONFLICT DO NOTHING;
 INSERT INTO distribution_source_organizations VALUES('builtin.direct',r.school_id::text,r.school_id,true) ON CONFLICT DO NOTHING;
 INSERT INTO distribution_materials(namespace,external_organization_id,external_content_id,school_id,version,title,content,allowed)
 VALUES('builtin.forum',r.school_id::text,r.id::text,r.school_id,r.revision::text,r.title,r.content,ok)
 ON CONFLICT(namespace,external_organization_id,external_content_id) DO UPDATE
 SET version=excluded.version,title=excluded.title,content=excluded.content,allowed=excluded.allowed,updated_at=now();
 INSERT INTO distribution_source_events(namespace,external_organization_id,external_content_id,version,allowed)
 VALUES('builtin.forum',r.school_id::text,r.id::text,r.revision::text,ok);
 RETURN NULL;
END $$;
CREATE TRIGGER forum_distribution_material_changed AFTER INSERT OR UPDATE OF title,content,audit_status,visibility,revision OR DELETE ON posts
 FOR EACH ROW EXECUTE FUNCTION forum_distribution_material_changed();
-- Compatibility read model belongs to distribution, not a view over posts.
CREATE VIEW distribution_forum_materials AS
 SELECT external_content_id::uuid AS id,school_id,title,content,version::bigint AS revision,
 CASE WHEN allowed THEN 'approved' ELSE 'rejected' END AS audit_status,
 CASE WHEN allowed THEN 'visible' ELSE 'hidden' END AS visibility
 FROM distribution_materials WHERE namespace='builtin.forum';
CREATE TABLE wecom_sender_qualifications (
 school_id uuid NOT NULL REFERENCES schools(id), account_id uuid NOT NULL REFERENCES accounts(id),
 enabled boolean NOT NULL, revision bigint NOT NULL DEFAULT 1,
 updated_by uuid NOT NULL REFERENCES accounts(id), updated_at timestamptz NOT NULL DEFAULT now(),
 PRIMARY KEY(school_id,account_id)
);
CREATE TABLE distribution_resource_policies (
 resource_key text PRIMARY KEY, window_seconds integer NOT NULL CHECK(window_seconds>0),
 max_calls integer NOT NULL CHECK(max_calls>0), enabled boolean NOT NULL DEFAULT true
);
-- One ledger for a real official resource across all schools and sources.
CREATE TABLE distribution_resource_usage (
 batch_id uuid PRIMARY KEY REFERENCES broadcast_batches(id), resource_key text NOT NULL,
 source_namespace text NOT NULL REFERENCES distribution_sources(namespace),
 called_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX distribution_resource_usage_window ON distribution_resource_usage(resource_key,called_at);
CREATE TABLE distribution_source_turns (
 source_namespace text PRIMARY KEY REFERENCES distribution_sources(namespace),
 last_claimed_at timestamptz NOT NULL DEFAULT '-infinity'
);
INSERT INTO distribution_source_turns SELECT namespace FROM distribution_sources;
CREATE TABLE broadcast_previews (
 task_id uuid PRIMARY KEY REFERENCES broadcast_tasks(id), revision bigint NOT NULL,
 digest text NOT NULL, expires_at timestamptz NOT NULL
);
ALTER TABLE distribution_sources ADD COLUMN revision bigint NOT NULL DEFAULT 1;
ALTER TABLE distribution_resource_policies ADD COLUMN revision bigint NOT NULL DEFAULT 1;
-- Existing calls remain in the common ledger; a deployment cannot reset usage.
INSERT INTO distribution_resource_usage(batch_id,resource_key,source_namespace,called_at)
 SELECT b.id,'wecom:'||b.corp_id||':'||lower(b.sender COLLATE "C"),t.source_namespace,coalesce(b.claimed_at,t.created_at)
 FROM broadcast_batches b JOIN broadcast_tasks t ON t.id=b.task_id WHERE b.invocation_id IS NOT NULL;
CREATE TABLE broadcast_draft_requests (
 school_id uuid NOT NULL, source_namespace text NOT NULL REFERENCES distribution_sources(namespace),
 actor_id uuid NOT NULL REFERENCES accounts(id), request_key text NOT NULL CHECK(length(request_key) BETWEEN 1 AND 128),
 request_hash text NOT NULL, task_id uuid NOT NULL,
 PRIMARY KEY(source_namespace,school_id,actor_id,request_key),
 FOREIGN KEY(school_id,task_id) REFERENCES broadcast_tasks(school_id,id)
);
