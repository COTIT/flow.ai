-- Customer deposits record where the money is with the shared money-location columns
-- (settlement MoneyLocation: cash place, bank account, provider clearing + proof).
ALTER TABLE customer_deposits
    DROP CONSTRAINT customer_deposits_place,
    DROP COLUMN money_role,
    ADD COLUMN location_kind text NOT NULL
        CHECK (location_kind IN ('cash_place', 'bank_account', 'provider_clearing')),
    ADD COLUMN bank_account_id uuid,
    ADD COLUMN confirmation_method text CHECK (confirmation_method IN
        ('counted_cash', 'bank_statement', 'provider_dashboard', 'provider_callback')),
    ADD CONSTRAINT customer_deposits_location CHECK (
        (location_kind = 'cash_place') = (num_nonnulls(register_id, vault_id) = 1)
        AND num_nonnulls(register_id, vault_id) <= 1
        AND (location_kind = 'bank_account') = (bank_account_id IS NOT NULL)
        AND (location_kind = 'cash_place') = (confirmation_method IS NULL)
        AND (location_kind = 'cash_place') = (reference IS NULL AND evidence_ref IS NULL)),
    ADD CONSTRAINT customer_deposits_bank_account_fk FOREIGN KEY (workspace_id, bank_account_id)
        REFERENCES bank_accounts (workspace_id, id);
