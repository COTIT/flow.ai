-- Flow ERP M5-A payables (masterplan §11/§13/§15): supplier bills matched to booked receipt
-- lines and landed components, supplier payments and refunds received, purchase returns,
-- purchase cost adjustments. Amounts are integer minor units: document currency on bills,
-- payments and returns (nominal), functional currency in *_functional / functional columns.
-- Posted rows are facts: a guard trigger lets them move only to `reversed` (appended effects
-- carry the economics). Capacity (B − U − M − D) is read from these rows under receipt-line
-- locks; every amount comes from SQL (money_round_div / money_convert / money_allocate).

-- PO-currency share of a booking (`purchase_order_lines.booked_value` step). NULL on lines
-- booked before M5-A: those can't be billed or returned (pre-release, no backfill).
ALTER TABLE purchase_receipt_lines ADD COLUMN doc_value bigint CHECK (doc_value >= 0);

-- === M5-A CHECK block: document_counters series (base main + supplier_bill, supplier_payment,
-- purchase_return) ===
ALTER TABLE document_counters DROP CONSTRAINT document_counters_series_check,
    ADD CONSTRAINT document_counters_series_check CHECK (series IN ('invoice', 'order', 'journal',
        'purchase_order', 'purchase_receipt', 'supplier_bill', 'supplier_payment', 'purchase_return'));
-- === end M5-A CHECK block ===

-- === M5-A CHECK block: inventory_value_entries classification (base main + supplier_return,
-- supplier_return_reversal, cost_adjustment) and value-only cost_adjustment rows ===
ALTER TABLE inventory_value_entries DROP CONSTRAINT inventory_value_entries_classification_check,
    ADD CONSTRAINT inventory_value_entries_classification_check CHECK (classification IN (
        'adjustment', 'sale_issue', 'customer_return', 'transfer_out', 'transfer_in',
        'pack_out', 'pack_in', 'purchase_receipt',
        'supplier_return', 'supplier_return_reversal', 'cost_adjustment')),
    ALTER COLUMN movement_id DROP NOT NULL,
    DROP CONSTRAINT inventory_value_entries_quantity_delta_check,
    ADD CONSTRAINT inventory_value_entries_value_only_check CHECK (
        CASE WHEN classification = 'cost_adjustment'
             THEN movement_id IS NULL AND quantity_delta = 0 AND value_delta <> 0
             ELSE movement_id IS NOT NULL AND quantity_delta <> 0 END);
-- === end M5-A CHECK block ===

CREATE TABLE supplier_bills (
    id                      uuid          PRIMARY KEY DEFAULT uuidv7(),
    workspace_id            uuid          NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    number                  bigint        NOT NULL CHECK (number > 0),
    state                   text          NOT NULL DEFAULT 'draft'
                                          CHECK (state IN ('draft', 'posted', 'reversed')),
    supplier_id             uuid          NOT NULL,
    currency_id             uuid          NOT NULL,
    supplier_invoice_number text          NOT NULL
                                          CHECK (char_length(btrim(supplier_invoice_number)) BETWEEN 1 AND 255),
    -- The reversed bill whose supplier invoice number this one reuses (explicit correction).
    corrects_bill_id        uuid,
    invoice_date            date          NOT NULL,
    due_date                date,
    accounting_date         date          NOT NULL,
    evidence_ref            text          CHECK (char_length(evidence_ref) BETWEEN 1 AND 255),
    note                    text          CHECK (char_length(note) BETWEEN 1 AND 1000),
    -- Frozen at post: bill currency → functional rate on accounting_date, totals.
    rate                    numeric(20,8) CHECK (rate > 0),
    rate_inverse            boolean,
    rate_source             text          CHECK (rate_source IN ('identity', 'workspace')),
    rate_date               date,
    net                     bigint        CHECK (net >= 0),
    tax                     bigint        CHECK (tax >= 0),
    total                   bigint        CHECK (total > 0),
    functional_total        bigint        CHECK (functional_total > 0),
    ap_item_id              uuid,
    posted_effect_id        uuid,
    posted_by               uuid,
    posted_at               timestamptz,
    reversal_date           date,
    reverse_reason          text          CHECK (char_length(btrim(reverse_reason)) BETWEEN 1 AND 1000),
    reversal_effect_id      uuid,
    reversed_by             uuid,
    reversed_at             timestamptz,
    version                 bigint        NOT NULL DEFAULT 1 CHECK (version >= 1),
    created_by              uuid          NOT NULL,
    created_at              timestamptz   NOT NULL DEFAULT now(),
    updated_at              timestamptz   NOT NULL DEFAULT now(),
    CONSTRAINT supplier_bills_number_key UNIQUE (workspace_id, number),
    CONSTRAINT supplier_bills_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT supplier_bills_due_check CHECK (due_date >= invoice_date),
    CONSTRAINT supplier_bills_posted_complete CHECK (
        (posted_at IS NULL) = (posted_effect_id IS NULL)
        AND (posted_at IS NULL) = (posted_by IS NULL)
        AND (posted_at IS NULL) = (ap_item_id IS NULL)
        AND (posted_at IS NULL) = (rate IS NULL)
        AND (posted_at IS NULL) = (functional_total IS NULL)
        AND (state <> 'posted' OR posted_at IS NOT NULL)
        AND (state <> 'draft' OR posted_at IS NULL)),
    CONSTRAINT supplier_bills_reversed_complete CHECK (
        (state = 'reversed') = (reversed_at IS NOT NULL)
        AND (reversed_at IS NULL) = (reversed_by IS NULL)
        AND (reversed_at IS NULL) = (reverse_reason IS NULL)
        AND (reversal_effect_id IS NULL OR reversed_at IS NOT NULL)
        AND ((reversal_effect_id IS NULL) = (posted_at IS NULL OR reversed_at IS NULL))),
    CONSTRAINT supplier_bills_supplier_fk FOREIGN KEY (workspace_id, supplier_id)
        REFERENCES relations (workspace_id, id),
    CONSTRAINT supplier_bills_currency_fk FOREIGN KEY (workspace_id, currency_id)
        REFERENCES currencies (workspace_id, id),
    CONSTRAINT supplier_bills_corrects_fk FOREIGN KEY (workspace_id, corrects_bill_id)
        REFERENCES supplier_bills (workspace_id, id),
    CONSTRAINT supplier_bills_ap_item_fk FOREIGN KEY (workspace_id, ap_item_id)
        REFERENCES accounting_open_items (workspace_id, id),
    CONSTRAINT supplier_bills_posted_effect_fk FOREIGN KEY (workspace_id, posted_effect_id)
        REFERENCES accounting_effects (workspace_id, id),
    CONSTRAINT supplier_bills_reversal_effect_fk FOREIGN KEY (workspace_id, reversal_effect_id)
        REFERENCES accounting_effects (workspace_id, id)
);
-- Duplicate supplier invoice: one live bill per supplier, currency and number in the book.
CREATE UNIQUE INDEX supplier_bills_invoice_key ON supplier_bills
    (workspace_id, supplier_id, currency_id, lower(btrim(supplier_invoice_number)))
    WHERE state <> 'reversed';
