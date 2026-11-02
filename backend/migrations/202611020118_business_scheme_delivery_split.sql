-- Product delivery no longer owns business schedules or quotas.
ALTER TABLE mobile_capability_deliveries DROP COLUMN policy;
