-- Farm records (RUAHA identity + production). Observed tables carry the provenance block:
-- where a fact came from, who captured it, who verified it. Every child foreign key carries
-- (workspace_id, village_id), so a row can never point into another village or workspace.
-- Rows are soft-deleted only (deleted_at); every read filters it. No captured_by/verified_by
-- index: users are never hard-deleted, so those keys are never checked on delete.

CREATE TABLE farm_crops (
    id           uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    code         text        NOT NULL CHECK (char_length(code) BETWEEN 1 AND 32),
    name_en      text        NOT NULL CHECK (char_length(name_en) BETWEEN 1 AND 120),
    name_sw      text        NOT NULL CHECK (char_length(name_sw) BETWEEN 1 AND 120),
    -- Which measure a crop cycle of this crop must carry.
    measured_by  text        NOT NULL DEFAULT 'area' CHECK (measured_by IN ('area', 'tree_count', 'unit_count')),
    is_active    boolean     NOT NULL DEFAULT true,
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT farm_crops_code_key UNIQUE (workspace_id, code),
    CONSTRAINT farm_crops_workspace_id_id_key UNIQUE (workspace_id, id)
);

CREATE TABLE farm_persons (
    id           uuid         PRIMARY KEY DEFAULT uuidv7(),
    workspace_id uuid         NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    village_id   uuid         NOT NULL,
    given_name   text         NOT NULL CHECK (char_length(given_name) BETWEEN 1 AND 80),
    family_name  text         NOT NULL CHECK (char_length(family_name) BETWEEN 1 AND 80),
    -- Normalised Tanzanian mobile; it is the farmer's login name.
    phone        text         CHECK (phone ~ '^\+255[67][0-9]{8}$'),
    -- The account this person signs in with, if any.
    user_id      uuid         REFERENCES users (id) ON DELETE SET NULL,
    source       text         NOT NULL CHECK (source IN ('farmer_reported', 'field_verified',
                     'transaction_derived', 'sensor_derived', 'model_estimated')),
    captured_at  timestamptz  NOT NULL DEFAULT now(),
    captured_by  uuid         REFERENCES users (id),
    verification text         NOT NULL DEFAULT 'unverified'
                     CHECK (verification IN ('unverified', 'pending', 'verified', 'disputed')),
    verified_by  uuid         REFERENCES users (id),
    verified_at  timestamptz,
    confidence   text         CHECK (confidence IN ('low', 'medium', 'high')),
    evidence_ref text         CHECK (char_length(evidence_ref) <= 500),
    created_at   timestamptz  NOT NULL DEFAULT now(),
    updated_at   timestamptz  NOT NULL DEFAULT now(),
    deleted_at   timestamptz,
    -- Verified rows always name their verifier and time.
    CHECK (verification <> 'verified' OR (verified_by IS NOT NULL AND verified_at IS NOT NULL)),
    CONSTRAINT farm_persons_village_fk FOREIGN KEY (workspace_id, village_id)
        REFERENCES farm_villages (workspace_id, id),
    -- Target of child foreign keys: a child row sits in its parent's workspace and village.
    CONSTRAINT farm_persons_scope_key UNIQUE (workspace_id, village_id, id)
);

CREATE INDEX farm_persons_village_idx ON farm_persons (workspace_id, village_id) WHERE deleted_at IS NULL;
-- One person per account per workspace.
CREATE UNIQUE INDEX farm_persons_user_key ON farm_persons (workspace_id, user_id)
    WHERE user_id IS NOT NULL;

CREATE TABLE farm_households (
    id           uuid         PRIMARY KEY DEFAULT uuidv7(),
    workspace_id uuid         NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    village_id   uuid         NOT NULL,
    label        text         NOT NULL CHECK (char_length(label) BETWEEN 1 AND 120),
    source       text         NOT NULL CHECK (source IN ('farmer_reported', 'field_verified',
                     'transaction_derived', 'sensor_derived', 'model_estimated')),
    captured_at  timestamptz  NOT NULL DEFAULT now(),
    captured_by  uuid         REFERENCES users (id),
    verification text         NOT NULL DEFAULT 'unverified'
                     CHECK (verification IN ('unverified', 'pending', 'verified', 'disputed')),
    verified_by  uuid         REFERENCES users (id),
    verified_at  timestamptz,
    confidence   text         CHECK (confidence IN ('low', 'medium', 'high')),
    evidence_ref text         CHECK (char_length(evidence_ref) <= 500),
    created_at   timestamptz  NOT NULL DEFAULT now(),
    updated_at   timestamptz  NOT NULL DEFAULT now(),
    deleted_at   timestamptz,
    -- Verified rows always name their verifier and time.
    CHECK (verification <> 'verified' OR (verified_by IS NOT NULL AND verified_at IS NOT NULL)),
    CONSTRAINT farm_households_village_fk FOREIGN KEY (workspace_id, village_id)
        REFERENCES farm_villages (workspace_id, id),
    -- Target of child foreign keys: a child row sits in its parent's workspace and village.
    CONSTRAINT farm_households_scope_key UNIQUE (workspace_id, village_id, id)
);

