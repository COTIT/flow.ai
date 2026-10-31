-- Phase 4b: payout desk (G13) and the officers' cash ledger. A payout pays a farmer cash from
-- their passbook balance; a void and every cash movement are new rows. Nothing here is ever
-- changed or removed.

CREATE TABLE farm_payouts (
    id           uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    village_id   uuid        NOT NULL,
    person_id    uuid        NOT NULL,
    currency_id  uuid        NOT NULL,
    -- Minor units handed over.
    amount       bigint      NOT NULL CHECK (amount > 0),
    -- The ID document the payer checked.
    id_type      text        NOT NULL CHECK (id_type IN ('nida', 'voter', 'driving_licence',
                                 'village_letter')),
    -- Retry key of the (offline) client; the hash tells a replay from another body.
    client_ref   uuid        NOT NULL,
    request_hash bytea       NOT NULL CHECK (octet_length(request_hash) = 32),
    paid_by      uuid        NOT NULL REFERENCES users (id),
    paid_at      timestamptz NOT NULL DEFAULT clock_timestamp(),
    CONSTRAINT farm_payouts_client_ref_key UNIQUE (workspace_id, client_ref),
    CONSTRAINT farm_payouts_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT farm_payouts_person_fk FOREIGN KEY (workspace_id, village_id, person_id)
        REFERENCES farm_persons (workspace_id, village_id, id),
    CONSTRAINT farm_payouts_currency_fk FOREIGN KEY (workspace_id, currency_id)
        REFERENCES currencies (workspace_id, id)
);
CREATE INDEX farm_payouts_person_idx ON farm_payouts (workspace_id, person_id, paid_at DESC);
CREATE INDEX farm_payouts_currency_idx ON farm_payouts (workspace_id, currency_id);
CREATE INDEX farm_payouts_paid_by_idx ON farm_payouts (paid_by);

-- One void per payout (the primary key is the guard).
CREATE TABLE farm_payout_voids (
    payout_id    uuid        PRIMARY KEY,
    workspace_id uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    reason       text        NOT NULL CHECK (char_length(reason) BETWEEN 1 AND 500),
    voided_by    uuid        NOT NULL REFERENCES users (id),
    voided_at    timestamptz NOT NULL DEFAULT clock_timestamp(),
    CONSTRAINT farm_payout_voids_payout_fk FOREIGN KEY (workspace_id, payout_id)
        REFERENCES farm_payouts (workspace_id, id)
);
CREATE INDEX farm_payout_voids_voided_by_idx ON farm_payout_voids (voided_by);

-- Cash an officer holds, as signed movements: float handed out (+), payouts (−), voided
-- payouts back (+), float handed back (−). `count` rows record what the officer counted
-- (the amount itself, not a movement). `day` is the Tanzania (UTC+3) date.
CREATE TABLE farm_cash_ledger (
    id            uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id  uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    village_id    uuid        NOT NULL,
    staff_user_id uuid        NOT NULL REFERENCES users (id),
    currency_id   uuid        NOT NULL,
    day           date        NOT NULL,
    kind          text        NOT NULL CHECK (kind IN ('float_out', 'payout', 'payout_void',
                                  'float_back', 'count')),
    amount        bigint      NOT NULL,
    payout_id     uuid,
    created_by    uuid        NOT NULL REFERENCES users (id),
    created_at    timestamptz NOT NULL DEFAULT clock_timestamp(),
    CHECK (CASE kind WHEN 'float_out' THEN amount > 0 WHEN 'payout_void' THEN amount > 0
                     WHEN 'count' THEN amount >= 0 ELSE amount < 0 END),
    CHECK ((kind IN ('payout', 'payout_void')) = (payout_id IS NOT NULL)),
    CONSTRAINT farm_cash_ledger_village_fk FOREIGN KEY (workspace_id, village_id)
        REFERENCES farm_villages (workspace_id, id),
    CONSTRAINT farm_cash_ledger_currency_fk FOREIGN KEY (workspace_id, currency_id)
        REFERENCES currencies (workspace_id, id),
    CONSTRAINT farm_cash_ledger_payout_fk FOREIGN KEY (workspace_id, payout_id)
        REFERENCES farm_payouts (workspace_id, id)
);
CREATE INDEX farm_cash_ledger_day_idx
    ON farm_cash_ledger (workspace_id, staff_user_id, currency_id, day);
CREATE INDEX farm_cash_ledger_village_idx ON farm_cash_ledger (workspace_id, village_id, day);
CREATE INDEX farm_cash_ledger_currency_idx ON farm_cash_ledger (workspace_id, currency_id);
CREATE INDEX farm_cash_ledger_payout_idx ON farm_cash_ledger (workspace_id, payout_id)
    WHERE payout_id IS NOT NULL;
CREATE INDEX farm_cash_ledger_staff_idx ON farm_cash_ledger (staff_user_id);
CREATE INDEX farm_cash_ledger_created_by_idx ON farm_cash_ledger (created_by);

CREATE FUNCTION farm_cash_append_only() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION '% rows cannot be changed: add a reversing row', TG_TABLE_NAME
        USING ERRCODE = 'restrict_violation';
END
$$;
CREATE TRIGGER farm_payouts_fixed BEFORE UPDATE OR DELETE ON farm_payouts
    FOR EACH ROW EXECUTE FUNCTION farm_cash_append_only();
CREATE TRIGGER farm_payout_voids_fixed BEFORE UPDATE OR DELETE ON farm_payout_voids
    FOR EACH ROW EXECUTE FUNCTION farm_cash_append_only();
CREATE TRIGGER farm_cash_ledger_fixed BEFORE UPDATE OR DELETE ON farm_cash_ledger
    FOR EACH ROW EXECUTE FUNCTION farm_cash_append_only();
CREATE TRIGGER farm_payouts_no_truncate BEFORE TRUNCATE ON farm_payouts
    FOR EACH STATEMENT EXECUTE FUNCTION farm_cash_append_only();
CREATE TRIGGER farm_payout_voids_no_truncate BEFORE TRUNCATE ON farm_payout_voids
    FOR EACH STATEMENT EXECUTE FUNCTION farm_cash_append_only();
CREATE TRIGGER farm_cash_ledger_no_truncate BEFORE TRUNCATE ON farm_cash_ledger
    FOR EACH STATEMENT EXECUTE FUNCTION farm_cash_append_only();
