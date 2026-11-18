-- Flow ERP M4 cash places: the cash ledger also holds money movements (receipt, payment,
-- refund, adjustment) with a member or system actor and immutable source/effect references.
-- No row is updated: new columns use defaults (old rows read actor_kind = 'till'), so the
-- append-only trigger never fires. New CHECKs validate the existing rows.

ALTER TABLE cash_ledger
    ADD COLUMN actor_kind text NOT NULL DEFAULT 'till'
        CONSTRAINT cash_ledger_actor_kind_check CHECK (actor_kind IN ('till', 'member', 'system')),
    -- Approving member (no FK, as journal_entries.posted_by: the row outlives memberships).
    ADD COLUMN member_user_id uuid,
    ADD COLUMN source_kind text CONSTRAINT cash_ledger_source_kind_check
        CHECK (source_kind ~ '^[a-z_]{1,64}$'),
    ADD COLUMN source_id uuid,
    ADD COLUMN effect_id uuid,
    ALTER COLUMN till_user_id DROP NOT NULL,
    ADD CONSTRAINT cash_ledger_effect_fk FOREIGN KEY (workspace_id, effect_id)
        REFERENCES accounting_effects (workspace_id, id),
    DROP CONSTRAINT cash_ledger_kind_check,
    ADD CONSTRAINT cash_ledger_kind_check CHECK (kind IN ('open_count', 'close_count', 'transfer',
        'sale', 'receipt', 'payment', 'refund', 'adjustment')),
    -- Drawer rows name register + shift; an adjustment is a book correction: never in a shift.
    DROP CONSTRAINT cash_ledger_place_check,
    ADD CONSTRAINT cash_ledger_place_check CHECK (num_nonnulls(register_id, vault_id) = 1
        AND (vault_id IS NULL OR kind IN ('transfer', 'receipt', 'payment', 'refund', 'adjustment'))
        AND (register_id IS NULL OR (shift_id IS NULL) = (kind = 'adjustment'))
        AND (vault_id IS NULL OR shift_id IS NULL)),
    DROP CONSTRAINT cash_ledger_denomination_check,
    ADD CONSTRAINT cash_ledger_denomination_check CHECK ((denomination IS NULL) = (count IS NULL)
        AND (denomination IS NOT NULL) = (kind IN ('open_count', 'close_count', 'transfer'))),
    ADD CONSTRAINT cash_ledger_actor_check CHECK (
        (kind NOT IN ('open_count', 'close_count', 'transfer', 'sale') OR actor_kind = 'till')
        AND (actor_kind <> 'till' OR (till_user_id IS NOT NULL AND member_user_id IS NULL))
        AND (actor_kind <> 'member' OR member_user_id IS NOT NULL)
        AND (actor_kind <> 'system' OR (till_user_id IS NULL AND member_user_id IS NULL))),
    ADD CONSTRAINT cash_ledger_money_check CHECK (
        (kind IN ('receipt', 'payment', 'refund', 'adjustment'))
            = (source_kind IS NOT NULL AND source_id IS NOT NULL AND effect_id IS NOT NULL)
        AND (source_kind IS NULL) = (source_id IS NULL)
        AND (source_kind IS NULL) = (effect_id IS NULL)),
    -- Signed into the place: receipt in, payment and refund out, adjustment either way.
    ADD CONSTRAINT cash_ledger_sign_check CHECK (CASE kind
        WHEN 'receipt' THEN amount > 0
        WHEN 'payment' THEN amount < 0
        WHEN 'refund' THEN amount < 0
        WHEN 'adjustment' THEN amount <> 0
        ELSE true END);
CREATE INDEX cash_ledger_effect_idx ON cash_ledger (workspace_id, effect_id) WHERE effect_id IS NOT NULL;

CREATE OR REPLACE FUNCTION cash_ledger_append_only() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION '% is append-only (%)', TG_TABLE_NAME, TG_OP USING ERRCODE = 'restrict_violation';
END
$$;

-- A reviewed count that corrected a place's book cash: expected (active balance under the
-- place lock), counted, difference. Append-only; the ledger row and journal point here.
CREATE TABLE cash_adjustments (
    id                 uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id       uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    register_id        uuid,
    vault_id           uuid,
    currency_id        uuid        NOT NULL,
    expected           bigint      NOT NULL,
    counted            bigint      NOT NULL CHECK (counted >= 0),
    difference         bigint      NOT NULL,
    count_evidence_ref text        NOT NULL CHECK (char_length(btrim(count_evidence_ref)) BETWEEN 1 AND 255),
    reason             text        NOT NULL CHECK (char_length(btrim(reason)) BETWEEN 1 AND 1000),
    accounting_date    date        NOT NULL,
    member_user_id     uuid        NOT NULL,
    -- Cashier context of a privileged Till approval; NULL from the Office.
    till_user_id       uuid,
    effect_id          uuid        NOT NULL,
    -- Set in active books only.
    journal_entry_id   uuid,
    created_at         timestamptz NOT NULL DEFAULT clock_timestamp(),
    CONSTRAINT cash_adjustments_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT cash_adjustments_place_check CHECK (num_nonnulls(register_id, vault_id) = 1),
    CONSTRAINT cash_adjustments_difference_check CHECK (difference <> 0
        AND difference = counted - expected),
    CONSTRAINT cash_adjustments_register_fk FOREIGN KEY (workspace_id, register_id)
        REFERENCES till_registers (workspace_id, id),
    CONSTRAINT cash_adjustments_vault_fk FOREIGN KEY (workspace_id, vault_id)
        REFERENCES cash_vaults (workspace_id, id),
    CONSTRAINT cash_adjustments_currency_fk FOREIGN KEY (workspace_id, currency_id)
        REFERENCES currencies (workspace_id, id),
    CONSTRAINT cash_adjustments_till_user_fk FOREIGN KEY (workspace_id, till_user_id)
        REFERENCES till_users (workspace_id, id),
    CONSTRAINT cash_adjustments_effect_fk FOREIGN KEY (workspace_id, effect_id)
        REFERENCES accounting_effects (workspace_id, id),
    CONSTRAINT cash_adjustments_effect_key UNIQUE (effect_id),
    CONSTRAINT cash_adjustments_journal_fk FOREIGN KEY (workspace_id, journal_entry_id)
        REFERENCES journal_entries (workspace_id, id)
);
CREATE INDEX cash_adjustments_list_idx ON cash_adjustments (workspace_id, created_at DESC, id DESC);
CREATE INDEX cash_adjustments_register_idx ON cash_adjustments (workspace_id, register_id)
    WHERE register_id IS NOT NULL;
CREATE INDEX cash_adjustments_vault_idx ON cash_adjustments (workspace_id, vault_id)
    WHERE vault_id IS NOT NULL;
CREATE INDEX cash_adjustments_till_user_idx ON cash_adjustments (workspace_id, till_user_id)
    WHERE till_user_id IS NOT NULL;
CREATE INDEX cash_adjustments_journal_idx ON cash_adjustments (workspace_id, journal_entry_id)
    WHERE journal_entry_id IS NOT NULL;
CREATE INDEX cash_adjustments_currency_idx ON cash_adjustments (workspace_id, currency_id);

CREATE TRIGGER cash_adjustments_fixed BEFORE UPDATE OR DELETE ON cash_adjustments
    FOR EACH ROW EXECUTE FUNCTION cash_ledger_append_only();
CREATE TRIGGER cash_adjustments_no_truncate BEFORE TRUNCATE ON cash_adjustments
    FOR EACH STATEMENT EXECUTE FUNCTION cash_ledger_append_only();
