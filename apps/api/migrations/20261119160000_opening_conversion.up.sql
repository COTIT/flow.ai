-- Flow ERP M6-B opening conversion (masterplan §14 OPENING MODES / PAYMENT CONTRACT /
-- CORRECTION / CASH, §8 accounting_opening_baselines). Opening import profiles stage typed
-- lines only; `opening::apply` (activation steps 7-9) writes baselines, place openings and the
-- opening value entries once. Every table is append-only (settlement_guard); workspace
-- deletion still cascades. Money: integer minor units, sums in SQL.

-- >>> import profiles (down migration restores the bank_statement CHECK)
ALTER TABLE import_batches DROP CONSTRAINT import_batches_profile_check;
ALTER TABLE import_batches ADD CONSTRAINT import_batches_profile_check
    CHECK (profile IN ('products', 'relations', 'bank_statement', 'opening_inventory',
                       'opening_receivables', 'opening_payables', 'opening_trial_balance'));
-- <<< import profiles

-- One staged line per import row of a completed opening batch.
CREATE TABLE accounting_opening_lines (
    id              uuid          PRIMARY KEY DEFAULT uuidv7(),
    workspace_id    uuid          NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    batch_id        uuid          NOT NULL,
    row_number      integer       NOT NULL CHECK (row_number BETWEEN 1 AND 500),
    kind            text          NOT NULL
                    CHECK (kind IN ('inventory', 'receivable', 'payable', 'balance')),
    -- inventory
    mode            text          CHECK (mode IN ('existing', 'empty')),
    warehouse_id    uuid,
    product_id      uuid,
    quantity        numeric(19,2) CHECK (quantity >= 0),
    value           bigint        CHECK (value >= 0),
    -- receivable / payable: reviewed amounts in the source currency, carrying in functional
    invoice_id      uuid,
    supplier_bill_id uuid,
    currency_id     uuid,
    face            bigint        CHECK (face >= 0),
    settled         bigint        CHECK (settled >= 0),
    credited        bigint        CHECK (credited >= 0),
    refunded        bigint        CHECK (refunded >= 0),
    outstanding     bigint        CHECK (outstanding >= 0),
    carrying        bigint        CHECK (carrying >= 0),
    -- balance (trial balance)
    account_id      uuid,
    debit           bigint        CHECK (debit >= 0),
    credit          bigint        CHECK (credit >= 0),
    register_id     uuid,
    vault_id        uuid,
    bank_account_id uuid,
    created_by      uuid          NOT NULL,
    created_at      timestamptz   NOT NULL DEFAULT clock_timestamp(),
    CONSTRAINT accounting_opening_lines_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT accounting_opening_lines_row_key UNIQUE (batch_id, row_number),
    CONSTRAINT accounting_opening_lines_shape CHECK (CASE kind
        WHEN 'inventory' THEN mode IS NOT NULL AND warehouse_id IS NOT NULL
            AND product_id IS NOT NULL AND quantity IS NOT NULL AND value IS NOT NULL
            AND (quantity > 0 OR value = 0)
        WHEN 'balance' THEN account_id IS NOT NULL AND debit IS NOT NULL AND credit IS NOT NULL
            AND (debit = 0) <> (credit = 0)
            AND num_nonnulls(register_id, vault_id, bank_account_id) <= 1
        ELSE ((kind = 'receivable') = (invoice_id IS NOT NULL))
            AND ((kind = 'payable') = (supplier_bill_id IS NOT NULL))
            AND currency_id IS NOT NULL AND carrying IS NOT NULL
            AND face::numeric - settled - credited + refunded = outstanding
            AND (outstanding = 0) = (carrying = 0) END),
    CONSTRAINT accounting_opening_lines_batch_fk FOREIGN KEY (workspace_id, batch_id)
        REFERENCES import_batches (workspace_id, id),
    CONSTRAINT accounting_opening_lines_warehouse_fk FOREIGN KEY (workspace_id, warehouse_id)
        REFERENCES warehouses (workspace_id, id),
    CONSTRAINT accounting_opening_lines_product_fk FOREIGN KEY (workspace_id, product_id)
        REFERENCES products (workspace_id, id),
    CONSTRAINT accounting_opening_lines_invoice_fk FOREIGN KEY (workspace_id, invoice_id)
        REFERENCES invoices (workspace_id, id),
    CONSTRAINT accounting_opening_lines_bill_fk FOREIGN KEY (workspace_id, supplier_bill_id)
        REFERENCES supplier_bills (workspace_id, id),
    CONSTRAINT accounting_opening_lines_currency_fk FOREIGN KEY (workspace_id, currency_id)
        REFERENCES currencies (workspace_id, id),
    CONSTRAINT accounting_opening_lines_account_fk FOREIGN KEY (workspace_id, account_id)
        REFERENCES accounts (workspace_id, id),
    CONSTRAINT accounting_opening_lines_register_fk FOREIGN KEY (workspace_id, register_id)
        REFERENCES till_registers (workspace_id, id),
    CONSTRAINT accounting_opening_lines_vault_fk FOREIGN KEY (workspace_id, vault_id)
        REFERENCES cash_vaults (workspace_id, id),
    CONSTRAINT accounting_opening_lines_bank_fk FOREIGN KEY (workspace_id, bank_account_id)
        REFERENCES bank_accounts (workspace_id, id)
);
CREATE INDEX accounting_opening_lines_batch_idx ON accounting_opening_lines (workspace_id, batch_id);

