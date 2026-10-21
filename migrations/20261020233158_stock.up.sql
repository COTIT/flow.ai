-- Batch 6F: stock levels per product, warehouse and optional bin; the append-only movement
-- ledger; storage locations (Laravel ERP/Stock, ERP/StorageLocation). Quantities keep
-- Laravel's decimal(19,2) scale; every reference is a composite FK inside the workspace.

-- Stocks point at a bin of their own warehouse and shelf.
ALTER TABLE warehouse_section_row_bins
    ADD CONSTRAINT warehouse_section_row_bins_stock_key UNIQUE (workspace_id, warehouse_id, shelf_id, id);

CREATE TABLE stocks (
    id                  uuid          PRIMARY KEY DEFAULT uuidv7(),
    workspace_id        uuid          NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    product_id          uuid          NOT NULL,
    warehouse_id        uuid          NOT NULL,
    -- Both null = the warehouse-level row; both set = a bin row (shelf = the bin's shelf).
    shelf_id            uuid,
    bin_id              uuid,
    -- Store warehouses may go negative (Laravel MutateStock), so no lower bound here.
    stock_lvl           numeric(19,2) NOT NULL DEFAULT 0,
    reserved            numeric(19,2) NOT NULL DEFAULT 0,
    minimal_stock_level integer       NOT NULL DEFAULT 0 CHECK (minimal_stock_level >= 0),
    maximum_stock_level integer       NOT NULL DEFAULT 0 CHECK (maximum_stock_level >= 0),
    meta                jsonb         CHECK (meta IS NULL OR jsonb_typeof(meta) = 'object'),
    created_at          timestamptz   NOT NULL DEFAULT now(),
    updated_at          timestamptz   NOT NULL DEFAULT now(),
    deleted_at          timestamptz,
    CONSTRAINT stocks_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT stocks_bin_pair CHECK ((shelf_id IS NULL) = (bin_id IS NULL)),
    CONSTRAINT stocks_product_fk FOREIGN KEY (workspace_id, product_id)
        REFERENCES products (workspace_id, id),
    CONSTRAINT stocks_warehouse_fk FOREIGN KEY (workspace_id, warehouse_id)
        REFERENCES warehouses (workspace_id, id),
    CONSTRAINT stocks_bin_fk FOREIGN KEY (workspace_id, warehouse_id, shelf_id, bin_id)
        REFERENCES warehouse_section_row_bins (workspace_id, warehouse_id, shelf_id, id)
-- Room on each page for HOT updates: every stock write rewrites its row.
) WITH (fillfactor = 85);
-- One live row per product, warehouse and bin (Laravel had none: mutate picked "first").
CREATE UNIQUE INDEX stocks_slot_unique ON stocks (product_id, warehouse_id, bin_id)
    NULLS NOT DISTINCT WHERE deleted_at IS NULL;
-- Plain (not partial) indexes: FK checks and cascades can't use partial ones. No index
-- mentions stock_lvl, so level writes stay HOT updates.
CREATE INDEX stocks_product_idx ON stocks (workspace_id, product_id);
CREATE INDEX stocks_warehouse_idx ON stocks (workspace_id, warehouse_id);
CREATE INDEX stocks_bin_idx ON stocks (bin_id) WHERE bin_id IS NOT NULL;

-- Append-only: every stock change writes one row in the same transaction.
CREATE TABLE stock_movements (
    id                         uuid          PRIMARY KEY DEFAULT uuidv7(),
    workspace_id               uuid          NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    stock_id                   uuid          NOT NULL,
    product_id                 uuid          NOT NULL,
    warehouse_id               uuid          NOT NULL,
    shelf_id                   uuid,
    bin_id                     uuid,
    movement_type              text          NOT NULL CHECK (movement_type IN
                                             ('increase', 'decrease', 'set_level', 'set_requirements')),
    quantity_change            numeric(19,2),
    quantity_before            numeric(19,2),
    quantity_after             numeric(19,2),
    minimal_stock_level_before integer,
    minimal_stock_level_after  integer,
    maximum_stock_level_before integer,
    maximum_stock_level_after  integer,
    reason                     text          CHECK (char_length(reason) <= 255),
    is_correction              boolean       NOT NULL DEFAULT false,
    user_id                    uuid          REFERENCES users (id) ON DELETE SET NULL,
    -- Who caused it (order, invoice, ...): polymorphic, so no FK.
    related_domain             text          CHECK (char_length(related_domain) <= 255),
    related_id                 uuid,
    meta                       jsonb         CHECK (meta IS NULL OR jsonb_typeof(meta) = 'object'),
    created_at                 timestamptz   NOT NULL DEFAULT clock_timestamp(),
    CONSTRAINT stock_movements_stock_fk FOREIGN KEY (workspace_id, stock_id)
        REFERENCES stocks (workspace_id, id),
    CONSTRAINT stock_movements_product_fk FOREIGN KEY (workspace_id, product_id)
        REFERENCES products (workspace_id, id),
    CONSTRAINT stock_movements_warehouse_fk FOREIGN KEY (workspace_id, warehouse_id)
        REFERENCES warehouses (workspace_id, id)
);
CREATE INDEX stock_movements_product_idx ON stock_movements (product_id, created_at DESC, id DESC);
CREATE INDEX stock_movements_stock_idx ON stock_movements (stock_id);
CREATE INDEX stock_movements_warehouse_idx ON stock_movements (warehouse_id);
CREATE INDEX stock_movements_user_idx ON stock_movements (user_id) WHERE user_id IS NOT NULL;

-- A product's place in a warehouse location (Laravel parent_domain is always 'product').
CREATE TABLE storage_locations (
    id                    uuid          PRIMARY KEY DEFAULT uuidv7(),
    workspace_id          uuid          NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    parent_domain         text          NOT NULL CHECK (parent_domain = 'product'),
    parent_id             uuid          NOT NULL,
    warehouse_location_id uuid          NOT NULL,
    description           text          CHECK (char_length(description) <= 1500),
    meta                  jsonb         CHECK (meta IS NULL OR jsonb_typeof(meta) = 'object'),
    created_at            timestamptz   NOT NULL DEFAULT now(),
    updated_at            timestamptz   NOT NULL DEFAULT now(),
    deleted_at            timestamptz,
    CONSTRAINT storage_locations_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT storage_locations_product_fk FOREIGN KEY (workspace_id, parent_id)
        REFERENCES products (workspace_id, id),
    CONSTRAINT storage_locations_location_fk FOREIGN KEY (workspace_id, warehouse_location_id)
        REFERENCES warehouse_locations (workspace_id, id)
);
CREATE INDEX storage_locations_parent_idx ON storage_locations (parent_id) WHERE deleted_at IS NULL;
CREATE INDEX storage_locations_location_idx ON storage_locations (warehouse_location_id)
    WHERE deleted_at IS NULL;
