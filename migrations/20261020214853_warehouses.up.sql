-- Batch 6A: warehouses + flat warehouse locations (Laravel ERP/Warehouse, ERP/WarehouseLocation).
-- Code shape (requests/resources/models), not the raw mysql-schema.sql snapshot.

CREATE TABLE warehouses (
    id                          uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id                uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    parent_id                   uuid,
    name                        text        NOT NULL CHECK (char_length(name) <= 150),
    -- Str::slug(name) at create, never changed after.
    slug                        text        NOT NULL,
    alias                       text        CHECK (char_length(alias) <= 55),
    description                 text,
    -- Nullable in Laravel too; POS treats only true as a store.
    is_store                    boolean,
    default_warehouse           boolean     NOT NULL DEFAULT false,
    stock_count_enabled         boolean     NOT NULL DEFAULT false,
    pickup_enabled              boolean     NOT NULL DEFAULT false,
    auto_generate_transfer_list boolean     NOT NULL DEFAULT false,
    total_sum_enabled           boolean     NOT NULL DEFAULT false,
    -- Workspace members only (composite FKs below); leaving the workspace clears them.
    user_id                     uuid,
    manager_id                  uuid,
    manager_assistant_id        uuid,
    -- Laravel Infrastructure\Values\Address.
    location                    jsonb       CHECK (location IS NULL OR jsonb_typeof(location) = 'object'),
    status                      text        CHECK (status IN ('active', 'construction', 'inactive', 'maintenance')),
    type                        text        CHECK (type IN ('cross-dock', 'distribution', 'fulfillment', 'processing', 'storage', 'store')),
    -- {total_area, storage_height} as JSON numbers (Laravel WarehouseDimensions).
    dimensions                  jsonb       CHECK (dimensions IS NULL OR jsonb_typeof(dimensions) = 'object'),
    loading_docks               integer,
    sections                    integer,
    rows_per_sections           integer,
    shelves_per_row             integer,
    bins_per_shelf              integer,
    management_contact          jsonb       CHECK (management_contact IS NULL OR jsonb_typeof(management_contact) = 'object'),
    operating_hours             jsonb       CHECK (operating_hours IS NULL OR jsonb_typeof(operating_hours) = 'object'),
    additional_settings         jsonb       CHECK (additional_settings IS NULL OR jsonb_typeof(additional_settings) = 'object'),
    meta                        jsonb       CHECK (meta IS NULL OR jsonb_typeof(meta) = 'object'),
    created_at                  timestamptz NOT NULL DEFAULT now(),
    updated_at                  timestamptz NOT NULL DEFAULT now(),
    deleted_at                  timestamptz,
    CONSTRAINT warehouses_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT warehouses_parent_fk FOREIGN KEY (workspace_id, parent_id)
        REFERENCES warehouses (workspace_id, id),
    CONSTRAINT warehouses_not_own_parent CHECK (parent_id <> id),
    CONSTRAINT warehouses_user_member_fk FOREIGN KEY (workspace_id, user_id)
        REFERENCES workspace_members (workspace_id, user_id) ON DELETE SET NULL (user_id),
    CONSTRAINT warehouses_manager_member_fk FOREIGN KEY (workspace_id, manager_id)
        REFERENCES workspace_members (workspace_id, user_id) ON DELETE SET NULL (manager_id),
    CONSTRAINT warehouses_assistant_member_fk FOREIGN KEY (workspace_id, manager_assistant_id)
        REFERENCES workspace_members (workspace_id, user_id)
        ON DELETE SET NULL (manager_assistant_id)
);
-- Laravel's unique (slug, workspace_id) counted deleted rows (500 on re-create); live rows only here.
CREATE UNIQUE INDEX warehouses_slug_unique ON warehouses (workspace_id, slug) WHERE deleted_at IS NULL;
-- Laravel kept "one default per workspace" in app code only.
CREATE UNIQUE INDEX warehouses_one_default ON warehouses (workspace_id)
    WHERE default_warehouse AND deleted_at IS NULL;
CREATE INDEX warehouses_parent_idx ON warehouses (workspace_id, parent_id) WHERE deleted_at IS NULL;

CREATE TABLE warehouse_locations (
    id           uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    warehouse_id uuid        NOT NULL,
    -- Free-text scan label, e.g. "A.01.02".
    location     text        NOT NULL CHECK (char_length(location) <= 500),
    meta         jsonb       CHECK (meta IS NULL OR jsonb_typeof(meta) = 'object'),
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now(),
    deleted_at   timestamptz,
    CONSTRAINT warehouse_locations_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT warehouse_locations_warehouse_fk FOREIGN KEY (workspace_id, warehouse_id)
        REFERENCES warehouses (workspace_id, id)
);
CREATE INDEX warehouse_locations_warehouse_idx ON warehouse_locations (workspace_id, warehouse_id)
    WHERE deleted_at IS NULL;
