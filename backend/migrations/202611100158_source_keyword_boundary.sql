-- New compositions carry source-message evidence only; historic checks remain readable.
ALTER TABLE source_message_compositions ALTER COLUMN workspace_check DROP NOT NULL;
ALTER TABLE source_message_compositions ALTER COLUMN final_check DROP NOT NULL;
-- Publications apply to future intake; do not revive old recheck work.
UPDATE keyword_recheck_batches SET complete=true WHERE NOT complete;
