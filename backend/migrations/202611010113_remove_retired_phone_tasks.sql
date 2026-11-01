-- The school-bound phone task feature was never deployed and has no data to migrate.
-- Keep migration history forward-only; shared v8 tasks and official WeCom are independent.
DROP TABLE mobile_execution_device_reservations;
DROP TABLE mobile_execution_task_items;
DROP TABLE mobile_execution_plans;
DROP TABLE mobile_execution_tasks;
DROP TABLE mobile_execution_task_previews;
DROP FUNCTION mobile_execution_immutable_item();
DROP FUNCTION mobile_execution_immutable_plan();
-- The binding identity is also referenced by shared v8 plans: retain and rename it.
ALTER TABLE mobile_device_target_bindings RENAME CONSTRAINT mobile_binding_task_identity TO mobile_binding_scope_identity;
