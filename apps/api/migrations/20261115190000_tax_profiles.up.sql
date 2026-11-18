-- Tax profiles (masterplan §13 TAX): effective-dated, verified percentage components per
-- jurisdiction, transaction type and product category, with exemptions. No seeds: every rate
-- comes from the workspace with evidence. `tax_line` is the one SQL calculation for a line;
-- nothing calls it yet (wiring is a later step).

CREATE TABLE tax_profiles (
    id                   uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id         uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    -- Revision 1's id; corrections share it.
    lineage_id           uuid        NOT NULL,
    revision             integer     NOT NULL DEFAULT 1 CHECK (revision >= 1),
    corrects_id          uuid,
    state                text        NOT NULL DEFAULT 'draft' CHECK (state IN ('draft', 'verified')),
    version              bigint      NOT NULL DEFAULT 1 CHECK (version >= 1),
    country              text        NOT NULL CHECK (country ~ '^[A-Z]{2}$'),
    -- ISO 3166-2; NULL = whole country.
    subdivision          text        CHECK (subdivision ~ '^[A-Z]{2}-[A-Z0-9]{1,3}$'
                                            AND left(subdivision, 2) = country),
    transaction_types    text[]      NOT NULL CHECK (cardinality(transaction_types) > 0
                                            AND transaction_types <@ ARRAY['sale', 'purchase']),
    -- Empty = every category.
    product_category_ids uuid[]      NOT NULL DEFAULT '{}',
    effective_from       date        NOT NULL,
    -- Exclusive end; NULL = open.
    effective_to         date        CHECK (effective_to > effective_from),
    -- Payment-based (cash) timing is unsupported.
    tax_point            text        NOT NULL CHECK (tax_point IN ('invoice', 'supply')),
    evidence_ref         text        CHECK (char_length(evidence_ref) BETWEEN 1 AND 255),
    verified_by          uuid,
    verified_at          timestamptz,
    verify_fingerprint   bytea       CHECK (octet_length(verify_fingerprint) = 32),
    superseded_by_id     uuid,
    created_by           uuid        NOT NULL,
    created_at           timestamptz NOT NULL DEFAULT now(),
    updated_at           timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT tax_profiles_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT tax_profiles_revision_key UNIQUE (workspace_id, lineage_id, revision),
    CONSTRAINT tax_profiles_corrects_key UNIQUE (workspace_id, corrects_id),
    CONSTRAINT tax_profiles_corrects_fk FOREIGN KEY (workspace_id, corrects_id)
        REFERENCES tax_profiles (workspace_id, id),
    CONSTRAINT tax_profiles_superseded_fk FOREIGN KEY (workspace_id, superseded_by_id)
        REFERENCES tax_profiles (workspace_id, id),
    CONSTRAINT tax_profiles_verified_check CHECK ((state = 'verified') = (verified_by IS NOT NULL)
        AND (state = 'verified') = (verified_at IS NOT NULL)
        AND (state = 'verified') = (verify_fingerprint IS NOT NULL)
        AND (state = 'draft' OR evidence_ref IS NOT NULL)
        AND (superseded_by_id IS NULL OR state = 'verified'))
);
CREATE INDEX tax_profiles_list_idx ON tax_profiles (workspace_id, country, effective_from);

-- Components: percentage only (fixed / compound rejected).
CREATE TABLE tax_rules (
    id           uuid          PRIMARY KEY DEFAULT uuidv7(),
    workspace_id uuid          NOT NULL,
    profile_id   uuid          NOT NULL,
    line_no      smallint      NOT NULL CHECK (line_no BETWEEN 1 AND 10),
    code         text          NOT NULL CHECK (code ~ '^[A-Z0-9_]{1,32}$'),
    name         text          NOT NULL CHECK (char_length(name) BETWEEN 1 AND 255),
    kind         text          NOT NULL DEFAULT 'percentage' CHECK (kind = 'percentage'),
    rate         numeric(11,8) NOT NULL CHECK (rate BETWEEN 0 AND 100),
    -- Purchase tax the business can reclaim (all or nothing).
    recoverable  boolean       NOT NULL,
    CONSTRAINT tax_rules_profile_fk FOREIGN KEY (workspace_id, profile_id)
        REFERENCES tax_profiles (workspace_id, id) ON DELETE CASCADE,
    CONSTRAINT tax_rules_line_key UNIQUE (profile_id, line_no),
    CONSTRAINT tax_rules_code_key UNIQUE (profile_id, code)
);

-- Exemption: all components 0 for one relation or one product category.
CREATE TABLE tax_exemptions (
    id                  uuid PRIMARY KEY DEFAULT uuidv7(),
    workspace_id        uuid NOT NULL,
    profile_id          uuid NOT NULL,
    relation_id         uuid,
    product_category_id uuid,
    reason              text NOT NULL CHECK (char_length(reason) BETWEEN 1 AND 1000),
    evidence_ref        text NOT NULL CHECK (char_length(evidence_ref) BETWEEN 1 AND 255),
    CONSTRAINT tax_exemptions_profile_fk FOREIGN KEY (workspace_id, profile_id)
        REFERENCES tax_profiles (workspace_id, id) ON DELETE CASCADE,
    CONSTRAINT tax_exemptions_relation_fk FOREIGN KEY (workspace_id, relation_id)
        REFERENCES relations (workspace_id, id),
    CONSTRAINT tax_exemptions_category_fk FOREIGN KEY (workspace_id, product_category_id)
        REFERENCES product_categories (workspace_id, id),
    CONSTRAINT tax_exemptions_target_check CHECK (num_nonnulls(relation_id, product_category_id) = 1)
);
CREATE INDEX tax_exemptions_profile_idx ON tax_exemptions (profile_id);

