-- 业务单位、来源路由、收件及消息交接。
-- 空库初始化基线；后续结构变更使用新增递增迁移。

-- function
CREATE FUNCTION business_unit_identity_immutable() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    IF NEW.id IS DISTINCT FROM OLD.id OR NEW.platform_id IS DISTINCT FROM OLD.platform_id THEN
        RAISE EXCEPTION 'business unit identity immutable';
    END IF;
    RETURN NEW;
END $$;

CREATE FUNCTION routing_group_scope() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
 IF NEW.unit_id IS DISTINCT FROM OLD.unit_id AND EXISTS(SELECT 1 FROM source_routes WHERE group_id=OLD.id) THEN
  RAISE EXCEPTION 'referenced group cannot move';
 END IF;
 RETURN NEW;
END $$;

CREATE FUNCTION source_identity_immutable() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
 IF TG_TABLE_NAME='source_credentials' THEN
 IF (NEW.key_id IS DISTINCT FROM OLD.key_id OR NEW.source_id IS DISTINCT FROM OLD.source_id OR (OLD.status='deleted' AND NEW.status<>'deleted')) THEN RAISE EXCEPTION 'credential identity immutable'; END IF;
 END IF;
 IF TG_TABLE_NAME='source_routes' THEN
 IF (NEW.route_id IS DISTINCT FROM OLD.route_id OR NEW.source_id IS DISTINCT FROM OLD.source_id) THEN RAISE EXCEPTION 'route identity immutable'; END IF;
 END IF;
 RETURN NEW;
END $$;

CREATE FUNCTION source_inbox_body_immutable() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    IF NEW.request_body IS DISTINCT FROM OLD.request_body THEN
        RAISE EXCEPTION 'source request evidence immutable';
    END IF;
    RETURN NEW;
END $$;

CREATE FUNCTION source_message_handoff_enqueue() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    INSERT INTO source_message_handoffs(receipt_id,platform_id)
        SELECT NEW.receipt_id,source_id FROM source_receipts WHERE receipt_id=NEW.receipt_id;
    RETURN NEW;
END $$;

CREATE FUNCTION source_receipt_business_identity_immutable() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    IF NEW.source_id IS DISTINCT FROM OLD.source_id
       OR NEW.business_unit_id IS DISTINCT FROM OLD.business_unit_id
       OR NEW.route_id IS DISTINCT FROM OLD.route_id THEN
        RAISE EXCEPTION 'source receipt business identity immutable';
    END IF;
    RETURN NEW;
END $$;

CREATE FUNCTION source_receipt_check_unit_binding() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM source_routes
        WHERE route_id = NEW.route_id AND source_id = NEW.source_id
          AND business_unit_id = NEW.business_unit_id
    ) THEN
        RAISE EXCEPTION 'source receipt unit does not match route' USING ERRCODE = '23503';
    END IF;
    RETURN NEW;
END $$;

CREATE FUNCTION source_receipt_evidence_immutable() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    IF NEW.receipt_id IS DISTINCT FROM OLD.receipt_id
       OR NEW.event_id IS DISTINCT FROM OLD.event_id
       OR NEW.body_sha256 IS DISTINCT FROM OLD.body_sha256
       OR NEW.schema_version IS DISTINCT FROM OLD.schema_version
       OR NEW.signature_version IS DISTINCT FROM OLD.signature_version THEN
        RAISE EXCEPTION 'source receipt evidence immutable';
    END IF;
    RETURN NEW;
END $$;

CREATE FUNCTION source_receipt_transition() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
 IF (NEW.status,NEW.processing_stage,NEW.reason_code)
  IS DISTINCT FROM (OLD.status,OLD.processing_stage,OLD.reason_code) THEN
  NEW.processing_revision=OLD.processing_revision+1;
  NEW.updated_at=clock_timestamp();
 END IF;
 RETURN NEW;
END $$;

CREATE FUNCTION source_route_business_unit_immutable() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
 IF NEW.business_unit_id IS DISTINCT FROM OLD.business_unit_id THEN
  RAISE EXCEPTION 'route business unit is immutable';
 END IF;
 RETURN NEW;
