-- ==== SHARED CHECK WIDENING (C12) revert: back to main's exact constraints. ====
ALTER TABLE accounting_allocations
    DROP CONSTRAINT accounting_allocations_items_check,
    ADD CONSTRAINT accounting_allocations_items_check CHECK (
        (kind = 'rounding') = (source_item_id IS NULL)
        AND source_item_id IS DISTINCT FROM target_item_id),
    DROP CONSTRAINT accounting_allocations_kind_check,
    ADD CONSTRAINT accounting_allocations_kind_check
        CHECK (kind IN ('application', 'reclassification', 'rounding'));
-- ==== END SHARED CHECK WIDENING (C12) ====
DROP TABLE accounting_exceptions;
DROP TABLE provider_observations;
DROP FUNCTION provider_observation_guard();
