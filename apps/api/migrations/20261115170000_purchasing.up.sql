-- Purchasing (Flow ERP M3, masterplan §11/§7/§10): purchase orders, partial receipts that
-- accept goods without stock, bookings that add stock + value (DR inventory / CR GRNI) once
-- per receipt line, landed costs capitalised once per component, and closures of open
-- remainders. Quantities are base units, numeric(19,2). Amounts are integer minor units:
-- PO currency on orders, functional currency on booked receipt lines and landed costs.

-- Generic money helpers (money rule: the database computes, the API passes through). Every
-- rounding goes through money_round_div (half away from zero, exact).

-- `minor` units of a currency with `from_dp` decimals in minor units of one with `to_dp`.
-- `rate` = major `to` per major `from`; `inverse`: stored the other way (1 `to` = rate `from`).
CREATE FUNCTION money_convert(minor bigint, rate numeric, from_dp int, to_dp int, inverse boolean)
RETURNS bigint LANGUAGE sql IMMUTABLE STRICT PARALLEL SAFE
RETURN (CASE WHEN inverse
             THEN money_round_div(minor * 10::numeric ^ to_dp, rate * 10::numeric ^ from_dp)
             ELSE money_round_div(minor * rate * 10::numeric ^ to_dp, 10::numeric ^ from_dp)
        END)::bigint;

-- Split `total` over `ids` by `bases` (same order): each part money_round_div(total × base,
-- Σ bases), the last id (uuid order) takes the exact rest. No rows when Σ bases is 0.
CREATE FUNCTION money_allocate(total bigint, ids uuid[], bases bigint[])
RETURNS TABLE (id uuid, amount bigint) LANGUAGE sql STABLE STRICT AS $$
    WITH x AS (
        SELECT u.id, u.base, sum(u.base) OVER () AS whole,
               row_number() OVER (ORDER BY u.id DESC) = 1 AS last
        FROM unnest(ids, bases) AS u(id, base)
    ), s AS (
        SELECT x.id, x.last,
               CASE WHEN x.last THEN 0 ELSE money_round_div(total::numeric * x.base, x.whole)::bigint END AS part
        FROM x WHERE x.whole > 0
    )
    SELECT s.id, CASE WHEN s.last THEN total - sum(s.part) OVER () ELSE s.part END
    FROM s ORDER BY s.id
$$;

CREATE TABLE purchase_orders (
    id                   uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id         uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    number               bigint      NOT NULL CHECK (number > 0),
    state                text        NOT NULL DEFAULT 'draft'
                                     CHECK (state IN ('draft', 'confirmed', 'cancelled')),
    supplier_id          uuid        NOT NULL,
    currency_id          uuid        NOT NULL,
    purchase_date        date        NOT NULL,
    expected_date        date,
    reference            text        CHECK (char_length(reference) BETWEEN 1 AND 255),
    note                 text        CHECK (char_length(note) BETWEEN 1 AND 1000),
    default_warehouse_id uuid,
    -- Frozen at confirm: supplier name, addresses and terms as they were.
    supplier_snapshot    jsonb       CHECK (supplier_snapshot IS NULL
                                            OR jsonb_typeof(supplier_snapshot) = 'object'),
    version              bigint      NOT NULL DEFAULT 1 CHECK (version >= 1),
    created_by           uuid        NOT NULL,
    confirmed_by         uuid,
    confirmed_at         timestamptz,
    cancelled_by         uuid,
    cancelled_at         timestamptz,
    cancel_reason        text        CHECK (char_length(cancel_reason) BETWEEN 1 AND 1000),
    created_at           timestamptz NOT NULL DEFAULT now(),
    updated_at           timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT purchase_orders_expected_after CHECK (expected_date >= purchase_date),
    CONSTRAINT purchase_orders_confirmed_frozen CHECK (
        state = 'draft' OR cancelled_at IS NOT NULL
        OR (supplier_snapshot IS NOT NULL AND confirmed_at IS NOT NULL AND confirmed_by IS NOT NULL)),
    CONSTRAINT purchase_orders_cancelled_complete CHECK (
        (state = 'cancelled') = (cancelled_at IS NOT NULL AND cancelled_by IS NOT NULL
                                 AND cancel_reason IS NOT NULL)),
    CONSTRAINT purchase_orders_number_key UNIQUE (workspace_id, number),
    CONSTRAINT purchase_orders_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT purchase_orders_supplier_fk FOREIGN KEY (workspace_id, supplier_id)
        REFERENCES relations (workspace_id, id),
    CONSTRAINT purchase_orders_currency_fk FOREIGN KEY (workspace_id, currency_id)
        REFERENCES currencies (workspace_id, id),
    CONSTRAINT purchase_orders_warehouse_fk FOREIGN KEY (workspace_id, default_warehouse_id)
        REFERENCES warehouses (workspace_id, id)
);
CREATE INDEX purchase_orders_supplier_idx ON purchase_orders (workspace_id, supplier_id, state);

