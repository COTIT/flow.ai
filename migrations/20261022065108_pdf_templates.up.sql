-- PDF templates (Laravel WorkspaceSetting/PdfTemplate): the layout a document prints with.
-- Columns pick item fields, the header and footer texts are the user's rich-text HTML with
-- `{variable}` placeholders, logo and background are stored files. Nothing limits a domain
-- to one default template (Laravel).
CREATE TABLE pdf_templates (
    id                 uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id       uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    name               text        NOT NULL CHECK (char_length(name) BETWEEN 1 AND 255),
    domain             text        NOT NULL CHECK (domain IN (
        'invoice', 'estimate', 'order', 'purchase_order', 'collective_invoice', 'work_order',
        'cash_register', 'relation')),
    domain_action      text        CHECK (char_length(domain_action) <= 255),
    columns            jsonb       NOT NULL CHECK (jsonb_typeof(columns) = 'array'
                                                   AND octet_length(columns::text) <= 16384),
    header_left        text        CHECK (char_length(header_left) <= 65535),
    header_right       text        CHECK (char_length(header_right) <= 65535),
    header_top         text        CHECK (char_length(header_top) <= 65535),
    footer             text        CHECK (char_length(footer) <= 65535),
    padding_top        integer     NOT NULL DEFAULT 0 CHECK (padding_top BETWEEN 0 AND 40),
    padding_bottom     integer     NOT NULL DEFAULT 0 CHECK (padding_bottom BETWEEN 0 AND 40),
    padding_x          integer     NOT NULL DEFAULT 0 CHECK (padding_x BETWEEN 0 AND 40),
    enable_summary     boolean     NOT NULL DEFAULT true,
    is_default         boolean     NOT NULL DEFAULT false,
    conditions         jsonb       CHECK (conditions IS NULL OR jsonb_typeof(conditions) = 'object'),
    extra_fields       jsonb       CHECK (extra_fields IS NULL OR (jsonb_typeof(extra_fields) = 'object'
                                          AND octet_length(extra_fields::text) <= 16384)),
    meta               jsonb       CHECK (meta IS NULL OR (jsonb_typeof(meta) = 'object'
                                          AND octet_length(meta::text) <= 16384)),
    background_file_id uuid,
    logo_file_id       uuid,
    created_at         timestamptz NOT NULL DEFAULT now(),
    updated_at         timestamptz NOT NULL DEFAULT now(),
    deleted_at         timestamptz,
    CONSTRAINT pdf_templates_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT pdf_templates_background_fk FOREIGN KEY (workspace_id, background_file_id)
        REFERENCES files (workspace_id, id),
    CONSTRAINT pdf_templates_logo_fk FOREIGN KEY (workspace_id, logo_file_id)
        REFERENCES files (workspace_id, id)
);

CREATE INDEX pdf_templates_list_idx ON pdf_templates (workspace_id, created_at DESC, id DESC)
    WHERE deleted_at IS NULL;
CREATE INDEX pdf_templates_background_idx ON pdf_templates (workspace_id, background_file_id)
    WHERE background_file_id IS NOT NULL;
CREATE INDEX pdf_templates_logo_idx ON pdf_templates (workspace_id, logo_file_id)
    WHERE logo_file_id IS NOT NULL;
