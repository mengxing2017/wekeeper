-- Explicit pause survives removal of finite deployment observations.
CREATE TABLE official_channel_controls (
 credential_scope text PRIMARY KEY, paused boolean NOT NULL DEFAULT false,
 revision bigint NOT NULL DEFAULT 1, reason text NOT NULL,
 updated_by uuid REFERENCES accounts(id), updated_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
INSERT INTO official_channel_controls(credential_scope,paused,reason)
 SELECT credential_scope,true,'保留原有明确暂停'
 FROM (SELECT DISTINCT ON(credential_scope) credential_scope,state
       FROM official_channel_deliveries ORDER BY credential_scope,checked_at DESC,id DESC) d
 WHERE state='paused';
CREATE TABLE official_channel_health (
 credential_scope text PRIMARY KEY,corp_id text,generation bigint,
 available boolean NOT NULL DEFAULT false, checked_at timestamptz,
 last_success_at timestamptz,last_failure_at timestamptz,error_code text,
 next_probe_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
DROP TABLE official_channel_deliveries;