-- A verified profile never changes; only its one-time `superseded_by_id` link may be set.
-- Deletes stay possible so a workspace delete cascades (no API deletes profiles).
CREATE FUNCTION tax_profiles_fixed() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF OLD.state = 'verified' AND (OLD.superseded_by_id IS NOT NULL
        OR to_jsonb(NEW) - 'superseded_by_id' - 'updated_at'
           IS DISTINCT FROM to_jsonb(OLD) - 'superseded_by_id' - 'updated_at') THEN
        RAISE EXCEPTION 'A verified tax profile is immutable.' USING ERRCODE = '55000';
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER tax_profiles_fixed BEFORE UPDATE ON tax_profiles
    FOR EACH ROW EXECUTE FUNCTION tax_profiles_fixed();

-- Components and exemptions of a verified profile are fixed (cascade from a deleted profile
-- finds no parent row and passes).
CREATE FUNCTION tax_profile_parts_fixed() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF EXISTS (SELECT 1 FROM tax_profiles p
               WHERE p.id IN (OLD.profile_id, NEW.profile_id) AND p.state = 'verified') THEN
        RAISE EXCEPTION 'A verified tax profile is immutable.' USING ERRCODE = '55000';
    END IF;
    RETURN coalesce(NEW, OLD);
END $$;
CREATE TRIGGER tax_rules_fixed BEFORE INSERT OR UPDATE OR DELETE ON tax_rules
    FOR EACH ROW EXECUTE FUNCTION tax_profile_parts_fixed();
CREATE TRIGGER tax_exemptions_fixed BEFORE INSERT OR UPDATE OR DELETE ON tax_exemptions
    FOR EACH ROW EXECUTE FUNCTION tax_profile_parts_fixed();

-- One line's tax per component, minor units. `amount` excludes tax unless `includes_tax`.
-- Exclusive: tax_i = round(amount × rate_i / 100). Inclusive: base = round(amount × 100 /
-- (100 + Σrate)); every component rounds on base except the last non-zero one, which takes the
-- exact remainder, so base + Σtax = amount. An exemption zeroes every component. Rounding is
-- half away from zero via money_round_div. Refusals raise 22023 with a message key.
CREATE FUNCTION tax_line(ws uuid, profile uuid, on_date date, txn text, amount bigint,
                         includes_tax boolean, category uuid, relation uuid)
    RETURNS TABLE (line_no smallint, code text, rate numeric, base bigint, tax bigint,
                   recoverable boolean, exempt boolean)
    LANGUAGE plpgsql STABLE AS $$
#variable_conflict use_column
DECLARE
    p      tax_profiles;
    free   boolean;
    total  numeric;
    net    numeric;
BEGIN
    SELECT * INTO p FROM tax_profiles t WHERE t.workspace_id = ws AND t.id = profile;
    IF NOT FOUND OR p.state <> 'verified' THEN
        RAISE EXCEPTION 'tax_profile_not_verified' USING ERRCODE = '22023';
    END IF;
    IF on_date < p.effective_from OR on_date >= p.effective_to THEN
        RAISE EXCEPTION 'tax_profile_not_effective' USING ERRCODE = '22023';
    END IF;
    IF NOT txn = ANY (p.transaction_types) THEN
        RAISE EXCEPTION 'tax_profile_wrong_transaction' USING ERRCODE = '22023';
    END IF;
    IF cardinality(p.product_category_ids) > 0
       AND (category IS NULL OR NOT category = ANY (p.product_category_ids)) THEN
        RAISE EXCEPTION 'tax_profile_wrong_category' USING ERRCODE = '22023';
    END IF;
    free := EXISTS (SELECT 1 FROM tax_exemptions e
                    WHERE e.workspace_id = ws AND e.profile_id = profile
                      AND (e.relation_id = relation OR e.product_category_id = category));
    SELECT CASE WHEN free THEN 0 ELSE coalesce(sum(r.rate), 0) END INTO total
    FROM tax_rules r WHERE r.workspace_id = ws AND r.profile_id = profile;
    net := CASE WHEN includes_tax THEN money_round_div(amount::numeric * 100, 100 + total)
                ELSE amount END;
    RETURN QUERY
    WITH c AS (
        SELECT r.line_no, r.code, r.rate, r.recoverable,
               CASE WHEN free THEN 0 ELSE money_round_div(net * r.rate, 100) END AS raw,
               row_number() OVER (ORDER BY r.rate > 0 DESC, r.line_no DESC) = 1 AS last
        FROM tax_rules r WHERE r.workspace_id = ws AND r.profile_id = profile
    )
    SELECT c.line_no, c.code, c.rate, net::bigint,
           CASE WHEN includes_tax AND c.last
                THEN amount - net - coalesce(sum(c.raw) OVER (ORDER BY c.line_no ROWS BETWEEN
                         UNBOUNDED PRECEDING AND 1 PRECEDING), 0)
                     - coalesce(sum(c.raw) OVER (ORDER BY c.line_no ROWS BETWEEN
                         1 FOLLOWING AND UNBOUNDED FOLLOWING), 0)
                ELSE c.raw END::bigint,
           c.recoverable, free
    FROM c ORDER BY c.line_no;
END $$;
