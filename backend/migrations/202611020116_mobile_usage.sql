-- Business authorization is independent of product release records. No approval is seeded.
ALTER TABLE mobile_v9_baselines ALTER COLUMN device_id DROP NOT NULL;
ALTER TABLE mobile_v9_baselines ADD COLUMN external_source jsonb;
ALTER TABLE mobile_v9_baselines ADD CONSTRAINT baseline_source CHECK ((device_id IS NOT NULL AND external_source IS NULL) OR (device_id IS NULL AND external_source IS NOT NULL));
CREATE TABLE mobile_capability_deliveries (
 release_id uuid PRIMARY KEY REFERENCES mobile_v9_adapter_releases(id),
 acceptance_id uuid NOT NULL REFERENCES mobile_v9_acceptance_runs(id),
 supported_apks text[] NOT NULL CHECK(cardinality(supported_apks)>0),
 policy jsonb NOT NULL, provenance jsonb NOT NULL, imported_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE mobile_usage_previews (
 id uuid PRIMARY KEY, school_id uuid NOT NULL REFERENCES schools(id), actor_id uuid NOT NULL REFERENCES accounts(id),
 body jsonb NOT NULL, digest text NOT NULL, expires_at timestamptz NOT NULL DEFAULT clock_timestamp()+interval '10 minutes'
);
CREATE TABLE mobile_usages (
 id uuid PRIMARY KEY, school_id uuid NOT NULL REFERENCES schools(id), actor_id uuid NOT NULL REFERENCES accounts(id),
 approval text NOT NULL CHECK(approval IN ('pending','approved','declined')),
 state text NOT NULL DEFAULT 'active' CHECK(state IN ('active','paused','revoked')),
 revision bigint NOT NULL DEFAULT 1, body jsonb NOT NULL, confirmed_until timestamptz NOT NULL,
 expires_at timestamptz NOT NULL, created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 budget_root uuid NOT NULL DEFAULT gen_random_uuid(), approved_by uuid REFERENCES accounts(id), supersedes uuid REFERENCES mobile_usages(id)
);
CREATE TABLE mobile_usage_events (
 id uuid PRIMARY KEY, usage_id uuid NOT NULL REFERENCES mobile_usages(id), actor_id uuid NOT NULL REFERENCES accounts(id),
 action text NOT NULL, body jsonb NOT NULL, created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE mobile_usage_scopes (
 scope_id uuid PRIMARY KEY REFERENCES mobile_v9_scope_grants(id), resource_snapshot jsonb, usage_id uuid NOT NULL REFERENCES mobile_usages(id),
 release_id uuid NOT NULL REFERENCES mobile_v9_adapter_releases(id), target_id uuid REFERENCES mobile_targets(id),
 device_id uuid REFERENCES execution_devices(id), UNIQUE(usage_id,release_id,target_id,device_id)
);
CREATE TABLE mobile_usage_jobs (
 usage_id uuid PRIMARY KEY REFERENCES mobile_usages(id), state text NOT NULL DEFAULT 'pending',
 reason text, updated_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE mobile_usage_issues (
 id uuid PRIMARY KEY, usage_id uuid NOT NULL REFERENCES mobile_usages(id), actor_id uuid NOT NULL REFERENCES accounts(id),
 description text NOT NULL, context jsonb NOT NULL, revision bigint NOT NULL DEFAULT 1, response text NOT NULL DEFAULT '', state text NOT NULL DEFAULT 'submitted' CHECK(state IN ('submitted','investigating','resolved','declined')), created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE INDEX mobile_usages_school ON mobile_usages(school_id,created_at DESC);
CREATE TRIGGER mobile_usage_events_immutable BEFORE UPDATE OR DELETE ON mobile_usage_events FOR EACH ROW EXECUTE FUNCTION mobile_capability_evidence_immutable();
CREATE TRIGGER mobile_capability_deliveries_immutable BEFORE UPDATE OR DELETE ON mobile_capability_deliveries FOR EACH ROW EXECUTE FUNCTION mobile_capability_evidence_immutable();

CREATE INDEX mobile_usages_budget ON mobile_usages(budget_root);

CREATE TABLE mobile_usage_confirmations (
 preview_id uuid PRIMARY KEY REFERENCES mobile_usage_previews(id), usage_id uuid NOT NULL REFERENCES mobile_usages(id)
);
CREATE TRIGGER mobile_usage_confirmations_immutable BEFORE UPDATE OR DELETE ON mobile_usage_confirmations FOR EACH ROW EXECUTE FUNCTION mobile_capability_evidence_immutable();
