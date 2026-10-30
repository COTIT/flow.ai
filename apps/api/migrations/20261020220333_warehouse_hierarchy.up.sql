-- Batch 6B: warehouse -> section -> row -> shelf -> bin (Laravel ERP/WarehouseSection*, code
-- shape after 2026_10_20_120000_converge_warehouse_sections_to_code_shape).
-- Every level stores its full parent chain, and the FK to its parent covers that whole chain,
-- so a row can't point at another warehouse's or workspace's parent. Codes are unique per
-- parent among live rows, ignoring case (Laravel's MySQL collation).

CREATE TABLE warehouse_sections (
    id                  uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id        uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    warehouse_id        uuid        NOT NULL,
    section_name        text        NOT NULL CHECK (char_length(section_name) <= 255),
    section_code        text        NOT NULL CHECK (char_length(section_code) <= 50),
    category            text        CHECK (char_length(category) <= 255),
    description         text,
    rows                integer     CHECK (rows >= 0),
    shelves_per_row     integer     CHECK (shelves_per_row >= 0),
    bins_per_shelf      integer     CHECK (bins_per_shelf >= 0),
    total_bins          integer     CHECK (total_bins >= 0),
    zone                text        CHECK (char_length(zone) <= 255),
    temperature_control text        NOT NULL DEFAULT 'ambient'
                                    CHECK (temperature_control IN ('ambient', 'cold', 'freezer', 'heated')),
    restricted_access   boolean     NOT NULL DEFAULT false,
    -- A workspace member (composite FK below); leaving the workspace clears it.
    assigned_manager    uuid,
    priority_level      text        NOT NULL DEFAULT 'standard'
                                    CHECK (priority_level IN ('low', 'standard', 'high', 'critical')),
    created_at          timestamptz NOT NULL DEFAULT now(),
    updated_at          timestamptz NOT NULL DEFAULT now(),
    deleted_at          timestamptz,
    CONSTRAINT warehouse_sections_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT warehouse_sections_chain_key UNIQUE (workspace_id, warehouse_id, id),
    CONSTRAINT warehouse_sections_warehouse_fk FOREIGN KEY (workspace_id, warehouse_id)
        REFERENCES warehouses (workspace_id, id),
    CONSTRAINT warehouse_sections_manager_member_fk FOREIGN KEY (workspace_id, assigned_manager)
        REFERENCES workspace_members (workspace_id, user_id) ON DELETE SET NULL (assigned_manager)
);
CREATE UNIQUE INDEX warehouse_sections_code_unique ON warehouse_sections (warehouse_id, lower(section_code))
    WHERE deleted_at IS NULL;

CREATE TABLE warehouse_section_rows (
    id                             uuid             PRIMARY KEY DEFAULT uuidv7(),
    workspace_id                   uuid             NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    warehouse_id                   uuid             NOT NULL,
    section_id                     uuid             NOT NULL,
    row_code                       text             NOT NULL CHECK (char_length(row_code) <= 100),
    row_name                       text             NOT NULL CHECK (char_length(row_name) <= 255),
    orientation                    text             CHECK (char_length(orientation) <= 100),
    row_type                       text             CHECK (char_length(row_type) <= 100),
    -- Physical sizes: Laravel doubles, printed as JSON numbers.
    length_meters                  double precision NOT NULL CHECK (length_meters >= 0),
    width_meters                   double precision NOT NULL CHECK (width_meters >= 0),
    height_meters                  double precision NOT NULL CHECK (height_meters >= 0),
    calculated_volume_cubic_meters double precision CHECK (calculated_volume_cubic_meters >= 0),
    number_of_shelves              integer          CHECK (number_of_shelves >= 0),
    shelf_spacing_cm               integer          CHECK (shelf_spacing_cm >= 0),
    position_x                     double precision,
    position_y                     double precision,
    access_side                    text             CHECK (char_length(access_side) <= 100),
    aisle_width_cm                 integer          CHECK (aisle_width_cm >= 0),
    equipment_access               text             CHECK (char_length(equipment_access) <= 100),
    priority_level                 text             NOT NULL DEFAULT 'standard'
                                                    CHECK (priority_level IN ('low', 'standard', 'high', 'critical')),
    temperature_zone               text             NOT NULL DEFAULT 'ambient'
                                                    CHECK (temperature_zone IN ('ambient', 'cold', 'freezer', 'heated')),
    notes                          text,
    enable_automatic_bin_labeling  boolean          NOT NULL DEFAULT false,
    generate_qr_codes_for_shelves  boolean          NOT NULL DEFAULT false,
    created_at                     timestamptz      NOT NULL DEFAULT now(),
    updated_at                     timestamptz      NOT NULL DEFAULT now(),
    deleted_at                     timestamptz,
    CONSTRAINT warehouse_section_rows_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT warehouse_section_rows_chain_key UNIQUE (workspace_id, warehouse_id, section_id, id),
    CONSTRAINT warehouse_section_rows_section_fk FOREIGN KEY (workspace_id, warehouse_id, section_id)
        REFERENCES warehouse_sections (workspace_id, warehouse_id, id)
);
CREATE UNIQUE INDEX warehouse_section_rows_code_unique ON warehouse_section_rows (section_id, lower(row_code))
    WHERE deleted_at IS NULL;

CREATE TABLE warehouse_section_row_shelves (
    id                 uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id       uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    warehouse_id       uuid        NOT NULL,
    section_id         uuid        NOT NULL,
    row_id             uuid        NOT NULL,
    shelf_code         text        NOT NULL CHECK (char_length(shelf_code) <= 100),
    shelf_name         text        CHECK (char_length(shelf_name) <= 255),
    shelf_level        integer     NOT NULL CHECK (shelf_level >= 1),
    side               text        CHECK (side IN ('left', 'right')),
    bins_per_shelf     integer     CHECK (bins_per_shelf >= 0),
    total_storage_bins integer     CHECK (total_storage_bins >= 0),
    max_weight_kg      integer     CHECK (max_weight_kg >= 0),
    notes              text,
    created_at         timestamptz NOT NULL DEFAULT now(),
    updated_at         timestamptz NOT NULL DEFAULT now(),
    deleted_at         timestamptz,
    CONSTRAINT warehouse_section_row_shelves_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT warehouse_section_row_shelves_chain_key
        UNIQUE (workspace_id, warehouse_id, section_id, row_id, id),
    CONSTRAINT warehouse_section_row_shelves_row_fk
        FOREIGN KEY (workspace_id, warehouse_id, section_id, row_id)
        REFERENCES warehouse_section_rows (workspace_id, warehouse_id, section_id, id)
);
CREATE UNIQUE INDEX warehouse_section_row_shelves_code_unique
    ON warehouse_section_row_shelves (row_id, lower(shelf_code)) WHERE deleted_at IS NULL;

CREATE TABLE warehouse_section_row_bins (
    id                     uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id           uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    warehouse_id           uuid        NOT NULL,
    section_id             uuid        NOT NULL,
    row_id                 uuid        NOT NULL,
    shelf_id               uuid        NOT NULL,
    bin_code               text        NOT NULL CHECK (char_length(bin_code) <= 150),
    bin_name               text        CHECK (char_length(bin_name) <= 255),
    side                   text        CHECK (side IN ('left', 'right')),
    position               text        CHECK (char_length(position) <= 100),
    bin_type               text        NOT NULL DEFAULT 'standard'
                                       CHECK (bin_type IN ('standard', 'oversize', 'fragile', 'hazmat', 'cold_storage')),
    length_cm              integer     CHECK (length_cm >= 0),
    width_cm               integer     CHECK (width_cm >= 0),
    height_cm              integer     CHECK (height_cm >= 0),
    volume_m3              double precision CHECK (volume_m3 >= 0),
    max_weight_kg          integer     CHECK (max_weight_kg >= 0),
    load_rating            text        CHECK (char_length(load_rating) <= 100),
    access_type            text        NOT NULL DEFAULT 'manual_access'
                                       CHECK (access_type IN ('manual_access', 'conveyor', 'robotic_pick')),
    priority_level         text        NOT NULL DEFAULT 'standard'
                                       CHECK (priority_level IN ('low', 'standard', 'high', 'critical')),
    temperature_monitoring boolean     NOT NULL DEFAULT false,
    barcode_scanner        boolean     NOT NULL DEFAULT false,
    rfid_enabled           boolean     NOT NULL DEFAULT false,
    security_lock          boolean     NOT NULL DEFAULT false,
    notes                  text,
    created_at             timestamptz NOT NULL DEFAULT now(),
    updated_at             timestamptz NOT NULL DEFAULT now(),
    deleted_at             timestamptz,
    CONSTRAINT warehouse_section_row_bins_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT warehouse_section_row_bins_chain_key
        UNIQUE (workspace_id, warehouse_id, section_id, row_id, shelf_id, id),
    CONSTRAINT warehouse_section_row_bins_shelf_fk
        FOREIGN KEY (workspace_id, warehouse_id, section_id, row_id, shelf_id)
        REFERENCES warehouse_section_row_shelves (workspace_id, warehouse_id, section_id, row_id, id)
);
-- Laravel's DB key was (row_id, bin_code): unique per row, not per shelf.
CREATE UNIQUE INDEX warehouse_section_row_bins_code_unique
    ON warehouse_section_row_bins (row_id, lower(bin_code)) WHERE deleted_at IS NULL;
-- Bin list of one shelf.
CREATE INDEX warehouse_section_row_bins_shelf_idx ON warehouse_section_row_bins (shelf_id, id)
    WHERE deleted_at IS NULL;
-- GET /warehouses/{id}/bins reads a whole warehouse.
CREATE INDEX warehouse_section_row_bins_warehouse_idx
    ON warehouse_section_row_bins (workspace_id, warehouse_id) WHERE deleted_at IS NULL;
