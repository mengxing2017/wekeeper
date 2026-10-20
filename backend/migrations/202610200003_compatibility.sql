-- Integer aliases preserve old mini-app routes without constraining new UUID entities.
CREATE SEQUENCE legacy_public_id START 1000000000000;
CREATE TABLE legacy_aliases(kind TEXT NOT NULL,target_id UUID NOT NULL,public_id BIGINT NOT NULL DEFAULT nextval('legacy_public_id'),PRIMARY KEY(kind,target_id),UNIQUE(kind,public_id));
CREATE TABLE historical_delivery_events(id UUID PRIMARY KEY,school_id UUID NOT NULL REFERENCES schools(id),post_id UUID REFERENCES posts(id),source_id TEXT NOT NULL UNIQUE,evidence JSONB NOT NULL,created_at TIMESTAMPTZ NOT NULL);
CREATE TABLE comment_reactions(school_id UUID NOT NULL REFERENCES schools(id),comment_id UUID NOT NULL REFERENCES comments(id),account_id UUID NOT NULL REFERENCES accounts(id),PRIMARY KEY(comment_id,account_id));
CREATE TABLE media_access_tokens(token_hash TEXT PRIMARY KEY,school_id UUID NOT NULL REFERENCES schools(id),asset_id UUID NOT NULL REFERENCES media_assets(id),expires_at TIMESTAMPTZ NOT NULL);
CREATE INDEX media_access_expiry ON media_access_tokens(expires_at);
ALTER TABLE accounts ADD COLUMN avatar_url TEXT NOT NULL DEFAULT '';
ALTER TABLE cases ADD COLUMN images JSONB NOT NULL DEFAULT '[]';
CREATE INDEX notification_owner ON notifications(school_id,account_id,created_at DESC);
CREATE INDEX cases_school ON cases(school_id,state,created_at DESC);
ALTER TABLE delivery_attempts ADD COLUMN client_invoked_at TIMESTAMPTZ;
ALTER TABLE school_memberships DROP CONSTRAINT school_memberships_role_check;
ALTER TABLE school_memberships ADD CHECK(role IN ('member','operator','admin','revoked'));
ALTER TABLE school_resources ALTER COLUMN school_id DROP NOT NULL;
ALTER TABLE audit_events ADD COLUMN request_id TEXT;
ALTER TABLE comments ALTER CONSTRAINT comments_parent_id_fkey DEFERRABLE INITIALLY DEFERRED;
