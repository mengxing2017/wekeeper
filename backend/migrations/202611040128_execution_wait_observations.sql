-- Read-model observations and best-effort feedback do not authorize or block execution.
CREATE TABLE mobile_v9_execution_wait_observations (
 attempt_id uuid PRIMARY KEY REFERENCES mobile_v9_attempts(id),
 conditions jsonb NOT NULL CHECK(jsonb_typeof(conditions)='array'),
 observed_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
ALTER TABLE mobile_v9_support_requests ADD COLUMN task_id uuid REFERENCES mobile_v9_tasks(id);
ALTER TABLE mobile_v9_support_requests ADD COLUMN wait_code text;
CREATE UNIQUE INDEX mobile_v9_wait_feedback_once ON mobile_v9_support_requests(task_id,wait_code)
 WHERE task_id IS NOT NULL AND wait_code IS NOT NULL;
