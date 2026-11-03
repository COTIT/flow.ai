-- Order trail: one row per action on an order, with who did it. Append-only; only orders
-- touched after this migration have rows (no backfill). Confirmed and picked times on the
-- order itself, null on older orders.
ALTER TABLE orders
    ADD COLUMN confirmed_at timestamptz,
    ADD COLUMN picked_at    timestamptz;

CREATE TABLE order_events (
    id           uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    order_id     uuid        NOT NULL,
    kind         text        NOT NULL CHECK (kind IN ('created', 'confirmed', 'paid', 'picked',
                             'completed', 'cancelled', 'expired', 'returned', 'refunded',
                             'invoiced')),
    -- Who did it: at most one; none = the system (expiry, a provider's webhook).
    user_id      uuid        REFERENCES users (id) ON DELETE SET NULL,
    till_user_id uuid,
    app_key_id   uuid,
    meta         jsonb       CHECK (meta IS NULL OR (jsonb_typeof(meta) = 'object'
                                    AND octet_length(meta::text) <= 4096)),
    created_at   timestamptz NOT NULL DEFAULT clock_timestamp(),
    CONSTRAINT order_events_one_actor CHECK (num_nonnulls(user_id, till_user_id, app_key_id) <= 1),
    CONSTRAINT order_events_order_fk FOREIGN KEY (workspace_id, order_id)
        REFERENCES orders (workspace_id, id) ON DELETE CASCADE,
    CONSTRAINT order_events_till_user_fk FOREIGN KEY (workspace_id, till_user_id)
        REFERENCES till_users (workspace_id, id) ON DELETE SET NULL (till_user_id),
    CONSTRAINT order_events_app_key_fk FOREIGN KEY (workspace_id, app_key_id)
        REFERENCES app_keys (workspace_id, id) ON DELETE SET NULL (app_key_id)
);
CREATE INDEX order_events_order_idx ON order_events (workspace_id, order_id, created_at DESC, id DESC);
CREATE INDEX order_events_user_idx ON order_events (user_id) WHERE user_id IS NOT NULL;
CREATE INDEX order_events_till_user_idx ON order_events (workspace_id, till_user_id)
    WHERE till_user_id IS NOT NULL;
CREATE INDEX order_events_app_key_idx ON order_events (workspace_id, app_key_id)
    WHERE app_key_id IS NOT NULL;
