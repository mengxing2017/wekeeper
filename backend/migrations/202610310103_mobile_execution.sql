-- Database baseline: phone execution and shared send ledger. Depends on core business.
-- Phone execution remains disabled until identities and historical holds are reviewed.
CREATE TABLE execution_devices (
 id uuid PRIMARY KEY, name text NOT NULL, enabled boolean NOT NULL DEFAULT false,
 revision bigint NOT NULL DEFAULT 1, credential_generation bigint NOT NULL DEFAULT 0,
 client_instance_id text, last_seen_at timestamptz, created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 execution_protocol_version integer NOT NULL DEFAULT 1 CHECK(execution_protocol_version IN (1,2,3,4,5)),
 supported_protocol_versions integer[] NOT NULL DEFAULT ARRAY[1]
);
CREATE TABLE device_credentials (
 token_hash text PRIMARY KEY, device_id uuid NOT NULL REFERENCES execution_devices(id),
 generation bigint NOT NULL, expires_at timestamptz NOT NULL, revoked_at timestamptz
);
CREATE TABLE device_enrollments (
 id uuid PRIMARY KEY, device_id uuid NOT NULL REFERENCES execution_devices(id), code_hash text NOT NULL UNIQUE,
 expires_at timestamptz NOT NULL, consumed_at timestamptz
);
CREATE TABLE sending_accounts (
 id uuid PRIMARY KEY, client_kind text NOT NULL CHECK(client_kind IN ('wechat','wecom')),
 identity_namespace text, external_key text, responsible_actor_id uuid NOT NULL REFERENCES accounts(id),
 identity_evidence_ref text NOT NULL, verified_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 enabled boolean NOT NULL DEFAULT true, revision bigint NOT NULL DEFAULT 1,
 daily_limit integer NOT NULL DEFAULT 30 CHECK(daily_limit>=0),
 UNIQUE(client_kind,identity_namespace,external_key)
);
CREATE TABLE device_account_bindings (
 device_id uuid PRIMARY KEY REFERENCES execution_devices(id), sender_account_id uuid NOT NULL UNIQUE REFERENCES sending_accounts(id),
 revision bigint NOT NULL, identity_evidence_ref text NOT NULL
);
CREATE TABLE delivery_groups (
 id uuid PRIMARY KEY, school_id uuid NOT NULL REFERENCES schools(id), name text NOT NULL,
 client_kind text NOT NULL CHECK(client_kind IN ('wechat','wecom')), identity_namespace text,
 external_key text, identity_evidence_ref text NOT NULL, wecom_group_id uuid UNIQUE,
 revision bigint NOT NULL DEFAULT 1, enabled boolean NOT NULL DEFAULT true,
 daily_limit integer NOT NULL DEFAULT 10 CHECK(daily_limit>=0),
 UNIQUE(client_kind,identity_namespace,external_key), UNIQUE(school_id,id),
 FOREIGN KEY(school_id,wecom_group_id) REFERENCES wecom_groups(school_id,id)
);
CREATE TABLE sender_group_bindings (
 school_id uuid NOT NULL, group_id uuid NOT NULL, sender_account_id uuid NOT NULL REFERENCES sending_accounts(id),
 revision bigint NOT NULL DEFAULT 1, evidence_ref text NOT NULL,
 PRIMARY KEY(group_id,sender_account_id), FOREIGN KEY(school_id,group_id) REFERENCES delivery_groups(school_id,id)
);
CREATE TABLE mobile_delivery_settings (
 school_id uuid PRIMARY KEY REFERENCES schools(id), enabled boolean NOT NULL DEFAULT false,
 history_reviewed boolean NOT NULL DEFAULT false, revision bigint NOT NULL DEFAULT 1,
 approved_client_version text, approved_script_version text, approved_baseline_version text,
 evidence_ref text NOT NULL
);
CREATE TABLE mobile_delivery_tasks (
 id uuid PRIMARY KEY, school_id uuid NOT NULL REFERENCES schools(id), creator_id uuid NOT NULL REFERENCES accounts(id),
 confirmed_by uuid REFERENCES accounts(id), revision bigint NOT NULL DEFAULT 1,
 state text NOT NULL DEFAULT 'draft' CHECK(state IN ('draft','confirmed','completed','stopped')),
 activity_id text NOT NULL, review_evidence_ref text, created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 UNIQUE(school_id,id)
);
CREATE TABLE mobile_delivery_items (
 id uuid PRIMARY KEY, school_id uuid NOT NULL, task_id uuid NOT NULL, group_id uuid NOT NULL,
 sender_account_id uuid NOT NULL REFERENCES sending_accounts(id), content text NOT NULL CHECK(octet_length(content) BETWEEN 1 AND 4000),
 content_digest text NOT NULL, source jsonb NOT NULL, snapshot jsonb NOT NULL, snapshot_digest text NOT NULL,
 revision bigint NOT NULL DEFAULT 1, fence bigint NOT NULL DEFAULT 0,
 state text NOT NULL DEFAULT 'queued' CHECK(state IN ('queued','claimed','prepared','permit_issued','ui_observed_sent','unknown','manually_sent','verified_not_sent','cancelled')),
 observed_at timestamptz, created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 FOREIGN KEY(school_id,task_id) REFERENCES mobile_delivery_tasks(school_id,id),
 FOREIGN KEY(school_id,group_id) REFERENCES delivery_groups(school_id,id), UNIQUE(school_id,id), UNIQUE(task_id,group_id)
);
CREATE TABLE mobile_execution_claims (
 id uuid PRIMARY KEY, item_id uuid NOT NULL REFERENCES mobile_delivery_items(id), device_id uuid NOT NULL REFERENCES execution_devices(id),
 generation bigint NOT NULL, binding_revision bigint NOT NULL, fence bigint NOT NULL, lease_until timestamptz NOT NULL,
 state text NOT NULL CHECK(state IN ('claimed','prepared','permit_issued','closed','expired')),
 preparation_id uuid, preparation_settings_revision bigint, created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 protocol_version integer NOT NULL DEFAULT 1 CHECK(protocol_version IN (1,2,3,4,5)), requirements_snapshot jsonb
);
CREATE UNIQUE INDEX mobile_active_device_claim ON mobile_execution_claims(device_id) WHERE state IN ('claimed','prepared','permit_issued');
CREATE UNIQUE INDEX mobile_active_item_claim ON mobile_execution_claims(item_id) WHERE state IN ('claimed','prepared','permit_issued');
CREATE TABLE mobile_send_permits (
 id uuid PRIMARY KEY, claim_id uuid NOT NULL UNIQUE REFERENCES mobile_execution_claims(id),
 item_id uuid NOT NULL UNIQUE REFERENCES mobile_delivery_items(id), invocation_id uuid NOT NULL UNIQUE,
 token_hash text NOT NULL, not_before timestamptz NOT NULL DEFAULT clock_timestamp(), execute_before timestamptz NOT NULL,
 state text NOT NULL DEFAULT 'issued' CHECK(state IN ('issued','ui_observed_sent','unknown','manually_sent','verified_not_sent'))
);
CREATE TABLE delivery_usage (
 id uuid PRIMARY KEY, source_channel text NOT NULL CHECK(source_channel IN ('mobile','official','legacy')),
 source_id uuid NOT NULL, school_id uuid NOT NULL REFERENCES schools(id), group_id uuid NOT NULL REFERENCES delivery_groups(id),
 sender_account_id uuid REFERENCES sending_accounts(id), content_digest text NOT NULL,
 state text NOT NULL CHECK(state IN ('pending','sent','unknown','not_sent')),
 observed_at timestamptz NOT NULL DEFAULT clock_timestamp(), evidence_kind text NOT NULL,
 UNIQUE(source_channel,source_id)
);
CREATE INDEX delivery_usage_group ON delivery_usage(group_id,observed_at);
CREATE INDEX delivery_usage_account ON delivery_usage(sender_account_id,observed_at);
CREATE INDEX delivery_usage_content ON delivery_usage(content_digest);
CREATE TABLE delivery_dedup_reservations (
 item_id uuid PRIMARY KEY REFERENCES mobile_delivery_items(id), school_id uuid NOT NULL REFERENCES schools(id),
 group_id uuid NOT NULL REFERENCES delivery_groups(id), content_digest text NOT NULL,
 active boolean NOT NULL DEFAULT true
);
CREATE INDEX delivery_dedup_school ON delivery_dedup_reservations(school_id,content_digest) WHERE active;
CREATE TABLE delivery_scheduler_cursors (school_id uuid PRIMARY KEY REFERENCES schools(id),last_group_id uuid NOT NULL);
CREATE TABLE mobile_delivery_operations (
 principal text NOT NULL, operation text NOT NULL, idempotency_key text NOT NULL,
 request_hash text NOT NULL, response jsonb NOT NULL,
 PRIMARY KEY(principal,operation,idempotency_key), created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE mobile_delivery_events (
 id uuid PRIMARY KEY, school_id uuid REFERENCES schools(id), task_id uuid REFERENCES mobile_delivery_tasks(id),
 actor_id uuid REFERENCES accounts(id), device_id uuid REFERENCES execution_devices(id),
 action text NOT NULL, detail jsonb NOT NULL DEFAULT '{}', created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE device_request_windows (
 device_id uuid PRIMARY KEY REFERENCES execution_devices(id), requests integer NOT NULL, reset_at timestamptz NOT NULL
);
CREATE TABLE device_pairing_windows (
 device_id uuid PRIMARY KEY REFERENCES execution_devices(id), requests integer NOT NULL, reset_at timestamptz NOT NULL
);

CREATE UNIQUE INDEX mobile_result_event_id ON mobile_delivery_events((detail->>'event_id')) WHERE action='device_result';
CREATE INDEX mobile_task_queue ON mobile_delivery_items(sender_account_id,group_id,created_at,id) WHERE state='queued';
CREATE INDEX mobile_item_task ON mobile_delivery_items(task_id,state);
ALTER TABLE delivery_usage ADD FOREIGN KEY(school_id,group_id) REFERENCES delivery_groups(school_id,id);
ALTER TABLE delivery_dedup_reservations ADD FOREIGN KEY(school_id,group_id) REFERENCES delivery_groups(school_id,id);

ALTER TABLE delivery_usage ADD COLUMN responsible_actor_id uuid REFERENCES accounts(id);

CREATE UNIQUE INDEX mobile_group_content_assignment ON delivery_dedup_reservations(group_id,content_digest) WHERE active;

-- Visible identities and strategies
-- External-system identifiers are never required by visible UI execution.
ALTER TABLE sending_accounts
 ADD COLUMN identity_mode text NOT NULL DEFAULT 'legacy_external' CHECK(identity_mode IN ('legacy_external','ui_visible')),
 ADD COLUMN visible_profile jsonb,
 ADD COLUMN profile_revision bigint NOT NULL DEFAULT 1 CHECK(profile_revision > 0),
 ADD COLUMN verification_state text NOT NULL DEFAULT 'pending' CHECK(verification_state IN ('pending','reviewed')),
 ADD CONSTRAINT sending_identity_mode CHECK (
  (identity_mode='legacy_external' AND identity_namespace IS NOT NULL AND external_key IS NOT NULL AND visible_profile IS NULL)
  OR (identity_mode='ui_visible' AND client_kind='wecom' AND identity_namespace IS NULL AND external_key IS NULL AND visible_profile IS NOT NULL AND jsonb_typeof(visible_profile)='object'));
ALTER TABLE delivery_groups
 ADD COLUMN identity_mode text NOT NULL DEFAULT 'legacy_external' CHECK(identity_mode IN ('legacy_external','ui_visible')),
 ADD COLUMN visible_profile jsonb,
 ADD COLUMN profile_revision bigint NOT NULL DEFAULT 1 CHECK(profile_revision > 0),
 ADD COLUMN verification_state text NOT NULL DEFAULT 'pending' CHECK(verification_state IN ('pending','reviewed')),
 ADD CONSTRAINT group_identity_mode CHECK (
  (identity_mode='legacy_external' AND identity_namespace IS NOT NULL AND external_key IS NOT NULL AND visible_profile IS NULL)
  OR (identity_mode='ui_visible' AND client_kind='wecom' AND identity_namespace IS NULL AND external_key IS NULL AND visible_profile IS NOT NULL AND jsonb_typeof(visible_profile)='object'));
CREATE UNIQUE INDEX visible_account_profile ON sending_accounts ((visible_profile->>'company_name'),(visible_profile->>'account_name')) WHERE identity_mode='ui_visible';
-- Conservative canonical target per school; indistinguishable same-name groups are unsupported.
CREATE UNIQUE INDEX visible_group_name ON delivery_groups(school_id,name) WHERE identity_mode='ui_visible';
CREATE TABLE mobile_delivery_strategies (
 id uuid PRIMARY KEY, school_id uuid NOT NULL REFERENCES schools(id), name text NOT NULL,
 sender_account_id uuid NOT NULL REFERENCES sending_accounts(id), group_id uuid,
 device_id uuid NOT NULL REFERENCES execution_devices(id), text_content text NOT NULL CHECK(octet_length(text_content) BETWEEN 1 AND 4000),
 trigger_mode text NOT NULL DEFAULT 'manual_once' CHECK(trigger_mode='manual_once'),
 enabled boolean NOT NULL DEFAULT false, revision bigint NOT NULL DEFAULT 1,
 created_by uuid NOT NULL REFERENCES accounts(id), created_at timestamptz NOT NULL DEFAULT clock_timestamp(), updated_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 UNIQUE(school_id,id), FOREIGN KEY(school_id,group_id) REFERENCES delivery_groups(school_id,id)
);
CREATE TABLE mobile_delivery_strategy_runs (
 id uuid PRIMARY KEY, school_id uuid NOT NULL, strategy_id uuid, strategy_revision bigint NOT NULL,
 task_id uuid NOT NULL UNIQUE, device_id uuid NOT NULL REFERENCES execution_devices(id), device_binding_revision bigint NOT NULL,
 strategy_snapshot jsonb NOT NULL, created_by uuid NOT NULL REFERENCES accounts(id), created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 FOREIGN KEY(school_id,strategy_id) REFERENCES mobile_delivery_strategies(school_id,id),
 FOREIGN KEY(school_id,task_id) REFERENCES mobile_delivery_tasks(school_id,id),
 UNIQUE(school_id,id)
);
CREATE INDEX strategy_runs_order ON mobile_delivery_strategy_runs(strategy_id,created_at,id);

-- Reusable rules, ordered groups and shared content ownership
CREATE TABLE client_rule_bundle_versions (
 id uuid PRIMARY KEY, name text NOT NULL, revision bigint NOT NULL DEFAULT 1,
 configuration jsonb NOT NULL, content_digest text NOT NULL, created_by uuid NOT NULL REFERENCES accounts(id),
 created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE client_rule_releases (
 id uuid PRIMARY KEY, configuration_id uuid NOT NULL REFERENCES client_rule_bundle_versions(id),
 school_id uuid NOT NULL REFERENCES schools(id), device_id uuid REFERENCES execution_devices(id),
 revision bigint NOT NULL DEFAULT 1, active boolean NOT NULL DEFAULT true, revoked boolean NOT NULL DEFAULT false,
 evidence_ref text NOT NULL, created_by uuid NOT NULL REFERENCES accounts(id), created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 UNIQUE(school_id,id)
);
CREATE INDEX rule_release_resolution ON client_rule_releases(school_id,device_id) WHERE active AND NOT revoked;
-- Legacy execution defaults live in the database, not hidden Rust/SQL constants.
CREATE TABLE delivery_execution_defaults (
 id boolean PRIMARY KEY DEFAULT true CHECK(id), account_cooldown_ms bigint NOT NULL,
 content_cooldown_ms bigint NOT NULL, quota_window_ms bigint NOT NULL
);
INSERT INTO delivery_execution_defaults VALUES(true,1800000,600000,86400000);
ALTER TABLE mobile_delivery_strategies ADD COLUMN delivery_mode text NOT NULL DEFAULT 'single_group' CHECK(delivery_mode IN ('single_group','ordered_groups')),
 ADD COLUMN targets jsonb NOT NULL DEFAULT '[]',
 ADD COLUMN inter_group_interval_seconds bigint CHECK(inter_group_interval_seconds>=0),
 ADD COLUMN configuration_release_id uuid,
 ADD FOREIGN KEY(school_id,configuration_release_id) REFERENCES client_rule_releases(school_id,id),
 ADD CHECK((delivery_mode='single_group' AND group_id IS NOT NULL AND targets='[]') OR (delivery_mode='ordered_groups' AND group_id IS NULL AND jsonb_array_length(targets)>=2 AND configuration_release_id IS NOT NULL));
CREATE TABLE mobile_delivery_strategy_targets (
 strategy_id uuid NOT NULL, school_id uuid NOT NULL, position integer NOT NULL CHECK(position>0), group_id uuid NOT NULL,
 PRIMARY KEY(strategy_id,position), UNIQUE(strategy_id,group_id),
 FOREIGN KEY(school_id,strategy_id) REFERENCES mobile_delivery_strategies(school_id,id),
 FOREIGN KEY(school_id,group_id) REFERENCES delivery_groups(school_id,id)
);
CREATE TABLE mobile_delivery_run_items (
 item_id uuid PRIMARY KEY, school_id uuid NOT NULL, run_id uuid NOT NULL, position integer NOT NULL CHECK(position>0),
 UNIQUE(run_id,position), FOREIGN KEY(school_id,item_id) REFERENCES mobile_delivery_items(school_id,id),
 FOREIGN KEY(school_id,run_id) REFERENCES mobile_delivery_strategy_runs(school_id,id)
);
CREATE TABLE mobile_delivery_run_progress (
 id uuid PRIMARY KEY REFERENCES mobile_delivery_strategy_runs(id), current_position integer NOT NULL DEFAULT 1 CHECK(current_position>0),
 confirmed_at timestamptz, last_completed_at timestamptz, next_eligible_at timestamptz, pause_reason text,
 configuration_release_id uuid REFERENCES client_rule_releases(id),
 revision bigint NOT NULL DEFAULT 1, execution_revision bigint NOT NULL DEFAULT 1,
 inter_group_interval_ms bigint NOT NULL CHECK(inter_group_interval_ms>=0)
);
CREATE TABLE mobile_delivery_run_execution_revisions (
 id uuid PRIMARY KEY, run_id uuid NOT NULL REFERENCES mobile_delivery_strategy_runs(id), revision bigint NOT NULL,
 configuration_release_id uuid NOT NULL REFERENCES client_rule_releases(id), preview_digest text NOT NULL, previous_progress jsonb NOT NULL,
 evidence_ref text NOT NULL, created_by uuid NOT NULL REFERENCES accounts(id), created_at timestamptz NOT NULL DEFAULT clock_timestamp(), UNIQUE(run_id,revision)
);
CREATE TABLE delivery_content_scope_owners (
 id uuid PRIMARY KEY, school_id uuid NOT NULL REFERENCES schools(id), content_digest text NOT NULL,
 owner_id uuid NOT NULL, active boolean NOT NULL DEFAULT true,
 UNIQUE(id,school_id,content_digest)
);
CREATE UNIQUE INDEX content_scope_assignment ON delivery_content_scope_owners(school_id,content_digest) WHERE active;
ALTER TABLE delivery_dedup_reservations ADD COLUMN content_scope_id uuid NOT NULL,
 ADD FOREIGN KEY(content_scope_id,school_id,content_digest) REFERENCES delivery_content_scope_owners(id,school_id,content_digest);

-- Short device pairing
-- Shared, persistent budgets include invalid guesses and survive failed pairing.
CREATE TABLE device_pairing_attempt_windows (
 scope text PRIMARY KEY,
 requests integer NOT NULL CHECK (requests BETWEEN 1 AND 101),
 reset_at timestamptz NOT NULL
);
CREATE INDEX device_pairing_attempt_windows_reset ON device_pairing_attempt_windows(reset_at);

-- Account profile lookup
-- Keep platform account projections bounded to the requested page's audit history.
CREATE INDEX mobile_account_profile_event_lookup
ON mobile_delivery_events ((detail->>'scope_id'), action, created_at DESC, id DESC)
WHERE action IN ('create_account', 'review_account_profile');
