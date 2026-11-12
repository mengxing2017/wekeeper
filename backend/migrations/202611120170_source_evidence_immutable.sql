-- Authentication evidence must retain the bytes and identity that were accepted.
CREATE FUNCTION source_receipt_evidence_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
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
CREATE TRIGGER source_receipt_evidence_immutable
    BEFORE UPDATE OF receipt_id,event_id,body_sha256,schema_version,signature_version ON source_receipts
    FOR EACH ROW EXECUTE FUNCTION source_receipt_evidence_immutable();
CREATE FUNCTION source_inbox_body_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.request_body IS DISTINCT FROM OLD.request_body THEN
        RAISE EXCEPTION 'source request evidence immutable';
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER source_inbox_body_immutable BEFORE UPDATE OF request_body ON source_inbox
    FOR EACH ROW EXECUTE FUNCTION source_inbox_body_immutable();