CREATE INDEX farm_households_village_idx ON farm_households (workspace_id, village_id) WHERE deleted_at IS NULL;
CREATE TABLE farm_household_members (
    id           uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    village_id   uuid        NOT NULL,
    household_id uuid        NOT NULL,
    person_id    uuid        NOT NULL,
    is_head      boolean     NOT NULL DEFAULT false,
    created_at   timestamptz NOT NULL DEFAULT now(),
    -- Both ends in the same village: a cross-village household is impossible.
    CONSTRAINT farm_household_members_household_fk FOREIGN KEY (workspace_id, village_id, household_id)
        REFERENCES farm_households (workspace_id, village_id, id),
    CONSTRAINT farm_household_members_person_fk FOREIGN KEY (workspace_id, village_id, person_id)
        REFERENCES farm_persons (workspace_id, village_id, id),
    CONSTRAINT farm_household_members_key UNIQUE (household_id, person_id)
);

CREATE UNIQUE INDEX farm_household_members_one_head ON farm_household_members (household_id) WHERE is_head;
CREATE INDEX farm_household_members_person_idx ON farm_household_members (person_id);

CREATE TABLE farms (
    id           uuid         PRIMARY KEY DEFAULT uuidv7(),
    workspace_id uuid         NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    village_id   uuid         NOT NULL,
    household_id uuid,
    label        text         NOT NULL CHECK (char_length(label) BETWEEN 1 AND 120),
    latitude     numeric(9,6) CHECK (latitude BETWEEN -90 AND 90),
    longitude    numeric(9,6) CHECK (longitude BETWEEN -180 AND 180),
    source       text         NOT NULL CHECK (source IN ('farmer_reported', 'field_verified',
                     'transaction_derived', 'sensor_derived', 'model_estimated')),
    captured_at  timestamptz  NOT NULL DEFAULT now(),
    captured_by  uuid         REFERENCES users (id),
    verification text         NOT NULL DEFAULT 'unverified'
                     CHECK (verification IN ('unverified', 'pending', 'verified', 'disputed')),
    verified_by  uuid         REFERENCES users (id),
    verified_at  timestamptz,
    confidence   text         CHECK (confidence IN ('low', 'medium', 'high')),
    evidence_ref text         CHECK (char_length(evidence_ref) <= 500),
    created_at   timestamptz  NOT NULL DEFAULT now(),
    updated_at   timestamptz  NOT NULL DEFAULT now(),
    deleted_at   timestamptz,
    -- Verified rows always name their verifier and time.
    CHECK (verification <> 'verified' OR (verified_by IS NOT NULL AND verified_at IS NOT NULL)),
    CONSTRAINT farms_village_fk FOREIGN KEY (workspace_id, village_id)
        REFERENCES farm_villages (workspace_id, id),
    -- Target of child foreign keys: a child row sits in its parent's workspace and village.
    CONSTRAINT farms_scope_key UNIQUE (workspace_id, village_id, id),
    CONSTRAINT farms_household_fk FOREIGN KEY (workspace_id, village_id, household_id)
        REFERENCES farm_households (workspace_id, village_id, id)
);

CREATE INDEX farms_village_idx ON farms (workspace_id, village_id) WHERE deleted_at IS NULL;
CREATE INDEX farms_household_idx ON farms (household_id) WHERE deleted_at IS NULL;

CREATE TABLE farm_managers (
    id           uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    village_id   uuid        NOT NULL,
    farm_id      uuid        NOT NULL,
    person_id    uuid        NOT NULL,
    is_primary   boolean     NOT NULL DEFAULT false,
    created_at   timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT farm_managers_farm_fk FOREIGN KEY (workspace_id, village_id, farm_id)
        REFERENCES farms (workspace_id, village_id, id),
    CONSTRAINT farm_managers_person_fk FOREIGN KEY (workspace_id, village_id, person_id)
        REFERENCES farm_persons (workspace_id, village_id, id),
    CONSTRAINT farm_managers_key UNIQUE (farm_id, person_id)
);