CREATE TABLE purchase_order_lines (
    id                uuid          PRIMARY KEY DEFAULT uuidv7(),
    workspace_id      uuid          NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    purchase_order_id uuid          NOT NULL,
    line_no           integer       NOT NULL CHECK (line_no BETWEEN 1 AND 100),
    product_id        uuid          NOT NULL,
    purchase_unit_id  uuid,
    input_quantity    numeric(19,2) NOT NULL CHECK (input_quantity > 0),
    -- Base units per input unit, frozen from the purchase unit (1 without one).
    ratio             numeric(20,8) NOT NULL CHECK (ratio > 0),
    base_quantity     numeric(19,2) NOT NULL CHECK (base_quantity > 0),
    unit_price        numeric(20,8) NOT NULL CHECK (unit_price >= 0),
    -- input_quantity × unit_price in PO-currency minor units: the authoritative amount.
    extension         bigint        NOT NULL CHECK (extension >= 0),
    warehouse_id      uuid,
    received_quantity numeric(19,2) NOT NULL DEFAULT 0 CHECK (received_quantity >= 0),
    booked_quantity   numeric(19,2) NOT NULL DEFAULT 0 CHECK (booked_quantity >= 0),
    closed_quantity   numeric(19,2) NOT NULL DEFAULT 0 CHECK (closed_quantity >= 0),
    -- Extension share already booked (PO-currency minor): partial bookings round cumulatively.
    booked_value      bigint        NOT NULL DEFAULT 0 CHECK (booked_value >= 0),
    CONSTRAINT purchase_order_lines_stocked_le_received CHECK (booked_quantity <= received_quantity),
    CONSTRAINT purchase_order_lines_no_overreceipt
        CHECK (received_quantity + closed_quantity <= base_quantity),
    CONSTRAINT purchase_order_lines_booked_value_cap CHECK (booked_value <= extension),
    CONSTRAINT purchase_order_lines_unit_ratio CHECK (purchase_unit_id IS NOT NULL OR ratio = 1),
    CONSTRAINT purchase_order_lines_line_no_key UNIQUE (purchase_order_id, line_no),
    CONSTRAINT purchase_order_lines_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT purchase_order_lines_order_fk FOREIGN KEY (workspace_id, purchase_order_id)
        REFERENCES purchase_orders (workspace_id, id) ON DELETE CASCADE,
    CONSTRAINT purchase_order_lines_product_fk FOREIGN KEY (workspace_id, product_id)
        REFERENCES products (workspace_id, id),
    CONSTRAINT purchase_order_lines_unit_fk FOREIGN KEY (workspace_id, purchase_unit_id)
        REFERENCES purchase_units (workspace_id, id),
    CONSTRAINT purchase_order_lines_warehouse_fk FOREIGN KEY (workspace_id, warehouse_id)
        REFERENCES warehouses (workspace_id, id)
);

-- Closed remainders (close-lines and cancel): history, never edited.
CREATE TABLE purchase_order_closures (
    id                uuid          PRIMARY KEY DEFAULT uuidv7(),
    workspace_id      uuid          NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    purchase_order_id uuid          NOT NULL,
    line_id           uuid          NOT NULL,
    quantity          numeric(19,2) NOT NULL CHECK (quantity > 0),
    reason            text          NOT NULL CHECK (char_length(reason) BETWEEN 1 AND 1000),
    created_by        uuid          NOT NULL,
    created_at        timestamptz   NOT NULL DEFAULT now(),
    CONSTRAINT purchase_order_closures_order_fk FOREIGN KEY (workspace_id, purchase_order_id)
        REFERENCES purchase_orders (workspace_id, id),
    CONSTRAINT purchase_order_closures_line_fk FOREIGN KEY (workspace_id, line_id)
        REFERENCES purchase_order_lines (workspace_id, id)
);
CREATE INDEX purchase_order_closures_order_idx ON purchase_order_closures (purchase_order_id);
CREATE TRIGGER purchase_order_closures_fixed BEFORE UPDATE OR DELETE ON purchase_order_closures
    FOR EACH ROW EXECUTE FUNCTION accounting_posted_fixed();

