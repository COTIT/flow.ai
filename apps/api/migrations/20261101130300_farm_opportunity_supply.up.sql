-- Phase 4c step 3b: opportunity supply lines, the shared committed-kg figure and harvest
-- availability (RUAHA market.sql, mvp_security.sql, supply_attached_by.sql, views.sql).

-- THE traceability edge: which harvest is promised to which buyer, by whom. Lines are never
-- edited or detached; the guard that admits them lives in farm_market::service and follows the
-- lock order below.
CREATE TABLE farm_opportunity_supply (
    id                uuid          PRIMARY KEY DEFAULT uuidv7(),
    workspace_id      uuid          NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    village_id        uuid          NOT NULL,
    opportunity_id    uuid          NOT NULL,
    harvest_report_id uuid          NOT NULL,
    crop_cycle_id     uuid          NOT NULL,
    contributed_kg    numeric(12,2) NOT NULL CHECK (contributed_kg > 0),
    -- The attacher is the caller, never the client's claim.
    captured_by       uuid          NOT NULL REFERENCES users (id),
    captured_at       timestamptz   NOT NULL DEFAULT now(),
    created_at        timestamptz   NOT NULL DEFAULT now(),
    CONSTRAINT farm_opportunity_supply_line_key UNIQUE (opportunity_id, harvest_report_id),
    -- Opportunity, harvest and cycle all sit in the line's village.
    CONSTRAINT farm_opportunity_supply_opportunity_fk FOREIGN KEY (workspace_id, village_id, opportunity_id)
        REFERENCES farm_opportunities (workspace_id, village_id, id),
    CONSTRAINT farm_opportunity_supply_harvest_fk FOREIGN KEY (workspace_id, village_id, harvest_report_id)
        REFERENCES farm_harvest_reports (workspace_id, village_id, id),
    CONSTRAINT farm_opportunity_supply_cycle_fk FOREIGN KEY (workspace_id, village_id, crop_cycle_id)
        REFERENCES farm_crop_cycles (workspace_id, village_id, id)
);

CREATE INDEX farm_opportunity_supply_harvest_idx ON farm_opportunity_supply (harvest_report_id);
CREATE INDEX farm_opportunity_supply_opportunity_idx ON farm_opportunity_supply (opportunity_id);

CREATE TRIGGER farm_opportunity_supply_fixed BEFORE UPDATE OR DELETE ON farm_opportunity_supply
    FOR EACH ROW EXECUTE FUNCTION farm_supply_append_only();
CREATE TRIGGER farm_opportunity_supply_audit AFTER INSERT ON farm_opportunity_supply
    FOR EACH ROW EXECUTE FUNCTION farm_write_audit();

-- Kg already committed from the harvest a report measures: every report of the same cycle and
-- kind counts, so a newer figure never frees kg promised on an older one. Supply-batch kg count
-- against the actual series and live-opportunity kg against the expected series; the two do
-- not block each other. Callers lock the report rows FOR UPDATE first (id order), then call
-- this in a separate statement so it sees what earlier lock holders committed.
CREATE OR REPLACE FUNCTION farm_harvest_committed_kg(report uuid) RETURNS numeric LANGUAGE sql STABLE AS $$
    SELECT (
        coalesce((SELECT sum(s.kg)
                  FROM farm_harvest_reports r
                  JOIN farm_harvest_reports series ON series.crop_cycle_id = r.crop_cycle_id
                       AND series.kind = r.kind
                  JOIN farm_supply_shares s ON s.harvest_report_id = series.id
                  WHERE r.id = report), 0)
      + coalesce((SELECT sum(os.contributed_kg)
                  FROM farm_harvest_reports r
                  JOIN farm_harvest_reports series ON series.crop_cycle_id = r.crop_cycle_id
                       AND series.kind = r.kind
                  JOIN farm_opportunity_supply os ON os.harvest_report_id = series.id
                  JOIN farm_opportunities o ON o.id = os.opportunity_id
                  WHERE r.id = report AND o.deleted_at IS NULL
                    AND o.status IN ('proposed', 'shared', 'accepted')), 0)
    )::numeric(12,2)
$$;

-- RUAHA v_harvest_available: current expected figures with what is already promised. Readers
-- filter workspace and scope.
CREATE VIEW farm_harvest_available AS
SELECT h.id AS harvest_report_id, h.workspace_id, h.crop_cycle_id, h.village_id, c.crop_id,
       c.plot_id, c.harvest_start, c.harvest_end, h.quantity_kg, h.confidence, h.verification,
       k.committed_kg, h.quantity_kg - k.committed_kg AS available_kg
FROM farm_harvest_reports h
JOIN farm_crop_cycles c ON c.id = h.crop_cycle_id AND c.deleted_at IS NULL
CROSS JOIN LATERAL (SELECT farm_harvest_committed_kg(h.id) AS committed_kg) k
WHERE h.deleted_at IS NULL AND h.is_current AND h.kind = 'expected';

-- Opportunities carrying supply from a person's farms (RUAHA app_supplied_opportunities): a
-- farmer sees an opportunity only when their own harvest is in it.
CREATE FUNCTION farm_person_opportunities(p uuid) RETURNS SETOF uuid LANGUAGE sql STABLE AS $$
    SELECT DISTINCT s.opportunity_id
    FROM farm_opportunity_supply s
    JOIN farm_crop_cycles c ON c.id = s.crop_cycle_id
    JOIN farm_plots pl ON pl.id = c.plot_id
    WHERE pl.farm_id IN (SELECT farm_person_farms(p))
$$;
