-- New registrations may connect immediately after pairing. Existing disabled/revoked
-- devices retain their state; business authorization and execution gates are unchanged.
ALTER TABLE execution_devices ALTER COLUMN enabled SET DEFAULT true;
