ALTER TABLE invoice_payments
    DROP CONSTRAINT invoice_payments_location_check,
    DROP COLUMN effect_id,
    DROP COLUMN confirmation_method,
    DROP COLUMN evidence_ref,
    DROP COLUMN reference,
    DROP COLUMN bank_account_id,
    DROP COLUMN vault_id,
    DROP COLUMN register_id,
    DROP COLUMN location_kind;

ALTER TABLE journal_lines DROP COLUMN bank_account_id;
