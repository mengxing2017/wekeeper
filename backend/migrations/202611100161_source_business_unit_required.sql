-- Existing business data is absent; every new route must reference a unit.
ALTER TABLE source_routes ALTER COLUMN business_unit_id SET NOT NULL;

CREATE FUNCTION source_route_business_unit_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.business_unit_id IS DISTINCT FROM OLD.business_unit_id THEN
  RAISE EXCEPTION 'route business unit is immutable';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER source_route_business_unit_immutable
 BEFORE UPDATE OF business_unit_id ON source_routes
 FOR EACH ROW EXECUTE FUNCTION source_route_business_unit_immutable();
