-- A business unit remains in its original platform even when unbound.
CREATE FUNCTION business_unit_identity_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.id IS DISTINCT FROM OLD.id OR NEW.platform_id IS DISTINCT FROM OLD.platform_id THEN
        RAISE EXCEPTION 'business unit identity immutable';
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER business_unit_identity_immutable
    BEFORE UPDATE OF id, platform_id ON business_units
    FOR EACH ROW EXECUTE FUNCTION business_unit_identity_immutable();
