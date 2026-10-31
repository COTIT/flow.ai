-- Phase 2b step 2: vaults, drawer shifts and the cash ledger. A register's drawer is counted
-- at open and close; between them, sales (step 4) and drawer<->vault transfers move cash.
-- Money is bigint minor units of the register's currency. The ledger is append-only.

CREATE TABLE cash_vaults (
    id           uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    name         text        NOT NULL CHECK (char_length(name) BETWEEN 1 AND 100),
    currency_id  uuid        NOT NULL,
    created_by   uuid        REFERENCES users (id) ON DELETE SET NULL,
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT cash_vaults_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT cash_vaults_currency_fk FOREIGN KEY (workspace_id, currency_id)
        REFERENCES currencies (workspace_id, id)
);
CREATE INDEX cash_vaults_list_idx ON cash_vaults (workspace_id, name, id);
CREATE INDEX cash_vaults_currency_idx ON cash_vaults (workspace_id, currency_id);
CREATE INDEX cash_vaults_created_by_idx ON cash_vaults (created_by) WHERE created_by IS NOT NULL;

-- One drawer session of a register: opened with a count, closed with a count. `expected` is
-- what the ledger says the drawer holds at close; `difference` = counted - expected.
CREATE TABLE drawer_shifts (
    id           uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    register_id  uuid        NOT NULL,
    opened_by    uuid        NOT NULL,
    opened_at    timestamptz NOT NULL DEFAULT now(),
    opening      bigint      NOT NULL CHECK (opening >= 0),
    closed_by    uuid,
    closed_at    timestamptz,
    expected     bigint,
    counted      bigint      CHECK (counted >= 0),
    difference   bigint,
    CONSTRAINT drawer_shifts_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT drawer_shifts_closed_check CHECK (
        num_nulls(closed_by, closed_at, expected, counted, difference) IN (0, 5)
        AND (difference IS NULL OR difference = counted - expected)),
    CONSTRAINT drawer_shifts_register_fk FOREIGN KEY (workspace_id, register_id)
        REFERENCES till_registers (workspace_id, id),
    CONSTRAINT drawer_shifts_opened_by_fk FOREIGN KEY (workspace_id, opened_by)
        REFERENCES till_users (workspace_id, id),
    CONSTRAINT drawer_shifts_closed_by_fk FOREIGN KEY (workspace_id, closed_by)
        REFERENCES till_users (workspace_id, id)
);
CREATE UNIQUE INDEX drawer_shifts_open_key ON drawer_shifts (register_id) WHERE closed_at IS NULL;
CREATE INDEX drawer_shifts_list_idx ON drawer_shifts (workspace_id, opened_at DESC, id DESC);
CREATE INDEX drawer_shifts_register_idx ON drawer_shifts (workspace_id, register_id, opened_at DESC);
CREATE INDEX drawer_shifts_opened_by_idx ON drawer_shifts (workspace_id, opened_by);
CREATE INDEX drawer_shifts_closed_by_idx ON drawer_shifts (workspace_id, closed_by)
    WHERE closed_by IS NOT NULL;

-- Every cash movement of a drawer (register + shift) or a vault. Counts are one row per
-- denomination; a sale (step 4) is one row without a denomination. A shift's balance is
-- Σ amount of its rows except `close_count`; a vault's is Σ amount of its rows.
CREATE TABLE cash_ledger (
    id           uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    register_id  uuid,
    shift_id     uuid,
    vault_id     uuid,
    kind         text        NOT NULL CHECK (kind IN ('open_count', 'close_count', 'transfer', 'sale')),
    -- Both sides of one transfer share it.
    transfer_id  uuid,
    -- The till sale (an order) a `sale` row belongs to (step 4).
    order_id     uuid,
    -- Minor units per note or coin, and how many.
    denomination bigint      CONSTRAINT cash_ledger_positive_check CHECK (denomination > 0),
    count        integer     CHECK (count >= 0),
    -- Signed minor units: into this drawer or vault is positive.
    amount       bigint      NOT NULL,
    till_user_id uuid        NOT NULL,
    created_at   timestamptz NOT NULL DEFAULT clock_timestamp(),
    CONSTRAINT cash_ledger_place_check CHECK (num_nonnulls(register_id, vault_id) = 1
        AND (register_id IS NULL) = (shift_id IS NULL)
        AND (vault_id IS NULL OR kind = 'transfer')),
    CONSTRAINT cash_ledger_transfer_check CHECK ((kind = 'transfer') = (transfer_id IS NOT NULL)),
    CONSTRAINT cash_ledger_sale_check CHECK ((kind = 'sale') = (order_id IS NOT NULL)),
    CONSTRAINT cash_ledger_denomination_check CHECK ((denomination IS NULL) = (count IS NULL)
        AND (kind = 'sale' OR denomination IS NOT NULL)),
    CONSTRAINT cash_ledger_register_fk FOREIGN KEY (workspace_id, register_id)
        REFERENCES till_registers (workspace_id, id),
    CONSTRAINT cash_ledger_shift_fk FOREIGN KEY (workspace_id, shift_id)
        REFERENCES drawer_shifts (workspace_id, id),
    CONSTRAINT cash_ledger_vault_fk FOREIGN KEY (workspace_id, vault_id)
        REFERENCES cash_vaults (workspace_id, id),
    CONSTRAINT cash_ledger_user_fk FOREIGN KEY (workspace_id, till_user_id)
        REFERENCES till_users (workspace_id, id)
);
CREATE INDEX cash_ledger_shift_idx ON cash_ledger (workspace_id, shift_id) WHERE shift_id IS NOT NULL;
CREATE INDEX cash_ledger_vault_idx ON cash_ledger (workspace_id, vault_id) WHERE vault_id IS NOT NULL;
CREATE INDEX cash_ledger_register_idx ON cash_ledger (workspace_id, register_id)
    WHERE register_id IS NOT NULL;
CREATE INDEX cash_ledger_transfer_idx ON cash_ledger (workspace_id, transfer_id)
    WHERE transfer_id IS NOT NULL;
CREATE INDEX cash_ledger_order_idx ON cash_ledger (workspace_id, order_id) WHERE order_id IS NOT NULL;
CREATE INDEX cash_ledger_user_idx ON cash_ledger (workspace_id, till_user_id);

CREATE FUNCTION cash_ledger_append_only() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'cash_ledger is append-only (%)', TG_OP USING ERRCODE = 'restrict_violation';
END
$$;

CREATE TRIGGER cash_ledger_fixed BEFORE UPDATE OR DELETE ON cash_ledger
    FOR EACH ROW EXECUTE FUNCTION cash_ledger_append_only();
CREATE TRIGGER cash_ledger_no_truncate BEFORE TRUNCATE ON cash_ledger
    FOR EACH STATEMENT EXECUTE FUNCTION cash_ledger_append_only();
