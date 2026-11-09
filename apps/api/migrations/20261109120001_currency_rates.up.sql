-- Workspace exchange rates, entered by staff (no outside rate provider). 1 major unit of
-- `from` = `rate` major units of `to`. History only grows: a fix is a newer row, the latest
-- row for a day wins.

CREATE TABLE currency_rates (
    id               uuid          PRIMARY KEY DEFAULT uuidv7(),
    workspace_id     uuid          NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    from_currency_id uuid          NOT NULL,
    to_currency_id   uuid          NOT NULL,
    rate             numeric(24,8) NOT NULL CHECK (rate > 0),
    effective_on     date          NOT NULL,
    note             text          CHECK (char_length(note) BETWEEN 1 AND 500),
    created_by       uuid          NOT NULL REFERENCES users (id),
    created_at       timestamptz   NOT NULL DEFAULT clock_timestamp(),
    CHECK (from_currency_id <> to_currency_id),
    CONSTRAINT currency_rates_from_fk FOREIGN KEY (workspace_id, from_currency_id)
        REFERENCES currencies (workspace_id, id),
    CONSTRAINT currency_rates_to_fk FOREIGN KEY (workspace_id, to_currency_id)
        REFERENCES currencies (workspace_id, id)
);
-- The rate of a pair on a day, and the pair's history.
CREATE INDEX currency_rates_pair_idx
    ON currency_rates (workspace_id, from_currency_id, to_currency_id, effective_on DESC,
                       created_at DESC);
CREATE INDEX currency_rates_list_idx ON currency_rates (workspace_id, created_at DESC, id DESC);
CREATE INDEX currency_rates_to_idx ON currency_rates (workspace_id, to_currency_id);
CREATE INDEX currency_rates_created_by_idx ON currency_rates (created_by);

CREATE FUNCTION currency_rates_append_only() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'currency rates cannot be changed: add a newer rate'
        USING ERRCODE = 'restrict_violation';
END
$$;
CREATE TRIGGER currency_rates_fixed BEFORE UPDATE OR DELETE ON currency_rates
    FOR EACH ROW EXECUTE FUNCTION currency_rates_append_only();
CREATE TRIGGER currency_rates_no_truncate BEFORE TRUNCATE ON currency_rates
    FOR EACH STATEMENT EXECUTE FUNCTION currency_rates_append_only();
