DROP TABLE supply_transfers;
ALTER TABLE lot_events DROP CONSTRAINT lot_events_kind_check,
    ADD CONSTRAINT lot_events_kind_check CHECK (kind IN ('created', 'updated', 'status_changed',
        'passport_published', 'passport_unpublished'));
ALTER TABLE lots DROP CONSTRAINT lots_pack_check, DROP CONSTRAINT lots_parent_fk,
    DROP COLUMN pack_units, DROP COLUMN pack_raw_kg, DROP COLUMN parent_lot_id;
