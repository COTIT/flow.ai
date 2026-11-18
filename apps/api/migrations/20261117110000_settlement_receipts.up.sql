-- Flow ERP M4: active-book invoice payments are canonical receipts with an explicit money
-- location (masterplan §9), and journal lines can name the bank account they hit.
-- Setup-book rows keep every new column NULL. Only nullable columns are added: no row update.

ALTER TABLE journal_lines
    ADD COLUMN bank_account_id uuid,
    ADD CONSTRAINT journal_lines_bank_account_fk FOREIGN KEY (workspace_id, bank_account_id)
        REFERENCES bank_accounts (workspace_id, id);

ALTER TABLE invoice_payments
    ADD COLUMN location_kind text CONSTRAINT invoice_payments_location_kind_check
        CHECK (location_kind IN ('cash_place', 'bank_account', 'provider_clearing')),
    ADD COLUMN register_id uuid,
    ADD COLUMN vault_id uuid,
    ADD COLUMN bank_account_id uuid,
    ADD COLUMN reference text CONSTRAINT invoice_payments_reference_check
        CHECK (char_length(btrim(reference)) BETWEEN 1 AND 255),
    ADD COLUMN evidence_ref text CONSTRAINT invoice_payments_evidence_ref_check
        CHECK (char_length(btrim(evidence_ref)) BETWEEN 1 AND 255),
    ADD COLUMN confirmation_method text CONSTRAINT invoice_payments_confirmation_method_check
        CHECK (confirmation_method IN ('counted_cash', 'bank_statement', 'provider_dashboard',
                                       'provider_callback')),
    -- The receipt's canonical effect (active books).
    ADD COLUMN effect_id uuid,
    ADD CONSTRAINT invoice_payments_register_fk FOREIGN KEY (workspace_id, register_id)
        REFERENCES till_registers (workspace_id, id),
    ADD CONSTRAINT invoice_payments_vault_fk FOREIGN KEY (workspace_id, vault_id)
        REFERENCES cash_vaults (workspace_id, id),
    ADD CONSTRAINT invoice_payments_bank_account_fk FOREIGN KEY (workspace_id, bank_account_id)
        REFERENCES bank_accounts (workspace_id, id),
    ADD CONSTRAINT invoice_payments_effect_fk FOREIGN KEY (workspace_id, effect_id)
        REFERENCES accounting_effects (workspace_id, id),
    ADD CONSTRAINT invoice_payments_location_check CHECK (
        (location_kind IS NULL) = (effect_id IS NULL)
        AND CASE location_kind
            WHEN 'cash_place' THEN num_nonnulls(register_id, vault_id) = 1
                AND num_nonnulls(bank_account_id, reference, evidence_ref, confirmation_method) = 0
            WHEN 'bank_account' THEN register_id IS NULL AND vault_id IS NULL
                AND num_nonnulls(bank_account_id, reference, evidence_ref, confirmation_method) = 4
            WHEN 'provider_clearing' THEN
                num_nonnulls(register_id, vault_id, bank_account_id) = 0
                AND num_nonnulls(reference, evidence_ref, confirmation_method) = 3
            ELSE num_nonnulls(register_id, vault_id, bank_account_id, reference, evidence_ref,
                              confirmation_method) = 0
        END);
