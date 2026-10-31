-- Database baseline: official WeCom broadcast. Depends on core business.
CREATE TABLE broadcast_tasks (
 id UUID PRIMARY KEY, school_id UUID NOT NULL REFERENCES schools(id), creator_id UUID NOT NULL REFERENCES accounts(id),
 template_version_id UUID, content TEXT NOT NULL CHECK(octet_length(content) BETWEEN 1 AND 4000), content_hash TEXT NOT NULL,
 revision BIGINT NOT NULL DEFAULT 1, state TEXT NOT NULL DEFAULT 'draft' CHECK(state IN ('draft','confirmed','abandoned')),
 corp_id TEXT NOT NULL, generation BIGINT NOT NULL, confirmed_by UUID REFERENCES accounts(id), confirmed_at TIMESTAMPTZ,
 stopped BOOLEAN NOT NULL DEFAULT FALSE, source_task_id UUID, source_draft_id UUID, resend_reason TEXT NOT NULL DEFAULT '',
 created_at TIMESTAMPTZ NOT NULL DEFAULT now(), UNIQUE(school_id,id),
 FOREIGN KEY(school_id,template_version_id) REFERENCES template_versions(school_id,id),
 FOREIGN KEY(school_id,source_task_id) REFERENCES broadcast_tasks(school_id,id),
 FOREIGN KEY(school_id,source_draft_id) REFERENCES delivery_drafts(school_id,id),
 content_source TEXT NOT NULL DEFAULT 'posts',
 CONSTRAINT broadcast_content_source CHECK (
  (content_source = 'posts' AND template_version_id IS NOT NULL) OR
  (content_source = 'text' AND template_version_id IS NULL)
 )
);
CREATE INDEX broadcast_task_school ON broadcast_tasks(school_id,created_at DESC,id);
CREATE TABLE broadcast_task_posts (
 school_id UUID NOT NULL, task_id UUID NOT NULL, post_id UUID NOT NULL, post_revision BIGINT NOT NULL, position INTEGER NOT NULL,
 PRIMARY KEY(task_id,post_id), FOREIGN KEY(school_id,task_id) REFERENCES broadcast_tasks(school_id,id),
 FOREIGN KEY(school_id,post_id) REFERENCES posts(school_id,id)
);
CREATE TABLE broadcast_batches (
 id UUID PRIMARY KEY, school_id UUID NOT NULL, task_id UUID NOT NULL, ordinal INTEGER NOT NULL,
 sender TEXT NOT NULL, sender_account UUID NOT NULL REFERENCES accounts(id), binding_revision BIGINT NOT NULL,
 state TEXT NOT NULL DEFAULT 'queued' CHECK(state IN ('queued','submitting','created','rejected','unknown','cancelled')),
 invocation_id UUID UNIQUE, claimed_at TIMESTAMPTZ, msgid TEXT, corp_id TEXT NOT NULL,
 error_code TEXT, member_status INTEGER, last_synced_at TIMESTAMPTZ, sync_error TEXT, next_sync_at TIMESTAMPTZ NOT NULL DEFAULT now(),
 stop_state TEXT NOT NULL DEFAULT 'none' CHECK(stop_state IN ('none','requested','stopped','unknown','rejected')),
 reconciliation TEXT CHECK(reconciliation IN ('not_created','sent','linked')), reconciliation_note TEXT,
 UNIQUE(task_id,ordinal), UNIQUE(school_id,task_id,id), UNIQUE(corp_id,msgid),
 FOREIGN KEY(school_id,task_id) REFERENCES broadcast_tasks(school_id,id)
);
CREATE INDEX broadcast_pending ON broadcast_batches(state,next_sync_at);
CREATE TABLE broadcast_targets (
 id UUID PRIMARY KEY, school_id UUID NOT NULL, task_id UUID NOT NULL, batch_id UUID NOT NULL, group_id UUID NOT NULL,
 chat_id TEXT NOT NULL, group_name TEXT NOT NULL, group_revision BIGINT NOT NULL,
 rejected BOOLEAN NOT NULL DEFAULT FALSE, official_status INTEGER, send_time BIGINT,
 UNIQUE(task_id,group_id), UNIQUE(batch_id,chat_id),
 FOREIGN KEY(school_id,task_id,batch_id) REFERENCES broadcast_batches(school_id,task_id,id),
 FOREIGN KEY(school_id,group_id) REFERENCES wecom_groups(school_id,id)
);
CREATE TABLE broadcast_events (
 id UUID PRIMARY KEY, school_id UUID NOT NULL, task_id UUID NOT NULL, actor_id UUID REFERENCES accounts(id),
 action TEXT NOT NULL, detail JSONB NOT NULL DEFAULT '{}', created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
 FOREIGN KEY(school_id,task_id) REFERENCES broadcast_tasks(school_id,id)
);
CREATE INDEX broadcast_events_task ON broadcast_events(task_id,created_at DESC,id);
CREATE TABLE broadcast_operations (
 id UUID PRIMARY KEY, school_id UUID NOT NULL, task_id UUID NOT NULL, batch_id UUID,
 actor_id UUID NOT NULL REFERENCES accounts(id), operation TEXT NOT NULL, idempotency_key TEXT NOT NULL, request_hash TEXT NOT NULL,
 state TEXT NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','calling','done','unknown','rejected')),
 claimed_at TIMESTAMPTZ, error_code TEXT, created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
 UNIQUE(actor_id,school_id,operation,idempotency_key),
 FOREIGN KEY(school_id,task_id) REFERENCES broadcast_tasks(school_id,id),
 FOREIGN KEY(school_id,task_id,batch_id) REFERENCES broadcast_batches(school_id,task_id,id)
);
