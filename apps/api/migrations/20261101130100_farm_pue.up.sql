-- Phase 4c step 2: productive-use (PUE) requests and their Level 1 energy estimates
-- (RUAHA pue.sql, energy.sql, resource_catalogue.sql, mvp_security.sql). Who may move a
-- request where is a farm-role rule and lives in farm_pue::rules; the estimate is
-- arithmetic over stored inputs and lives here, so no writer can get it wrong.

-- OBSERVED: a person applied, so it carries the provenance block. MVP stops at the
-- decision; delivery, terms and repayment are not modelled.
CREATE TABLE farm_pue_requests (
    id            uuid         PRIMARY KEY DEFAULT uuidv7(),
    workspace_id  uuid         NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    village_id    uuid         NOT NULL,
    person_id     uuid         NOT NULL,
    farm_id       uuid,
    equipment_id  uuid         NOT NULL,
    -- What the farmer asks for; the catalogue must offer it (checked on create).
    acquisition   text         NOT NULL DEFAULT 'buy' CHECK (acquisition IN ('rent', 'buy')),
    quantity      integer      NOT NULL DEFAULT 1 CHECK (quantity > 0),
    hours_per_day numeric(4,2) CHECK (hours_per_day BETWEEN 0 AND 24),
    days_per_week numeric(3,1) CHECK (days_per_week BETWEEN 0 AND 7),
    purpose       text         CHECK (char_length(purpose) <= 1000),
    status        text         NOT NULL DEFAULT 'submitted'
                      CHECK (status IN ('draft', 'submitted', 'under_review', 'approved',
                                        'rejected', 'withdrawn')),
    submitted_at  timestamptz,
    decided_at    timestamptz,
    decided_by    uuid         REFERENCES users (id) ON DELETE SET NULL,
    decision_note text         CHECK (char_length(decision_note) <= 1000),
    source        text         NOT NULL CHECK (source IN ('farmer_reported', 'field_verified',
                      'transaction_derived', 'sensor_derived', 'model_estimated')),
    captured_at   timestamptz  NOT NULL DEFAULT now(),
    captured_by   uuid         REFERENCES users (id),
    verification  text         NOT NULL DEFAULT 'unverified'
                      CHECK (verification IN ('unverified', 'pending', 'verified', 'disputed')),
    verified_by   uuid         REFERENCES users (id),
    verified_at   timestamptz,
    confidence    text         CHECK (confidence IN ('low', 'medium', 'high')),
    evidence_ref  text         CHECK (char_length(evidence_ref) <= 500),
    created_at    timestamptz  NOT NULL DEFAULT now(),
    updated_at    timestamptz  NOT NULL DEFAULT now(),
    deleted_at    timestamptz,
    CHECK (verification <> 'verified' OR (verified_by IS NOT NULL AND verified_at IS NOT NULL)),
    -- Person and farm sit in the request's village: a cross-village request cannot exist.
    CONSTRAINT farm_pue_requests_person_fk FOREIGN KEY (workspace_id, village_id, person_id)
        REFERENCES farm_persons (workspace_id, village_id, id),
    CONSTRAINT farm_pue_requests_farm_fk FOREIGN KEY (workspace_id, village_id, farm_id)
        REFERENCES farms (workspace_id, village_id, id),
    CONSTRAINT farm_pue_requests_equipment_fk FOREIGN KEY (workspace_id, equipment_id)
        REFERENCES farm_equipment (workspace_id, id),
    CONSTRAINT farm_pue_requests_scope_key UNIQUE (workspace_id, village_id, id)
);

CREATE INDEX farm_pue_requests_village_idx ON farm_pue_requests (workspace_id, village_id, status)
    WHERE deleted_at IS NULL;
CREATE INDEX farm_pue_requests_person_idx ON farm_pue_requests (person_id) WHERE deleted_at IS NULL;

-- One row per request. Inputs are SNAPSHOTTED at compute time so the number stays
-- explainable after the catalogue changes underneath it; outputs are GENERATED, so stored
-- arithmetic can never drift from stored inputs. No route writes this table.
CREATE TABLE farm_energy_estimates (
    id               uuid         PRIMARY KEY DEFAULT uuidv7(),
    workspace_id     uuid         NOT NULL,
    village_id       uuid         NOT NULL,
    pue_request_id   uuid         NOT NULL UNIQUE,
    rated_power_kw   numeric(8,3) NOT NULL CHECK (rated_power_kw >= 0),
    quantity         integer      NOT NULL CHECK (quantity > 0),
    hours_per_day    numeric(4,2) NOT NULL CHECK (hours_per_day BETWEEN 0 AND 24),
    days_per_week    numeric(3,1) NOT NULL CHECK (days_per_week BETWEEN 0 AND 7),
    est_power_kw     numeric(12,3) GENERATED ALWAYS AS (rated_power_kw * quantity) STORED,
    est_kwh_per_day  numeric(12,3) GENERATED ALWAYS AS
                         (rated_power_kw * quantity * hours_per_day) STORED,
    est_kwh_per_week numeric(12,3) GENERATED ALWAYS AS
                         (rated_power_kw * quantity * hours_per_day * days_per_week) STORED,
    method           text         NOT NULL DEFAULT 'level1_rated_x_hours',
    source           text         NOT NULL DEFAULT 'model_estimated' CHECK (source = 'model_estimated'),
    confidence       text         CHECK (confidence IN ('low', 'medium', 'high')),
    computed_at      timestamptz  NOT NULL DEFAULT now(),
    created_at       timestamptz  NOT NULL DEFAULT now(),
    updated_at       timestamptz  NOT NULL DEFAULT now(),
    CONSTRAINT farm_energy_estimates_request_fk FOREIGN KEY (workspace_id, village_id, pue_request_id)
        REFERENCES farm_pue_requests (workspace_id, village_id, id) ON DELETE CASCADE
        ON UPDATE CASCADE
);

CREATE INDEX farm_energy_estimates_village_idx ON farm_energy_estimates (workspace_id, village_id);

-- Every write of the inputs recomputes; a status change does not. Hours and days fall back
-- to the catalogue's typical figures, then 0. A machine with no rated power has no estimate.
CREATE FUNCTION farm_pue_recompute_estimate() RETURNS trigger
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

CREATE TRIGGER farm_pue_recompute_estimate AFTER INSERT OR UPDATE OF equipment_id, quantity,
    hours_per_day, days_per_week, village_id, confidence ON farm_pue_requests
    FOR EACH ROW EXECUTE FUNCTION farm_pue_recompute_estimate();

CREATE TRIGGER farm_pue_requests_audit AFTER INSERT OR UPDATE OR DELETE ON farm_pue_requests
    FOR EACH ROW EXECUTE FUNCTION farm_write_audit();
CREATE TRIGGER farm_energy_estimates_audit AFTER INSERT OR UPDATE OR DELETE ON farm_energy_estimates
    FOR EACH ROW EXECUTE FUNCTION farm_write_audit();
