-- A sampled reason is independent of UI lease ownership and development delivery.
ALTER TABLE mobile_v9_runtime_details ADD COLUMN ready_reason text;
ALTER TABLE mobile_v9_runtime_details ADD CONSTRAINT runtime_ready_reason_code CHECK (
    ready_reason IS NULL OR ready_reason ~ '^[a-z0-9_]{1,100}$'
);
