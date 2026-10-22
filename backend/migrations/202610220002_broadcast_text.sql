-- Existing tasks retain their frozen content and posts semantics.
ALTER TABLE broadcast_tasks ADD COLUMN content_source TEXT NOT NULL DEFAULT 'posts';
ALTER TABLE broadcast_tasks ALTER COLUMN template_version_id DROP NOT NULL;
ALTER TABLE broadcast_tasks ADD CONSTRAINT broadcast_content_source CHECK (
 (content_source = 'posts' AND template_version_id IS NOT NULL) OR
 (content_source = 'text' AND template_version_id IS NULL)
);
