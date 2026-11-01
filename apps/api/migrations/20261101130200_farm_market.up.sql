-- Phase 4c step 3a: the buyer order book and village opportunities (RUAHA market.sql,
-- mvp_security.sql). An opportunity records that a village CAN supply a demand: never a sale,
-- delivery, payment or contract. Who may write and how status moves live in farm_market::rules.

-- 'afm' is a channel LABEL only: no API, no partnership assumed.
CREATE TABLE farm_buyers (
    id           uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    name         text        NOT NULL CHECK (char_length(name) BETWEEN 1 AND 120),
    channel      text        NOT NULL DEFAULT 'direct' CHECK (channel IN ('direct', 'afm', 'other')),
    contact_note text        CHECK (char_length(contact_note) <= 500),
    is_active    boolean     NOT NULL DEFAULT true,
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT farm_buyers_name_key UNIQUE (workspace_id, name),
    CONSTRAINT farm_buyers_workspace_id_id_key UNIQUE (workspace_id, id)
);

-- NO `source` column, on purpose (RUAHA S13): the five source categories classify how we
-- learned a fact about the productive economy. A buyer's stated requirement is a counterparty
-- input recorded by ops, so it carries captured_by / verification instead.
CREATE TABLE farm_buyer_demands (
    id                            uuid          PRIMARY KEY DEFAULT uuidv7(),
    workspace_id                  uuid          NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    buyer_id                      uuid          NOT NULL,
    crop_id                       uuid          NOT NULL,
    quantity_kg                   numeric(12,2) NOT NULL CHECK (quantity_kg > 0),
    quality_note                  text          CHECK (char_length(quality_note) <= 500),
    window_start                  date          NOT NULL,
    window_end                    date          NOT NULL,
    delivery_point                text          CHECK (char_length(delivery_point) <= 200),
    -- INDICATIVE, minor units of `currency_id` per kg.
    indicative_price_per_kg_minor bigint        CHECK (indicative_price_per_kg_minor >= 0),
    currency_id                   uuid,
    status                        text          NOT NULL DEFAULT 'open'
                                      CHECK (status IN ('open', 'matched', 'closed', 'cancelled')),
    captured_at                   timestamptz   NOT NULL DEFAULT now(),
    captured_by                   uuid          REFERENCES users (id),
    verification                  text          NOT NULL DEFAULT 'unverified'
                                      CHECK (verification IN ('unverified', 'pending', 'verified', 'disputed')),
    confidence                    text          CHECK (confidence IN ('low', 'medium', 'high')),
    created_at                    timestamptz   NOT NULL DEFAULT now(),
    updated_at                    timestamptz   NOT NULL DEFAULT now(),
    deleted_at                    timestamptz,
    CONSTRAINT farm_buyer_demands_window_sane CHECK (window_end >= window_start),
    CONSTRAINT farm_buyer_demands_priced CHECK (indicative_price_per_kg_minor IS NULL OR currency_id IS NOT NULL),
    CONSTRAINT farm_buyer_demands_buyer_fk FOREIGN KEY (workspace_id, buyer_id)
        REFERENCES farm_buyers (workspace_id, id),
    CONSTRAINT farm_buyer_demands_crop_fk FOREIGN KEY (workspace_id, crop_id)
        REFERENCES farm_crops (workspace_id, id),
    CONSTRAINT farm_buyer_demands_currency_fk FOREIGN KEY (workspace_id, currency_id)
        REFERENCES currencies (workspace_id, id),
    -- Target of the opportunity FK: an opportunity carries its demand's crop.
    CONSTRAINT farm_buyer_demands_crop_key UNIQUE (workspace_id, id, crop_id)
);

CREATE INDEX farm_buyer_demands_status_idx ON farm_buyer_demands (workspace_id, status, window_start)
    WHERE deleted_at IS NULL;
CREATE INDEX farm_buyer_demands_crop_idx ON farm_buyer_demands (workspace_id, crop_id, window_start)
    WHERE deleted_at IS NULL;
CREATE INDEX farm_buyer_demands_buyer_idx ON farm_buyer_demands (workspace_id, buyer_id);

-- 'accepted' means both sides agreed to talk further: nothing has moved.
CREATE TABLE farm_opportunities (
    id                  uuid          PRIMARY KEY DEFAULT uuidv7(),
    workspace_id        uuid          NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    buyer_demand_id     uuid          NOT NULL,
    village_id          uuid          NOT NULL,
    crop_id             uuid          NOT NULL,
    -- The sum of its supply lines; only the server writes it.
    offered_quantity_kg numeric(12,2) NOT NULL DEFAULT 0 CHECK (offered_quantity_kg >= 0),
    status              text          NOT NULL DEFAULT 'proposed'
                            CHECK (status IN ('proposed', 'shared', 'accepted', 'declined', 'lapsed')),
    note                text          CHECK (char_length(note) <= 1000),
    captured_at         timestamptz   NOT NULL DEFAULT now(),
    captured_by         uuid          REFERENCES users (id),
    created_at          timestamptz   NOT NULL DEFAULT now(),
    updated_at          timestamptz   NOT NULL DEFAULT now(),
    deleted_at          timestamptz,
    CONSTRAINT farm_opportunities_village_key UNIQUE (buyer_demand_id, village_id),
    -- Same crop and workspace as the demand, by construction.
    CONSTRAINT farm_opportunities_demand_fk FOREIGN KEY (workspace_id, buyer_demand_id, crop_id)
        REFERENCES farm_buyer_demands (workspace_id, id, crop_id),
    CONSTRAINT farm_opportunities_village_fk FOREIGN KEY (workspace_id, village_id)
        REFERENCES farm_villages (workspace_id, id),
    CONSTRAINT farm_opportunities_scope_key UNIQUE (workspace_id, village_id, id)
);

CREATE INDEX farm_opportunities_village_idx ON farm_opportunities (workspace_id, village_id, status)
    WHERE deleted_at IS NULL;

CREATE TRIGGER farm_buyers_audit AFTER INSERT OR UPDATE OR DELETE ON farm_buyers
    FOR EACH ROW EXECUTE FUNCTION farm_write_audit();
CREATE TRIGGER farm_buyer_demands_audit AFTER INSERT OR UPDATE OR DELETE ON farm_buyer_demands
    FOR EACH ROW EXECUTE FUNCTION farm_write_audit();
CREATE TRIGGER farm_opportunities_audit AFTER INSERT OR UPDATE OR DELETE ON farm_opportunities
    FOR EACH ROW EXECUTE FUNCTION farm_write_audit();