CREATE TABLE purchase_receipts (
    id                     uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id           uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    purchase_order_id      uuid        NOT NULL,
    number                 bigint      NOT NULL CHECK (number > 0),
    effective_receipt_date date        NOT NULL,
    note                   text        CHECK (char_length(note) BETWEEN 1 AND 1000),
    version                bigint      NOT NULL DEFAULT 1 CHECK (version >= 1),
    created_by             uuid        NOT NULL,
    recorded_at            timestamptz NOT NULL DEFAULT clock_timestamp(),
    CONSTRAINT purchase_receipts_number_key UNIQUE (workspace_id, number),
    CONSTRAINT purchase_receipts_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT purchase_receipts_order_fk FOREIGN KEY (workspace_id, purchase_order_id)
        REFERENCES purchase_orders (workspace_id, id)
);
CREATE INDEX purchase_receipts_order_idx ON purchase_receipts (purchase_order_id);

-- Accepted goods: gross, immutable quantity. Booking fills the booked_* columns once.
CREATE TABLE purchase_receipt_lines (
    id               uuid          PRIMARY KEY DEFAULT uuidv7(),
    workspace_id     uuid          NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    receipt_id       uuid          NOT NULL,
    po_line_id       uuid          NOT NULL,
    product_id       uuid          NOT NULL,
    warehouse_id     uuid          NOT NULL,
    bin_id           uuid,
    lot_id           uuid,
    base_quantity    numeric(19,2) NOT NULL CHECK (base_quantity > 0),
    -- Copies of the receipt's date and record time: one index holds the booking order.
    effective_date   date          NOT NULL,
    recorded_at      timestamptz   NOT NULL,
    -- Accepted after a later valued date of its pool (active book): kept as evidence, owns no
    -- guard, books only through the late-receipt review (M5).
    late             boolean       NOT NULL DEFAULT false,
    booked_at        timestamptz,
    booked_by        uuid,
    movement_id      uuid          UNIQUE REFERENCES stock_movements (id),
    accounting_date  date,
    -- Functional minor units (active books); NULL for setup bookings.
    net_value        bigint        CHECK (net_value >= 0),
    landed_value     bigint        CHECK (landed_value >= 0),
    rate             numeric(20,8) CHECK (rate > 0),
    rate_inverse     boolean,
    rate_source      text          CHECK (rate_source IN ('identity', 'workspace')),
    rate_date        date,
    journal_entry_id uuid,
    CONSTRAINT purchase_receipt_lines_booking_complete CHECK (
        (booked_at IS NULL) = (movement_id IS NULL)
        AND (booked_at IS NULL) = (booked_by IS NULL)
        AND (booked_at IS NULL) = (accounting_date IS NULL)),
    CONSTRAINT purchase_receipt_lines_valued_complete CHECK (
        (net_value IS NULL) = (landed_value IS NULL)
        AND (net_value IS NULL) = (rate IS NULL)
        AND (net_value IS NULL) = (rate_inverse IS NULL)
        AND (net_value IS NULL) = (rate_source IS NULL)
        AND (net_value IS NULL OR booked_at IS NOT NULL)),
    CONSTRAINT purchase_receipt_lines_late_unbooked CHECK (NOT late OR booked_at IS NULL),
    CONSTRAINT purchase_receipt_lines_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT purchase_receipt_lines_receipt_fk FOREIGN KEY (workspace_id, receipt_id)
        REFERENCES purchase_receipts (workspace_id, id),
    CONSTRAINT purchase_receipt_lines_po_line_fk FOREIGN KEY (workspace_id, po_line_id)
        REFERENCES purchase_order_lines (workspace_id, id),
    CONSTRAINT purchase_receipt_lines_product_fk FOREIGN KEY (workspace_id, product_id)
        REFERENCES products (workspace_id, id),
    CONSTRAINT purchase_receipt_lines_warehouse_fk FOREIGN KEY (workspace_id, warehouse_id)
        REFERENCES warehouses (workspace_id, id),
    CONSTRAINT purchase_receipt_lines_bin_fk FOREIGN KEY (workspace_id, bin_id)
        REFERENCES warehouse_section_row_bins (workspace_id, id),
    CONSTRAINT purchase_receipt_lines_lot_fk FOREIGN KEY (workspace_id, lot_id)
        REFERENCES lots (workspace_id, id),
    CONSTRAINT purchase_receipt_lines_journal_fk FOREIGN KEY (workspace_id, journal_entry_id)
        REFERENCES journal_entries (workspace_id, id)
);
CREATE INDEX purchase_receipt_lines_receipt_idx ON purchase_receipt_lines (receipt_id);
-- Pending accepted-unbooked receipts per pool, in eligible booking order (masterplan §7).
CREATE INDEX purchase_receipt_lines_pending_idx
    ON purchase_receipt_lines (workspace_id, warehouse_id, product_id, effective_date, recorded_at, id)
    WHERE booked_at IS NULL AND NOT late;

