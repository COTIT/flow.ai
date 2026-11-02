ALTER TABLE farm_energy_estimates
    ALTER COLUMN est_power_kw SET EXPRESSION AS (rated_power_kw * quantity),
    ALTER COLUMN est_kwh_per_day SET EXPRESSION AS (rated_power_kw * quantity * hours_per_day),
    ALTER COLUMN est_kwh_per_week SET EXPRESSION AS
        (rated_power_kw * quantity * hours_per_day * days_per_week);

CREATE OR REPLACE FUNCTION farm_pue_recompute_estimate() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
    eq farm_equipment%ROWTYPE;
BEGIN
    SELECT * INTO eq FROM farm_equipment
     WHERE workspace_id = NEW.workspace_id AND id = NEW.equipment_id;
    IF eq.rated_power_kw IS NULL THEN
        DELETE FROM farm_energy_estimates WHERE pue_request_id = NEW.id;
        RETURN NEW;
    END IF;
    INSERT INTO farm_energy_estimates (workspace_id, village_id, pue_request_id, rated_power_kw,
        quantity, hours_per_day, days_per_week, confidence)
    VALUES (NEW.workspace_id, NEW.village_id, NEW.id, eq.rated_power_kw, NEW.quantity,
        coalesce(NEW.hours_per_day, eq.typical_hours_per_day, 0),
        coalesce(NEW.days_per_week, eq.typical_days_per_week, 0), NEW.confidence)
    ON CONFLICT (pue_request_id) DO UPDATE SET
        village_id = excluded.village_id, rated_power_kw = excluded.rated_power_kw,
        quantity = excluded.quantity, hours_per_day = excluded.hours_per_day,
        days_per_week = excluded.days_per_week, confidence = excluded.confidence,
        computed_at = now(), updated_at = now();
    RETURN NEW;
END $$;

DROP FUNCTION farm_est_kwh_per_week(numeric, integer, numeric, numeric);
DROP FUNCTION farm_est_kwh_per_day(numeric, integer, numeric);
DROP FUNCTION farm_est_power_kw(numeric, integer);
DROP FUNCTION farm_pue_estimate_inputs(uuid, uuid, integer, numeric, numeric);
