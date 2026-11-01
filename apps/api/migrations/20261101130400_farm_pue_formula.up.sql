-- Phase 4c: the Level 1 energy formula, written once. The stored estimate's generated columns
-- and the no-write preview (POST /farm/pue-requests/estimate) both call these functions, so a
-- preview is exactly what a request would store.

-- Inputs: rated power from the catalogue; hours and days as sent, else the catalogue's typical
-- figures, else 0. No row when the machine has no rated power (no estimate).
CREATE FUNCTION farm_pue_estimate_inputs(ws uuid, equipment uuid, qty integer, hours numeric, days numeric)
RETURNS TABLE (rated_power_kw numeric(8,3), quantity integer, hours_per_day numeric(4,2), days_per_week numeric(3,1))
LANGUAGE sql STABLE AS $$
    SELECT eq.rated_power_kw, qty,
           coalesce(hours, eq.typical_hours_per_day, 0)::numeric(4,2),
           coalesce(days, eq.typical_days_per_week, 0)::numeric(3,1)
    FROM farm_equipment eq
    WHERE eq.workspace_id = ws AND eq.id = equipment AND eq.rated_power_kw IS NOT NULL
$$;

CREATE FUNCTION farm_est_power_kw(rated numeric, qty integer) RETURNS numeric
LANGUAGE sql IMMUTABLE PARALLEL SAFE RETURN rated * qty;
CREATE FUNCTION farm_est_kwh_per_day(rated numeric, qty integer, hours numeric) RETURNS numeric
LANGUAGE sql IMMUTABLE PARALLEL SAFE RETURN rated * qty * hours;
CREATE FUNCTION farm_est_kwh_per_week(rated numeric, qty integer, hours numeric, days numeric) RETURNS numeric
LANGUAGE sql IMMUTABLE PARALLEL SAFE RETURN rated * qty * hours * days;

ALTER TABLE farm_energy_estimates
    ALTER COLUMN est_power_kw SET EXPRESSION AS (farm_est_power_kw(rated_power_kw, quantity)),
    ALTER COLUMN est_kwh_per_day SET EXPRESSION AS
        (farm_est_kwh_per_day(rated_power_kw, quantity, hours_per_day)),
    ALTER COLUMN est_kwh_per_week SET EXPRESSION AS
        (farm_est_kwh_per_week(rated_power_kw, quantity, hours_per_day, days_per_week));

CREATE OR REPLACE FUNCTION farm_pue_recompute_estimate() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
    i record;
BEGIN
    SELECT * INTO i FROM farm_pue_estimate_inputs(NEW.workspace_id, NEW.equipment_id, NEW.quantity,
        NEW.hours_per_day, NEW.days_per_week);
    IF NOT FOUND THEN
        DELETE FROM farm_energy_estimates WHERE pue_request_id = NEW.id;
        RETURN NEW;
    END IF;

    INSERT INTO farm_energy_estimates (workspace_id, village_id, pue_request_id, rated_power_kw,
        quantity, hours_per_day, days_per_week, confidence)
    VALUES (NEW.workspace_id, NEW.village_id, NEW.id, i.rated_power_kw, i.quantity,
        i.hours_per_day, i.days_per_week, NEW.confidence)
    ON CONFLICT (pue_request_id) DO UPDATE SET
        village_id     = excluded.village_id,
        rated_power_kw = excluded.rated_power_kw,
        quantity       = excluded.quantity,
        hours_per_day  = excluded.hours_per_day,
        days_per_week  = excluded.days_per_week,
        confidence     = excluded.confidence,
        computed_at    = now(),
        updated_at     = now();
    RETURN NEW;
END $$;
