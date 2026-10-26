-- Phone execution is deliberately disabled until identities and historical holds are reviewed.
CREATE TABLE execution_devices (
 id uuid PRIMARY KEY, name text NOT NULL, enabled boolean NOT NULL DEFAULT false,
 revision bigint NOT NULL DEFAULT 1, credential_generation bigint NOT NULL DEFAULT 0,
 client_instance_id text, last_seen_at timestamptz, created_at timestamptz NOT NULL DEFAULT clock_timestamp()
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
 identity_namespace text NOT NULL, external_key text NOT NULL, responsible_actor_id uuid NOT NULL REFERENCES accounts(id),
 identity_evidence_ref text NOT NULL, verified_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 enabled boolean NOT NULL DEFAULT true, revision bigint NOT NULL DEFAULT 1,
 daily_limit integer NOT NULL DEFAULT 30 CHECK(daily_limit BETWEEN 0 AND 50),
 UNIQUE(client_kind,identity_namespace,external_key)
);
CREATE TABLE device_account_bindings (
 device_id uuid PRIMARY KEY REFERENCES execution_devices(id), sender_account_id uuid NOT NULL UNIQUE REFERENCES sending_accounts(id),
 revision bigint NOT NULL, identity_evidence_ref text NOT NULL
);
CREATE TABLE delivery_groups (
 id uuid PRIMARY KEY, school_id uuid NOT NULL REFERENCES schools(id), name text NOT NULL,
 client_kind text NOT NULL CHECK(client_kind IN ('wechat','wecom')), identity_namespace text NOT NULL,
 external_key text NOT NULL, identity_evidence_ref text NOT NULL, wecom_group_id uuid UNIQUE,
 revision bigint NOT NULL DEFAULT 1, enabled boolean NOT NULL DEFAULT true,
 daily_limit integer NOT NULL DEFAULT 10 CHECK(daily_limit BETWEEN 0 AND 30),
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
 approved_client_version text NOT NULL, approved_script_version text NOT NULL, approved_baseline_version text NOT NULL,
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
 preparation_id uuid, preparation_settings_revision bigint, created_at timestamptz NOT NULL DEFAULT clock_timestamp()
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

CREATE UNIQUE INDEX mobile_school_content_assignment ON delivery_dedup_reservations(school_id,content_digest) WHERE active;
CREATE UNIQUE INDEX mobile_group_content_assignment ON delivery_dedup_reservations(group_id,content_digest) WHERE active;
