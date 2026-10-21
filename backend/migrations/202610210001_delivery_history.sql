-- Related-history checks must remain efficient as schools accumulate drafts.
CREATE INDEX delivery_drafts_group ON delivery_drafts(school_id,group_id);
CREATE INDEX delivery_draft_posts_post ON delivery_draft_posts(post_id,draft_id);
