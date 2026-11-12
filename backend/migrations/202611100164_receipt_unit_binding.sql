-- An accepted receipt must refer to the unit associated with its source route.
-- The receipt retains that identity if the live route is later removed.
CREATE FUNCTION source_receipt_check_unit_binding() RETURNS trigger LANGUAGE plpgsql AS $$
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
CREATE TRIGGER source_receipt_check_unit_binding
    BEFORE INSERT ON source_receipts
    FOR EACH ROW EXECUTE FUNCTION source_receipt_check_unit_binding();
