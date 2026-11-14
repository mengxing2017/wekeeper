-- Storage partitions are allocated by the system; operators configure only units.
CREATE TABLE unit_receiving_storage (
 business_unit_id uuid NOT NULL REFERENCES business_units(id),
 channel text NOT NULL CHECK(channel IN ('mobile','wecom')),
 storage_scope uuid NOT NULL UNIQUE,
 PRIMARY KEY(business_unit_id,channel)
);
