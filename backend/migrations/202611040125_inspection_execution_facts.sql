-- Retry budgets measure executed failures, never queued work or invalidated resources.
ALTER TABLE mobile_v9_send_inspections
 ADD COLUMN started_at timestamptz,
 ADD COLUMN finished_at timestamptz,
 ADD COLUMN termination_reason text;
-- Do not invent historical start/completion timestamps. A server claim alone
-- does not prove execution; historical completed UI observations remain usable.
CREATE INDEX mobile_v9_inspection_executed_failures
 ON mobile_v9_send_inspections(binding_id,finished_at)
 WHERE state='failed' AND (started_at IS NOT NULL OR result->'checks'->>'application_opened'='true');

ALTER TABLE mobile_v9_attempts ADD COLUMN invalidated_at timestamptz;
