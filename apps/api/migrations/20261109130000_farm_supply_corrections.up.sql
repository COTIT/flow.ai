-- Correcting a bought farm delivery (docs/farm-supply-corrections.md, D2-a..f). Shares stay
-- frozen: a correction is a request (header + kg lines), a decision by a second ops/admin, and on
-- approval the re-split of past credits per attribution plus one passbook line per farmer.
-- Every table is append-only.

CREATE TABLE farm_supply_corrections (
    id           uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    village_id   uuid        NOT NULL,
    batch_id     uuid        NOT NULL,
    reason       text        NOT NULL CHECK (char_length(reason) BETWEEN 1 AND 500),
    -- Retry key of the client; the hash tells a replay from another body under the same key.
    client_ref   uuid        NOT NULL,
    request_hash bytea       NOT NULL CHECK (octet_length(request_hash) = 32),
    requested_by uuid        NOT NULL REFERENCES users (id),
    requested_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    CONSTRAINT farm_supply_corrections_client_ref_key UNIQUE (workspace_id, client_ref),
    -- Target of the decision's four-eyes FK.
    CONSTRAINT farm_supply_corrections_requester_key UNIQUE (id, requested_by),
    CONSTRAINT farm_supply_corrections_scope_key UNIQUE (workspace_id, village_id, id),
    CONSTRAINT farm_supply_corrections_batch_fk FOREIGN KEY (workspace_id, village_id, batch_id)
        REFERENCES farm_supply_batches (workspace_id, village_id, id)
);
CREATE INDEX farm_supply_corrections_list_idx
    ON farm_supply_corrections (workspace_id, requested_at DESC, id DESC);
CREATE INDEX farm_supply_corrections_batch_idx ON farm_supply_corrections (workspace_id, batch_id);
CREATE INDEX farm_supply_corrections_requested_by_idx ON farm_supply_corrections (requested_by);

-- One kg change: `person_id`'s kg on share `share_id`. A wrong farmer is two lines on one
-- share: −kg on the share's farmer, +kg on the right one.
CREATE TABLE farm_supply_correction_lines (
    id            uuid          PRIMARY KEY DEFAULT uuidv7(),
    workspace_id  uuid          NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    village_id    uuid          NOT NULL,
    correction_id uuid          NOT NULL,
    share_id      uuid          NOT NULL REFERENCES farm_supply_shares (id),
    person_id     uuid          NOT NULL,
    kg_delta      numeric(12,2) NOT NULL CHECK (kg_delta <> 0),
    CONSTRAINT farm_supply_correction_lines_key UNIQUE (correction_id, share_id, person_id),
    CONSTRAINT farm_supply_correction_lines_correction_fk FOREIGN KEY (workspace_id, village_id, correction_id)
        REFERENCES farm_supply_corrections (workspace_id, village_id, id),
    CONSTRAINT farm_supply_correction_lines_person_fk FOREIGN KEY (workspace_id, village_id, person_id)
        REFERENCES farm_persons (workspace_id, village_id, id)
);
CREATE INDEX farm_supply_correction_lines_share_idx ON farm_supply_correction_lines (share_id);
CREATE INDEX farm_supply_correction_lines_person_idx ON farm_supply_correction_lines (person_id);

-- Approve or reject, once, by someone other than the requester (four eyes in the schema).
CREATE TABLE farm_supply_correction_decisions (
    id            uuid        PRIMARY KEY DEFAULT uuidv7(),
    correction_id uuid        NOT NULL UNIQUE,
    workspace_id  uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    decision      text        NOT NULL CHECK (decision IN ('approved', 'rejected')),
    requested_by  uuid        NOT NULL,
    decided_by    uuid        NOT NULL REFERENCES users (id),
    decided_at    timestamptz NOT NULL DEFAULT clock_timestamp(),
    CHECK (decided_by <> requested_by),
    CONSTRAINT farm_supply_correction_decisions_request_fk FOREIGN KEY (correction_id, requested_by)
        REFERENCES farm_supply_corrections (id, requested_by)
);
CREATE INDEX farm_supply_correction_decisions_decided_by_idx
    ON farm_supply_correction_decisions (decided_by);
CREATE INDEX farm_supply_correction_decisions_workspace_idx
    ON farm_supply_correction_decisions (workspace_id);

