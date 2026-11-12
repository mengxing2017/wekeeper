-- Link source routes to a distinct business unit in the same platform.
-- The application creates both records in one transaction.
ALTER TABLE source_routes
    ADD COLUMN business_unit_id uuid,
    ADD CONSTRAINT source_routes_business_unit_unique UNIQUE (business_unit_id),
    ADD CONSTRAINT source_routes_business_unit_platform_fk
        FOREIGN KEY (source_id, business_unit_id)
        REFERENCES business_units (platform_id, id);
