-- Independent device presence: no sending-account or execution-session dependency.
CREATE TABLE device_control_sessions (
 device_id uuid PRIMARY KEY REFERENCES execution_devices(id),
 connection_id uuid NOT NULL, epoch bigint NOT NULL DEFAULT 1,
 transport text NOT NULL CHECK(transport IN ('wss','https')),
 connected boolean NOT NULL DEFAULT true,
 last_heartbeat_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 readiness text NOT NULL DEFAULT 'waiting', block_reason text,
 desired_paused boolean NOT NULL DEFAULT false, desired_revision bigint NOT NULL DEFAULT 0
);
CREATE TABLE device_control_commands (
 id uuid PRIMARY KEY, sequence bigserial UNIQUE NOT NULL,
 device_id uuid NOT NULL REFERENCES execution_devices(id),
 kind text NOT NULL CHECK(kind IN ('task_available','configuration_changed','pause_execution','resume_execution','request_status')),
 state text NOT NULL DEFAULT 'queued' CHECK(state IN ('queued','delivered','accepted','completed','rejected','expired')),
 resource_id uuid, desired_revision bigint,
 request_key text, request_digest text, actor_id uuid REFERENCES accounts(id),
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 expires_at timestamptz NOT NULL DEFAULT (clock_timestamp()+interval '1 day'),
 accepted_at timestamptz, completed_at timestamptz, result_code text,
 UNIQUE(device_id,request_key)
);
CREATE INDEX device_control_pending ON device_control_commands(device_id,sequence)
 WHERE state IN ('queued','delivered','accepted');
-- Durable outbox is authoritative; sockets poll it even if a notification is lost.
CREATE FUNCTION notify_device_control_task() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 INSERT INTO device_control_commands(id,device_id,kind,resource_id)
 SELECT gen_random_uuid(),b.device_id,'task_available',NEW.id
 FROM device_account_bindings b
 WHERE EXISTS(SELECT 1 FROM mobile_delivery_items i WHERE i.task_id=NEW.id AND i.sender_account_id=b.sender_account_id);
 RETURN NEW;
END $$;
CREATE TRIGGER device_control_task_confirmed AFTER UPDATE OF state ON mobile_delivery_tasks
 FOR EACH ROW WHEN (NEW.state='confirmed' AND OLD.state IS DISTINCT FROM NEW.state)
 EXECUTE FUNCTION notify_device_control_task();
CREATE FUNCTION notify_device_control_binding() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 INSERT INTO device_control_commands(id,device_id,kind) VALUES(gen_random_uuid(),NEW.device_id,'configuration_changed');
 RETURN NEW;
END $$;
CREATE TRIGGER device_control_binding_changed AFTER INSERT OR UPDATE ON device_account_bindings
 FOR EACH ROW EXECUTE FUNCTION notify_device_control_binding();
-- Two-phase renewal: candidate secrets are generated and stored by the device,
-- only hashes reach the registration endpoint. Administrator generations still fence both.
CREATE TABLE device_credential_renewals (
 candidate_hash text PRIMARY KEY REFERENCES device_credentials(token_hash),
 previous_hash text NOT NULL REFERENCES device_credentials(token_hash),
 activated_at timestamptz
);
CREATE UNIQUE INDEX device_credential_pending_renewal ON device_credential_renewals(previous_hash) WHERE activated_at IS NULL;
-- v5 creates confirmed tasks directly; notify from its transactional delivery outbox.
CREATE FUNCTION notify_device_control_dispatch() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 INSERT INTO device_control_commands(id,device_id,kind,resource_id)
 VALUES(gen_random_uuid(),NEW.device_id,'task_available',NEW.id);
 RETURN NEW;
END $$;
CREATE TRIGGER device_control_dispatch_created AFTER INSERT ON automation_task_deliveries
 FOR EACH ROW EXECUTE FUNCTION notify_device_control_dispatch();
CREATE FUNCTION notify_device_control_unbinding() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 INSERT INTO device_control_commands(id,device_id,kind)
 VALUES(gen_random_uuid(),OLD.device_id,'configuration_changed');
 RETURN OLD;
END $$;
CREATE TRIGGER device_control_binding_deleted AFTER DELETE ON device_account_bindings
 FOR EACH ROW EXECUTE FUNCTION notify_device_control_unbinding();