END $$;

CREATE FUNCTION source_route_lifecycle() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
 IF TG_OP='INSERT' THEN
  IF EXISTS(SELECT 1 FROM source_route_tombstones WHERE route_id=NEW.route_id) THEN RAISE EXCEPTION 'retired route identifier'; END IF;
  RETURN NEW;
 ELSIF TG_OP='DELETE' THEN
  INSERT INTO source_route_tombstones(route_id,source_id,reason) VALUES(OLD.route_id,OLD.source_id,'credential_deleted') ON CONFLICT DO NOTHING;
  RETURN OLD;
 END IF;
 RETURN NEW;
END $$;

CREATE FUNCTION source_routing_immutable() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN RAISE EXCEPTION 'routing history is immutable'; END $$;

-- table
CREATE TABLE business_units (
    id uuid NOT NULL,
    platform_id uuid NOT NULL,
    name text NOT NULL,
    description text DEFAULT ''::text NOT NULL,
    active boolean DEFAULT true NOT NULL,
    revision bigint DEFAULT 1 NOT NULL,
    created_by uuid NOT NULL,
    updated_by uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT business_units_description_check CHECK ((length(description) <= 2000)),
    CONSTRAINT business_units_id_check CHECK ((id <> '00000000-0000-0000-0000-000000000000'::uuid)),
    CONSTRAINT business_units_name_check CHECK (((length(TRIM(BOTH FROM name)) >= 1) AND (length(TRIM(BOTH FROM name)) <= 120))),
    CONSTRAINT business_units_revision_check CHECK ((revision > 0))
);

CREATE TABLE source_inbox (
    receipt_id uuid NOT NULL,
    request_body bytea NOT NULL,
    state text DEFAULT 'received'::text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);

CREATE TABLE source_key_routes (
    key_id text NOT NULL,
    route_id text NOT NULL,
    source_id uuid NOT NULL,
    status text NOT NULL,
    revision bigint DEFAULT 1 NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT source_key_routes_status_check CHECK ((status = ANY (ARRAY['active'::text, 'revoked'::text])))
);

CREATE TABLE source_message_handoffs (
    receipt_id uuid NOT NULL,
    platform_id uuid NOT NULL,
    state text DEFAULT 'pending'::text NOT NULL,
    message_id uuid,
    claim_token uuid,
    claim_until timestamp with time zone,
    attempts integer DEFAULT 0 NOT NULL,
    next_attempt_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    last_error text,
    created_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    completed_at timestamp with time zone,
    CONSTRAINT source_message_handoffs_attempts_check CHECK ((attempts >= 0)),
    CONSTRAINT source_message_handoffs_check CHECK (((claim_token IS NULL) = (claim_until IS NULL))),
    CONSTRAINT source_message_handoffs_check1 CHECK (((state = 'complete'::text) = (message_id IS NOT NULL))),
    CONSTRAINT source_message_handoffs_check2 CHECK (((state = 'complete'::text) = (completed_at IS NOT NULL))),
    CONSTRAINT source_message_handoffs_check3 CHECK (((state <> 'complete'::text) OR (claim_token IS NULL))),
    CONSTRAINT source_message_handoffs_state_check CHECK ((state = ANY (ARRAY['pending'::text, 'complete'::text])))
);

CREATE TABLE source_receipts (
    receipt_id uuid NOT NULL,
    source_id uuid NOT NULL,
    event_id uuid NOT NULL,
    route_id text NOT NULL,
    body_sha256 text NOT NULL,
    schema_version integer NOT NULL,
    signature_version integer DEFAULT 2 NOT NULL,
    status text DEFAULT 'received'::text NOT NULL,
    reason_code text,
    processing_stage text DEFAULT 'received'::text NOT NULL,
    sending jsonb,
    received_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    processing_revision bigint DEFAULT 0 NOT NULL,
    summary_version text,
    summary_revision bigint DEFAULT 0 NOT NULL,
    summary_updated_at timestamp with time zone,
    summary_total bigint,
    execution_check_state text,
    business_unit_id uuid NOT NULL,
    CONSTRAINT source_receipts_schema_version_check CHECK ((schema_version = 2)),
    CONSTRAINT source_receipts_signature_version_check CHECK ((signature_version = 2))
);

