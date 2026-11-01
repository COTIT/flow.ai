DROP FUNCTION farm_person_opportunities(uuid);
DROP VIEW farm_harvest_available;

-- Back to 4b's supply-batch-only figure.
CREATE OR REPLACE FUNCTION farm_harvest_committed_kg(report uuid) RETURNS numeric LANGUAGE sql STABLE AS $$
    SELECT coalesce(sum(s.kg), 0)::numeric(12,2)
    FROM farm_harvest_reports r
    JOIN farm_harvest_reports series ON series.crop_cycle_id = r.crop_cycle_id
         AND series.kind = r.kind
    JOIN farm_supply_shares s ON s.harvest_report_id = series.id
    WHERE r.id = report
$$;

DROP TABLE farm_opportunity_supply;
