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

-- Passbook lines are append-only: drop the trigger to remove correction lines, then restore it.
ALTER TABLE farm_passbook_entries DISABLE TRIGGER farm_passbook_entries_fixed;
DELETE FROM farm_passbook_entries WHERE kind = 'supply_correction';
ALTER TABLE farm_passbook_entries ENABLE TRIGGER farm_passbook_entries_fixed;
ALTER TABLE farm_passbook_entries
    DROP CONSTRAINT farm_passbook_entries_correction_kind,
    DROP COLUMN correction_id,
    DROP CONSTRAINT farm_passbook_entries_kind_check,
    ADD CONSTRAINT farm_passbook_entries_kind_check CHECK (kind IN ('sale_credit', 'sale_reversal',
        'payout', 'payout_void'));

DROP VIEW farm_supply_share_kg;
DROP TABLE farm_supply_correction_splits;
DROP TABLE farm_supply_correction_decisions;
DROP TABLE farm_supply_correction_lines;
DROP TABLE farm_supply_corrections;
