-- Meter business roots independently of optional execution scope projections.
CREATE TABLE mobile_usage_invocation_budgets (
 invocation_id uuid NOT NULL REFERENCES mobile_v9_invocations(id),
 usage_id uuid NOT NULL REFERENCES mobile_usages(id),
 budget_root uuid NOT NULL,
 PRIMARY KEY(invocation_id,budget_root)
);
CREATE INDEX mobile_usage_invocation_budgets_root ON mobile_usage_invocation_budgets(budget_root);

-- Preserve all previous scope-attributed calls; renewal never resets a root.
INSERT INTO mobile_usage_invocation_budgets(invocation_id,usage_id,budget_root)
SELECT i.id,u.id,u.budget_root
FROM mobile_v9_invocations i
JOIN mobile_v9_attempts a ON a.id=i.attempt_id
JOIN mobile_v9_plans p ON p.id=a.plan_id
JOIN mobile_usage_scopes m ON m.scope_id=(p.plan->>'scope_id')::uuid
JOIN mobile_usages u ON u.id=m.usage_id AND u.school_id=i.school_id
ON CONFLICT DO NOTHING;

-- Older scope-less calls still belong to the business roots effective at issue.
INSERT INTO mobile_usage_invocation_budgets(invocation_id,usage_id,budget_root)
SELECT DISTINCT ON(i.id,u.budget_root) i.id,u.id,u.budget_root
FROM mobile_v9_invocations i
JOIN mobile_v9_attempts a ON a.id=i.attempt_id
JOIN mobile_v9_plans p ON p.id=a.plan_id
JOIN mobile_usages u ON u.school_id=i.school_id AND u.approval='approved'
 AND coalesce((SELECT min(e.created_at) FROM mobile_usage_events e WHERE e.usage_id=u.id AND e.action IN ('approve','approved')),u.created_at)<=i.issued_at
 AND u.expires_at>i.issued_at
 AND u.body->'device_ids' @> jsonb_build_array(i.device_id)
 AND (u.body->'auto_targets'='true'::jsonb OR EXISTS(
  SELECT 1 FROM jsonb_array_elements(u.body->'targets') t WHERE t->>'id'=p.target_id::text))
WHERE p.plan->>'scope_id' IS NULL
 AND NOT EXISTS(SELECT 1 FROM mobile_usages n WHERE n.school_id=u.school_id AND n.budget_root=u.budget_root
  AND n.approval='approved'
  AND coalesce((SELECT min(e.created_at) FROM mobile_usage_events e WHERE e.usage_id=n.id AND e.action IN ('approve','approved')),n.created_at)<=i.issued_at
  AND (n.created_at,n.id)>(u.created_at,u.id))
ORDER BY i.id,u.budget_root,u.created_at DESC,u.id DESC
ON CONFLICT DO NOTHING;

CREATE TRIGGER mobile_usage_invocation_budgets_immutable BEFORE UPDATE OR DELETE
 ON mobile_usage_invocation_budgets FOR EACH ROW EXECUTE FUNCTION mobile_capability_evidence_immutable();
