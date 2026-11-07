DROP INDEX email_send_logs_action_idx;
ALTER TABLE email_send_logs DROP COLUMN assistant_action_id;
DROP TABLE invoice_deliveries;
-- Queued/partial audit states remain valid when rolling application code back.
