-- GET /attention: partial indexes so each section count and its oldest items read only the
-- rows that need action, not every row of the workspace.
CREATE INDEX orders_attention_idx ON orders (workspace_id, created_at, id)
    WHERE status IN ('draft', 'open');
CREATE INDEX invoices_attention_idx ON invoices (workspace_id, due_date)
    WHERE deleted_at IS NULL AND status IN ('draft', 'pending', 'partially_paid');
CREATE INDEX drawer_shifts_open_idx ON drawer_shifts (workspace_id, opened_at)
    WHERE closed_at IS NULL;
CREATE INDEX lots_off_sale_idx ON lots (workspace_id, updated_at) WHERE status <> 'released';
CREATE INDEX webhook_deliveries_failed_idx ON webhook_deliveries (workspace_id, updated_at)
    WHERE status = 'failed';