CREATE UNIQUE INDEX farm_managers_one_primary ON farm_managers (farm_id) WHERE is_primary;
CREATE INDEX farm_managers_person_idx ON farm_managers (person_id);

CREATE TABLE farm_plots (
    id           uuid         PRIMARY KEY DEFAULT uuidv7(),
    workspace_id uuid         NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    village_id   uuid         NOT NULL,
    farm_id      uuid         NOT NULL,
    label        text         NOT NULL CHECK (char_length(label) BETWEEN 1 AND 120),
    -- Always hectares; acres are a display choice.
    area_ha      numeric(10,4) CHECK (area_ha >= 0),
    latitude     numeric(9,6) CHECK (latitude BETWEEN -90 AND 90),
    longitude    numeric(9,6) CHECK (longitude BETWEEN -180 AND 180),
    source       text         NOT NULL CHECK (source IN ('farmer_reported', 'field_verified',
                     'transaction_derived', 'sensor_derived', 'model_estimated')),
    captured_at  timestamptz  NOT NULL DEFAULT now(),
    captured_by  uuid         REFERENCES users (id),
    verification text         NOT NULL DEFAULT 'unverified'
                     CHECK (verification IN ('unverified', 'pending', 'verified', 'disputed')),
    verified_by  uuid         REFERENCES users (id),
    verified_at  timestamptz,
    confidence   text         CHECK (confidence IN ('low', 'medium', 'high')),
    evidence_ref text         CHECK (char_length(evidence_ref) <= 500),
    created_at   timestamptz  NOT NULL DEFAULT now(),
    updated_at   timestamptz  NOT NULL DEFAULT now(),
    deleted_at   timestamptz,
    -- Verified rows always name their verifier and time.
    CHECK (verification <> 'verified' OR (verified_by IS NOT NULL AND verified_at IS NOT NULL)),
    CONSTRAINT farm_plots_village_fk FOREIGN KEY (workspace_id, village_id)
        REFERENCES farm_villages (workspace_id, id),
    -- Target of child foreign keys: a child row sits in its parent's workspace and village.
    CONSTRAINT farm_plots_scope_key UNIQUE (workspace_id, village_id, id),
    CONSTRAINT farm_plots_farm_fk FOREIGN KEY (workspace_id, village_id, farm_id)
        REFERENCES farms (workspace_id, village_id, id)
);

CREATE INDEX farm_plots_village_idx ON farm_plots (workspace_id, village_id) WHERE deleted_at IS NULL;
CREATE INDEX farm_plots_farm_idx ON farm_plots (farm_id) WHERE deleted_at IS NULL;

-- Intercropping: several concurrent cycles per plot, so cycle areas may exceed plot area.
CREATE TABLE farm_crop_cycles (
    id           uuid         PRIMARY KEY DEFAULT uuidv7(),
    workspace_id uuid         NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    village_id   uuid         NOT NULL,
    plot_id      uuid         NOT NULL,
    crop_id      uuid         NOT NULL,
    season_label text         CHECK (char_length(season_label) <= 60),
    area_ha      numeric(10,4) CHECK (area_ha >= 0),
    tree_count   integer      CHECK (tree_count >= 0),
    unit_count   integer      CHECK (unit_count >= 0),
    planted_on   date,
    harvest_start date,
    harvest_end  date,
    status       text         NOT NULL DEFAULT 'planned'
                     CHECK (status IN ('planned', 'growing', 'harvested', 'abandoned')),
    source       text         NOT NULL CHECK (source IN ('farmer_reported', 'field_verified',
                     'transaction_derived', 'sensor_derived', 'model_estimated')),
    captured_at  timestamptz  NOT NULL DEFAULT now(),
    captured_by  uuid         REFERENCES users (id),
    verification text         NOT NULL DEFAULT 'unverified'
                     CHECK (verification IN ('unverified', 'pending', 'verified', 'disputed')),
    verified_by  uuid         REFERENCES users (id),
    verified_at  timestamptz,
    confidence   text         CHECK (confidence IN ('low', 'medium', 'high')),
    evidence_ref text         CHECK (char_length(evidence_ref) <= 500),
    created_at   timestamptz  NOT NULL DEFAULT now(),
    updated_at   timestamptz  NOT NULL DEFAULT now(),
    deleted_at   timestamptz,
    -- Verified rows always name their verifier and time.
    CHECK (verification <> 'verified' OR (verified_by IS NOT NULL AND verified_at IS NOT NULL)),
    CONSTRAINT farm_crop_cycles_village_fk FOREIGN KEY (workspace_id, village_id)
        REFERENCES farm_villages (workspace_id, id),
    -- Target of child foreign keys: a child row sits in its parent's workspace and village.
    CONSTRAINT farm_crop_cycles_scope_key UNIQUE (workspace_id, village_id, id),
    CONSTRAINT farm_crop_cycles_plot_fk FOREIGN KEY (workspace_id, village_id, plot_id)
        REFERENCES farm_plots (workspace_id, village_id, id),
    CONSTRAINT farm_crop_cycles_crop_fk FOREIGN KEY (workspace_id, crop_id)
        REFERENCES farm_crops (workspace_id, id),
    CONSTRAINT farm_crop_cycles_window CHECK (harvest_end IS NULL OR harvest_start IS NULL
                                              OR harvest_end >= harvest_start)
);

