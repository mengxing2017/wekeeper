-- The target business group name is the only executable locator.
-- Preserve immutable plans, candidate scopes, signatures and result history.
-- Old unissued plans are compared with current candidates before any grant;
-- exhausted obsolete scopes terminate without changing the confirmed intent.
ALTER TABLE mobile_device_target_bindings DROP COLUMN search_text_override;