CREATE TABLE source_route_tombstones (
    route_id text NOT NULL,
    source_id uuid NOT NULL,
    reason text NOT NULL,
    retired_at timestamp with time zone DEFAULT now() NOT NULL
);

CREATE TABLE source_routes (
    route_id text NOT NULL,
    source_id uuid NOT NULL,
    route_name text NOT NULL,
    remark text DEFAULT ''::text NOT NULL,
    status text NOT NULL,
    revision bigint DEFAULT 1 NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    execution_state text DEFAULT 'draft'::text NOT NULL,
    retired_at timestamp with time zone,
    authorized_by uuid,
    business_unit_id uuid NOT NULL,
    CONSTRAINT source_routes_execution_state_check CHECK ((execution_state = ANY (ARRAY['draft'::text, 'running'::text, 'paused'::text]))),
    CONSTRAINT source_routes_route_name_check CHECK ((length(TRIM(BOTH FROM route_name)) > 0)),
    CONSTRAINT source_routes_status_check CHECK ((status = ANY (ARRAY['active'::text, 'disabled'::text])))
);

-- constraint
ALTER TABLE ONLY business_units
    ADD CONSTRAINT business_units_pkey PRIMARY KEY (id);

ALTER TABLE ONLY business_units
    ADD CONSTRAINT business_units_platform_id_id_key UNIQUE (platform_id, id);

ALTER TABLE ONLY source_inbox
    ADD CONSTRAINT source_inbox_pkey PRIMARY KEY (receipt_id);

ALTER TABLE ONLY source_key_routes
    ADD CONSTRAINT source_key_routes_pkey PRIMARY KEY (key_id, route_id);

ALTER TABLE ONLY source_message_handoffs
    ADD CONSTRAINT source_message_handoffs_pkey PRIMARY KEY (receipt_id);

ALTER TABLE ONLY source_receipts
    ADD CONSTRAINT source_receipts_pkey PRIMARY KEY (receipt_id);

ALTER TABLE ONLY source_receipts
    ADD CONSTRAINT source_receipts_source_id_event_id_key UNIQUE (source_id, event_id);

ALTER TABLE ONLY source_route_tombstones
    ADD CONSTRAINT source_route_tombstones_pkey PRIMARY KEY (route_id);

ALTER TABLE ONLY source_routes
    ADD CONSTRAINT source_routes_business_unit_unique UNIQUE (business_unit_id);

ALTER TABLE ONLY source_routes
    ADD CONSTRAINT source_routes_pkey PRIMARY KEY (route_id);

ALTER TABLE ONLY source_routes
    ADD CONSTRAINT source_routes_route_id_source_id_key UNIQUE (route_id, source_id);

-- index
CREATE INDEX business_units_platform ON business_units USING btree (platform_id, id);

CREATE INDEX message_summary_reconcile ON source_receipts USING btree (summary_updated_at, receipt_id);

CREATE INDEX source_message_handoffs_due ON source_message_handoffs USING btree (next_attempt_at, receipt_id) WHERE (state = 'pending'::text);

CREATE INDEX source_receipt_route_page ON source_receipts USING btree (route_id, received_at DESC, receipt_id DESC);

-- fk constraint
ALTER TABLE ONLY business_units
    ADD CONSTRAINT business_units_created_by_fkey FOREIGN KEY (created_by) REFERENCES accounts(id);

ALTER TABLE ONLY business_units
    ADD CONSTRAINT business_units_platform_id_fkey FOREIGN KEY (platform_id) REFERENCES source_callers(source_id);

ALTER TABLE ONLY business_units
    ADD CONSTRAINT business_units_updated_by_fkey FOREIGN KEY (updated_by) REFERENCES accounts(id);

ALTER TABLE ONLY source_inbox
    ADD CONSTRAINT source_inbox_receipt_id_fkey FOREIGN KEY (receipt_id) REFERENCES source_receipts(receipt_id);