-- What an approval moved per sale/return attribution and farmer (minor units, signed). The
-- passbook carries one line per farmer (their sum); returns read these per attribution.
CREATE TABLE farm_supply_correction_splits (
    id             uuid   PRIMARY KEY DEFAULT uuidv7(),
    workspace_id   uuid   NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    correction_id  uuid   NOT NULL REFERENCES farm_supply_correction_decisions (correction_id),
    attribution_id uuid   NOT NULL REFERENCES farm_sale_attributions (id),
    person_id      uuid   NOT NULL,
    amount         bigint NOT NULL CHECK (amount <> 0),
    CONSTRAINT farm_supply_correction_splits_key UNIQUE (correction_id, attribution_id, person_id)
);
CREATE INDEX farm_supply_correction_splits_attribution_idx
    ON farm_supply_correction_splits (attribution_id, person_id);
CREATE INDEX farm_supply_correction_splits_workspace_idx ON farm_supply_correction_splits (workspace_id);

CREATE TRIGGER farm_supply_corrections_fixed BEFORE UPDATE OR DELETE ON farm_supply_corrections
    FOR EACH ROW EXECUTE FUNCTION farm_supply_append_only();
CREATE TRIGGER farm_supply_correction_lines_fixed BEFORE UPDATE OR DELETE ON farm_supply_correction_lines
    FOR EACH ROW EXECUTE FUNCTION farm_supply_append_only();
CREATE TRIGGER farm_supply_correction_decisions_fixed BEFORE UPDATE OR DELETE ON farm_supply_correction_decisions
    FOR EACH ROW EXECUTE FUNCTION farm_supply_append_only();
CREATE TRIGGER farm_supply_correction_splits_fixed BEFORE UPDATE OR DELETE ON farm_supply_correction_splits
    FOR EACH ROW EXECUTE FUNCTION farm_supply_append_only();
CREATE TRIGGER farm_supply_correction_splits_no_truncate BEFORE TRUNCATE ON farm_supply_correction_splits
    FOR EACH STATEMENT EXECUTE FUNCTION farm_supply_append_only();
CREATE TRIGGER farm_supply_corrections_audit AFTER INSERT ON farm_supply_corrections
    FOR EACH ROW EXECUTE FUNCTION farm_write_audit();
CREATE TRIGGER farm_supply_correction_decisions_audit AFTER INSERT ON farm_supply_correction_decisions
    FOR EACH ROW EXECUTE FUNCTION farm_write_audit();

-- Effective kg per share and farmer: the frozen share plus every approved line.
CREATE VIEW farm_supply_share_kg AS
    SELECT s.workspace_id, s.village_id, s.batch_id, s.id AS share_id, s.harvest_report_id,
           s.person_id, s.kg
    FROM farm_supply_shares s
    UNION ALL
    SELECT s.workspace_id, s.village_id, s.batch_id, s.id, s.harvest_report_id, l.person_id,
           l.kg_delta
    FROM farm_supply_correction_lines l
    JOIN farm_supply_correction_decisions d
         ON d.correction_id = l.correction_id AND d.decision = 'approved'
    JOIN farm_supply_shares s ON s.id = l.share_id;

ALTER TABLE farm_passbook_entries DROP CONSTRAINT farm_passbook_entries_kind_check;
ALTER TABLE farm_passbook_entries
    ADD CONSTRAINT farm_passbook_entries_kind_check CHECK (kind IN ('sale_credit', 'sale_reversal',
        'payout', 'payout_void', 'supply_correction')),
    ADD COLUMN correction_id uuid REFERENCES farm_supply_correction_decisions (correction_id),
    ADD CONSTRAINT farm_passbook_entries_correction_kind
        CHECK ((kind = 'supply_correction') = (correction_id IS NOT NULL));
CREATE INDEX farm_passbook_entries_correction_idx ON farm_passbook_entries (correction_id)
    WHERE correction_id IS NOT NULL;

-- Committed kg read effective share kg: a cut frees kg, an increase takes more.
CREATE OR REPLACE FUNCTION farm_harvest_committed_kg(report uuid) RETURNS numeric LANGUAGE sql STABLE AS $$
    SELECT (
        coalesce((SELECT sum(s.kg)
                  FROM farm_harvest_reports r
                  JOIN farm_harvest_reports series ON series.crop_cycle_id = r.crop_cycle_id
                       AND series.kind = r.kind
                  JOIN farm_supply_share_kg s ON s.harvest_report_id = series.id
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
