-- All tasks use the candidate dispatch model. No historical tasks require migration.
ALTER TABLE mobile_v9_tasks DROP COLUMN dispatch_mode;
