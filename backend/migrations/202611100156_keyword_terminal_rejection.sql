-- A sensitive-word rejection belongs to this receipt's content version.
ALTER TABLE source_receipts ADD COLUMN keyword_terminal_rejected boolean NOT NULL DEFAULT false;
UPDATE source_receipts SET keyword_terminal_rejected = true
WHERE reason_code IN ('keyword_blocked', 'workspace_keyword_blocked', 'final_keyword_blocked')
   OR EXISTS (SELECT 1 FROM source_receipt_history h WHERE h.receipt_id=source_receipts.receipt_id AND h.reason_code IN ('keyword_blocked', 'workspace_keyword_blocked', 'final_keyword_blocked')); 
