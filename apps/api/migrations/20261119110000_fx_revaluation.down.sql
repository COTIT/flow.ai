DROP TRIGGER accounting_carrying_adjustments_applied ON accounting_carrying_adjustments;
DROP FUNCTION accounting_carrying_adjusted();
ALTER TABLE accounting_open_items DROP CONSTRAINT accounting_open_items_residual_check;
ALTER TABLE accounting_open_items ADD CONSTRAINT accounting_open_items_residual_check
    CHECK (residual BETWEEN 0 AND amount
           AND functional_residual BETWEEN 0 AND functional_amount
           AND (residual = 0) = (functional_residual = 0));
DROP TABLE accounting_revaluations;
