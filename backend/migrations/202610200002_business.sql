CREATE TABLE accounts (
 id UUID PRIMARY KEY, username TEXT UNIQUE NOT NULL, password_hash TEXT,
 display_name TEXT NOT NULL, platform_admin BOOLEAN NOT NULL DEFAULT FALSE,
 active BOOLEAN NOT NULL DEFAULT TRUE, wecom_userid TEXT UNIQUE, wx_openid TEXT UNIQUE,
 bound_school_id UUID, created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE TABLE schools (
 id UUID PRIMARY KEY, name TEXT NOT NULL, active BOOLEAN NOT NULL DEFAULT TRUE,
 config JSONB NOT NULL DEFAULT '{}', revision BIGINT NOT NULL DEFAULT 1,
 created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
ALTER TABLE accounts ADD FOREIGN KEY (bound_school_id) REFERENCES schools(id);
CREATE TABLE school_memberships (
 school_id UUID NOT NULL REFERENCES schools(id), account_id UUID NOT NULL REFERENCES accounts(id),
 role TEXT NOT NULL CHECK (role IN ('member','operator','admin')), PRIMARY KEY (school_id,account_id)
);
CREATE TABLE sessions (
 token_hash TEXT PRIMARY KEY, account_id UUID NOT NULL REFERENCES accounts(id), csrf TEXT NOT NULL,
 expires_at TIMESTAMPTZ NOT NULL, last_seen_at TIMESTAMPTZ NOT NULL DEFAULT now(),
 kind TEXT NOT NULL CHECK (kind IN ('browser','miniapp')), wecom_authenticated BOOLEAN NOT NULL DEFAULT FALSE
);
CREATE INDEX sessions_expiry ON sessions(expires_at);
CREATE TABLE login_attempts (username TEXT PRIMARY KEY, failures INTEGER NOT NULL, reset_at TIMESTAMPTZ NOT NULL);
CREATE TABLE audit_events (
 id UUID PRIMARY KEY, school_id UUID REFERENCES schools(id), actor_id UUID REFERENCES accounts(id),
 action TEXT NOT NULL, object_id UUID, detail JSONB NOT NULL DEFAULT '{}', created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX audit_scope ON audit_events(school_id,created_at DESC);
CREATE TABLE posts (
 id UUID PRIMARY KEY, school_id UUID NOT NULL REFERENCES schools(id), author_id UUID NOT NULL REFERENCES accounts(id),
 title TEXT NOT NULL, content TEXT NOT NULL, images JSONB NOT NULL DEFAULT '[]', tags JSONB NOT NULL DEFAULT '[]',
 audit_status TEXT NOT NULL DEFAULT 'pending' CHECK(audit_status IN ('pending','approved','rejected')),
 visibility TEXT NOT NULL DEFAULT 'visible' CHECK(visibility IN ('visible','hidden','deleted')),
 top_sort INTEGER NOT NULL DEFAULT 0, browse BIGINT NOT NULL DEFAULT 0, revision BIGINT NOT NULL DEFAULT 1,
 created_at TIMESTAMPTZ NOT NULL DEFAULT now(), updated_at TIMESTAMPTZ NOT NULL DEFAULT now(), UNIQUE(school_id,id)
);
CREATE INDEX post_feed ON posts(school_id,audit_status,visibility,top_sort DESC,created_at DESC,id);
CREATE TABLE comments (
 id UUID PRIMARY KEY, school_id UUID NOT NULL, post_id UUID NOT NULL, author_id UUID NOT NULL REFERENCES accounts(id),
 parent_id UUID REFERENCES comments(id), content TEXT NOT NULL,
 audit_status TEXT NOT NULL DEFAULT 'pending' CHECK(audit_status IN ('pending','approved','rejected')),
 deleted BOOLEAN NOT NULL DEFAULT FALSE, revision BIGINT NOT NULL DEFAULT 1, created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
 FOREIGN KEY(school_id,post_id) REFERENCES posts(school_id,id)
);
CREATE INDEX comment_post ON comments(school_id,post_id,created_at);
CREATE TABLE post_reactions (
 school_id UUID NOT NULL, post_id UUID NOT NULL, account_id UUID NOT NULL REFERENCES accounts(id),
 kind TEXT NOT NULL CHECK(kind IN ('like','collect')), PRIMARY KEY(post_id,account_id,kind),
 FOREIGN KEY(school_id,post_id) REFERENCES posts(school_id,id)
);
CREATE TABLE follows (
 school_id UUID NOT NULL REFERENCES schools(id), follower_id UUID NOT NULL REFERENCES accounts(id),
 target_id UUID NOT NULL REFERENCES accounts(id), PRIMARY KEY(school_id,follower_id,target_id), CHECK(follower_id<>target_id)
);
CREATE TABLE notifications (
 id UUID PRIMARY KEY, school_id UUID NOT NULL REFERENCES schools(id), account_id UUID NOT NULL REFERENCES accounts(id),
 kind TEXT NOT NULL, object_id UUID, read_at TIMESTAMPTZ, created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE TABLE moderation_events (
 id UUID PRIMARY KEY, school_id UUID NOT NULL REFERENCES schools(id), actor_id UUID NOT NULL REFERENCES accounts(id),
 object_id UUID NOT NULL, object_kind TEXT NOT NULL, decision TEXT NOT NULL, reason TEXT NOT NULL,
 created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE TABLE cases (
 id UUID PRIMARY KEY, school_id UUID NOT NULL REFERENCES schools(id), account_id UUID NOT NULL REFERENCES accounts(id),
 kind TEXT NOT NULL CHECK(kind IN ('report','feedback')), post_id UUID, content TEXT NOT NULL, contact TEXT NOT NULL DEFAULT '',
 state TEXT NOT NULL DEFAULT 'open' CHECK(state IN ('open','resolved','dismissed')), resolution TEXT NOT NULL DEFAULT '',
 revision BIGINT NOT NULL DEFAULT 1, created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
 FOREIGN KEY(school_id,post_id) REFERENCES posts(school_id,id)
);
CREATE TABLE school_resources (
 id UUID PRIMARY KEY, school_id UUID NOT NULL REFERENCES schools(id),
 kind TEXT NOT NULL CHECK(kind IN ('banner','qrcode','service','legal','tag','blocked_word')),
 title TEXT NOT NULL, content TEXT NOT NULL DEFAULT '', url TEXT NOT NULL DEFAULT '', active BOOLEAN NOT NULL DEFAULT TRUE,
 sort_order INTEGER NOT NULL DEFAULT 0, starts_at TIMESTAMPTZ, ends_at TIMESTAMPTZ,
 revision BIGINT NOT NULL DEFAULT 1, clicks BIGINT NOT NULL DEFAULT 0, CHECK(ends_at IS NULL OR starts_at IS NULL OR ends_at>starts_at)
);
CREATE TABLE wecom_groups (
 id UUID PRIMARY KEY, school_id UUID NOT NULL REFERENCES schools(id), corp_id TEXT NOT NULL, chat_id TEXT NOT NULL,
 name TEXT NOT NULL, active BOOLEAN NOT NULL DEFAULT TRUE, revision BIGINT NOT NULL DEFAULT 1,
 UNIQUE(corp_id,chat_id), UNIQUE(school_id,id)
);
CREATE TABLE templates (
 id UUID PRIMARY KEY, school_id UUID NOT NULL REFERENCES schools(id), name TEXT NOT NULL,
 created_at TIMESTAMPTZ NOT NULL DEFAULT now(), UNIQUE(school_id,id)
);
CREATE TABLE template_versions (
 id UUID PRIMARY KEY, school_id UUID NOT NULL, template_id UUID NOT NULL, body TEXT NOT NULL,
 published BOOLEAN NOT NULL DEFAULT FALSE, revision BIGINT NOT NULL DEFAULT 1, created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
 FOREIGN KEY(school_id,template_id) REFERENCES templates(school_id,id), UNIQUE(school_id,id)
);
CREATE TABLE delivery_drafts (
 id UUID PRIMARY KEY, school_id UUID NOT NULL, group_id UUID NOT NULL, template_version_id UUID NOT NULL,
 creator_id UUID NOT NULL REFERENCES accounts(id), content TEXT NOT NULL, content_hash TEXT NOT NULL,
 state TEXT NOT NULL DEFAULT 'ready' CHECK(state IN ('ready','invalidated')), created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
 FOREIGN KEY(school_id,group_id) REFERENCES wecom_groups(school_id,id),
 FOREIGN KEY(school_id,template_version_id) REFERENCES template_versions(school_id,id), UNIQUE(school_id,id)
);
CREATE TABLE delivery_draft_posts (
 draft_id UUID NOT NULL REFERENCES delivery_drafts(id), school_id UUID NOT NULL, post_id UUID NOT NULL,
 post_revision BIGINT NOT NULL, PRIMARY KEY(draft_id,post_id), FOREIGN KEY(school_id,post_id) REFERENCES posts(school_id,id)
);
CREATE TABLE delivery_attempts (
 id UUID PRIMARY KEY, school_id UUID NOT NULL, draft_id UUID NOT NULL, actor_id UUID NOT NULL REFERENCES accounts(id),
 idempotency_key TEXT NOT NULL, state TEXT NOT NULL DEFAULT 'prepared' CHECK(state IN ('prepared','client_reported_success','client_reported_failure','cancelled','unknown')),
 client_result JSONB, reconciliation TEXT CHECK(reconciliation IN ('operator_confirmed','operator_rejected')),
 reconciliation_note TEXT, reconciled_by UUID REFERENCES accounts(id), reconciled_at TIMESTAMPTZ,
 resend_of UUID REFERENCES delivery_attempts(id), resend_reason TEXT,
 created_at TIMESTAMPTZ NOT NULL DEFAULT now(), updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
 FOREIGN KEY(school_id,draft_id) REFERENCES delivery_drafts(school_id,id), UNIQUE(actor_id,idempotency_key)
);
CREATE INDEX attempt_draft ON delivery_attempts(draft_id,created_at DESC);
CREATE TABLE media_assets (
 id UUID PRIMARY KEY, school_id UUID NOT NULL REFERENCES schools(id), owner_id UUID NOT NULL REFERENCES accounts(id),
 mime_type TEXT NOT NULL, size_bytes BIGINT NOT NULL, content BYTEA NOT NULL, created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE TABLE oauth_states (state_hash TEXT PRIMARY KEY, session_hash TEXT, return_path TEXT NOT NULL, expires_at TIMESTAMPTZ NOT NULL);
CREATE TABLE platform_cache (key TEXT PRIMARY KEY, value TEXT NOT NULL, expires_at TIMESTAMPTZ NOT NULL);
CREATE TABLE legacy_id_map (
 source_table TEXT NOT NULL, legacy_id TEXT NOT NULL, target_id UUID NOT NULL, batch_id UUID NOT NULL,
 PRIMARY KEY(source_table,legacy_id)
);
CREATE TABLE migration_runs (id UUID PRIMARY KEY, source_hash TEXT NOT NULL UNIQUE, summary JSONB NOT NULL, created_at TIMESTAMPTZ NOT NULL DEFAULT now());