ALTER TABLE ONLY source_key_routes
    ADD CONSTRAINT source_key_routes_key_id_source_id_fkey FOREIGN KEY (key_id, source_id) REFERENCES source_credentials(key_id, source_id);

ALTER TABLE ONLY source_key_routes
    ADD CONSTRAINT source_key_routes_route_id_source_id_fkey FOREIGN KEY (route_id, source_id) REFERENCES source_routes(route_id, source_id);

ALTER TABLE ONLY source_message_handoffs
    ADD CONSTRAINT source_message_handoffs_platform_id_fkey FOREIGN KEY (platform_id) REFERENCES source_callers(source_id);

ALTER TABLE ONLY source_message_handoffs
    ADD CONSTRAINT source_message_handoffs_receipt_id_fkey FOREIGN KEY (receipt_id) REFERENCES source_inbox(receipt_id);

ALTER TABLE ONLY source_receipts
    ADD CONSTRAINT source_receipts_business_unit_platform_fk FOREIGN KEY (source_id, business_unit_id) REFERENCES business_units(platform_id, id);

ALTER TABLE ONLY source_receipts
    ADD CONSTRAINT source_receipts_source_id_fkey FOREIGN KEY (source_id) REFERENCES source_callers(source_id);

ALTER TABLE ONLY source_route_tombstones
    ADD CONSTRAINT source_route_tombstones_source_id_fkey FOREIGN KEY (source_id) REFERENCES source_callers(source_id);

ALTER TABLE ONLY source_routes
    ADD CONSTRAINT source_routes_authorized_by_fkey FOREIGN KEY (authorized_by) REFERENCES accounts(id);

ALTER TABLE ONLY source_routes
    ADD CONSTRAINT source_routes_business_unit_platform_fk FOREIGN KEY (source_id, business_unit_id) REFERENCES business_units(platform_id, id);

ALTER TABLE ONLY source_routes
    ADD CONSTRAINT source_routes_source_id_fkey FOREIGN KEY (source_id) REFERENCES source_callers(source_id);

-- trigger
CREATE TRIGGER business_unit_identity_immutable BEFORE UPDATE OF id, platform_id ON business_units FOR EACH ROW EXECUTE FUNCTION business_unit_identity_immutable();

CREATE TRIGGER source_inbox_body_immutable BEFORE UPDATE OF request_body ON source_inbox FOR EACH ROW EXECUTE FUNCTION source_inbox_body_immutable();

CREATE TRIGGER source_message_handoff_enqueue AFTER INSERT ON source_inbox FOR EACH ROW EXECUTE FUNCTION source_message_handoff_enqueue();

CREATE TRIGGER source_receipt_business_identity_immutable BEFORE UPDATE OF source_id, business_unit_id, route_id ON source_receipts FOR EACH ROW EXECUTE FUNCTION source_receipt_business_identity_immutable();

CREATE TRIGGER source_receipt_check_unit_binding BEFORE INSERT ON source_receipts FOR EACH ROW EXECUTE FUNCTION source_receipt_check_unit_binding();

CREATE TRIGGER source_receipt_evidence_immutable BEFORE UPDATE OF receipt_id, event_id, body_sha256, schema_version, signature_version ON source_receipts FOR EACH ROW EXECUTE FUNCTION source_receipt_evidence_immutable();

CREATE TRIGGER source_receipt_transition BEFORE UPDATE ON source_receipts FOR EACH ROW EXECUTE FUNCTION source_receipt_transition();

CREATE TRIGGER source_route_business_unit_immutable BEFORE UPDATE OF business_unit_id ON source_routes FOR EACH ROW EXECUTE FUNCTION source_route_business_unit_immutable();

CREATE TRIGGER source_route_identity BEFORE UPDATE ON source_routes FOR EACH ROW EXECUTE FUNCTION source_identity_immutable();

CREATE TRIGGER source_route_lifecycle BEFORE INSERT OR DELETE OR UPDATE ON source_routes FOR EACH ROW EXECUTE FUNCTION source_route_lifecycle();

