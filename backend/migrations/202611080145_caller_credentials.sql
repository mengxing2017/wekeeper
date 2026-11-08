-- Caller identity is internal to credentials, not a separately managed platform.
-- Preserve disabled callers as disabled credentials before removing the extra state.
UPDATE source_credentials c SET status='disabled', revision=c.revision+1, updated_at=now()
FROM source_platforms p WHERE c.source_id=p.source_id AND p.status='disabled' AND c.status='active';
ALTER TABLE source_platforms RENAME TO source_callers;
ALTER TABLE source_callers DROP COLUMN origins, DROP COLUMN path_prefix, DROP COLUMN name, DROP COLUMN status;
COMMENT ON TABLE source_callers IS 'Internal identity and authorization revision; created atomically with credentials and retained across key rotation';