CREATE INDEX supplier_bills_list_idx ON supplier_bills (workspace_id, number DESC);

-- goods: receipt-line matches carry the quantity; landed_cost: clears one landed component
-- (quantity 1); expense: no stock, booked to its expense account.
CREATE TABLE supplier_bill_lines (
    id                 uuid          PRIMARY KEY DEFAULT uuidv7(),
    workspace_id       uuid          NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    bill_id            uuid          NOT NULL,
    line_no            integer       NOT NULL CHECK (line_no BETWEEN 1 AND 100),
    kind               text          NOT NULL CHECK (kind IN ('goods', 'landed_cost', 'expense')),
    product_id         uuid,
    landed_cost_id     uuid,
    expense_account_id uuid,
    quantity           numeric(19,2) NOT NULL CHECK (quantity > 0),
    unit_price         numeric(20,8) NOT NULL CHECK (unit_price >= 0),
    -- quantity × unit_price in bill-currency minor units (SQL rounding).
    extension          bigint        NOT NULL CHECK (extension >= 0),
    tax_profile_id     uuid,
    description        text          CHECK (char_length(description) BETWEEN 1 AND 255),
    -- Frozen at post (bill currency / functional).
    tax_recoverable    bigint        CHECK (tax_recoverable >= 0),
    tax_nonrecoverable bigint        CHECK (tax_nonrecoverable >= 0),
    functional         bigint        CHECK (functional >= 0),
    CONSTRAINT supplier_bill_lines_kind_refs CHECK (CASE kind
        WHEN 'goods' THEN product_id IS NOT NULL AND landed_cost_id IS NULL
                          AND expense_account_id IS NULL
        WHEN 'landed_cost' THEN landed_cost_id IS NOT NULL AND product_id IS NULL
                          AND expense_account_id IS NULL AND quantity = 1
        ELSE expense_account_id IS NOT NULL AND product_id IS NULL AND landed_cost_id IS NULL END),
    CONSTRAINT supplier_bill_lines_line_no_key UNIQUE (bill_id, line_no),
    CONSTRAINT supplier_bill_lines_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT supplier_bill_lines_bill_fk FOREIGN KEY (workspace_id, bill_id)
        REFERENCES supplier_bills (workspace_id, id) ON DELETE CASCADE,
    CONSTRAINT supplier_bill_lines_product_fk FOREIGN KEY (workspace_id, product_id)
        REFERENCES products (workspace_id, id),
    CONSTRAINT supplier_bill_lines_landed_fk FOREIGN KEY (workspace_id, landed_cost_id)
        REFERENCES purchase_landed_costs (workspace_id, id),
    CONSTRAINT supplier_bill_lines_account_fk FOREIGN KEY (workspace_id, expense_account_id)
        REFERENCES accounts (workspace_id, id),
    CONSTRAINT supplier_bill_lines_tax_profile_fk FOREIGN KEY (workspace_id, tax_profile_id)
        REFERENCES tax_profiles (workspace_id, id)
);
CREATE INDEX supplier_bill_lines_bill_idx ON supplier_bill_lines (bill_id);

-- Tax per `tax_line` component, frozen at post.
CREATE TABLE supplier_bill_line_taxes (
    id             uuid         PRIMARY KEY DEFAULT uuidv7(),
    workspace_id   uuid         NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    bill_line_id   uuid         NOT NULL,
    component_no   smallint     NOT NULL,
    code           text         NOT NULL,
    rate           numeric      NOT NULL,
    base           bigint       NOT NULL,
    tax            bigint       NOT NULL CHECK (tax >= 0),
    recoverable    boolean      NOT NULL,
    exempt         boolean      NOT NULL,
    CONSTRAINT supplier_bill_line_taxes_key UNIQUE (bill_line_id, component_no),
    CONSTRAINT supplier_bill_line_taxes_line_fk FOREIGN KEY (workspace_id, bill_line_id)
        REFERENCES supplier_bill_lines (workspace_id, id) ON DELETE CASCADE
);

-- A bill line's claim on a booked receipt line (goods) or on a landed component. draft =
-- reservation (D), posted = matched (M), reversed = released by the bill reversal.
CREATE TABLE supplier_bill_matches (
    id               uuid          PRIMARY KEY DEFAULT uuidv7(),
    workspace_id     uuid          NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    bill_id          uuid          NOT NULL,
    bill_line_id     uuid          NOT NULL,
    receipt_line_id  uuid,
    landed_cost_id   uuid,
    quantity         numeric(19,2) NOT NULL CHECK (quantity > 0),
    state            text          NOT NULL DEFAULT 'draft'
                                   CHECK (state IN ('draft', 'posted', 'reversed')),
    -- Frozen at post: GRNI relieved (nominal PO currency / functional), the functional share of
    -- AP it stands for, realized FX and pending purchase cost variance (share = g + fx + v).
    grni_nominal     bigint,
    grni_functional  bigint,
    share_functional bigint,
    fx               bigint,
    variance         bigint,
    CONSTRAINT supplier_bill_matches_target_check
        CHECK (num_nonnulls(receipt_line_id, landed_cost_id) = 1),
    CONSTRAINT supplier_bill_matches_posted_check CHECK ((state = 'draft') = (grni_functional IS NULL)
        AND (grni_functional IS NULL) = (grni_nominal IS NULL)
        AND (grni_functional IS NULL) = (share_functional IS NULL)
        AND (grni_functional IS NULL) = (fx IS NULL)
        AND (grni_functional IS NULL) = (variance IS NULL)
        AND (grni_functional IS NULL OR share_functional = grni_functional + fx + variance)),
    CONSTRAINT supplier_bill_matches_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT supplier_bill_matches_line_key UNIQUE (bill_line_id, receipt_line_id),
    CONSTRAINT supplier_bill_matches_bill_fk FOREIGN KEY (workspace_id, bill_id)
        REFERENCES supplier_bills (workspace_id, id) ON DELETE CASCADE,
    CONSTRAINT supplier_bill_matches_line_fk FOREIGN KEY (workspace_id, bill_line_id)
        REFERENCES supplier_bill_lines (workspace_id, id) ON DELETE CASCADE,
    CONSTRAINT supplier_bill_matches_receipt_line_fk FOREIGN KEY (workspace_id, receipt_line_id)
        REFERENCES purchase_receipt_lines (workspace_id, id),
    CONSTRAINT supplier_bill_matches_landed_fk FOREIGN KEY (workspace_id, landed_cost_id)
        REFERENCES purchase_landed_costs (workspace_id, id)
);
CREATE INDEX supplier_bill_matches_receipt_line_idx ON supplier_bill_matches (receipt_line_id)
    WHERE receipt_line_id IS NOT NULL;
