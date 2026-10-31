-- Keep platform account projections bounded to the requested page's audit history.
CREATE INDEX mobile_account_profile_event_lookup
ON mobile_delivery_events ((detail->>'scope_id'), action, created_at DESC, id DESC)
WHERE action IN ('create_account', 'review_account_profile');
