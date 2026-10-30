-- Shipment methods (Laravel WorkspaceSetting/ShipmentMethod). Money and weights are
-- numeric(24,8); price impacts are percentages on the shipping price (-100..100).
CREATE TABLE shipment_methods (
    id                    uuid          PRIMARY KEY DEFAULT uuidv7(),
    workspace_id          uuid          NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    name                  text          NOT NULL CHECK (char_length(name) <= 255),
    carrier               text          NOT NULL CHECK (char_length(carrier) <= 255),
    shipping_costs        numeric(24,8) NOT NULL DEFAULT 0 CHECK (shipping_costs >= 0),
    min_weight            numeric(24,8) NOT NULL DEFAULT 0 CHECK (min_weight >= 0),
    max_weight            numeric(24,8) NOT NULL DEFAULT 0,
    price_impact_business numeric(24,8) NOT NULL DEFAULT 0
                          CHECK (price_impact_business BETWEEN -100 AND 100),
    price_impact_consumer numeric(24,8) NOT NULL DEFAULT 0
                          CHECK (price_impact_consumer BETWEEN -100 AND 100),
    -- [{name, country (ISO-2), price, lead_time_hours}], as the client sent it.
    countries             jsonb         CHECK (countries IS NULL OR jsonb_typeof(countries) = 'array'),
    -- Set by fulfilment integrations (Sendcloud), never by the settings form.
    provider              text,
    provider_id           text,
    can_be_selected       boolean       NOT NULL DEFAULT false,
    meta                  jsonb         CHECK (meta IS NULL OR jsonb_typeof(meta) = 'object'),
    created_at            timestamptz   NOT NULL DEFAULT now(),
    updated_at            timestamptz   NOT NULL DEFAULT now(),
    deleted_at            timestamptz,
    CONSTRAINT shipment_methods_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT shipment_methods_weight_range CHECK (max_weight > min_weight)
);