-- A landed component is billed once (draft reservations included).
CREATE UNIQUE INDEX supplier_bill_matches_landed_key ON supplier_bill_matches (landed_cost_id)
    WHERE landed_cost_id IS NOT NULL AND state <> 'reversed';
CREATE INDEX supplier_bill_matches_bill_idx ON supplier_bill_matches (bill_id);

-- payment: money out to the supplier, opens a supplier_credit (debit) item.
-- refund_received: money back from the supplier, opens a payable (credit) item.
CREATE TABLE supplier_payments (
    id                  uuid          PRIMARY KEY DEFAULT uuidv7(),
    workspace_id        uuid          NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    number              bigint        NOT NULL CHECK (number > 0),
    direction           text          NOT NULL CHECK (direction IN ('payment', 'refund_received')),
    state               text          NOT NULL DEFAULT 'posted'
                                      CHECK (state IN ('posted', 'reversed')),
    supplier_id         uuid          NOT NULL,
    currency_id         uuid          NOT NULL,
    amount              bigint        NOT NULL CHECK (amount > 0),
    date                date          NOT NULL,
    payment_reference   text          CHECK (char_length(payment_reference) BETWEEN 1 AND 255),
    payment_evidence_ref text         NOT NULL
                                      CHECK (char_length(btrim(payment_evidence_ref)) BETWEEN 1 AND 255),
    note                text          CHECK (char_length(note) BETWEEN 1 AND 1000),
    -- MoneyLocation columns (same names as invoice_payments).
    location_kind       text          NOT NULL CHECK (location_kind IN ('cash_place', 'bank_account')),
    register_id         uuid,
    vault_id            uuid,
    bank_account_id     uuid,
    reference           text          CHECK (char_length(reference) BETWEEN 1 AND 255),
    evidence_ref        text          CHECK (char_length(evidence_ref) BETWEEN 1 AND 255),
    confirmation_method text,
    rate                numeric(20,8) NOT NULL CHECK (rate > 0),
    rate_inverse        boolean       NOT NULL,
    rate_source         text          NOT NULL CHECK (rate_source IN ('identity', 'workspace')),
    rate_date           date,
    functional_amount   bigint        NOT NULL CHECK (functional_amount > 0),
    item_id             uuid          NOT NULL,
    effect_id           uuid          NOT NULL,
    reversal_date       date,
    reverse_reason      text          CHECK (char_length(btrim(reverse_reason)) BETWEEN 1 AND 1000),
    reversal_effect_id  uuid,
    reversed_by         uuid,
    reversed_at         timestamptz,
    version             bigint        NOT NULL DEFAULT 1 CHECK (version >= 1),
    created_by          uuid          NOT NULL,
    created_at          timestamptz   NOT NULL DEFAULT now(),
    updated_at          timestamptz   NOT NULL DEFAULT now(),
    CONSTRAINT supplier_payments_number_key UNIQUE (workspace_id, number),
    CONSTRAINT supplier_payments_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT supplier_payments_location_check CHECK (CASE location_kind
        WHEN 'cash_place' THEN num_nonnulls(register_id, vault_id) = 1 AND bank_account_id IS NULL
        ELSE bank_account_id IS NOT NULL AND register_id IS NULL AND vault_id IS NULL
             AND reference IS NOT NULL AND evidence_ref IS NOT NULL
             AND confirmation_method IS NOT NULL END),
    CONSTRAINT supplier_payments_reversed_complete CHECK (
        (state = 'reversed') = (reversed_at IS NOT NULL)
        AND (reversed_at IS NULL) = (reversed_by IS NULL)
        AND (reversed_at IS NULL) = (reverse_reason IS NULL)
        AND (reversed_at IS NULL) = (reversal_date IS NULL)
        AND (reversed_at IS NULL) = (reversal_effect_id IS NULL)),
    CONSTRAINT supplier_payments_supplier_fk FOREIGN KEY (workspace_id, supplier_id)
        REFERENCES relations (workspace_id, id),
    CONSTRAINT supplier_payments_currency_fk FOREIGN KEY (workspace_id, currency_id)
        REFERENCES currencies (workspace_id, id),
    CONSTRAINT supplier_payments_bank_fk FOREIGN KEY (workspace_id, bank_account_id)
        REFERENCES bank_accounts (workspace_id, id),
    CONSTRAINT supplier_payments_item_fk FOREIGN KEY (workspace_id, item_id)
        REFERENCES accounting_open_items (workspace_id, id),
    CONSTRAINT supplier_payments_effect_fk FOREIGN KEY (workspace_id, effect_id)
        REFERENCES accounting_effects (workspace_id, id),
    CONSTRAINT supplier_payments_reversal_effect_fk FOREIGN KEY (workspace_id, reversal_effect_id)
        REFERENCES accounting_effects (workspace_id, id)
);
CREATE INDEX supplier_payments_list_idx ON supplier_payments (workspace_id, number DESC);

