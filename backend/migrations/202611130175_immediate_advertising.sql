-- Preserve historical text; enforce the limit on new publications without truncation.
ALTER TABLE ad_material_versions ADD CONSTRAINT ad_material_body_140_characters
    CHECK (char_length(body) <= 140) NOT VALID;
ALTER TABLE advertisement_content_versions ADD CONSTRAINT advertisement_body_140_characters
    CHECK (char_length(body) <= 140) NOT VALID;

CREATE TABLE ad_unit_counters (
    platform_id uuid NOT NULL,
    unit_id uuid NOT NULL,
    selected_count bigint NOT NULL DEFAULT 0 CHECK (selected_count >= 0),
    PRIMARY KEY (platform_id, unit_id),
    FOREIGN KEY (platform_id, unit_id) REFERENCES business_units(platform_id, id)
);
CREATE TABLE ad_message_decisions (
    platform_id uuid NOT NULL,
    unit_id uuid NOT NULL,
    message_id uuid NOT NULL,
    original text NOT NULL,
    channel text NOT NULL CHECK (channel IN ('mobile', 'wecom')),
    decision jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (platform_id, unit_id, message_id),
    FOREIGN KEY (platform_id, unit_id) REFERENCES business_units(platform_id, id)
);

-- Existing configuration can no longer place messages in an advertising wait state.
UPDATE source_advertisement_configs
SET config=jsonb_set(jsonb_set(config,'{unavailable_policy}','"without_ad"'),'{auto_recover}','false');
UPDATE source_inbox SET state='waiting',next_attempt_at=now()
WHERE state='waiting' AND last_error IN ('ad_unavailable','ad_frozen_unavailable','ad_configuration_unavailable');
DELETE FROM advertisement_target_blocks;
UPDATE advertisement_recheck_jobs SET complete=true WHERE NOT complete;
