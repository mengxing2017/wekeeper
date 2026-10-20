-- Run after applying a reviewed batch. Compare with report.json by school and status.
SELECT school_id,audit_status,visibility,count(*) FROM posts GROUP BY 1,2,3 ORDER BY 1,2,3;
SELECT school_id,audit_status,deleted,count(*) FROM comments GROUP BY 1,2,3 ORDER BY 1,2,3;
SELECT source_table,count(*) FROM legacy_id_map GROUP BY 1 ORDER BY 1;
SELECT id,source_hash,summary,created_at FROM migration_runs ORDER BY created_at;
SELECT count(*) AS invalid_comment_parent FROM comments c JOIN comments p ON p.id=c.parent_id WHERE c.school_id<>p.school_id OR c.post_id<>p.post_id;
SELECT count(*) AS invalid_draft_scope FROM delivery_draft_posts p JOIN delivery_drafts d ON d.id=p.draft_id WHERE p.school_id<>d.school_id;