CREATE TABLE purchase_returns (
    id                 uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id       uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    number             bigint      NOT NULL CHECK (number > 0),
    state              text        NOT NULL DEFAULT 'draft'
                                   CHECK (state IN ('draft', 'posted', 'reversed')),
    purpose            text        NOT NULL CHECK (purpose IN ('vendor_return', 'receipt_correction')),
    receipt_id         uuid        NOT NULL,
    supplier_id        uuid        NOT NULL,
    currency_id        uuid        NOT NULL,
    accounting_date    date        NOT NULL,
    reason             text        NOT NULL CHECK (char_length(btrim(reason)) BETWEEN 1 AND 1000),
    evidence_ref       text        CHECK (char_length(evidence_ref) BETWEEN 1 AND 255),
    -- Apply the supplier credit to the original bill's open AP at post.
    apply_credit       boolean     NOT NULL DEFAULT false,
    credit_item_id     uuid,
    application_id     uuid,
    posted_effect_id   uuid,
    posted_by          uuid,
    posted_at          timestamptz,
    reversal_date      date,
    reverse_reason     text        CHECK (char_length(btrim(reverse_reason)) BETWEEN 1 AND 1000),
    reversal_effect_id uuid,
    reversed_by        uuid,
    reversed_at        timestamptz,
    version            bigint      NOT NULL DEFAULT 1 CHECK (version >= 1),
    created_by         uuid        NOT NULL,
    created_at         timestamptz NOT NULL DEFAULT now(),
    updated_at         timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT purchase_returns_number_key UNIQUE (workspace_id, number),
    CONSTRAINT purchase_returns_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT purchase_returns_posted_complete CHECK (
        (posted_at IS NULL) = (posted_effect_id IS NULL)
        AND (posted_at IS NULL) = (posted_by IS NULL)
        AND (state <> 'posted' OR posted_at IS NOT NULL)
        AND (state <> 'draft' OR posted_at IS NULL)
        AND (application_id IS NULL OR credit_item_id IS NOT NULL)),
    CONSTRAINT purchase_returns_reversed_complete CHECK (
        (state = 'reversed') = (reversed_at IS NOT NULL)
        AND (reversed_at IS NULL) = (reversed_by IS NULL)
        AND (reversed_at IS NULL) = (reverse_reason IS NULL)
        AND ((reversal_effect_id IS NULL) = (posted_at IS NULL OR reversed_at IS NULL))),
    CONSTRAINT purchase_returns_receipt_fk FOREIGN KEY (workspace_id, receipt_id)
        REFERENCES purchase_receipts (workspace_id, id),
    CONSTRAINT purchase_returns_supplier_fk FOREIGN KEY (workspace_id, supplier_id)
        REFERENCES relations (workspace_id, id),
    CONSTRAINT purchase_returns_currency_fk FOREIGN KEY (workspace_id, currency_id)
        REFERENCES currencies (workspace_id, id),
    CONSTRAINT purchase_returns_credit_fk FOREIGN KEY (workspace_id, credit_item_id)
        REFERENCES accounting_open_items (workspace_id, id),
    CONSTRAINT purchase_returns_application_fk FOREIGN KEY (workspace_id, application_id)
        REFERENCES accounting_allocations (workspace_id, id),
    CONSTRAINT purchase_returns_posted_effect_fk FOREIGN KEY (workspace_id, posted_effect_id)
        REFERENCES accounting_effects (workspace_id, id),
    CONSTRAINT purchase_returns_reversal_effect_fk FOREIGN KEY (workspace_id, reversal_effect_id)
        REFERENCES accounting_effects (workspace_id, id)
);
CREATE INDEX purchase_returns_list_idx ON purchase_returns (workspace_id, number DESC);

-- bill_match_id NULL = unbilled (consumes the shared capacity); set = billed (supplier credit).
CREATE TABLE purchase_return_lines (
    id                uuid          PRIMARY KEY DEFAULT uuidv7(),
    workspace_id      uuid          NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    return_id         uuid          NOT NULL,
    line_no           integer       NOT NULL CHECK (line_no BETWEEN 1 AND 100),
    receipt_line_id   uuid          NOT NULL,
    bill_match_id     uuid,
    quantity          numeric(19,2) NOT NULL CHECK (quantity > 0),
    warehouse_id      uuid          NOT NULL,
    bin_id            uuid,
    lot_id            uuid,
    -- Frozen at post (functional unless *_nominal): C, GRNI relieved, supplier credit, tax
    -- reversed, purchase-return variance, FX.
    issue_value       bigint        CHECK (issue_value >= 0),
    grni_nominal      bigint,
    grni_functional   bigint,
    credit_nominal    bigint,
    credit_functional bigint,
    tax_nominal       bigint,
    tax_functional    bigint,
    variance          bigint,
    fx                bigint,
    movement_id       uuid,
    CONSTRAINT purchase_return_lines_line_no_key UNIQUE (return_id, line_no),
    CONSTRAINT purchase_return_lines_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT purchase_return_lines_return_fk FOREIGN KEY (workspace_id, return_id)
        REFERENCES purchase_returns (workspace_id, id) ON DELETE CASCADE,
    CONSTRAINT purchase_return_lines_receipt_line_fk FOREIGN KEY (workspace_id, receipt_line_id)
        REFERENCES purchase_receipt_lines (workspace_id, id),
    CONSTRAINT purchase_return_lines_match_fk FOREIGN KEY (workspace_id, bill_match_id)
        REFERENCES supplier_bill_matches (workspace_id, id),
    CONSTRAINT purchase_return_lines_warehouse_fk FOREIGN KEY (workspace_id, warehouse_id)
        REFERENCES warehouses (workspace_id, id),
    CONSTRAINT purchase_return_lines_bin_fk FOREIGN KEY (workspace_id, bin_id)
        REFERENCES warehouse_section_row_bins (workspace_id, id),
    CONSTRAINT purchase_return_lines_lot_fk FOREIGN KEY (workspace_id, lot_id)
        REFERENCES lots (workspace_id, id)
);
CREATE INDEX purchase_return_lines_receipt_line_idx ON purchase_return_lines (receipt_line_id);
CREATE INDEX purchase_return_lines_match_idx ON purchase_return_lines (bill_match_id)
    WHERE bill_match_id IS NOT NULL;

