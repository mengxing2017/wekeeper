-- External-system identifiers are never required by visible UI execution.
ALTER TABLE sending_accounts
 ADD COLUMN identity_mode text NOT NULL DEFAULT 'legacy_external' CHECK(identity_mode IN ('legacy_external','ui_visible')),
 ADD COLUMN visible_profile jsonb,
 ADD COLUMN profile_revision bigint NOT NULL DEFAULT 1 CHECK(profile_revision > 0),
 ADD COLUMN verification_state text NOT NULL DEFAULT 'pending' CHECK(verification_state IN ('pending','reviewed')),
 ALTER COLUMN identity_namespace DROP NOT NULL, ALTER COLUMN external_key DROP NOT NULL,
 ADD CONSTRAINT sending_identity_mode CHECK (
  (identity_mode='legacy_external' AND identity_namespace IS NOT NULL AND external_key IS NOT NULL AND visible_profile IS NULL)
  OR (identity_mode='ui_visible' AND client_kind='wecom' AND identity_namespace IS NULL AND external_key IS NULL AND visible_profile IS NOT NULL AND jsonb_typeof(visible_profile)='object'));
ALTER TABLE delivery_groups
 ADD COLUMN identity_mode text NOT NULL DEFAULT 'legacy_external' CHECK(identity_mode IN ('legacy_external','ui_visible')),
 ADD COLUMN visible_profile jsonb,
 ADD COLUMN profile_revision bigint NOT NULL DEFAULT 1 CHECK(profile_revision > 0),
 ADD COLUMN verification_state text NOT NULL DEFAULT 'pending' CHECK(verification_state IN ('pending','reviewed')),
 ALTER COLUMN identity_namespace DROP NOT NULL, ALTER COLUMN external_key DROP NOT NULL,
 ADD CONSTRAINT group_identity_mode CHECK (
  (identity_mode='legacy_external' AND identity_namespace IS NOT NULL AND external_key IS NOT NULL AND visible_profile IS NULL)
  OR (identity_mode='ui_visible' AND client_kind='wecom' AND identity_namespace IS NULL AND external_key IS NULL AND visible_profile IS NOT NULL AND jsonb_typeof(visible_profile)='object'));
CREATE UNIQUE INDEX visible_account_profile ON sending_accounts ((visible_profile->>'company_name'),(visible_profile->>'account_name')) WHERE identity_mode='ui_visible';
-- Conservative canonical target per school; indistinguishable same-name groups are unsupported.
CREATE UNIQUE INDEX visible_group_name ON delivery_groups(school_id,name) WHERE identity_mode='ui_visible';
CREATE TABLE mobile_delivery_strategies (
 id uuid PRIMARY KEY, school_id uuid NOT NULL REFERENCES schools(id), name text NOT NULL,
 sender_account_id uuid NOT NULL REFERENCES sending_accounts(id), group_id uuid NOT NULL,
 device_id uuid NOT NULL REFERENCES execution_devices(id), text_content text NOT NULL CHECK(octet_length(text_content) BETWEEN 1 AND 4000),
 trigger_mode text NOT NULL DEFAULT 'manual_once' CHECK(trigger_mode='manual_once'),
 enabled boolean NOT NULL DEFAULT false, revision bigint NOT NULL DEFAULT 1,
 created_by uuid NOT NULL REFERENCES accounts(id), created_at timestamptz NOT NULL DEFAULT clock_timestamp(), updated_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 UNIQUE(school_id,id), FOREIGN KEY(school_id,group_id) REFERENCES delivery_groups(school_id,id)
);
CREATE TABLE mobile_delivery_strategy_runs (
 id uuid PRIMARY KEY, school_id uuid NOT NULL, strategy_id uuid NOT NULL, strategy_revision bigint NOT NULL,
 task_id uuid NOT NULL UNIQUE, device_id uuid NOT NULL REFERENCES execution_devices(id), device_binding_revision bigint NOT NULL,
 strategy_snapshot jsonb NOT NULL, created_by uuid NOT NULL REFERENCES accounts(id), created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 FOREIGN KEY(school_id,strategy_id) REFERENCES mobile_delivery_strategies(school_id,id),
 FOREIGN KEY(school_id,task_id) REFERENCES mobile_delivery_tasks(school_id,id)
);
CREATE INDEX strategy_runs_order ON mobile_delivery_strategy_runs(strategy_id,created_at,id);
ALTER TABLE mobile_execution_claims ADD COLUMN protocol_version integer NOT NULL DEFAULT 1 CHECK(protocol_version IN (1,2)), ADD COLUMN requirements_snapshot jsonb;
-- Existing snapshots, invocation IDs, permits and unknown holds are deliberately untouched.

ALTER TABLE execution_devices ADD COLUMN execution_protocol_version integer NOT NULL DEFAULT 1 CHECK(execution_protocol_version IN (1,2));
