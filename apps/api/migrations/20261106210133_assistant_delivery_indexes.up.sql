-- Keep new resolved receipts and pending invoice guards indexable.
CREATE INDEX assistant_actions_receipt_retention_idx ON assistant_actions(finished_at) WHERE state IN ('completed','cancelled','expired','failed','queued','partial');
CREATE INDEX invoice_deliveries_invoice_pending_idx ON invoice_deliveries(workspace_id,invoice_id) WHERE status IN ('queued','running','sending','unknown');
CREATE INDEX invoice_deliveries_retention_idx ON invoice_deliveries(finished_at) WHERE status IN ('accepted','blocked','manual','failed');