-- Reviewed split of a posted bill's pending purchase cost variance: inventory (value-only pool
-- entry) and COGS. Signed: positive raises inventory/COGS (unfavourable variance).
CREATE TABLE purchase_cost_adjustments (
    id                 uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id       uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    bill_id            uuid        NOT NULL,
    state              text        NOT NULL DEFAULT 'draft'
                                   CHECK (state IN ('draft', 'posted', 'reversed')),
    reason             text        NOT NULL CHECK (char_length(btrim(reason)) BETWEEN 1 AND 1000),
    evidence_ref       text        NOT NULL CHECK (char_length(btrim(evidence_ref)) BETWEEN 1 AND 255),
    accounting_date    date,
    posted_effect_id   uuid,
    posted_by          uuid,
    posted_at          timestamptz,
    reversal_date      date,
    reverse_reason     text        CHECK (char_length(btrim(reverse_reason)) BETWEEN 1 AND 1000),
    reversal_effect_id uuid,
    reversed_by        uuid,
    reversed_at        timestamptz,
    version            bigint      NOT NULL DEFAULT 1 CHECK (version >= 1),
    created_by         uuid        NOT NULL,
    created_at         timestamptz NOT NULL DEFAULT now(),
    updated_at         timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT purchase_cost_adjustments_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT purchase_cost_adjustments_posted_complete CHECK (
        (posted_at IS NULL) = (posted_effect_id IS NULL)
        AND (posted_at IS NULL) = (posted_by IS NULL)
        AND (posted_at IS NULL) = (accounting_date IS NULL)
        AND (state <> 'posted' OR posted_at IS NOT NULL)
        AND (state <> 'draft' OR posted_at IS NULL)),
    CONSTRAINT purchase_cost_adjustments_reversed_complete CHECK (
        (state = 'reversed') = (reversed_at IS NOT NULL)
        AND (reversed_at IS NULL) = (reversed_by IS NULL)
        AND (reversed_at IS NULL) = (reverse_reason IS NULL)
        AND ((reversal_effect_id IS NULL) = (posted_at IS NULL OR reversed_at IS NULL))),
    CONSTRAINT purchase_cost_adjustments_bill_fk FOREIGN KEY (workspace_id, bill_id)
        REFERENCES supplier_bills (workspace_id, id),
    CONSTRAINT purchase_cost_adjustments_posted_effect_fk FOREIGN KEY (workspace_id, posted_effect_id)
        REFERENCES accounting_effects (workspace_id, id),
    CONSTRAINT purchase_cost_adjustments_reversal_effect_fk FOREIGN KEY (workspace_id, reversal_effect_id)
        REFERENCES accounting_effects (workspace_id, id)
);
CREATE INDEX purchase_cost_adjustments_bill_idx ON purchase_cost_adjustments (workspace_id, bill_id);

CREATE TABLE purchase_cost_adjustment_lines (
    id               uuid    PRIMARY KEY DEFAULT uuidv7(),
    workspace_id     uuid    NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    adjustment_id    uuid    NOT NULL,
    line_no          integer NOT NULL CHECK (line_no BETWEEN 1 AND 100),
    receipt_line_id  uuid    NOT NULL,
    inventory_amount bigint  NOT NULL,
    cogs_amount      bigint  NOT NULL,
    -- Value-only pool entry of the inventory part (posted, inventory_amount <> 0).
    value_entry_id   uuid,
    CONSTRAINT purchase_cost_adjustment_lines_amount_check
        CHECK (inventory_amount <> 0 OR cogs_amount <> 0),
    CONSTRAINT purchase_cost_adjustment_lines_line_no_key UNIQUE (adjustment_id, line_no),
    CONSTRAINT purchase_cost_adjustment_lines_adjustment_fk FOREIGN KEY (workspace_id, adjustment_id)
        REFERENCES purchase_cost_adjustments (workspace_id, id) ON DELETE CASCADE,
    CONSTRAINT purchase_cost_adjustment_lines_receipt_line_fk FOREIGN KEY (workspace_id, receipt_line_id)
        REFERENCES purchase_receipt_lines (workspace_id, id)
);

-- Posted payables facts move only forward: a document from draft (free) to posted, a version
-- bump (an allocation), and from posted to reversed with nothing but its reversal columns, state, version and updated_at
-- changing; a match only from posted to reversed. Child lines/taxes change only while their
-- document is a draft. Workspace deletion still cascades.
CREATE FUNCTION payables_fixed() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    ws    uuid := CASE WHEN TG_OP = 'INSERT' THEN NEW.workspace_id ELSE OLD.workspace_id END;
    draft boolean;
BEGIN
    IF TG_OP = 'DELETE' AND NOT EXISTS (SELECT 1 FROM workspaces WHERE id = ws) THEN
        RETURN OLD;
    END IF;
    IF TG_TABLE_NAME IN ('supplier_bills', 'purchase_returns', 'purchase_cost_adjustments',
                         'supplier_payments') THEN
        IF TG_OP = 'UPDATE' AND (OLD.state = 'draft'
           OR (to_jsonb(NEW) - ARRAY['version', 'updated_at'])
            = (to_jsonb(OLD) - ARRAY['version', 'updated_at'])) THEN
            RETURN NEW;
        END IF;
        IF TG_OP = 'UPDATE' AND OLD.state = 'posted' AND NEW.state = 'reversed'
           AND (to_jsonb(NEW) - ARRAY['state', 'version', 'updated_at', 'reversal_date',
                    'reverse_reason', 'reversal_effect_id', 'reversed_by', 'reversed_at'])
             = (to_jsonb(OLD) - ARRAY['state', 'version', 'updated_at', 'reversal_date',
                    'reverse_reason', 'reversal_effect_id', 'reversed_by', 'reversed_at']) THEN
            RETURN NEW;
        END IF;
        IF TG_OP = 'DELETE' AND OLD.state = 'draft' THEN
            RETURN OLD;
        END IF;
    ELSIF TG_TABLE_NAME = 'supplier_bill_matches' THEN
        IF TG_OP = 'UPDATE' AND OLD.state = 'posted' AND NEW.state = 'reversed'
           AND (to_jsonb(NEW) - 'state') = (to_jsonb(OLD) - 'state') THEN
            RETURN NEW;
        END IF;
        SELECT b.state = 'draft' INTO draft FROM supplier_bills b WHERE b.id = OLD.bill_id;
        IF coalesce(draft, true) THEN
            RETURN CASE WHEN TG_OP = 'DELETE' THEN OLD ELSE NEW END;
        END IF;
    ELSE
        -- supplier_bill_lines, supplier_bill_line_taxes, purchase_return_lines,
        -- purchase_cost_adjustment_lines: free while the parent is a draft. jsonb, not record
        -- fields: plpgsql would resolve OLD.<field> on every table.
        draft := CASE TG_TABLE_NAME
            WHEN 'supplier_bill_lines' THEN (SELECT b.state = 'draft' FROM supplier_bills b
                                             WHERE b.id = (to_jsonb(OLD) ->> 'bill_id')::uuid)
            WHEN 'supplier_bill_line_taxes' THEN (SELECT b.state = 'draft'
                FROM supplier_bill_lines l JOIN supplier_bills b ON b.id = l.bill_id
                WHERE l.id = (to_jsonb(OLD) ->> 'bill_line_id')::uuid)
            WHEN 'purchase_return_lines' THEN (SELECT r.state = 'draft' FROM purchase_returns r
                                               WHERE r.id = (to_jsonb(OLD) ->> 'return_id')::uuid)
            ELSE (SELECT a.state = 'draft' FROM purchase_cost_adjustments a
                  WHERE a.id = (to_jsonb(OLD) ->> 'adjustment_id')::uuid) END;
        IF coalesce(draft, true) THEN
            RETURN CASE WHEN TG_OP = 'DELETE' THEN OLD ELSE NEW END;
        END IF;
    END IF;
    RAISE EXCEPTION 'Posted payables facts are immutable (% %).', TG_TABLE_NAME, TG_OP
        USING ERRCODE = '23514';
