-- Business intents and confirmations. No manufactured delivery acceptance.
CREATE TABLE product_previews (
 id uuid PRIMARY KEY, actor_id uuid NOT NULL REFERENCES accounts(id), school_id uuid REFERENCES schools(id),
 kind text NOT NULL, payload jsonb NOT NULL, dependencies jsonb NOT NULL DEFAULT '{}', digest text NOT NULL,
 created_at timestamptz NOT NULL DEFAULT now(), expires_at timestamptz NOT NULL DEFAULT now()+interval '15 minutes',
 result jsonb, confirmed_at timestamptz, CHECK ((result IS NULL)=(confirmed_at IS NULL))
);
CREATE INDEX product_previews_expiry ON product_previews(expires_at) WHERE result IS NULL;
CREATE TABLE product_requests (
 id uuid PRIMARY KEY, school_id uuid NOT NULL REFERENCES schools(id), kind text NOT NULL CHECK(kind IN ('channel','official_usage','broadcast_issue')),
 actor_id uuid NOT NULL REFERENCES accounts(id), context_key text NOT NULL, payload jsonb NOT NULL,
 state text NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','investigating','waiting_user','approved','declined','resolved','revoked')),
 owner_id uuid REFERENCES accounts(id), response text NOT NULL DEFAULT '', revision bigint NOT NULL DEFAULT 1,
 created_at timestamptz NOT NULL DEFAULT now(), updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX product_requests_open ON product_requests(school_id,kind,context_key) WHERE state IN ('pending','investigating','waiting_user');
CREATE TABLE product_request_events (
 id uuid PRIMARY KEY, request_id uuid NOT NULL REFERENCES product_requests(id), actor_id uuid NOT NULL REFERENCES accounts(id),
 state text NOT NULL, response text NOT NULL, created_at timestamptz NOT NULL DEFAULT now()
);
CREATE FUNCTION product_events_immutable() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'immutable business event'; END $$;
CREATE TRIGGER product_request_events_immutable BEFORE UPDATE OR DELETE ON product_request_events FOR EACH ROW EXECUTE FUNCTION product_events_immutable();
CREATE TABLE official_channel_deliveries (
 id uuid PRIMARY KEY, corp_id text NOT NULL, generation bigint NOT NULL, credential_scope text NOT NULL,
 state text NOT NULL CHECK(state IN ('available','paused','error')), checked_at timestamptz NOT NULL,
 valid_until timestamptz NOT NULL, source text NOT NULL, CHECK(valid_until>checked_at)
);
ALTER TABLE broadcast_tasks ALTER COLUMN corp_id DROP NOT NULL;
ALTER TABLE broadcast_tasks ALTER COLUMN generation DROP NOT NULL;
ALTER TABLE broadcast_tasks ADD COLUMN draft_input jsonb NOT NULL;
ALTER TABLE broadcast_tasks DROP COLUMN source_draft_id;
ALTER TABLE broadcast_tasks ADD CONSTRAINT broadcast_resolved_scope CHECK((corp_id IS NULL)=(generation IS NULL));
ALTER TABLE broadcast_tasks ADD CONSTRAINT broadcast_confirmed_scope CHECK(state<>'confirmed' OR (corp_id IS NOT NULL AND generation IS NOT NULL));
CREATE TABLE broadcast_preview_intents (
 school_id uuid NOT NULL REFERENCES schools(id), actor_id uuid NOT NULL REFERENCES accounts(id), request_key text NOT NULL,
 input_hash text NOT NULL, task_id uuid NOT NULL REFERENCES broadcast_tasks(id), response jsonb NOT NULL,
 PRIMARY KEY(school_id,actor_id,request_key)
);
ALTER TABLE templates ADD COLUMN revision bigint NOT NULL DEFAULT 1;
ALTER TABLE templates ADD COLUMN draft_body text NOT NULL DEFAULT '';
ALTER TABLE templates ADD COLUMN current_version_id uuid;
ALTER TABLE template_versions ADD COLUMN published_by uuid REFERENCES accounts(id);
ALTER TABLE template_versions ADD COLUMN published_at timestamptz;
ALTER TABLE template_versions ADD COLUMN change_note text NOT NULL DEFAULT '';
ALTER TABLE templates ADD FOREIGN KEY(school_id,current_version_id) REFERENCES template_versions(school_id,id);
CREATE TABLE operational_media (
 id uuid PRIMARY KEY, school_id uuid REFERENCES schools(id), owner_id uuid NOT NULL REFERENCES accounts(id),
 mime_type text NOT NULL, size_bytes bigint NOT NULL CHECK(size_bytes BETWEEN 1 AND 5242880),
 width integer NOT NULL CHECK(width BETWEEN 1 AND 8192), height integer NOT NULL CHECK(height BETWEEN 1 AND 8192),
 content bytea NOT NULL, created_at timestamptz NOT NULL DEFAULT now(), CHECK(width::bigint*height<=24000000)
);
ALTER TABLE school_resources DROP COLUMN url;
ALTER TABLE school_resources ADD COLUMN image_ref jsonb;
ALTER TABLE school_resources ADD COLUMN action jsonb NOT NULL DEFAULT '{"kind":"none"}';
CREATE TABLE resource_publications (
 id uuid PRIMARY KEY, resource_id uuid NOT NULL REFERENCES school_resources(id), school_id uuid REFERENCES schools(id),
 actor_id uuid NOT NULL REFERENCES accounts(id), snapshot jsonb NOT NULL, created_at timestamptz NOT NULL DEFAULT now()
);
CREATE TRIGGER resource_publications_immutable BEFORE UPDATE OR DELETE ON resource_publications FOR EACH ROW EXECUTE FUNCTION product_events_immutable();

ALTER TABLE broadcast_previews ADD COLUMN actor_id uuid REFERENCES accounts(id);
ALTER TABLE distribution_resource_policies ADD COLUMN id uuid NOT NULL DEFAULT gen_random_uuid() UNIQUE;

CREATE TABLE product_decision_replays (
 request_id uuid NOT NULL REFERENCES product_requests(id), previous_revision bigint NOT NULL,
 actor_id uuid NOT NULL REFERENCES accounts(id), input_hash text NOT NULL, result jsonb NOT NULL,
 PRIMARY KEY(request_id,previous_revision)
);

CREATE TABLE broadcast_evidence_queries (
 actor_id uuid NOT NULL REFERENCES accounts(id), task_id uuid NOT NULL REFERENCES broadcast_tasks(id),
 batch_id uuid NOT NULL REFERENCES broadcast_batches(id), queried_at timestamptz NOT NULL DEFAULT now(),
 PRIMARY KEY(actor_id, task_id, batch_id)
);

-- Every business scope is created from an approved usage and a frozen resource snapshot.
ALTER TABLE mobile_usage_scopes ALTER COLUMN resource_snapshot SET NOT NULL;
