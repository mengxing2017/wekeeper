-- A scanner pass is not a dispatch round: each round waits at least five minutes.
ALTER TABLE mobile_v9_dispatch_state ADD COLUMN round_started_at timestamptz;
-- Preserve consumed budgets and terminal history. Start the remaining active round
-- when it is next business-eligible; do not infer elapsed time from old scans.
