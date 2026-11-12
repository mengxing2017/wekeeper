-- Freeze the authenticated business identity with the accepted source receipt.
ALTER TABLE source_receipts
    ADD COLUMN business_unit_id uuid NOT NULL,
    ADD CONSTRAINT source_receipts_business_unit_platform_fk
        FOREIGN KEY (source_id, business_unit_id)
        REFERENCES business_units (platform_id, id);

CREATE FUNCTION source_receipt_business_identity_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.source_id IS DISTINCT FROM OLD.source_id
       OR NEW.business_unit_id IS DISTINCT FROM OLD.business_unit_id
       OR NEW.route_id IS DISTINCT FROM OLD.route_id THEN
        RAISE EXCEPTION 'source receipt business identity immutable';
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER source_receipt_business_identity_immutable
    BEFORE UPDATE OF source_id, business_unit_id, route_id ON source_receipts
    FOR EACH ROW EXECUTE FUNCTION source_receipt_business_identity_immutable();
