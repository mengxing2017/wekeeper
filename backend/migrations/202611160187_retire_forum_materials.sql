-- Forum content and material-based manual drafts are not part of this product.
-- Keep all message snapshots, execution authorizations, results and audit records.
-- Refuse unexpected non-text tasks instead of deleting delivery history.
ALTER TABLE broadcast_tasks DROP CONSTRAINT broadcast_content_source;
ALTER TABLE broadcast_tasks ADD CONSTRAINT broadcast_content_source CHECK (content_source = 'text');
ALTER TABLE broadcast_tasks ALTER COLUMN content_source SET DEFAULT 'text';
ALTER TABLE broadcast_tasks ALTER COLUMN source_namespace SET DEFAULT 'builtin.direct';
ALTER TABLE broadcast_tasks DROP COLUMN template_version_id;

DROP TABLE media_access_tokens;
DROP VIEW distribution_forum_materials;
DROP TABLE broadcast_task_posts;
DROP TABLE broadcast_content_references;
DROP TABLE distribution_materials;
DROP TABLE distribution_source_organizations;
DROP TABLE IF EXISTS distribution_source_events;
ALTER TABLE templates DROP CONSTRAINT templates_current_version_id_fkey;
DROP TABLE template_versions;
DROP TABLE templates;
DELETE FROM distribution_source_turns WHERE source_namespace = 'builtin.forum';
DELETE FROM distribution_sources WHERE namespace = 'builtin.forum';
