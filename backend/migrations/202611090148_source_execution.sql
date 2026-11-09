-- Received content and automatic execution stay entirely inside WeKeeper.
ALTER TABLE source_routes ADD COLUMN authorized_by uuid REFERENCES accounts;
CREATE TABLE source_content_versions (
 source_id uuid NOT NULL REFERENCES source_callers, route_id text NOT NULL, external_content_id text NOT NULL,
 sequence bigint NOT NULL, version text NOT NULL, receipt_id uuid NOT NULL REFERENCES source_receipts,
 withdrawn boolean NOT NULL, updated_at timestamptz NOT NULL DEFAULT now(),
 PRIMARY KEY(source_id,route_id,external_content_id)
);
CREATE TABLE source_execution_tasks (
 receipt_id uuid PRIMARY KEY REFERENCES source_receipts, task_id uuid NOT NULL UNIQUE,
 channel text NOT NULL CHECK(channel IN ('wecom','mobile')), workspace_id uuid NOT NULL,
 authorization_actor_id uuid NOT NULL REFERENCES accounts, execution_actor text NOT NULL DEFAULT 'source-routing',
 config_revision bigint NOT NULL, content_digest text NOT NULL, target_digest text NOT NULL,
 created_at timestamptz NOT NULL DEFAULT now()
);
ALTER TABLE mobile_v9_tasks ADD COLUMN authorization_kind text NOT NULL DEFAULT 'manual' CHECK(authorization_kind IN ('manual','source_route'));
ALTER TABLE broadcast_tasks ADD COLUMN authorization_kind text NOT NULL DEFAULT 'manual' CHECK(authorization_kind IN ('manual','source_route')),
 ADD COLUMN authorized_by uuid REFERENCES accounts;
COMMENT ON COLUMN mobile_v9_tasks.confirmed_by IS 'Authorizing principal; authorization_kind distinguishes human per-task confirmation from explicit source-route authorization';
INSERT INTO distribution_sources(namespace) VALUES('builtin.source-route');
