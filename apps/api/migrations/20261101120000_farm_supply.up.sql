-- Phase 4b: supply batches. Verified actual harvests bought at an agreed price per kg become a
-- raw lot; each line is one farmer's kg share of it. Both tables are append-only: a batch is
-- frozen once received.

CREATE TABLE farm_supply_batches (
    id           uuid          PRIMARY KEY DEFAULT uuidv7(),
    workspace_id uuid          NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    village_id   uuid          NOT NULL,
    crop_id      uuid          NOT NULL,
    lot_id       uuid          NOT NULL,
    -- Minor units of `currency_id` per kg (TZS 8,000/kg at 2 decimals = 800000).
    price_per_kg bigint        NOT NULL CHECK (price_per_kg > 0),
    currency_id  uuid          NOT NULL,
    kg_total     numeric(12,2) NOT NULL CHECK (kg_total > 0),
    -- Retry key of the client; the hash tells a replay from another body under the same key.
    client_ref   uuid          NOT NULL,
    request_hash bytea         NOT NULL CHECK (octet_length(request_hash) = 32),
    created_by   uuid          NOT NULL REFERENCES users (id),
    created_at   timestamptz   NOT NULL DEFAULT now(),
    CONSTRAINT farm_supply_batches_client_ref_key UNIQUE (workspace_id, client_ref),
    CONSTRAINT farm_supply_batches_scope_key UNIQUE (workspace_id, village_id, id),
    CONSTRAINT farm_supply_batches_lot_key UNIQUE (workspace_id, lot_id),
    CONSTRAINT farm_supply_batches_village_fk FOREIGN KEY (workspace_id, village_id)
        REFERENCES farm_villages (workspace_id, id),
    CONSTRAINT farm_supply_batches_crop_fk FOREIGN KEY (workspace_id, crop_id)
        REFERENCES farm_crops (workspace_id, id),
    CONSTRAINT farm_supply_batches_lot_fk FOREIGN KEY (workspace_id, lot_id)
        REFERENCES lots (workspace_id, id),
    CONSTRAINT farm_supply_batches_currency_fk FOREIGN KEY (workspace_id, currency_id)
        REFERENCES currencies (workspace_id, id)
);
CREATE INDEX farm_supply_batches_list_idx
    ON farm_supply_batches (workspace_id, created_at DESC, id DESC);
CREATE INDEX farm_supply_batches_village_idx ON farm_supply_batches (workspace_id, village_id);
CREATE INDEX farm_supply_batches_crop_idx ON farm_supply_batches (workspace_id, crop_id);
CREATE INDEX farm_supply_batches_currency_idx ON farm_supply_batches (workspace_id, currency_id);
CREATE INDEX farm_supply_batches_created_by_idx ON farm_supply_batches (created_by);

CREATE TABLE farm_supply_shares (
    id                uuid          PRIMARY KEY DEFAULT uuidv7(),
    workspace_id      uuid          NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    village_id        uuid          NOT NULL,
    batch_id          uuid          NOT NULL,
    -- The farm's primary manager when the batch was made: who is credited.
    person_id         uuid          NOT NULL,
    harvest_report_id uuid          NOT NULL,
    kg                numeric(12,2) NOT NULL CHECK (kg > 0),
    created_at        timestamptz   NOT NULL DEFAULT now(),
    CONSTRAINT farm_supply_shares_line_key UNIQUE (batch_id, harvest_report_id),
    CONSTRAINT farm_supply_shares_batch_fk FOREIGN KEY (workspace_id, village_id, batch_id)
        REFERENCES farm_supply_batches (workspace_id, village_id, id),
    CONSTRAINT farm_supply_shares_person_fk FOREIGN KEY (workspace_id, village_id, person_id)
        REFERENCES farm_persons (workspace_id, village_id, id),
    CONSTRAINT farm_supply_shares_harvest_fk FOREIGN KEY (workspace_id, village_id, harvest_report_id)
        REFERENCES farm_harvest_reports (workspace_id, village_id, id)
);
CREATE INDEX farm_supply_shares_harvest_idx ON farm_supply_shares (harvest_report_id);
CREATE INDEX farm_supply_shares_person_idx ON farm_supply_shares (person_id);
-- Every figure of a cycle and kind, for the committed-kg sum.
CREATE INDEX farm_harvest_reports_series_idx ON farm_harvest_reports (crop_cycle_id, kind);

CREATE FUNCTION farm_supply_append_only() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION '% rows cannot be changed', TG_TABLE_NAME USING ERRCODE = 'restrict_violation';
END
$$;
CREATE TRIGGER farm_supply_batches_fixed BEFORE UPDATE OR DELETE ON farm_supply_batches
    FOR EACH ROW EXECUTE FUNCTION farm_supply_append_only();
CREATE TRIGGER farm_supply_shares_fixed BEFORE UPDATE OR DELETE ON farm_supply_shares
    FOR EACH ROW EXECUTE FUNCTION farm_supply_append_only();
CREATE TRIGGER farm_supply_batches_audit AFTER INSERT ON farm_supply_batches
    FOR EACH ROW EXECUTE FUNCTION farm_write_audit();

-- Kg already committed from the harvest a report measures: every report of the same cycle
-- and kind counts, so a newer figure never frees kg bought on an older one. Shared with the
-- market module (opportunity supply): whichever lands second adds its table here. Callers
-- lock the report row first, then call this in a separate statement.
CREATE FUNCTION farm_harvest_committed_kg(report uuid) RETURNS numeric LANGUAGE sql STABLE AS $$
    SELECT coalesce(sum(s.kg), 0)::numeric(12,2)
    FROM farm_harvest_reports r
    JOIN farm_harvest_reports series ON series.crop_cycle_id = r.crop_cycle_id
         AND series.kind = r.kind
    JOIN farm_supply_shares s ON s.harvest_report_id = series.id
    WHERE r.id = report
$$;
