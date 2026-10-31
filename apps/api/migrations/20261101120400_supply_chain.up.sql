-- Phase 4b: packing (a child lot of jars made from a supply lot, with the exact kg it took)
-- and supply transfers (lot stock moved over a live supply link into the receiver's own lot).

ALTER TABLE lots
    ADD COLUMN parent_lot_id uuid,
    -- Kg of the parent and units made: kg per unit = pack_raw_kg / pack_units, kept as the two
    -- exact numbers (10 kg into 3 jars has no exact decimal).
    ADD COLUMN pack_raw_kg   numeric(12,2),
    ADD COLUMN pack_units    numeric(19,2),
    ADD CONSTRAINT lots_parent_fk FOREIGN KEY (workspace_id, parent_lot_id)
        REFERENCES lots (workspace_id, id),
    ADD CONSTRAINT lots_pack_check CHECK (
        (parent_lot_id IS NULL) = (pack_raw_kg IS NULL)
        AND (parent_lot_id IS NULL) = (pack_units IS NULL)
        AND coalesce(pack_raw_kg, 1) > 0 AND coalesce(pack_units, 1) > 0);
CREATE INDEX lots_parent_idx ON lots (workspace_id, parent_lot_id) WHERE parent_lot_id IS NOT NULL;

ALTER TABLE lot_events DROP CONSTRAINT lot_events_kind_check,
    ADD CONSTRAINT lot_events_kind_check CHECK (kind IN ('created', 'updated', 'status_changed',
        'passport_published', 'passport_unpublished', 'packed', 'dispatched'));

CREATE TABLE supply_transfers (
    id                uuid          PRIMARY KEY DEFAULT uuidv7(),
    link_id           uuid          NOT NULL REFERENCES supply_links (id) ON DELETE CASCADE,
    from_workspace_id uuid          NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    from_lot_id       uuid          NOT NULL,
    from_warehouse_id uuid          NOT NULL,
    quantity          numeric(19,2) NOT NULL CHECK (quantity > 0),
    dispatched_by     uuid          NOT NULL REFERENCES users (id),
    dispatched_at     timestamptz   NOT NULL DEFAULT now(),
    to_workspace_id   uuid          NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    to_lot_id         uuid,
    received_quantity numeric(19,2) CHECK (received_quantity > 0 AND received_quantity <= quantity),
    received_by       uuid          REFERENCES users (id),
    received_at       timestamptz,
    -- The link's allow-list when received: the received lot keeps showing exactly this, even
    -- after the link is revoked.
    fields            text[],
    CHECK (num_nonnulls(to_lot_id, received_quantity, received_by, received_at, fields) IN (0, 5)),
    CONSTRAINT supply_transfers_from_lot_fk FOREIGN KEY (from_workspace_id, from_lot_id)
        REFERENCES lots (workspace_id, id),
    CONSTRAINT supply_transfers_from_warehouse_fk FOREIGN KEY (from_workspace_id, from_warehouse_id)
        REFERENCES warehouses (workspace_id, id),
    CONSTRAINT supply_transfers_to_lot_fk FOREIGN KEY (to_workspace_id, to_lot_id)
        REFERENCES lots (workspace_id, id),
    -- A received lot comes from exactly one transfer (the lineage walks back through it).
    CONSTRAINT supply_transfers_to_lot_key UNIQUE (to_workspace_id, to_lot_id)
);
CREATE INDEX supply_transfers_from_idx ON supply_transfers (from_workspace_id, dispatched_at DESC);
CREATE INDEX supply_transfers_to_idx ON supply_transfers (to_workspace_id, dispatched_at DESC);
CREATE INDEX supply_transfers_link_idx ON supply_transfers (link_id);
CREATE INDEX supply_transfers_from_lot_idx ON supply_transfers (from_workspace_id, from_lot_id);
CREATE INDEX supply_transfers_from_warehouse_idx ON supply_transfers (from_workspace_id, from_warehouse_id);
CREATE INDEX supply_transfers_dispatched_by_idx ON supply_transfers (dispatched_by);
CREATE INDEX supply_transfers_received_by_idx ON supply_transfers (received_by) WHERE received_by IS NOT NULL;
