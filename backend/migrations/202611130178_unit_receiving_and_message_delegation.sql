-- Configuration belongs to business units. Groups may be shared by configurations.
CREATE TABLE channel_receiving_configurations (
 id uuid PRIMARY KEY, business_unit_id uuid NOT NULL REFERENCES business_units(id),
 channel text NOT NULL CHECK(channel IN ('mobile','wecom')),
 configuration jsonb NOT NULL, authorized_by uuid NOT NULL REFERENCES accounts(id),
 updated_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE INDEX channel_receiving_unit ON channel_receiving_configurations(business_unit_id,channel,id);
-- Preserve maintenance evidence for unbound configurations without retaining a second runtime model.
CREATE TABLE receiving_configuration_evidence (id uuid NOT NULL,channel text NOT NULL,configuration jsonb NOT NULL,recorded_at timestamptz NOT NULL DEFAULT clock_timestamp(),PRIMARY KEY(channel,id));
INSERT INTO receiving_configuration_evidence SELECT id,'mobile',configuration FROM mobile_receiving_configurations;
INSERT INTO receiving_configuration_evidence SELECT id,'wecom',configuration FROM wecom_receiving_configurations;
-- Existing explicit route grants become unit receiving configurations. Disabled routes
-- remain disabled. Merely having a configuration never invents an authorization.
INSERT INTO channel_receiving_configurations(id,business_unit_id,channel,configuration,authorized_by)
SELECT gen_random_uuid(),r.business_unit_id,s.snapshot #>> '{execution,channel}',
 jsonb_build_object('id',NULL,'business_unit_id',r.business_unit_id,'name',COALESCE(r.route_name,r.route_id),
 'enabled',r.execution_state='running' AND r.authorized_by IS NOT NULL,
 'target_ids',COALESCE(s.snapshot #> '{execution,target_ids}','[]'),
 'device_ids',CASE WHEN s.snapshot #>> '{execution,channel}'='mobile' THEN COALESCE((SELECT jsonb_agg(DISTINCT b.device_id) FROM mobile_device_target_bindings b WHERE b.target_id IN (SELECT value::uuid FROM jsonb_array_elements_text(s.snapshot #> '{execution,target_ids}'))),'[]') ELSE '[]' END,
 'sender_ids',CASE WHEN s.snapshot #>> '{execution,channel}'='wecom' THEN COALESCE((SELECT jsonb_agg(DISTINCT (sender->>'account_id')::uuid) FROM jsonb_array_elements(s.snapshot->'targets') t CROSS JOIN LATERAL jsonb_array_elements(t #> '{identity,senders}') sender),'[]') ELSE '[]' END),
 COALESCE(r.authorized_by,(s.snapshot->>'configured_by')::uuid)
FROM source_routes r JOIN LATERAL (SELECT snapshot FROM source_route_revisions WHERE route_id=r.route_id ORDER BY revision DESC LIMIT 1) s ON true
WHERE s.snapshot #>> '{execution,channel}' IN ('mobile','wecom');
UPDATE channel_receiving_configurations SET configuration=jsonb_set(configuration,'{id}',to_jsonb(id));
DROP TABLE mobile_receiving_configuration_targets,wecom_receiving_configuration_targets,mobile_receiving_configuration_devices,wecom_receiving_configuration_senders;
DROP TABLE mobile_receiving_configuration_versions,wecom_receiving_configuration_versions;
DROP TABLE mobile_receiving_configurations,wecom_receiving_configurations;
DROP FUNCTION receiving_configuration_version_immutable();

-- Frozen input and retry state are separate from immutable accepted messages.
CREATE TABLE orchestration_dispatches (
 message_id uuid PRIMARY KEY REFERENCES orchestration_messages(id),
 content text NOT NULL, decision jsonb NOT NULL, plans jsonb NOT NULL,
 state text NOT NULL CHECK(state IN ('preparing','ready','delegated','failed','historical')),
 reason_code text, failures integer NOT NULL DEFAULT 0,
 next_attempt_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),updated_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE orchestration_delegates (
 id uuid PRIMARY KEY, message_id uuid NOT NULL REFERENCES orchestration_messages(id),
 configuration_id uuid NOT NULL,channel text NOT NULL CHECK(channel IN ('mobile','wecom')),
 storage_scope uuid NOT NULL, task_id uuid UNIQUE, snapshot_id uuid REFERENCES distribution_message_snapshots(id),
 plan jsonb NOT NULL,state text NOT NULL DEFAULT 'delegated' CHECK(state IN ('delegated','failed')),reason_code text, created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 CHECK((state='delegated')=(task_id IS NOT NULL AND snapshot_id IS NOT NULL))
);
CREATE TRIGGER orchestration_delegate_immutable BEFORE UPDATE OR DELETE ON orchestration_delegates FOR EACH ROW EXECUTE FUNCTION orchestration_message_immutable();
-- Existing channel tasks are evidence, never a second opportunity to send.
INSERT INTO orchestration_dispatches(message_id,content,decision,plans,state)
SELECT m.id,COALESCE(c.content,''),'{}','[]','historical' FROM orchestration_messages m JOIN source_execution_tasks t ON t.receipt_id=m.evidence_ref LEFT JOIN source_message_compositions c ON c.id=t.composition_id;