CREATE INDEX farm_crop_cycles_village_idx ON farm_crop_cycles (workspace_id, village_id) WHERE deleted_at IS NULL;
CREATE INDEX farm_crop_cycles_plot_idx ON farm_crop_cycles (plot_id) WHERE deleted_at IS NULL;
CREATE INDEX farm_crop_cycles_crop_idx ON farm_crop_cycles (workspace_id, crop_id) WHERE deleted_at IS NULL;

-- A series, not a column: estimates get revised and the old figures stay on record.
CREATE TABLE farm_harvest_reports (
    id           uuid         PRIMARY KEY DEFAULT uuidv7(),
    workspace_id uuid         NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    village_id   uuid         NOT NULL,
    crop_cycle_id uuid        NOT NULL,
    kind         text         NOT NULL CHECK (kind IN ('expected', 'actual')),
    quantity_kg  numeric(12,2) NOT NULL CHECK (quantity_kg >= 0),
    reported_for date,
    is_current   boolean      NOT NULL DEFAULT true,
    source       text         NOT NULL CHECK (source IN ('farmer_reported', 'field_verified',
                     'transaction_derived', 'sensor_derived', 'model_estimated')),
    captured_at  timestamptz  NOT NULL DEFAULT now(),
    captured_by  uuid         REFERENCES users (id),
    verification text         NOT NULL DEFAULT 'unverified'
                     CHECK (verification IN ('unverified', 'pending', 'verified', 'disputed')),
    verified_by  uuid         REFERENCES users (id),
    verified_at  timestamptz,
    confidence   text         CHECK (confidence IN ('low', 'medium', 'high')),
    evidence_ref text         CHECK (char_length(evidence_ref) <= 500),
    created_at   timestamptz  NOT NULL DEFAULT now(),
    updated_at   timestamptz  NOT NULL DEFAULT now(),
    deleted_at   timestamptz,
    -- Verified rows always name their verifier and time.
    CHECK (verification <> 'verified' OR (verified_by IS NOT NULL AND verified_at IS NOT NULL)),
    CONSTRAINT farm_harvest_reports_village_fk FOREIGN KEY (workspace_id, village_id)
        REFERENCES farm_villages (workspace_id, id),
    -- Target of child foreign keys: a child row sits in its parent's workspace and village.
    CONSTRAINT farm_harvest_reports_scope_key UNIQUE (workspace_id, village_id, id),
    CONSTRAINT farm_harvest_reports_cycle_fk FOREIGN KEY (workspace_id, village_id, crop_cycle_id)
        REFERENCES farm_crop_cycles (workspace_id, village_id, id)
);

CREATE INDEX farm_harvest_reports_village_idx ON farm_harvest_reports (workspace_id, village_id) WHERE deleted_at IS NULL;
-- Exactly one live figure per cycle and kind, so totals can't double count.
CREATE UNIQUE INDEX farm_harvest_reports_one_current ON farm_harvest_reports (crop_cycle_id, kind)
    WHERE is_current AND deleted_at IS NULL;