-- Immutable reviewed opening of one historical invoice or supplier bill (masterplan §8).
CREATE TABLE accounting_opening_baselines (
    id           uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    line_id      uuid        NOT NULL,
    source_kind  text        NOT NULL CHECK (source_kind IN ('invoice', 'supplier_bill')),
    source_id    uuid        NOT NULL,
    currency_id  uuid        NOT NULL,
    face         bigint      NOT NULL CHECK (face >= 0),
    settled      bigint      NOT NULL CHECK (settled >= 0),
    credited     bigint      NOT NULL CHECK (credited >= 0),
    refunded     bigint      NOT NULL CHECK (refunded >= 0),
    outstanding  bigint      NOT NULL CHECK (outstanding >= 0),
    carrying     bigint      NOT NULL CHECK (carrying >= 0),
    -- NULL when nothing was outstanding at cutover.
    open_item_id uuid,
    effect_id    uuid        NOT NULL,
    -- sha256 of the covered historical payment ids and amounts.
    coverage     bytea       NOT NULL CHECK (octet_length(coverage) = 32),
    created_at   timestamptz NOT NULL DEFAULT clock_timestamp(),
    CONSTRAINT accounting_opening_baselines_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT accounting_opening_baselines_source_key UNIQUE (workspace_id, source_kind, source_id),
    CONSTRAINT accounting_opening_baselines_item_check CHECK ((outstanding = 0) = (open_item_id IS NULL)),
    CONSTRAINT accounting_opening_baselines_line_fk FOREIGN KEY (workspace_id, line_id)
        REFERENCES accounting_opening_lines (workspace_id, id),
    CONSTRAINT accounting_opening_baselines_currency_fk FOREIGN KEY (workspace_id, currency_id)
        REFERENCES currencies (workspace_id, id),
    CONSTRAINT accounting_opening_baselines_item_fk FOREIGN KEY (workspace_id, open_item_id)
        REFERENCES accounting_open_items (workspace_id, id),
    CONSTRAINT accounting_opening_baselines_effect_fk FOREIGN KEY (workspace_id, effect_id)
        REFERENCES accounting_effects (workspace_id, id)
);

-- Typed opening correction: appended current-period deltas; the baseline never changes.
CREATE TABLE accounting_opening_corrections (
    id                uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id      uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    baseline_id       uuid        NOT NULL,
    accounting_date   date        NOT NULL,
    outstanding_delta bigint      NOT NULL,
    carrying_delta    bigint      NOT NULL,
    -- Visible paid moves opposite to the nominal outstanding.
    settled_delta     bigint      NOT NULL CHECK (settled_delta = -outstanding_delta),
    effect_id         uuid        NOT NULL,
    reason            text        NOT NULL CHECK (char_length(btrim(reason)) BETWEEN 1 AND 1000),
    evidence_ref      text        NOT NULL CHECK (char_length(btrim(evidence_ref)) BETWEEN 1 AND 255),
    created_by        uuid        NOT NULL,
    created_at        timestamptz NOT NULL DEFAULT clock_timestamp(),
    CONSTRAINT accounting_opening_corrections_change CHECK (outstanding_delta <> 0 OR carrying_delta <> 0),
    CONSTRAINT accounting_opening_corrections_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT accounting_opening_corrections_baseline_fk FOREIGN KEY (workspace_id, baseline_id)
        REFERENCES accounting_opening_baselines (workspace_id, id),
    CONSTRAINT accounting_opening_corrections_effect_fk FOREIGN KEY (workspace_id, effect_id)
        REFERENCES accounting_effects (workspace_id, id)
);
CREATE INDEX accounting_opening_corrections_baseline_idx
    ON accounting_opening_corrections (workspace_id, baseline_id);

