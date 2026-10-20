-- CRM relations (customers, suppliers, leads) and their contacts (Laravel CRM/Relation,
-- CRM/Contact). Addresses and phones stay JSON arrays, as in Laravel.
-- Search is a workspace-scoped scan (no trigram index: its OR over joined and correlated
-- arms can't use one); see docs/api/relations.md for the size it holds up to.

CREATE TABLE relations (
    id                            uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id                  uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    relation_type_id              uuid        NOT NULL,
    source_type_id                uuid,
    -- Display number = appendix prefix + zero-padded number + suffix. Never reused, so the
    -- unique index below also covers soft-deleted rows.
    relation_number               bigint,
    relation_appendix             jsonb       CHECK (relation_appendix IS NULL
                                                     OR jsonb_typeof(relation_appendix) = 'object'),
    type                          text        NOT NULL DEFAULT 'personal'
                                              CHECK (type IN ('business', 'personal')),
    allow_invoicing_on_account    boolean     NOT NULL DEFAULT false,
    disable_order_workflow_emails boolean     NOT NULL DEFAULT false,
    is_lead                       boolean     NOT NULL DEFAULT false,
    alias                         text,
    company_name                  text,
    email                         text,
    website                       text,
    phone_number                  text,
    coc_number                    text,
    vat_number                    text,
    import_source                 text,
    import_id                     text,
    default_language              text        CHECK (default_language IN ('en', 'nl')),
    addresses                     jsonb       NOT NULL DEFAULT '[]'
                                              CHECK (jsonb_typeof(addresses) = 'array'),
    financial_account             jsonb       CHECK (financial_account IS NULL
                                                     OR jsonb_typeof(financial_account) = 'object'),
    terms_and_periods             jsonb       CHECK (terms_and_periods IS NULL
                                                     OR jsonb_typeof(terms_and_periods) = 'object'),
    supplier_data                 jsonb       CHECK (supplier_data IS NULL
                                                     OR jsonb_typeof(supplier_data) = 'object'),
    -- Jortt/accounting provider ids; written by integrations, never by the API.
    provider_data                 jsonb       CHECK (provider_data IS NULL
                                                     OR jsonb_typeof(provider_data) = 'object'),
    meta                          jsonb       CHECK (meta IS NULL OR jsonb_typeof(meta) = 'object'),
    created_by                    uuid        REFERENCES users (id) ON DELETE SET NULL,
    owner_id                      uuid        REFERENCES users (id) ON DELETE SET NULL,
    created_at                    timestamptz NOT NULL DEFAULT now(),
    updated_at                    timestamptz NOT NULL DEFAULT now(),
    deleted_at                    timestamptz,
    CONSTRAINT relations_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT relations_relation_type_fk FOREIGN KEY (workspace_id, relation_type_id)
        REFERENCES custom_types (workspace_id, id),
    CONSTRAINT relations_source_type_fk FOREIGN KEY (workspace_id, source_type_id)
        REFERENCES custom_types (workspace_id, id)
);
CREATE UNIQUE INDEX relations_number_unique ON relations (workspace_id, relation_number)
    WHERE relation_number IS NOT NULL;
-- Default list order: newest first.
CREATE INDEX relations_live_idx ON relations (workspace_id, id DESC) WHERE deleted_at IS NULL;
CREATE INDEX relations_import_idx ON relations (workspace_id, import_source, import_id)
    WHERE import_id IS NOT NULL;
-- Referencing side of the custom type FKs (probe on type delete, "type in use" checks).
CREATE INDEX relations_relation_type_idx ON relations (workspace_id, relation_type_id);
CREATE INDEX relations_source_type_idx ON relations (workspace_id, source_type_id)
    WHERE source_type_id IS NOT NULL;

CREATE TABLE contacts (
    id              uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id    uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    relation_id     uuid        NOT NULL,
    first_name      text,
    middle_name     text,
    last_name       text,
    email           text,
    -- Legacy single number: accepted and stored, never returned (Laravel ContactResource).
    phone_number    text,
    phones          jsonb       NOT NULL DEFAULT '[]' CHECK (jsonb_typeof(phones) = 'array'),
    job_title       text,
    memo            text,
    initials        text,
    prefix          text,
    attn            text,
    salutation      text,
    default_contact boolean     NOT NULL DEFAULT false,
    default_for     text        CHECK (default_for IN ('default_for_estimate', 'default_for_invoice',
                                                       'default_for_order', 'default_for_purchase_order',
                                                       'default_for_work_order')),
    import_source   text,
    import_id       text,
    meta            jsonb       CHECK (meta IS NULL OR jsonb_typeof(meta) = 'object'),
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now(),
    deleted_at      timestamptz,
    CONSTRAINT contacts_workspace_id_id_key UNIQUE (workspace_id, id),
    -- Contact and relation share the tenant: the DB refuses a cross-workspace link.
    CONSTRAINT contacts_relation_fk FOREIGN KEY (workspace_id, relation_id)
        REFERENCES relations (workspace_id, id) ON DELETE CASCADE
);
-- Serves the composite FK (cascade probe) and the per-page contact load.
CREATE INDEX contacts_relation_idx ON contacts (workspace_id, relation_id);
-- Laravel let a relation end up with several defaults; one is enough.
CREATE UNIQUE INDEX contacts_one_default ON contacts (relation_id)
    WHERE default_contact AND deleted_at IS NULL;
