-- Farm equipment catalogue and village capacity (RUAHA pue.sql, energy.sql,
-- resource_catalogue.sql). Config tables: no provenance block. Prices are INDICATIVE, in
-- integer minor units of a workspace currency: not a quotation, not a financial record.

CREATE TABLE farm_equipment_categories (
    id           uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    code         text        NOT NULL CHECK (char_length(code) BETWEEN 1 AND 32),
    name_en      text        NOT NULL CHECK (char_length(name_en) BETWEEN 1 AND 120),
    name_sw      text        NOT NULL CHECK (char_length(name_sw) BETWEEN 1 AND 120),
    is_active    boolean     NOT NULL DEFAULT true,
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT farm_equipment_categories_code_key UNIQUE (workspace_id, code),
    CONSTRAINT farm_equipment_categories_workspace_id_id_key UNIQUE (workspace_id, id)
);

CREATE TABLE farm_equipment (
    id                            uuid         PRIMARY KEY DEFAULT uuidv7(),
    workspace_id                  uuid         NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    category_id                   uuid         NOT NULL,
    code                          text         NOT NULL CHECK (char_length(code) BETWEEN 1 AND 32),
    name_en                       text         NOT NULL CHECK (char_length(name_en) BETWEEN 1 AND 120),
    name_sw                       text         NOT NULL CHECK (char_length(name_sw) BETWEEN 1 AND 120),
    -- Null rated power = no energy estimate for requests of this machine.
    rated_power_kw                numeric(8,3) CHECK (rated_power_kw >= 0),
    typical_hours_per_day         numeric(4,2) CHECK (typical_hours_per_day BETWEEN 0 AND 24),
    typical_days_per_week         numeric(3,1) CHECK (typical_days_per_week BETWEEN 0 AND 7),
    can_rent                      boolean      NOT NULL DEFAULT false,
    can_buy                       boolean      NOT NULL DEFAULT true,
    indicative_price_minor        bigint       CHECK (indicative_price_minor >= 0),
    indicative_rent_per_day_minor bigint       CHECK (indicative_rent_per_day_minor >= 0),
    currency_id                   uuid         NOT NULL,
    is_active                     boolean      NOT NULL DEFAULT true,
    created_at                    timestamptz  NOT NULL DEFAULT now(),
    updated_at                    timestamptz  NOT NULL DEFAULT now(),
    CONSTRAINT farm_equipment_category_fk FOREIGN KEY (workspace_id, category_id)
        REFERENCES farm_equipment_categories (workspace_id, id),
    CONSTRAINT farm_equipment_currency_fk FOREIGN KEY (workspace_id, currency_id)
        REFERENCES currencies (workspace_id, id),
    CONSTRAINT farm_equipment_offered CHECK (can_rent OR can_buy),
    CONSTRAINT farm_equipment_code_key UNIQUE (workspace_id, code),
    CONSTRAINT farm_equipment_workspace_id_id_key UNIQUE (workspace_id, id)
);

CREATE INDEX farm_equipment_category_idx ON farm_equipment (workspace_id, category_id);
CREATE INDEX farm_equipment_currency_idx ON farm_equipment (workspace_id, currency_id);

-- Programme infrastructure, not observed data. 'measured' is absent on purpose (plan S03): add
-- it the day real meter data exists. Peak is rated power x simultaneity, never the plain sum.
CREATE TABLE farm_village_capacities (
    id                  uuid          PRIMARY KEY DEFAULT uuidv7(),
    workspace_id        uuid          NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    village_id          uuid          NOT NULL,
    capacity_kw         numeric(10,2) NOT NULL CHECK (capacity_kw >= 0),
    basis               text          NOT NULL DEFAULT 'planned' CHECK (basis IN ('planned', 'nameplate')),
    simultaneity_factor numeric(4,3)  NOT NULL DEFAULT 1.000
                            CHECK (simultaneity_factor > 0 AND simultaneity_factor <= 1),
    source_note         text          CHECK (char_length(source_note) <= 500),
    effective_from      date          NOT NULL DEFAULT current_date,
    -- A new figure retires the old one; history is kept.
    is_current          boolean       NOT NULL DEFAULT true,
    created_by          uuid          REFERENCES users (id) ON DELETE SET NULL,
    created_at          timestamptz   NOT NULL DEFAULT now(),
    CONSTRAINT farm_village_capacities_village_fk FOREIGN KEY (workspace_id, village_id)
        REFERENCES farm_villages (workspace_id, id)
);

CREATE UNIQUE INDEX farm_village_capacities_one_current ON farm_village_capacities (village_id)
    WHERE is_current;
CREATE INDEX farm_village_capacities_village_idx ON farm_village_capacities (workspace_id, village_id);
CREATE INDEX farm_village_capacities_created_by_idx ON farm_village_capacities (created_by)
    WHERE created_by IS NOT NULL;

CREATE TRIGGER farm_equipment_categories_audit AFTER INSERT OR UPDATE OR DELETE ON farm_equipment_categories
    FOR EACH ROW EXECUTE FUNCTION farm_write_audit();
CREATE TRIGGER farm_equipment_audit AFTER INSERT OR UPDATE OR DELETE ON farm_equipment
    FOR EACH ROW EXECUTE FUNCTION farm_write_audit();
CREATE TRIGGER farm_village_capacities_audit AFTER INSERT OR UPDATE OR DELETE ON farm_village_capacities
    FOR EACH ROW EXECUTE FUNCTION farm_write_audit();
