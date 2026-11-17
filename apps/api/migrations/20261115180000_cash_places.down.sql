-- Undo M4 cash places. Fails once any receipt/payment/refund/adjustment or member/system row
-- exists: the ledger is append-only, so those rows can't be removed and the old CHECKs refuse
-- them. Pre-release only.

DROP TABLE cash_adjustments;

CREATE OR REPLACE FUNCTION cash_ledger_append_only() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'cash_ledger is append-only (%)', TG_OP USING ERRCODE = 'restrict_violation';
END
$$;

DROP INDEX cash_ledger_effect_idx;
ALTER TABLE cash_ledger
    DROP CONSTRAINT cash_ledger_sign_check,
    DROP CONSTRAINT cash_ledger_money_check,
    DROP CONSTRAINT cash_ledger_actor_check,
    DROP CONSTRAINT cash_ledger_denomination_check,
    ADD CONSTRAINT cash_ledger_denomination_check CHECK ((denomination IS NULL) = (count IS NULL)
        AND (kind = 'sale' OR denomination IS NOT NULL)),
    DROP CONSTRAINT cash_ledger_place_check,
    ADD CONSTRAINT cash_ledger_place_check CHECK (num_nonnulls(register_id, vault_id) = 1
        AND (register_id IS NULL) = (shift_id IS NULL)
        AND (vault_id IS NULL OR kind = 'transfer')),
    DROP CONSTRAINT cash_ledger_kind_check,
    ADD CONSTRAINT cash_ledger_kind_check CHECK (kind IN ('open_count', 'close_count', 'transfer', 'sale')),
    DROP CONSTRAINT cash_ledger_effect_fk,
    ALTER COLUMN till_user_id SET NOT NULL,
    DROP COLUMN effect_id,
    DROP COLUMN source_id,
    DROP COLUMN source_kind,
    DROP COLUMN member_user_id,
    DROP COLUMN actor_kind;
