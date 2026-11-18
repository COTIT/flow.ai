-- Stocktakes (Flow ERP M6, masterplan D10/§10): a counting row pauses its warehouse until the
-- count is posted or cancelled. Lines are the stock rows frozen at start; they stay as history.
CREATE TABLE stocktakes (
    id              uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id    uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    warehouse_id    uuid        NOT NULL REFERENCES warehouses (id),
    state           text        NOT NULL DEFAULT 'draft'
                                CHECK (state IN ('draft', 'counting', 'posted', 'cancelled')),
    version         bigint      NOT NULL DEFAULT 1 CHECK (version >= 1),
    reason          text        NOT NULL CHECK (char_length(reason) BETWEEN 1 AND 1000),
    cancel_reason   text        CHECK (char_length(cancel_reason) BETWEEN 1 AND 1000),
    accounting_date date,
    created_by      uuid        NOT NULL,
    created_at      timestamptz NOT NULL DEFAULT now(),
    started_by      uuid,
    started_at      timestamptz,
    posted_by       uuid,
    posted_at       timestamptz,
    cancelled_by    uuid,
    cancelled_at    timestamptz,
    updated_at      timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT stocktakes_workspace_id_id_key UNIQUE (workspace_id, id)
);
CREATE INDEX stocktakes_workspace_idx ON stocktakes (workspace_id, created_at DESC);
-- The pause: one counting stocktake per warehouse; stock writers look it up here.
CREATE UNIQUE INDEX stocktakes_counting_key ON stocktakes (warehouse_id) WHERE state = 'counting';

CREATE TABLE stocktake_lines (
    id                        uuid          PRIMARY KEY DEFAULT uuidv7(),
    workspace_id              uuid          NOT NULL,
    stocktake_id              uuid          NOT NULL,
    stock_id                  uuid          NOT NULL REFERENCES stocks (id),
    product_id                uuid          NOT NULL,
    shelf_id                  uuid,
    bin_id                    uuid,
    lot_id                    uuid,
    expected                  numeric(19,2) NOT NULL,
    counted                   numeric(19,2) CHECK (counted >= 0),
    -- Functional minor units; read only when an active book adds stock to an empty pool.
    positive_adjustment_value bigint        CHECK (positive_adjustment_value >= 0),
    counted_by                uuid,
    counted_at                timestamptz,
    CONSTRAINT stocktake_lines_stock_key UNIQUE (stocktake_id, stock_id),
    CONSTRAINT stocktake_lines_stocktake_fk FOREIGN KEY (workspace_id, stocktake_id)
        REFERENCES stocktakes (workspace_id, id) ON DELETE CASCADE
);
