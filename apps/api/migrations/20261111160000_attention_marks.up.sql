-- Flow noticed: one user's acknowledge or snooze of one attention item. Display state only:
-- an item resolves when its condition clears, never by a mark. No history; pruned hourly.
CREATE TABLE attention_marks (
    id            uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id  uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    user_id       uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    kind          text        NOT NULL CHECK (kind IN ('stock.below_minimum', 'orders.open',
                      'invoices.overdue', 'till.open_shifts', 'lots.quarantined', 'webhooks.failed')),
    -- Product, order, invoice, shift, lot or delivery id; checked in the workspace on write.
    ref_id        uuid        NOT NULL,
    state         text        NOT NULL CHECK (state IN ('acknowledged', 'snoozed')),
    snoozed_until timestamptz,
    created_at    timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT attention_marks_snooze CHECK ((state = 'snoozed') = (snoozed_until IS NOT NULL)),
    CONSTRAINT attention_marks_one UNIQUE (workspace_id, user_id, kind, ref_id)
);
-- FK cascade from users.
CREATE INDEX attention_marks_user ON attention_marks (user_id);

-- Does `ref` name a live record of `k`'s type in workspace `ws`? Mark writes and the prune share it.
CREATE FUNCTION attention_ref_exists(ws uuid, k text, ref uuid) RETURNS boolean
LANGUAGE sql STABLE AS $$
    SELECT CASE k
        WHEN 'stock.below_minimum' THEN EXISTS (SELECT 1 FROM products
            WHERE workspace_id = ws AND id = ref AND deleted_at IS NULL AND has_stock)
        WHEN 'orders.open' THEN EXISTS (SELECT 1 FROM orders WHERE workspace_id = ws AND id = ref)
        WHEN 'invoices.overdue' THEN EXISTS (SELECT 1 FROM invoices
            WHERE workspace_id = ws AND id = ref AND deleted_at IS NULL)
        WHEN 'till.open_shifts' THEN EXISTS (SELECT 1 FROM drawer_shifts
            WHERE workspace_id = ws AND id = ref)
        WHEN 'lots.quarantined' THEN EXISTS (SELECT 1 FROM lots WHERE workspace_id = ws AND id = ref)
        WHEN 'webhooks.failed' THEN EXISTS (SELECT 1 FROM webhook_deliveries
            WHERE workspace_id = ws AND id = ref)
        ELSE false
    END
$$;
