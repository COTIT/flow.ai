-- Phase 4c step 5: loan listings and bug reports (RUAHA resource_catalogue.sql,
-- bug_reports.sql).

-- Loan LISTINGS: a name, a description and an INDICATIVE amount range. No interest, deposit,
-- term, schedule or repayment: research item C (BoT Tier 2 classification) is unresolved and
-- nothing here models lending. A farmer cannot request a loan in the app; they ask at the
-- office. Retired with is_active = false, never deleted (no route deletes one).
CREATE TABLE farm_loan_products (
    id                    uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id          uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    code                  text        NOT NULL CHECK (char_length(btrim(code)) BETWEEN 1 AND 32),
    name_en               text        NOT NULL CHECK (char_length(btrim(name_en)) BETWEEN 1 AND 120),
    name_sw               text        NOT NULL CHECK (char_length(btrim(name_sw)) BETWEEN 1 AND 120),
    description_en        text        CHECK (char_length(description_en) <= 1000),
    description_sw        text        CHECK (char_length(description_sw) <= 1000),
    -- Minor units of `currency_id`.
    indicative_min_minor  bigint      NOT NULL,
    indicative_max_minor  bigint      NOT NULL,
    currency_id           uuid        NOT NULL,
    is_active             boolean     NOT NULL DEFAULT true,
    created_at            timestamptz NOT NULL DEFAULT now(),
    updated_at            timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT farm_loan_products_range CHECK
        (indicative_min_minor > 0 AND indicative_min_minor <= indicative_max_minor),
    CONSTRAINT farm_loan_products_code_key UNIQUE (workspace_id, code),
    CONSTRAINT farm_loan_products_currency_fk FOREIGN KEY (workspace_id, currency_id)
        REFERENCES currencies (workspace_id, id)
);

CREATE INDEX farm_loan_products_currency_idx ON farm_loan_products (workspace_id, currency_id);

CREATE TRIGGER farm_loan_products_audit AFTER INSERT OR UPDATE OR DELETE ON farm_loan_products
    FOR EACH ROW EXECUTE FUNCTION farm_write_audit();

-- Reports from the in-app bug button: a screenshot (or a photo when that fails), the last 30
-- things the user did, and what they say went wrong. Anyone with a farm role files, always
-- under their own name; only farm admin reads. No route changes or deletes one; status moves
-- through operator SQL only. Images live under the reporter's own folder in the bucket.
CREATE TABLE farm_bug_reports (
    id             uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id   uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    -- The ticket number people say out loud: BUG-12.
    ref            bigint      GENERATED ALWAYS AS IDENTITY UNIQUE,
    reporter_id    uuid        NOT NULL REFERENCES users (id),
    route          text        NOT NULL CHECK (char_length(route) BETWEEN 1 AND 500),
    user_agent     text        NOT NULL CHECK (char_length(user_agent) BETWEEN 1 AND 500),
    viewport       text        NOT NULL CHECK (char_length(viewport) BETWEEN 1 AND 50),
    description    text        NOT NULL CHECK (char_length(btrim(description)) BETWEEN 1 AND 5000),
    actions        jsonb       NOT NULL CHECK (jsonb_typeof(actions) = 'array'
                                               AND jsonb_array_length(actions) <= 30),
    screenshot_key text,
    photo_key      text,
    status         text        NOT NULL DEFAULT 'open'
                       CHECK (status IN ('open', 'in_progress', 'fixed', 'wont_fix')),
    resolution     text,
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT farm_bug_reports_has_image CHECK (screenshot_key IS NOT NULL OR photo_key IS NOT NULL)
);

CREATE INDEX farm_bug_reports_list_idx ON farm_bug_reports (workspace_id, created_at DESC, id DESC);
CREATE INDEX farm_bug_reports_reporter_idx ON farm_bug_reports (reporter_id);

CREATE TRIGGER farm_bug_reports_audit AFTER INSERT OR UPDATE OR DELETE ON farm_bug_reports
    FOR EACH ROW EXECUTE FUNCTION farm_write_audit();
