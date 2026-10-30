-- Shared, persistent budgets include invalid guesses and survive failed pairing.
CREATE TABLE device_pairing_attempt_windows (
 scope text PRIMARY KEY,
 requests integer NOT NULL CHECK (requests BETWEEN 1 AND 101),
 reset_at timestamptz NOT NULL
);
CREATE INDEX device_pairing_attempt_windows_reset ON device_pairing_attempt_windows(reset_at);