-- Whether an active book holds accepted-unbooked receipts in this warehouse (for this product,
-- or any when NULL). Sale availability, the stock-layer guard and count start share it.
CREATE FUNCTION purchase_receipt_pending(ws uuid, warehouse uuid, product uuid)
RETURNS boolean LANGUAGE sql STABLE AS $$
    SELECT EXISTS (
        SELECT 1 FROM purchase_receipt_lines l
        JOIN accounting_settings a ON a.workspace_id = l.workspace_id AND a.state = 'active'
        WHERE l.workspace_id = ws AND l.warehouse_id = warehouse
          AND (product IS NULL OR l.product_id = product)
          AND l.booked_at IS NULL AND NOT l.late)
$$;

-- A freight/duty component capitalised into one booking; its ref is spent for the whole PO.
CREATE TABLE purchase_landed_costs (
    id                    uuid          PRIMARY KEY DEFAULT uuidv7(),
    workspace_id          uuid          NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    purchase_order_id     uuid          NOT NULL,
    receipt_id            uuid          NOT NULL,
    client_component_ref  uuid          NOT NULL,
    amount                bigint        NOT NULL CHECK (amount > 0),
    currency_id           uuid          NOT NULL,
    functional_amount     bigint        NOT NULL CHECK (functional_amount >= 0),
    rate                  numeric(20,8) NOT NULL CHECK (rate > 0),
    rate_inverse          boolean       NOT NULL,
    rate_source           text          NOT NULL CHECK (rate_source IN ('identity', 'workspace')),
    rate_date             date,
    evidence_ref          text          NOT NULL CHECK (char_length(evidence_ref) BETWEEN 1 AND 255),
    created_by            uuid          NOT NULL,
    created_at            timestamptz   NOT NULL DEFAULT now(),
    CONSTRAINT purchase_landed_costs_component_key
        UNIQUE (workspace_id, purchase_order_id, client_component_ref),
    CONSTRAINT purchase_landed_costs_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT purchase_landed_costs_order_fk FOREIGN KEY (workspace_id, purchase_order_id)
        REFERENCES purchase_orders (workspace_id, id),
    CONSTRAINT purchase_landed_costs_receipt_fk FOREIGN KEY (workspace_id, receipt_id)
        REFERENCES purchase_receipts (workspace_id, id),
    CONSTRAINT purchase_landed_costs_currency_fk FOREIGN KEY (workspace_id, currency_id)
        REFERENCES currencies (workspace_id, id)
);
CREATE INDEX purchase_landed_costs_receipt_idx ON purchase_landed_costs (receipt_id);
CREATE TRIGGER purchase_landed_costs_fixed BEFORE UPDATE OR DELETE ON purchase_landed_costs
    FOR EACH ROW EXECUTE FUNCTION accounting_posted_fixed();

CREATE TABLE purchase_landed_cost_allocations (
    id                uuid    PRIMARY KEY DEFAULT uuidv7(),
    workspace_id      uuid    NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    landed_cost_id    uuid    NOT NULL,
    receipt_line_id   uuid    NOT NULL,
    functional_amount bigint  NOT NULL CHECK (functional_amount >= 0),
    CONSTRAINT purchase_landed_cost_allocations_key UNIQUE (landed_cost_id, receipt_line_id),
    CONSTRAINT purchase_landed_cost_allocations_cost_fk FOREIGN KEY (workspace_id, landed_cost_id)
        REFERENCES purchase_landed_costs (workspace_id, id),
    CONSTRAINT purchase_landed_cost_allocations_line_fk FOREIGN KEY (workspace_id, receipt_line_id)
        REFERENCES purchase_receipt_lines (workspace_id, id)
);
CREATE TRIGGER purchase_landed_cost_allocations_fixed
    BEFORE UPDATE OR DELETE ON purchase_landed_cost_allocations
    FOR EACH ROW EXECUTE FUNCTION accounting_posted_fixed();

ALTER TABLE document_counters DROP CONSTRAINT document_counters_series_check,
    ADD CONSTRAINT document_counters_series_check
        CHECK (series IN ('invoice', 'order', 'journal', 'purchase_order', 'purchase_receipt'));

ALTER TABLE inventory_value_entries DROP CONSTRAINT inventory_value_entries_classification_check,
    ADD CONSTRAINT inventory_value_entries_classification_check CHECK (classification IN (
        'adjustment', 'sale_issue', 'customer_return', 'transfer_out', 'transfer_in',
        'pack_out', 'pack_in', 'purchase_receipt'));
