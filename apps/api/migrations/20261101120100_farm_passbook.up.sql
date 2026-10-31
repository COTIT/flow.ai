-- Phase 4b: the farmer passbook and the passport consent flag. A passbook line is a signed
-- amount in minor units of one currency: credits from sales, debits from payouts and
-- reversals. Lines are never changed or removed; a mistake gets a reversing line.

CREATE TABLE farm_passbook_entries (
    id             uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id   uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    village_id     uuid        NOT NULL,
    person_id      uuid        NOT NULL,
    currency_id    uuid        NOT NULL,
    kind           text        NOT NULL CHECK (kind IN ('sale_credit', 'sale_reversal', 'payout',
                                   'payout_void')),
    amount         bigint      NOT NULL CHECK (amount <> 0),
    -- The sale attribution or payout behind the line (tables of later steps).
    attribution_id uuid,
    payout_id      uuid,
    created_by     uuid        REFERENCES users (id),
    created_at     timestamptz NOT NULL DEFAULT clock_timestamp(),
    CONSTRAINT farm_passbook_entries_person_fk FOREIGN KEY (workspace_id, village_id, person_id)
        REFERENCES farm_persons (workspace_id, village_id, id),
    CONSTRAINT farm_passbook_entries_currency_fk FOREIGN KEY (workspace_id, currency_id)
        REFERENCES currencies (workspace_id, id)
);
-- A person's passbook, newest first, and the running balance per currency.
CREATE INDEX farm_passbook_entries_person_idx
    ON farm_passbook_entries (workspace_id, person_id, currency_id, created_at, id);
CREATE INDEX farm_passbook_entries_currency_idx ON farm_passbook_entries (workspace_id, currency_id);
CREATE INDEX farm_passbook_entries_created_by_idx ON farm_passbook_entries (created_by)
    WHERE created_by IS NOT NULL;

CREATE FUNCTION farm_passbook_append_only() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'passbook lines cannot be changed: add a reversing line'
        USING ERRCODE = 'restrict_violation';
END
$$;
CREATE TRIGGER farm_passbook_entries_fixed BEFORE UPDATE OR DELETE ON farm_passbook_entries
    FOR EACH ROW EXECUTE FUNCTION farm_passbook_append_only();
CREATE TRIGGER farm_passbook_entries_no_truncate BEFORE TRUNCATE ON farm_passbook_entries
    FOR EACH STATEMENT EXECUTE FUNCTION farm_passbook_append_only();

-- Passport consent (G10): may the public lot page ever name or picture this person. Off by
-- default. Nothing shows names while the PDPC gate is closed; this only records the choice.
ALTER TABLE farm_persons
    ADD COLUMN passport_profile_consent boolean NOT NULL DEFAULT false,
    ADD COLUMN passport_profile_consent_at timestamptz,
    ADD COLUMN passport_profile_consent_by uuid REFERENCES users (id),
    ADD CONSTRAINT farm_persons_consent_stamp CHECK (
        passport_profile_consent = (passport_profile_consent_at IS NOT NULL));
