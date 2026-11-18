-- Moving-average inventory valuation (Flow ERP M2, masterplan §7/§10): one cost pool per
-- workspace/product/warehouse, an immutable value entry per valued stock movement, and the
-- cost each order/invoice took per product and warehouse (returns restore it). Values are
-- integer minor units of the book's functional currency. Only active books write here.

CREATE TABLE inventory_cost_pools (
    id                   uuid          PRIMARY KEY DEFAULT uuidv7(),
    workspace_id         uuid          NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    product_id           uuid          NOT NULL,
    warehouse_id         uuid          NOT NULL,
    quantity             numeric(19,2) NOT NULL DEFAULT 0 CHECK (quantity >= 0),
    value                bigint        NOT NULL DEFAULT 0 CHECK (value >= 0),
    version              bigint        NOT NULL DEFAULT 1 CHECK (version >= 1),
    -- Chronology: costing dates never move behind the last valued date; same-date order is
    -- the sequence, not timestamps.
    last_accounting_date date,
    valuation_sequence   bigint        NOT NULL DEFAULT 0 CHECK (valuation_sequence >= 0),
    created_at           timestamptz   NOT NULL DEFAULT now(),
    updated_at           timestamptz   NOT NULL DEFAULT now(),
    CONSTRAINT inventory_cost_pools_empty_no_value CHECK (quantity > 0 OR value = 0),
    CONSTRAINT inventory_cost_pools_identity_key UNIQUE (workspace_id, product_id, warehouse_id),
    CONSTRAINT inventory_cost_pools_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT inventory_cost_pools_product_fk FOREIGN KEY (workspace_id, product_id)
        REFERENCES products (workspace_id, id),
    CONSTRAINT inventory_cost_pools_warehouse_fk FOREIGN KEY (workspace_id, warehouse_id)
        REFERENCES warehouses (workspace_id, id)
);

CREATE TABLE inventory_value_entries (
    id              uuid          PRIMARY KEY DEFAULT uuidv7(),
    workspace_id    uuid          NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    pool_id         uuid          NOT NULL,
    sequence        bigint        NOT NULL CHECK (sequence >= 1),
    movement_id     uuid          NOT NULL REFERENCES stock_movements (id),
    accounting_date date          NOT NULL,
    classification  text          NOT NULL CHECK (classification IN (
        'adjustment', 'sale_issue', 'customer_return', 'transfer_out', 'transfer_in',
        'pack_out', 'pack_in')),
    source_kind     text          CHECK (source_kind ~ '^[a-z_]{1,64}$'),
    source_id       uuid,
    quantity_before numeric(19,2) NOT NULL,
    quantity_delta  numeric(19,2) NOT NULL CHECK (quantity_delta <> 0),
    quantity_after  numeric(19,2) NOT NULL CHECK (quantity_after >= 0),
    value_before    bigint        NOT NULL,
    value_delta     bigint        NOT NULL,
    value_after     bigint        NOT NULL CHECK (value_after >= 0),
    journal_entry_id uuid,
    created_at      timestamptz   NOT NULL DEFAULT now(),
    CONSTRAINT inventory_value_entries_quantity_sum CHECK (quantity_after = quantity_before + quantity_delta),
    CONSTRAINT inventory_value_entries_value_sum CHECK (value_after = value_before + value_delta),
    CONSTRAINT inventory_value_entries_source_pair CHECK ((source_kind IS NULL) = (source_id IS NULL)),
    CONSTRAINT inventory_value_entries_sequence_key UNIQUE (pool_id, sequence),
    CONSTRAINT inventory_value_entries_movement_key UNIQUE (movement_id),
    CONSTRAINT inventory_value_entries_pool_fk FOREIGN KEY (workspace_id, pool_id)
        REFERENCES inventory_cost_pools (workspace_id, id),
    CONSTRAINT inventory_value_entries_journal_fk FOREIGN KEY (workspace_id, journal_entry_id)
        REFERENCES journal_entries (workspace_id, id)
);
CREATE INDEX inventory_value_entries_source_idx
    ON inventory_value_entries (workspace_id, source_kind, source_id) WHERE source_id IS NOT NULL;

-- Value entries are posted facts: same immutability as journals (workspace deletion cascades).
CREATE TRIGGER inventory_value_entries_fixed BEFORE UPDATE OR DELETE ON inventory_value_entries
    FOR EACH ROW EXECUTE FUNCTION accounting_posted_fixed();
CREATE TRIGGER inventory_value_entries_no_truncate BEFORE TRUNCATE ON inventory_value_entries
    FOR EACH STATEMENT EXECUTE FUNCTION accounting_posted_fixed();

-- Cumulative cost an order/invoice took per product and warehouse; returns restore from it.
CREATE TABLE inventory_cost_allocations (
    id                uuid          PRIMARY KEY DEFAULT uuidv7(),
    workspace_id      uuid          NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    source_kind       text          NOT NULL CHECK (source_kind IN ('order', 'invoice')),
    source_id         uuid          NOT NULL,
    product_id        uuid          NOT NULL,
    warehouse_id      uuid          NOT NULL,
    issued_quantity   numeric(19,2) NOT NULL CHECK (issued_quantity > 0),
    issued_value      bigint        NOT NULL CHECK (issued_value >= 0),
    returned_quantity numeric(19,2) NOT NULL DEFAULT 0 CHECK (returned_quantity >= 0),
    returned_value    bigint        NOT NULL DEFAULT 0 CHECK (returned_value >= 0),
    version           bigint        NOT NULL DEFAULT 1 CHECK (version >= 1),
    created_at        timestamptz   NOT NULL DEFAULT now(),
    updated_at        timestamptz   NOT NULL DEFAULT now(),
    CONSTRAINT inventory_cost_allocations_returned_cap CHECK (
        returned_quantity <= issued_quantity AND returned_value <= issued_value),
    CONSTRAINT inventory_cost_allocations_identity_key
        UNIQUE (workspace_id, source_kind, source_id, product_id, warehouse_id),
    CONSTRAINT inventory_cost_allocations_product_fk FOREIGN KEY (workspace_id, product_id)
        REFERENCES products (workspace_id, id),
    CONSTRAINT inventory_cost_allocations_warehouse_fk FOREIGN KEY (workspace_id, warehouse_id)
        REFERENCES warehouses (workspace_id, id)
);
