-- One business group name; recognition and uniqueness are owned by the device.
ALTER TABLE mobile_targets DROP COLUMN exact_name;
ALTER TABLE mobile_targets DROP COLUMN identity_revision;
ALTER TABLE mobile_targets DROP COLUMN alias;
