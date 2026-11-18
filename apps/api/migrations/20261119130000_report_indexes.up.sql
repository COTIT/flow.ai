-- Flow ERP M5-D historical reports: open items by issue date (aging rebuilds residuals at
-- as_of, so the residual > 0 index cannot serve it) and relation-tagged journal lines
-- (party ledger).
CREATE INDEX accounting_open_items_issue_idx ON accounting_open_items (workspace_id, issue_date);
CREATE INDEX journal_lines_relation_idx ON journal_lines (workspace_id, relation_id)
    WHERE relation_id IS NOT NULL;