END $$;

CREATE TRIGGER supplier_bills_fixed BEFORE UPDATE OR DELETE ON supplier_bills
    FOR EACH ROW EXECUTE FUNCTION payables_fixed();
CREATE TRIGGER supplier_bill_lines_fixed BEFORE UPDATE OR DELETE ON supplier_bill_lines
    FOR EACH ROW EXECUTE FUNCTION payables_fixed();
CREATE TRIGGER supplier_bill_line_taxes_fixed BEFORE UPDATE OR DELETE ON supplier_bill_line_taxes
    FOR EACH ROW EXECUTE FUNCTION payables_fixed();
CREATE TRIGGER supplier_bill_matches_fixed BEFORE UPDATE OR DELETE ON supplier_bill_matches
    FOR EACH ROW EXECUTE FUNCTION payables_fixed();
CREATE TRIGGER supplier_payments_fixed BEFORE UPDATE OR DELETE ON supplier_payments
    FOR EACH ROW EXECUTE FUNCTION payables_fixed();
CREATE TRIGGER purchase_returns_fixed BEFORE UPDATE OR DELETE ON purchase_returns
    FOR EACH ROW EXECUTE FUNCTION payables_fixed();
CREATE TRIGGER purchase_return_lines_fixed BEFORE UPDATE OR DELETE ON purchase_return_lines
    FOR EACH ROW EXECUTE FUNCTION payables_fixed();
CREATE TRIGGER purchase_cost_adjustments_fixed BEFORE UPDATE OR DELETE ON purchase_cost_adjustments
    FOR EACH ROW EXECUTE FUNCTION payables_fixed();
CREATE TRIGGER purchase_cost_adjustment_lines_fixed
    BEFORE UPDATE OR DELETE ON purchase_cost_adjustment_lines
    FOR EACH ROW EXECUTE FUNCTION payables_fixed();

-- Last relief of a remainder takes it whole; earlier ones their share, half away from zero.
CREATE FUNCTION grni_relief(q numeric, rem_q numeric, rem bigint) RETURNS bigint
    LANGUAGE sql IMMUTABLE STRICT PARALLEL SAFE
    RETURN (CASE WHEN q = rem_q THEN rem ELSE money_round_div(rem::numeric * q, rem_q) END)::bigint;

-- §11 capacity of a valued booked receipt line (no row otherwise): B, U, M, D (other bills'
-- drafts), matchable = B − U − M − D, every posted return, and the GRNI still carried
-- (quantity, functional, nominal PO currency) after posted matches and unbilled returns.
CREATE FUNCTION receipt_line_capacity(ws uuid, line uuid, except_bill uuid)
RETURNS TABLE (booked numeric, unbilled_returned numeric, matched numeric, reserved numeric,
               matchable numeric, returned numeric, rem_quantity numeric,
               rem_functional bigint, rem_nominal bigint)
LANGUAGE sql STABLE AS $$
    SELECT l.base_quantity, u.q, m.q, d.q, l.base_quantity - u.q - m.q - d.q, r.q,
           l.base_quantity - u.q - m.q, l.net_value - u.f - m.f, l.doc_value - u.n - m.n
    FROM purchase_receipt_lines l,
    LATERAL (SELECT coalesce(sum(x.quantity), 0) AS q,
                    coalesce(sum(x.grni_functional), 0)::bigint AS f,
                    coalesce(sum(x.grni_nominal), 0)::bigint AS n
             FROM purchase_return_lines x JOIN purchase_returns h ON h.id = x.return_id
             WHERE x.receipt_line_id = l.id AND x.bill_match_id IS NULL AND h.state = 'posted') u,
    LATERAL (SELECT coalesce(sum(x.quantity), 0) AS q
             FROM purchase_return_lines x JOIN purchase_returns h ON h.id = x.return_id
             WHERE x.receipt_line_id = l.id AND h.state = 'posted') r,
    LATERAL (SELECT coalesce(sum(x.quantity), 0) AS q,
                    coalesce(sum(x.grni_functional), 0)::bigint AS f,
                    coalesce(sum(x.grni_nominal), 0)::bigint AS n
             FROM supplier_bill_matches x
             WHERE x.receipt_line_id = l.id AND x.state = 'posted') m,
    LATERAL (SELECT coalesce(sum(x.quantity), 0) AS q FROM supplier_bill_matches x
             WHERE x.receipt_line_id = l.id AND x.state = 'draft'
               AND x.bill_id IS DISTINCT FROM except_bill) d
    WHERE l.workspace_id = ws AND l.id = line AND l.booked_at IS NOT NULL
      AND l.net_value IS NOT NULL AND l.doc_value IS NOT NULL
$$;

-- What posting draft bill `bill` books at `rate` (bill currency → functional), all in minor
-- units. One `bill` row: nominal = net, tax_* nominal, tax_functional = recoverable tax,
-- functional = AP. One `line` row each: nominal = extension, its taxes, functional (expense:
-- extension + nonrecoverable tax converted; goods/landed: Σ its match shares). One `match`
-- row each: GRNI relieved, AP share s (money_allocate of AP − tax − expenses by nominal),
-- fx = convert(GRNI nominal at bill rate) − GRNI functional, variance = s − GRNI − fx.
CREATE FUNCTION supplier_bill_plan(ws uuid, bill uuid, rate numeric, inverse boolean)
RETURNS TABLE (row_kind text, id uuid, line_id uuid, nominal bigint, tax_recoverable bigint,
               tax_nonrecoverable bigint, tax_functional bigint, grni_nominal bigint,
               grni_functional bigint, share_functional bigint, fx bigint, variance bigint,
               functional bigint)