-- Idempotent registration: the device makes client_ref once per form; a retry returns this.
CREATE TABLE farm_registration_receipts (
    workspace_id uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    client_ref   uuid        NOT NULL,
    created_by   uuid        NOT NULL REFERENCES users (id),
    result       jsonb       NOT NULL,
    created_at   timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (workspace_id, client_ref)
);

CREATE TRIGGER farm_crops_audit AFTER INSERT OR UPDATE OR DELETE ON farm_crops
    FOR EACH ROW EXECUTE FUNCTION farm_write_audit();
CREATE TRIGGER farm_persons_audit AFTER INSERT OR UPDATE OR DELETE ON farm_persons
    FOR EACH ROW EXECUTE FUNCTION farm_write_audit();
CREATE TRIGGER farm_households_audit AFTER INSERT OR UPDATE OR DELETE ON farm_households
    FOR EACH ROW EXECUTE FUNCTION farm_write_audit();
CREATE TRIGGER farm_household_members_audit AFTER INSERT OR UPDATE OR DELETE ON farm_household_members
    FOR EACH ROW EXECUTE FUNCTION farm_write_audit();
CREATE TRIGGER farms_audit AFTER INSERT OR UPDATE OR DELETE ON farms
    FOR EACH ROW EXECUTE FUNCTION farm_write_audit();
CREATE TRIGGER farm_managers_audit AFTER INSERT OR UPDATE OR DELETE ON farm_managers
    FOR EACH ROW EXECUTE FUNCTION farm_write_audit();
CREATE TRIGGER farm_plots_audit AFTER INSERT OR UPDATE OR DELETE ON farm_plots
    FOR EACH ROW EXECUTE FUNCTION farm_write_audit();
CREATE TRIGGER farm_crop_cycles_audit AFTER INSERT OR UPDATE OR DELETE ON farm_crop_cycles
    FOR EACH ROW EXECUTE FUNCTION farm_write_audit();
CREATE TRIGGER farm_harvest_reports_audit AFTER INSERT OR UPDATE OR DELETE ON farm_harvest_reports
    FOR EACH ROW EXECUTE FUNCTION farm_write_audit();

-- What a farmer may read, from the person their account is linked to. Staff reads go by
-- village and never need these. NULL person = nothing.
CREATE FUNCTION farm_person_households(p uuid) RETURNS SETOF uuid LANGUAGE sql STABLE AS $$
    SELECT hm.household_id
    FROM farm_household_members hm
    JOIN farm_households h ON h.id = hm.household_id AND h.deleted_at IS NULL
    WHERE hm.person_id = p
$$;

-- The person plus everyone sharing a household with them.
CREATE FUNCTION farm_person_persons(p uuid) RETURNS SETOF uuid LANGUAGE sql STABLE AS $$
    SELECT p WHERE p IS NOT NULL
    UNION
    SELECT them.person_id
    FROM farm_household_members them
    WHERE them.household_id IN (SELECT farm_person_households(p))
$$;

-- Farms the person manages, or that belong to their household.
CREATE FUNCTION farm_person_farms(p uuid) RETURNS SETOF uuid LANGUAGE sql STABLE AS $$
    SELECT fm.farm_id
    FROM farm_managers fm
    JOIN farms f ON f.id = fm.farm_id AND f.deleted_at IS NULL
    WHERE fm.person_id = p
    UNION
    SELECT f.id FROM farms f
    WHERE f.deleted_at IS NULL AND f.household_id IN (SELECT farm_person_households(p))
$$;

-- The provenance block as one JSON object, actors as {id, name}. Only called on rows the
-- caller may already see, so a name never leaks past its record.
CREATE FUNCTION farm_provenance(
    source text, captured_at timestamptz, captured_by uuid, verification text,
    verified_by uuid, verified_at timestamptz, confidence text, evidence_ref text
) RETURNS jsonb LANGUAGE sql STABLE AS $$
    SELECT jsonb_build_object(
        'source', source,
        'captured_at', captured_at,
        'captured_by', (SELECT jsonb_build_object('id', u.id, 'name', concat_ws(' ', u.first_name, u.last_name))
                        FROM users u WHERE u.id = captured_by),
        'verification', verification,
        'verified_by', (SELECT jsonb_build_object('id', u.id, 'name', concat_ws(' ', u.first_name, u.last_name))
                        FROM users u WHERE u.id = verified_by),
        'verified_at', verified_at,
        'confidence', confidence,
        'evidence_ref', evidence_ref)
$$;
