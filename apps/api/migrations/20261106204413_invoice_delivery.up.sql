-- Specific invoice post-commit work; no arbitrary HTTP replay.
ALTER TABLE assistant_actions DROP CONSTRAINT assistant_actions_state_check;
ALTER TABLE assistant_actions ADD CONSTRAINT assistant_actions_state_check CHECK (state IN ('proposed','approved','dispatching','completed','unknown','cancelled','expired','failed','queued','partial'));
CREATE TABLE invoice_deliveries (
 id uuid PRIMARY KEY DEFAULT uuidv7(),
 workspace_id uuid NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
 invoice_id uuid NOT NULL,
 user_id uuid NOT NULL REFERENCES users(id),
 action_id uuid REFERENCES assistant_actions(id) ON DELETE SET NULL,
 kind text NOT NULL CHECK(kind IN ('send','resend')),
 status text NOT NULL DEFAULT 'queued' CHECK(status IN ('queued','running','sending','accepted','blocked','manual','failed','unknown')),
 request jsonb CHECK(request IS NULL OR octet_length(request::text)<=3145728),
 reserved_pdf_id uuid NOT NULL DEFAULT uuidv7(),
 pdf_file_id uuid,
 send_log_id uuid,
 created_at timestamptz NOT NULL DEFAULT now(),
 started_at timestamptz,
 finished_at timestamptz,
 FOREIGN KEY(workspace_id,invoice_id) REFERENCES invoices(workspace_id,id),
 UNIQUE(workspace_id,id)
);
CREATE UNIQUE INDEX invoice_deliveries_action_idx ON invoice_deliveries(action_id) WHERE action_id IS NOT NULL;
CREATE INDEX invoice_deliveries_pending_idx ON invoice_deliveries(started_at,created_at) WHERE status IN ('queued','running','sending');
CREATE INDEX invoice_deliveries_workspace_idx ON invoice_deliveries(workspace_id,created_at);
ALTER TABLE email_send_logs ADD COLUMN assistant_action_id uuid REFERENCES assistant_actions(id) ON DELETE SET NULL;
CREATE UNIQUE INDEX email_send_logs_action_idx ON email_send_logs(assistant_action_id) WHERE assistant_action_id IS NOT NULL;
