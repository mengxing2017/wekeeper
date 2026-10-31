-- Database baseline: application metadata and core business. Requires an empty schema.
CREATE TABLE application_metadata (
    singleton BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (singleton),
    schema_generation INTEGER NOT NULL CHECK (schema_generation > 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO application_metadata (singleton, schema_generation) VALUES (TRUE, 1);

CREATE TABLE accounts (
 id UUID PRIMARY KEY, username TEXT UNIQUE NOT NULL, password_hash TEXT,
 display_name TEXT NOT NULL, platform_admin BOOLEAN NOT NULL DEFAULT FALSE,
 active BOOLEAN NOT NULL DEFAULT TRUE, wecom_member_id UUID UNIQUE, wecom_binding_revision BIGINT NOT NULL DEFAULT 0, wx_openid TEXT UNIQUE,
 bound_school_id UUID, created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
 avatar_url TEXT NOT NULL DEFAULT ''
);
CREATE TABLE schools (
 id UUID PRIMARY KEY, name TEXT NOT NULL, active BOOLEAN NOT NULL DEFAULT TRUE,
 config JSONB NOT NULL DEFAULT '{}', revision BIGINT NOT NULL DEFAULT 1,
 created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
ALTER TABLE accounts ADD FOREIGN KEY (bound_school_id) REFERENCES schools(id);
CREATE TABLE school_memberships (
 school_id UUID NOT NULL REFERENCES schools(id), account_id UUID NOT NULL REFERENCES accounts(id),
 role TEXT NOT NULL CHECK (role IN ('member','operator','admin','revoked')), PRIMARY KEY (school_id,account_id)
);
CREATE TABLE sessions (
 token_hash TEXT PRIMARY KEY, account_id UUID NOT NULL REFERENCES accounts(id), csrf TEXT NOT NULL,
 expires_at TIMESTAMPTZ NOT NULL, last_seen_at TIMESTAMPTZ NOT NULL DEFAULT now(),
 credential_generation BIGINT, kind TEXT NOT NULL CHECK (kind IN ('browser','miniapp')), wecom_authenticated BOOLEAN NOT NULL DEFAULT FALSE, wecom_binding_revision BIGINT NOT NULL DEFAULT 0
);
CREATE INDEX sessions_expiry ON sessions(expires_at);
CREATE TABLE login_attempts (username TEXT PRIMARY KEY, failures INTEGER NOT NULL, reset_at TIMESTAMPTZ NOT NULL);
CREATE TABLE audit_events (
 id UUID PRIMARY KEY, school_id UUID REFERENCES schools(id), actor_id UUID REFERENCES accounts(id),
 action TEXT NOT NULL, object_id UUID, detail JSONB NOT NULL DEFAULT '{}', created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
 request_id TEXT
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
 parent_id UUID REFERENCES comments(id) DEFERRABLE INITIALLY DEFERRED, content TEXT NOT NULL,
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
 FOREIGN KEY(school_id,post_id) REFERENCES posts(school_id,id),
 images JSONB NOT NULL DEFAULT '[]'
);
CREATE TABLE school_resources (
 id UUID PRIMARY KEY, school_id UUID REFERENCES schools(id),
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
 wecom_binding_revision BIGINT NOT NULL DEFAULT 0, idempotency_key TEXT NOT NULL, state TEXT NOT NULL DEFAULT 'prepared' CHECK(state IN ('prepared','client_reported_success','client_reported_failure','cancelled','unknown')),
 client_result JSONB, reconciliation TEXT CHECK(reconciliation IN ('operator_confirmed','operator_rejected')),
 reconciliation_note TEXT, reconciled_by UUID REFERENCES accounts(id), reconciled_at TIMESTAMPTZ,
 resend_of UUID REFERENCES delivery_attempts(id), resend_reason TEXT,
 created_at TIMESTAMPTZ NOT NULL DEFAULT now(), updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
 FOREIGN KEY(school_id,draft_id) REFERENCES delivery_drafts(school_id,id), UNIQUE(actor_id,idempotency_key),
 client_invoked_at TIMESTAMPTZ
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

-- Single-enterprise directory. Runs stage remotely; publication is one transaction.
CREATE TABLE wecom_directory_state (
 singleton BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK(singleton), corp_id TEXT NOT NULL, credential_scope TEXT NOT NULL,
 credential_generation BIGINT NOT NULL DEFAULT 1 CHECK(credential_generation>0),
 snapshot_id UUID, last_success_at TIMESTAMPTZ, next_call_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE TABLE wecom_directory_sync_runs (
 id UUID PRIMARY KEY, actor_id UUID REFERENCES accounts(id), generation BIGINT NOT NULL CHECK(generation>0), state TEXT NOT NULL DEFAULT 'queued' CHECK(state IN ('queued','running','succeeded','failed')),
 created_at TIMESTAMPTZ NOT NULL DEFAULT now(), started_at TIMESTAMPTZ, finished_at TIMESTAMPTZ,
 lease_id UUID, lease_until TIMESTAMPTZ, error_code TEXT,
 department_count INTEGER NOT NULL DEFAULT 0, member_count INTEGER NOT NULL DEFAULT 0, snapshot JSONB
);
CREATE UNIQUE INDEX wecom_directory_one_job ON wecom_directory_sync_runs((TRUE)) WHERE state IN ('queued','running');
CREATE TABLE wecom_departments (
 id BIGINT PRIMARY KEY, parentid BIGINT NOT NULL, name TEXT NOT NULL, "order" BIGINT NOT NULL DEFAULT 0
);
CREATE TABLE wecom_members (
 id UUID PRIMARY KEY, userid TEXT NOT NULL, userid_key TEXT GENERATED ALWAYS AS (lower(userid COLLATE "C")) STORED UNIQUE,
 name TEXT NOT NULL, status INTEGER, department BIGINT[] NOT NULL DEFAULT '{}', main_department BIGINT,
 visibility TEXT NOT NULL DEFAULT 'visible' CHECK(visibility IN ('visible','not_seen')),
 verification TEXT NOT NULL DEFAULT 'unverified' CHECK(verification IN ('verified','unavailable','unverified')),
 checked_at TIMESTAMPTZ, snapshot_id UUID
);
ALTER TABLE accounts ADD FOREIGN KEY(wecom_member_id) REFERENCES wecom_members(id);

-- Customer groups and their members form a complete snapshot, scoped to the directory credential generation.
CREATE TABLE wecom_asset_sync_state (
 dataset TEXT PRIMARY KEY CHECK(dataset IN ('groups')), current_run_id UUID,
 last_success_at TIMESTAMPTZ, published_generation BIGINT, freshness TEXT NOT NULL DEFAULT 'unverified',
 scope JSONB NOT NULL DEFAULT '[]', scope_changed BOOLEAN NOT NULL DEFAULT FALSE
);
INSERT INTO wecom_asset_sync_state(dataset) VALUES('groups');
CREATE TABLE wecom_asset_sync_runs (
 id UUID PRIMARY KEY, dataset TEXT NOT NULL REFERENCES wecom_asset_sync_state(dataset), actor_id UUID REFERENCES accounts(id),
 generation BIGINT NOT NULL, state TEXT NOT NULL DEFAULT 'queued' CHECK(state IN ('queued','running','succeeded','failed')),
 stage TEXT NOT NULL DEFAULT 'queued', created_at TIMESTAMPTZ NOT NULL DEFAULT now(), started_at TIMESTAMPTZ, finished_at TIMESTAMPTZ,
 lease_id UUID, lease_until TIMESTAMPTZ, processed BIGINT NOT NULL DEFAULT 0, pages BIGINT NOT NULL DEFAULT 0,
 total BIGINT NOT NULL DEFAULT 0, error_code TEXT, complete BOOLEAN NOT NULL DEFAULT FALSE,
 scope JSONB NOT NULL DEFAULT '[]', scope_changed BOOLEAN NOT NULL DEFAULT FALSE
);
CREATE UNIQUE INDEX wecom_assets_one_active ON wecom_asset_sync_runs(dataset) WHERE state IN ('queued','running');
CREATE TABLE wecom_asset_sync_items (
 run_id UUID NOT NULL REFERENCES wecom_asset_sync_runs(id) ON DELETE CASCADE,
 kind TEXT NOT NULL, key TEXT NOT NULL, data JSONB NOT NULL, PRIMARY KEY(run_id,kind,key)
);
-- Normalized minimal payloads; identity and relation keys are indexed separately from optional official fields.
CREATE TABLE wecom_asset_objects (
 id UUID PRIMARY KEY, kind TEXT NOT NULL CHECK(kind IN ('groups','members')),
 corp_id TEXT NOT NULL, key TEXT NOT NULL, parent_key TEXT, member_key TEXT,
 member_type INTEGER CHECK(member_type IN (1,2)), data JSONB NOT NULL,
 last_seen_run_id UUID, checked_at TIMESTAMPTZ NOT NULL, credential_generation BIGINT NOT NULL,
 visibility TEXT NOT NULL CHECK(visibility IN ('visible','not_seen')),
 verification TEXT NOT NULL CHECK(verification IN ('verified','unavailable','unverified')),
 source TEXT NOT NULL DEFAULT 'snapshot' CHECK(source IN ('snapshot','point')),
 UNIQUE(corp_id,kind,key)
);
CREATE INDEX wecom_assets_parent ON wecom_asset_objects(kind,parent_key);
CREATE INDEX wecom_assets_member ON wecom_asset_objects(kind,member_key);
CREATE VIEW wecom_group_directory AS SELECT * FROM wecom_asset_objects WHERE kind='groups';
CREATE VIEW wecom_group_directory_members AS SELECT o.*,g.id AS directory_group_id,m.id AS member_id FROM wecom_asset_objects o LEFT JOIN wecom_asset_objects g ON g.kind='groups' AND g.corp_id=o.corp_id AND g.key=o.parent_key LEFT JOIN wecom_members m ON o.member_type=1 AND m.userid_key=o.member_key WHERE o.kind='members';
ALTER TABLE wecom_groups ADD COLUMN directory_group_id UUID UNIQUE REFERENCES wecom_asset_objects(id);
-- The polymorphic directory reference must always denote this exact official group.
CREATE OR REPLACE FUNCTION check_wecom_directory_group() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.directory_group_id IS NOT NULL AND NOT EXISTS (
  SELECT 1 FROM wecom_asset_objects WHERE id=NEW.directory_group_id AND kind='groups' AND corp_id=NEW.corp_id AND key=NEW.chat_id
 ) THEN RAISE EXCEPTION 'directory group identity mismatch' USING ERRCODE='23514'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER wecom_group_directory_identity BEFORE INSERT OR UPDATE ON wecom_groups FOR EACH ROW EXECUTE FUNCTION check_wecom_directory_group();
ALTER TABLE wecom_asset_sync_runs ADD COLUMN failed_owner TEXT;
CREATE TABLE wecom_asset_owner_exclusions (
 owner_key TEXT PRIMARY KEY, generation BIGINT NOT NULL, actor_id UUID NOT NULL REFERENCES accounts(id),
 reason TEXT NOT NULL, confirmed_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
-- Deferred parent identity FK allows publishing objects and relations in the same transaction.
ALTER TABLE wecom_asset_objects ADD COLUMN parent_kind TEXT GENERATED ALWAYS AS (
 CASE kind WHEN 'members' THEN 'groups' ELSE NULL END
) STORED;
ALTER TABLE wecom_asset_objects ADD CONSTRAINT wecom_asset_parent_identity
 FOREIGN KEY(corp_id,parent_kind,parent_key) REFERENCES wecom_asset_objects(corp_id,kind,key) DEFERRABLE INITIALLY DEFERRED;
ALTER TABLE wecom_asset_objects ADD CHECK (kind<>'members' OR (parent_key IS NOT NULL AND member_key IS NOT NULL));
ALTER TABLE wecom_asset_objects ADD CHECK (kind<>'members' OR member_type IS NOT NULL);
ALTER TABLE wecom_asset_objects ADD CHECK (credential_generation>0);
CREATE UNIQUE INDEX wecom_group_member_identity ON wecom_asset_objects(corp_id,parent_key,member_type,member_key) WHERE kind='members';

-- Public aliases and historical business records
-- Integer aliases preserve old mini-app routes without constraining new UUID entities.
-- A table-only development reset can leave this formerly standalone sequence.
-- Reuse its counter, then make its lifetime follow the column that uses it.
CREATE SEQUENCE IF NOT EXISTS legacy_public_id START 1000000000000;
CREATE TABLE legacy_aliases(kind TEXT NOT NULL,target_id UUID NOT NULL,public_id BIGINT NOT NULL DEFAULT nextval('legacy_public_id'),PRIMARY KEY(kind,target_id),UNIQUE(kind,public_id));
ALTER SEQUENCE legacy_public_id OWNED BY legacy_aliases.public_id;
CREATE TABLE historical_delivery_events(id UUID PRIMARY KEY,school_id UUID NOT NULL REFERENCES schools(id),post_id UUID REFERENCES posts(id),source_id TEXT NOT NULL UNIQUE,evidence JSONB NOT NULL,created_at TIMESTAMPTZ NOT NULL);
CREATE TABLE comment_reactions(school_id UUID NOT NULL REFERENCES schools(id),comment_id UUID NOT NULL REFERENCES comments(id),account_id UUID NOT NULL REFERENCES accounts(id),PRIMARY KEY(comment_id,account_id));
CREATE TABLE media_access_tokens(token_hash TEXT PRIMARY KEY,school_id UUID NOT NULL REFERENCES schools(id),asset_id UUID NOT NULL REFERENCES media_assets(id),expires_at TIMESTAMPTZ NOT NULL);
CREATE INDEX media_access_expiry ON media_access_tokens(expires_at);
CREATE INDEX notification_owner ON notifications(school_id,account_id,created_at DESC);
CREATE INDEX cases_school ON cases(school_id,state,created_at DESC);

-- History lookup indexes
-- Related-history checks must remain efficient as schools accumulate drafts.
CREATE INDEX delivery_drafts_group ON delivery_drafts(school_id,group_id);
CREATE INDEX delivery_draft_posts_post ON delivery_draft_posts(post_id,draft_id);
