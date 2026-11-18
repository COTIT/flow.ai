DROP TABLE bank_reconciliation_allocations;
DROP TABLE bank_reconciliations;
DROP TABLE bank_statement_lines;
DROP TABLE bank_statements;
DROP FUNCTION bank_facts_fixed();
DROP FUNCTION bank_statement_balanced();
DROP FUNCTION bank_journal_reversed(uuid, uuid);
DROP FUNCTION bank_amount_text(numeric, integer);

-- >>> import profile `bank_statement`: drop its batches, restore the two-profile CHECK
DELETE FROM import_batches WHERE profile = 'bank_statement';
ALTER TABLE import_batches DROP CONSTRAINT import_batches_profile_check;
ALTER TABLE import_batches ADD CONSTRAINT import_batches_profile_check
    CHECK (profile IN ('products', 'relations'));
-- <<< import profile
