-- Deposits are immutable: a recorded deposit blocks this down migration (NOT NULL money_role).
ALTER TABLE customer_deposits
    DROP CONSTRAINT customer_deposits_bank_account_fk,
    DROP CONSTRAINT customer_deposits_location,
    DROP COLUMN confirmation_method,
    DROP COLUMN bank_account_id,
    DROP COLUMN location_kind,
    ADD COLUMN money_role text NOT NULL CHECK (money_role IN ('bank', 'cash', 'provider_clearing')),
    ADD CONSTRAINT customer_deposits_place CHECK (
        num_nonnulls(register_id, vault_id) = CASE WHEN money_role = 'cash' THEN 1 ELSE 0 END);
