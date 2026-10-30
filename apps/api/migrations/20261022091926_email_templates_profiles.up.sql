-- Email templates and profiles (Laravel WorkspaceSetting/EmailTemplate, EmailProfile).
-- jsonb size CHECKs are 128 KiB backstops over the API's 16 KiB caps (see `files`).

-- A template's subject and body with `{variable}` placeholders, for one of Laravel's 84
-- domains. One default per domain (the partial unique index; writers also take an advisory
-- lock per workspace and domain).
CREATE TABLE email_templates (
    id               uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id     uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    template_name    text        NOT NULL CHECK (char_length(template_name) BETWEEN 1 AND 255),
    template_domain  text        NOT NULL CHECK (template_domain IN (
        'activity', 'approval', 'association', 'cash_register', 'cash_register_daily_log',
        'collective_invoice', 'collective_invoice_payment', 'contact', 'currency',
        'custom_field', 'custom_field_section', 'custom_field_value', 'custom_status',
        'custom_type', 'dashboard', 'data_synchronization', 'deal', 'deal_pipeline',
        'deal_pipeline_status', 'discount', 'discount_group', 'domain_log', 'email_profile',
        'email_template', 'employment_group', 'estimate', 'external_association',
        'f_t_p_server', 'file', 'ftp_server', 'hour_registration', 'hour_registration_type',
        'hourly_rate', 'incoming_invoice', 'integration_profile', 'inventory_reconciliation',
        'invite', 'invoice', 'invoice_payment', 'item', 'label_template', 'lead', 'lead_source',
        'leave_request', 'order', 'pdf_template', 'planned_work_order_date', 'pos_register',
        'pos_vault', 'price_correction', 'product', 'product_brand', 'product_category',
        'product_return_reason', 'product_variant', 'project', 'purchase_order',
        'purchase_order_item', 'recurring_invoice', 'relation', 'reminder', 'return_item',
        'sales_group', 'setting', 'shipment_method', 'special_offer', 'stock',
        'storage_location', 'tag', 'team', 'ticket', 'ticket_box', 'ticket_label', 'todo',
        'transfer_list', 'transfer_list_item', 'user', 'vat_group', 'warehouse',
        'warehouse_location', 'warehouse_section', 'work_order', 'workflow', 'workspace')),
    default_template boolean     NOT NULL DEFAULT false,
    subject          text        NOT NULL CHECK (char_length(subject) <= 255),
    body             text        NOT NULL CHECK (char_length(body) <= 25000),
    conditions       jsonb       CHECK (conditions IS NULL OR (jsonb_typeof(conditions) = 'object'
                                        AND octet_length(conditions::text) <= 131072)),
    meta             jsonb       CHECK (meta IS NULL OR (jsonb_typeof(meta) = 'object'
                                        AND octet_length(meta::text) <= 131072)),
    created_at       timestamptz NOT NULL DEFAULT now(),
    updated_at       timestamptz NOT NULL DEFAULT now(),
    deleted_at       timestamptz,
    CONSTRAINT email_templates_workspace_id_id_key UNIQUE (workspace_id, id)
);

CREATE INDEX email_templates_list_idx ON email_templates (workspace_id, created_at DESC, id DESC)
    WHERE deleted_at IS NULL;
CREATE UNIQUE INDEX email_templates_default_key ON email_templates (workspace_id, template_domain)
    WHERE default_template AND deleted_at IS NULL;

-- A sender: name, address, footer HTML and optionally its own SMTP server. The SMTP
-- password is never in `smtp`: it is sealed (XChaCha20-Poly1305, the credentials key, bound
-- to workspace and profile) in `smtp_password`, with the key's id.
CREATE TABLE email_profiles (
    id            uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id  uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    name          text        NOT NULL CHECK (char_length(name) BETWEEN 1 AND 80),
    sender_name   text        NOT NULL CHECK (char_length(sender_name) BETWEEN 1 AND 80),
    email         text        NOT NULL CHECK (char_length(email) BETWEEN 3 AND 255),
    html          text        CHECK (char_length(html) <= 25000),
    is_default    boolean     NOT NULL DEFAULT false,
    smtp          jsonb       CHECK (smtp IS NULL OR (jsonb_typeof(smtp) = 'object'
                                     AND NOT smtp ? 'password'
                                     AND octet_length(smtp::text) <= 131072)),
    smtp_password bytea       CHECK (octet_length(smtp_password) BETWEEN 41 AND 2048),
    smtp_key_id   text        CHECK (char_length(smtp_key_id) = 16),
    conditions    jsonb       CHECK (conditions IS NULL OR (jsonb_typeof(conditions) = 'object'
                                     AND octet_length(conditions::text) <= 131072)),
    created_by    uuid        REFERENCES users (id) ON DELETE SET NULL,
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now(),
    deleted_at    timestamptz,
    CONSTRAINT email_profiles_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT email_profiles_sealed_check CHECK ((smtp_password IS NULL) = (smtp_key_id IS NULL)),
    CONSTRAINT email_profiles_password_server_check CHECK (smtp_password IS NULL OR smtp ? 'host')
);

CREATE INDEX email_profiles_list_idx ON email_profiles (workspace_id, created_at, id)
    WHERE deleted_at IS NULL;
CREATE INDEX email_profiles_created_by_idx ON email_profiles (created_by)
    WHERE created_by IS NOT NULL;