-- Reviewed opening cash per drawer or vault (masterplan §14 OPENING CASH).
CREATE TABLE cash_place_openings (
    id           uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    register_id  uuid,
    vault_id     uuid,
    amount       bigint      NOT NULL CHECK (amount >= 0),
    effect_id    uuid        NOT NULL,
    created_at   timestamptz NOT NULL DEFAULT clock_timestamp(),
    CONSTRAINT cash_place_openings_place CHECK (num_nonnulls(register_id, vault_id) = 1),
    CONSTRAINT cash_place_openings_register_key UNIQUE (workspace_id, register_id),
    CONSTRAINT cash_place_openings_vault_key UNIQUE (workspace_id, vault_id),
    CONSTRAINT cash_place_openings_register_fk FOREIGN KEY (workspace_id, register_id)
        REFERENCES till_registers (workspace_id, id),
    CONSTRAINT cash_place_openings_vault_fk FOREIGN KEY (workspace_id, vault_id)
        REFERENCES cash_vaults (workspace_id, id),
    CONSTRAINT cash_place_openings_effect_fk FOREIGN KEY (workspace_id, effect_id)
        REFERENCES accounting_effects (workspace_id, id)
);

CREATE TRIGGER accounting_opening_lines_fixed BEFORE UPDATE OR DELETE ON accounting_opening_lines
    FOR EACH ROW EXECUTE FUNCTION settlement_guard();
CREATE TRIGGER accounting_opening_baselines_fixed BEFORE UPDATE OR DELETE ON accounting_opening_baselines
    FOR EACH ROW EXECUTE FUNCTION settlement_guard();
CREATE TRIGGER accounting_opening_corrections_fixed
    BEFORE UPDATE OR DELETE ON accounting_opening_corrections
    FOR EACH ROW EXECUTE FUNCTION settlement_guard();
CREATE TRIGGER cash_place_openings_fixed BEFORE UPDATE OR DELETE ON cash_place_openings
    FOR EACH ROW EXECUTE FUNCTION settlement_guard();
CREATE TRIGGER accounting_opening_lines_no_truncate BEFORE TRUNCATE ON accounting_opening_lines
    FOR EACH STATEMENT EXECUTE FUNCTION settlement_guard();
CREATE TRIGGER accounting_opening_baselines_no_truncate BEFORE TRUNCATE ON accounting_opening_baselines
    FOR EACH STATEMENT EXECUTE FUNCTION settlement_guard();
CREATE TRIGGER accounting_opening_corrections_no_truncate
    BEFORE TRUNCATE ON accounting_opening_corrections
    FOR EACH STATEMENT EXECUTE FUNCTION settlement_guard();
CREATE TRIGGER cash_place_openings_no_truncate BEFORE TRUNCATE ON cash_place_openings
    FOR EACH STATEMENT EXECUTE FUNCTION settlement_guard();

-- === M6-B CHECK block: inventory_value_entries classification (M5-A list + opening). An
-- existing-workspace opening seeds value without a movement (quantity already on hand); an
-- empty-workspace opening carries its stock movement. ===
ALTER TABLE inventory_value_entries DROP CONSTRAINT inventory_value_entries_classification_check,
    ADD CONSTRAINT inventory_value_entries_classification_check CHECK (classification IN (
        'adjustment', 'sale_issue', 'customer_return', 'transfer_out', 'transfer_in',
        'pack_out', 'pack_in', 'purchase_receipt', 'legacy_return', 'customer_return_reversal',
        'supplier_return', 'supplier_return_reversal', 'cost_adjustment', 'opening')),
    DROP CONSTRAINT inventory_value_entries_value_only_check,
    ADD CONSTRAINT inventory_value_entries_value_only_check CHECK (
        CASE classification
             WHEN 'cost_adjustment' THEN movement_id IS NULL AND quantity_delta = 0 AND value_delta <> 0
             WHEN 'opening' THEN quantity_before = 0 AND quantity_delta > 0
             ELSE movement_id IS NOT NULL AND quantity_delta <> 0 END);
-- === end M6-B CHECK block ===
