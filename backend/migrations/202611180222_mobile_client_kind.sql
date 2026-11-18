-- Pre-release model. No legacy protocol or implicit group type is supported.
ALTER TABLE mobile_targets ADD COLUMN client_kind text NOT NULL CHECK (client_kind IN ('wecom','wechat'));
CREATE INDEX mobile_targets_client ON mobile_targets(client_kind,created_at,id) WHERE archived_at IS NULL;
ALTER TABLE mobile_v9_task_drafts ADD COLUMN client_kind text NOT NULL CHECK (client_kind IN ('wecom','wechat'));

-- Replace the v1 observation checks while retaining the common fact checks.
DO $$
DECLARE constraint_name text;
BEGIN
    FOR constraint_name IN
        SELECT conname FROM pg_constraint
        WHERE conrelid = 'mobile_device_environments'::regclass AND contype = 'c'
          AND (pg_get_constraintdef(oid) LIKE '%wecom_login%'
               OR pg_get_constraintdef(oid) LIKE '%device_environment/1%')
    LOOP
        EXECUTE format('ALTER TABLE mobile_device_environments DROP CONSTRAINT %I', constraint_name);
    END LOOP;
END $$;

ALTER TABLE mobile_device_environments
    ADD CONSTRAINT mobile_environment_v2_fields CHECK (
        observation ?& ARRAY['schema','observation_id','observed_at','interactive','keyguard_locked','device_locked','keyguard_secure','accessibility','notification','applications']
        AND observation - ARRAY['schema','observation_id','observed_at','interactive','keyguard_locked','device_locked','keyguard_secure','accessibility','notification','applications'] = '{}'::jsonb
    ),
    ADD CONSTRAINT mobile_environment_v2_schema CHECK (observation->>'schema' = 'device_environment/2'),
    ADD CONSTRAINT mobile_environment_applications CHECK (
        jsonb_typeof(observation->'applications') = 'object'
        AND (observation->'applications') ?& ARRAY['wecom','wechat']
        AND (observation->'applications') - ARRAY['wecom','wechat'] = '{}'::jsonb
        AND observation->'applications'->>'wecom' IS NOT NULL
        AND observation->'applications'->>'wechat' IS NOT NULL
        AND observation->'applications'->>'wecom' IN ('logged_in','login_required','unknown')
        AND observation->'applications'->>'wechat' IN ('logged_in','login_required','unknown')
    );
