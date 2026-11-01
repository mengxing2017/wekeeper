-- Independent phone task intent, immutable preview and plan. No legacy/official references.
CREATE TABLE mobile_execution_task_previews (
 id uuid PRIMARY KEY,school_id uuid NOT NULL REFERENCES schools(id),device_id uuid NOT NULL REFERENCES execution_devices(id),
 input jsonb NOT NULL,snapshot jsonb NOT NULL,digest text NOT NULL CHECK(length(digest)=64),
 created_by uuid NOT NULL REFERENCES accounts(id),created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 expires_at timestamptz NOT NULL DEFAULT clock_timestamp()+interval '10 minutes',
 UNIQUE(school_id,id)
);
CREATE TABLE mobile_execution_tasks (
 id uuid PRIMARY KEY,school_id uuid NOT NULL REFERENCES schools(id),intent_id uuid NOT NULL UNIQUE,
 device_id uuid NOT NULL REFERENCES execution_devices(id),preview_id uuid NOT NULL UNIQUE,
 state text NOT NULL DEFAULT 'queued' CHECK(state IN ('queued','running','paused','completed','partially_completed','cancelled')),
 revision bigint NOT NULL DEFAULT 1 CHECK(revision>0),
 created_by uuid NOT NULL REFERENCES accounts(id),confirmed_by uuid NOT NULL REFERENCES accounts(id),
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),cancelled_at timestamptz,
 UNIQUE(school_id,id),UNIQUE(school_id,device_id,id),UNIQUE(intent_id,id),
 FOREIGN KEY(school_id,preview_id) REFERENCES mobile_execution_task_previews(school_id,id)
);
CREATE TABLE mobile_execution_plans (
 id uuid PRIMARY KEY,school_id uuid NOT NULL,task_id uuid NOT NULL UNIQUE,snapshot jsonb NOT NULL,
 digest text NOT NULL CHECK(length(digest)=64),created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 FOREIGN KEY(school_id,task_id) REFERENCES mobile_execution_tasks(school_id,id)
);
CREATE TABLE mobile_execution_task_items (
 id uuid PRIMARY KEY,school_id uuid NOT NULL,task_id uuid NOT NULL,intent_id uuid NOT NULL,
 device_id uuid NOT NULL,target_id uuid NOT NULL,binding_id uuid NOT NULL REFERENCES mobile_device_target_bindings(id),
 position integer NOT NULL CHECK(position BETWEEN 1 AND 100),attempt integer NOT NULL DEFAULT 1 CHECK(attempt>0),
 snapshot jsonb NOT NULL,state text NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','claimed','prepared','invocation_issued','reported_sent','verified_not_sent','unknown','failed_before_send','invalidated','cancelled')),
 UNIQUE(intent_id,target_id),UNIQUE(task_id,position),
 FOREIGN KEY(school_id,device_id,task_id) REFERENCES mobile_execution_tasks(school_id,device_id,id),
 FOREIGN KEY(intent_id,task_id) REFERENCES mobile_execution_tasks(intent_id,id),
 FOREIGN KEY(school_id,target_id) REFERENCES mobile_targets(school_id,id)
);
-- A reservation never expires into permission to replay. Later invocation/unknown states retain it.
CREATE TABLE mobile_execution_device_reservations (
 device_id uuid PRIMARY KEY REFERENCES execution_devices(id),school_id uuid NOT NULL,task_id uuid NOT NULL UNIQUE,
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 FOREIGN KEY(school_id,device_id,task_id) REFERENCES mobile_execution_tasks(school_id,device_id,id)
);
CREATE INDEX mobile_execution_tasks_school ON mobile_execution_tasks(school_id,created_at,id);
CREATE INDEX mobile_execution_active_target_items ON mobile_execution_task_items(school_id,target_id) WHERE state IN ('pending','claimed','prepared','invocation_issued','unknown');
ALTER TABLE mobile_device_target_bindings ADD CONSTRAINT mobile_binding_task_identity UNIQUE(school_id,device_id,target_id,id);
ALTER TABLE mobile_execution_task_items ADD CONSTRAINT mobile_task_item_binding_scope FOREIGN KEY(school_id,device_id,target_id,binding_id) REFERENCES mobile_device_target_bindings(school_id,device_id,target_id,id);
CREATE FUNCTION mobile_execution_immutable_plan() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 RAISE EXCEPTION 'mobile execution preview/plan is immutable' USING ERRCODE='23514';
END $$;
CREATE TRIGGER mobile_preview_immutable BEFORE UPDATE OR DELETE ON mobile_execution_task_previews FOR EACH ROW EXECUTE FUNCTION mobile_execution_immutable_plan();
CREATE TRIGGER mobile_plan_immutable BEFORE UPDATE OR DELETE ON mobile_execution_plans FOR EACH ROW EXECUTE FUNCTION mobile_execution_immutable_plan();
CREATE FUNCTION mobile_execution_immutable_item() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF (to_jsonb(NEW)-'state') IS DISTINCT FROM (to_jsonb(OLD)-'state') THEN
  RAISE EXCEPTION 'mobile execution item snapshot is immutable' USING ERRCODE='23514';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER mobile_item_immutable BEFORE UPDATE ON mobile_execution_task_items FOR EACH ROW EXECUTE FUNCTION mobile_execution_immutable_item();
