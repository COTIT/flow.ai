-- Flow ERP M5-B period FX revaluation (masterplan §13 "Period revaluation", §15): one run row
-- per posted revaluation of a period, and the carrying-adjustment writer. A carrying
-- adjustment moves its open item's functional carrying (functional_residual), so a later
-- allocation relieves the adjusted carrying and realizes only the rest (allocation trigger).

CREATE TABLE accounting_revaluations (
    id              uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id    uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    period_id       uuid        NOT NULL,
    rate_date       date        NOT NULL,
    accounting_date date        NOT NULL CHECK (rate_date <= accounting_date),
    effect_id       uuid        NOT NULL,
    created_by      uuid        NOT NULL,
    created_at      timestamptz NOT NULL DEFAULT clock_timestamp(),
    CONSTRAINT accounting_revaluations_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT accounting_revaluations_effect_key UNIQUE (effect_id),
    CONSTRAINT accounting_revaluations_period_fk FOREIGN KEY (workspace_id, period_id)
        REFERENCES accounting_periods (workspace_id, id),
    CONSTRAINT accounting_revaluations_effect_fk FOREIGN KEY (workspace_id, effect_id)
        REFERENCES accounting_effects (workspace_id, id)
);
CREATE INDEX accounting_revaluations_period_idx
    ON accounting_revaluations (workspace_id, period_id, created_at DESC);
CREATE TRIGGER accounting_revaluations_fixed BEFORE UPDATE OR DELETE ON accounting_revaluations
    FOR EACH ROW EXECUTE FUNCTION settlement_guard();
CREATE TRIGGER accounting_revaluations_no_truncate BEFORE TRUNCATE ON accounting_revaluations
    FOR EACH STATEMENT EXECUTE FUNCTION settlement_guard();

-- Revalued carrying may exceed the original functional amount.
ALTER TABLE accounting_open_items DROP CONSTRAINT accounting_open_items_residual_check;
ALTER TABLE accounting_open_items ADD CONSTRAINT accounting_open_items_residual_check
    CHECK (residual BETWEEN 0 AND amount AND functional_residual >= 0
           AND (residual = 0) = (functional_residual = 0));

-- Apply a carrying adjustment to its open item. The residual CHECK refuses a carrying that
-- would reach zero or below while the item is still open.
CREATE FUNCTION accounting_carrying_adjusted() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    item accounting_open_items;
BEGIN
    SELECT * INTO item FROM accounting_open_items
    WHERE workspace_id = NEW.workspace_id AND id = NEW.item_id FOR UPDATE;
    IF item.residual = 0 OR NEW.accounting_date < item.issue_date THEN
        RAISE EXCEPTION 'Carrying adjustment does not fit its item.' USING ERRCODE = '23514';
    END IF;
    UPDATE accounting_open_items
    SET functional_residual = functional_residual + NEW.functional_delta, version = version + 1
    WHERE workspace_id = NEW.workspace_id AND id = NEW.item_id;
    RETURN NEW;
END $$;
CREATE TRIGGER accounting_carrying_adjustments_applied
    BEFORE INSERT ON accounting_carrying_adjustments
    FOR EACH ROW EXECUTE FUNCTION accounting_carrying_adjusted();