LANGUAGE sql STABLE AS $$
    WITH b AS (
        SELECT b.id, b.invoice_date, b.supplier_id, c.decimals AS dp, f.decimals AS fdp
        FROM supplier_bills b
        JOIN currencies c ON c.workspace_id = b.workspace_id AND c.id = b.currency_id
        JOIN accounting_settings s ON s.workspace_id = b.workspace_id
        JOIN currencies f ON f.workspace_id = b.workspace_id AND f.id = s.functional_currency_id
        WHERE b.workspace_id = ws AND b.id = bill
    ), l AS (
        SELECT l.id, l.kind, l.extension, coalesce(t.rec, 0)::bigint AS rec,
               coalesce(t.nonrec, 0)::bigint AS nonrec
        FROM supplier_bill_lines l CROSS JOIN b
        LEFT JOIN products p ON p.workspace_id = l.workspace_id AND p.id = l.product_id
        LEFT JOIN LATERAL (
            -- No profile, no call: tax_line refuses an unknown profile.
            SELECT sum(x.tax) FILTER (WHERE x.recoverable) AS rec,
                   sum(x.tax) FILTER (WHERE NOT x.recoverable) AS nonrec
            FROM (SELECT 1 WHERE l.tax_profile_id IS NOT NULL) g
            CROSS JOIN LATERAL tax_line(ws, l.tax_profile_id, b.invoice_date, 'purchase',
                                        l.extension, false, p.category_id, b.supplier_id) x
        ) t ON true
        WHERE l.workspace_id = ws AND l.bill_id = bill
    ), tot AS (
        SELECT sum(l.extension)::bigint AS net, sum(l.rec)::bigint AS rec,
               sum(l.nonrec)::bigint AS nonrec,
               money_convert(sum(l.extension + l.rec + l.nonrec)::bigint, rate, b.dp, b.fdp, inverse) AS a_f,
               money_convert(sum(l.rec)::bigint, rate, b.dp, b.fdp, inverse) AS t_f
        FROM l CROSS JOIN b GROUP BY b.dp, b.fdp
    ), e AS (
        SELECT l.id, money_convert(l.extension + l.nonrec, rate, b.dp, b.fdp, inverse) AS e_f
        FROM l CROSS JOIN b WHERE l.kind = 'expense'
    ), m AS (
        SELECT m.id, m.bill_line_id,
               money_round_div(ln.extension::numeric * m.quantity, ln.quantity)::bigint AS base,
               CASE WHEN m.landed_cost_id IS NOT NULL THEN lc.amount
                    ELSE grni_relief(m.quantity, c.rem_quantity, c.rem_nominal) END AS gn,
               CASE WHEN m.landed_cost_id IS NOT NULL THEN lc.functional_amount
                    ELSE grni_relief(m.quantity, c.rem_quantity, c.rem_functional) END AS gf
        FROM supplier_bill_matches m
        JOIN supplier_bill_lines ln ON ln.id = m.bill_line_id
        LEFT JOIN purchase_landed_costs lc ON lc.id = m.landed_cost_id
        LEFT JOIN LATERAL receipt_line_capacity(ws, m.receipt_line_id, bill) c
               ON m.receipt_line_id IS NOT NULL
        WHERE m.workspace_id = ws AND m.bill_id = bill AND m.state = 'draft'
    ), s AS (
        SELECT a.id, a.amount
        FROM tot, money_allocate(
            (tot.a_f - tot.t_f - coalesce((SELECT sum(e.e_f) FROM e), 0))::bigint,
            (SELECT array_agg(m.id ORDER BY m.id) FROM m),
            (SELECT array_agg(m.base ORDER BY m.id) FROM m)) a
    ), mx AS (
        SELECT m.id, m.bill_line_id, m.gn, m.gf, coalesce(s.amount, 0) AS share,
               money_convert(m.gn, rate, b.dp, b.fdp, inverse) - m.gf AS fx
        FROM m CROSS JOIN b LEFT JOIN s ON s.id = m.id
    )
    SELECT 'bill', bill, NULL::uuid, tot.net, tot.rec, tot.nonrec, tot.t_f, NULL::bigint,
           NULL::bigint, NULL::bigint, NULL::bigint, NULL::bigint, tot.a_f
    FROM tot
    UNION ALL
    SELECT 'line', l.id, l.id, l.extension, l.rec, l.nonrec, NULL, NULL, NULL, NULL, NULL, NULL,
           coalesce(e.e_f, (SELECT sum(mx.share) FROM mx WHERE mx.bill_line_id = l.id), 0)::bigint
    FROM l LEFT JOIN e ON e.id = l.id
    UNION ALL
    SELECT 'match', mx.id, mx.bill_line_id, NULL, NULL, NULL, NULL, mx.gn, mx.gf, mx.share,
           mx.fx, mx.share - mx.gf - mx.fx, NULL
    FROM mx
$$;

-- The posting of supplier_bill_plan, signed debit − credit per line, zero lines dropped:
-- DR grni Σg, DR recoverable_tax, DR each expense account, ±realized_fx Σfx,
-- ±pending_purchase_cost_variance Σv, CR payables AP. Balanced by construction.
CREATE FUNCTION supplier_bill_journal(ws uuid, bill uuid, rate numeric, inverse boolean)
RETURNS TABLE (line_no int, role text, account_id uuid, debit bigint, credit bigint)
LANGUAGE sql STABLE AS $$
    WITH p AS (SELECT * FROM supplier_bill_plan(ws, bill, rate, inverse)), j AS (
        SELECT 1 AS o, 'grni'::text AS role, NULL::uuid AS account_id,
               coalesce(sum(p.grni_functional), 0) AS amt FROM p WHERE p.row_kind = 'match'
        UNION ALL
        SELECT 2, 'recoverable_tax', NULL, p.tax_functional FROM p WHERE p.row_kind = 'bill'
        UNION ALL
        SELECT 3, NULL, l.expense_account_id, sum(p.functional)
        FROM p JOIN supplier_bill_lines l ON l.id = p.id
        WHERE p.row_kind = 'line' AND l.kind = 'expense' GROUP BY l.expense_account_id
        UNION ALL
        SELECT 4, 'realized_fx', NULL, coalesce(sum(p.fx), 0) FROM p WHERE p.row_kind = 'match'
        UNION ALL
        SELECT 5, 'pending_purchase_cost_variance', NULL, coalesce(sum(p.variance), 0)
        FROM p WHERE p.row_kind = 'match'
        UNION ALL
        SELECT 6, 'payables', NULL, -p.functional FROM p WHERE p.row_kind = 'bill'
    )
    SELECT (row_number() OVER (ORDER BY j.o, j.account_id))::int, j.role, j.account_id,
           greatest(j.amt, 0)::bigint, greatest(-j.amt, 0)::bigint
    FROM j WHERE j.amt <> 0
