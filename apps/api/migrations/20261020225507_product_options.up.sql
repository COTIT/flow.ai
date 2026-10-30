-- Batch 6E: option types and values for product variants (Laravel ERP/Product option tables).
-- Names and values are stored lowercased (Laravel firstOrCreate lowercased them); the typed
-- text lives in display_name / display_value.

CREATE TABLE option_types (
    id           uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    name         text        NOT NULL CHECK (char_length(name) <= 255 AND name = lower(name)),
    display_name text        NOT NULL CHECK (char_length(display_name) <= 255),
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now(),
    deleted_at   timestamptz,
    CONSTRAINT option_types_workspace_id_id_key UNIQUE (workspace_id, id)
);
CREATE UNIQUE INDEX option_types_name_unique ON option_types (workspace_id, name) WHERE deleted_at IS NULL;

CREATE TABLE option_values (
    id             uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id   uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    option_type_id uuid        NOT NULL,
    value          text        NOT NULL CHECK (char_length(value) <= 255 AND value = lower(value)),
    display_value  text        NOT NULL CHECK (char_length(display_value) <= 255),
    sort_order     integer,
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now(),
    deleted_at     timestamptz,
    CONSTRAINT option_values_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT option_values_type_fk FOREIGN KEY (workspace_id, option_type_id)
        REFERENCES option_types (workspace_id, id)
);
CREATE UNIQUE INDEX option_values_value_unique ON option_values (option_type_id, value)
    WHERE deleted_at IS NULL;

CREATE TABLE product_option_types (
    workspace_id   uuid    NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    product_id     uuid    NOT NULL,
    option_type_id uuid    NOT NULL,
    sort_order     integer NOT NULL DEFAULT 0,
    PRIMARY KEY (product_id, option_type_id),
    CONSTRAINT product_option_types_product_fk FOREIGN KEY (workspace_id, product_id)
        REFERENCES products (workspace_id, id),
    CONSTRAINT product_option_types_type_fk FOREIGN KEY (workspace_id, option_type_id)
        REFERENCES option_types (workspace_id, id)
);

CREATE TABLE variant_option_values (
    workspace_id       uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    product_variant_id uuid NOT NULL,
    option_value_id    uuid NOT NULL,
    PRIMARY KEY (product_variant_id, option_value_id),
    CONSTRAINT variant_option_values_variant_fk FOREIGN KEY (workspace_id, product_variant_id)
        REFERENCES product_variants (workspace_id, id),
    CONSTRAINT variant_option_values_value_fk FOREIGN KEY (workspace_id, option_value_id)
        REFERENCES option_values (workspace_id, id)
);
CREATE INDEX variant_option_values_value_idx ON variant_option_values (option_value_id);