$$;

-- Per line of return `ret`: the pool now and C (current average issue value; one line per
-- pool, so exact), GRNI still carried relieved for an unbilled line, or for a billed line the
-- supplier credit at the original bill basis (nominal + functional, goods and recoverable
-- tax), each the line's share of what its match still holds (last return takes the rest).
CREATE FUNCTION purchase_return_plan(ws uuid, ret uuid)
RETURNS TABLE (line_id uuid, receipt_line_id uuid, bill_match_id uuid, quantity numeric,
               pool_quantity numeric, pool_value bigint, pool_sequence bigint,
               issue_value bigint, grni_nominal bigint, grni_functional bigint,
               credit_nominal bigint, credit_functional bigint, tax_nominal bigint,
               tax_functional bigint)
LANGUAGE sql STABLE AS $$
    SELECT l.id, l.receipt_line_id, l.bill_match_id, l.quantity,
           p.quantity, p.value, p.valuation_sequence,
           coalesce(l.issue_value,
               CASE WHEN p.quantity IS NULL OR l.quantity > p.quantity THEN NULL
                    WHEN l.quantity = p.quantity THEN p.value
                    ELSE money_round_div(p.value::numeric * l.quantity, p.quantity)::bigint END),
           CASE WHEN l.bill_match_id IS NULL
                THEN grni_relief(l.quantity, c.rem_quantity, c.rem_nominal) END,
           CASE WHEN l.bill_match_id IS NULL
                THEN grni_relief(l.quantity, c.rem_quantity, c.rem_functional) END,
           b.cn, b.cf, b.tn, b.tf
    FROM purchase_return_lines l
    JOIN purchase_receipt_lines rl ON rl.workspace_id = l.workspace_id AND rl.id = l.receipt_line_id
    LEFT JOIN inventory_cost_pools p ON p.workspace_id = l.workspace_id
         AND p.product_id = rl.product_id AND p.warehouse_id = l.warehouse_id
    LEFT JOIN LATERAL receipt_line_capacity(ws, l.receipt_line_id, NULL) c ON true
    LEFT JOIN LATERAL (
        SELECT grni_relief(l.quantity, m.quantity - r.q, mb.n - r.cn) AS cn,
               grni_relief(l.quantity, m.quantity - r.q, m.share_functional - r.cf) AS cf,
               grni_relief(l.quantity, m.quantity - r.q, mb.t - r.tn) AS tn,
               grni_relief(l.quantity, m.quantity - r.q, mb.tf - r.tf) AS tf
        FROM supplier_bill_matches m
        JOIN supplier_bill_lines bl ON bl.id = m.bill_line_id
        JOIN supplier_bills sb ON sb.id = m.bill_id
        JOIN currencies cc ON cc.workspace_id = sb.workspace_id AND cc.id = sb.currency_id
        JOIN accounting_settings s ON s.workspace_id = sb.workspace_id
        JOIN currencies fc ON fc.workspace_id = sb.workspace_id AND fc.id = s.functional_currency_id
        CROSS JOIN LATERAL (
            SELECT money_round_div(bl.extension::numeric * m.quantity, bl.quantity)::bigint AS n,
                   money_round_div(coalesce(bl.tax_recoverable, 0)::numeric * m.quantity,
                                   bl.quantity)::bigint AS t,
                   money_round_div(money_convert(coalesce(bl.tax_recoverable, 0), sb.rate,
                                                 cc.decimals, fc.decimals, sb.rate_inverse)::numeric
                                   * m.quantity, bl.quantity)::bigint AS tf) mb
        CROSS JOIN LATERAL (
            SELECT coalesce(sum(x.quantity), 0) AS q,
                   coalesce(sum(x.credit_nominal), 0)::bigint AS cn,
                   coalesce(sum(x.credit_functional), 0)::bigint AS cf,
                   coalesce(sum(x.tax_nominal), 0)::bigint AS tn,
                   coalesce(sum(x.tax_functional), 0)::bigint AS tf
            FROM purchase_return_lines x
            JOIN purchase_returns h ON h.id = x.return_id AND h.state = 'posted'
            WHERE x.bill_match_id = m.id) r
        WHERE m.workspace_id = ws AND m.id = l.bill_match_id
    ) b ON l.bill_match_id IS NOT NULL
    WHERE l.workspace_id = ws AND l.return_id = ret
$$;

-- The posting of purchase_return_plan, signed debit − credit, zero lines dropped:
-- DR grni (unbilled), DR payables supplier credit (billed goods + tax), CR recoverable_tax,
-- CR inventory C, purchase_return_variance the explicit difference.
CREATE FUNCTION purchase_return_journal(ws uuid, ret uuid)
RETURNS TABLE (line_no int, role text, account_id uuid, debit bigint, credit bigint)
LANGUAGE sql STABLE AS $$
    WITH p AS (SELECT * FROM purchase_return_plan(ws, ret)), t AS (
        SELECT coalesce(sum(p.grni_functional), 0) AS g,
               coalesce(sum(p.credit_functional), 0) AS cf,
               coalesce(sum(p.tax_functional), 0) AS tf,
               coalesce(sum(p.issue_value), 0) AS c
        FROM p
    ), j AS (
        SELECT 1 AS o, 'grni'::text AS role, t.g AS amt FROM t
        UNION ALL SELECT 2, 'payables', t.cf + t.tf FROM t
        UNION ALL SELECT 3, 'recoverable_tax', -t.tf FROM t
        UNION ALL SELECT 4, 'inventory', -t.c FROM t
        UNION ALL SELECT 5, 'purchase_return_variance', t.c - t.g - t.cf FROM t
    )
    SELECT (row_number() OVER (ORDER BY j.o))::int, j.role, NULL::uuid,
           greatest(j.amt, 0)::bigint, greatest(-j.amt, 0)::bigint
    FROM j WHERE j.amt <> 0
$$;
