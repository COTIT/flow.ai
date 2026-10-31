-- Surveys and incentive vouchers (RUAHA, G14). Admin authors a survey; a farmer answers once
-- per household and gets a single-use voucher for a fixed cash incentive, paid face to face.
-- A voucher is not a payment instrument: no balance, no transfer. Money is integer minor units
-- of a workspace currency.

CREATE TABLE farm_surveys (
    id             uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id   uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    -- NULL = every village of the workspace.
    village_id     uuid,
    title_en       text        NOT NULL CHECK (char_length(btrim(title_en)) BETWEEN 1 AND 200),
    title_sw       text        CHECK (char_length(title_sw) <= 200),
    description_en text        CHECK (char_length(description_en) <= 2000),
    description_sw text        CHECK (char_length(description_sw) <= 2000),
    reward_minor   bigint      NOT NULL CHECK (reward_minor > 0),
    currency_id    uuid        NOT NULL,
    max_households integer     CHECK (max_households > 0),
    -- Share of vouchers held for an ops/admin audit at redemption.
    audit_percent  smallint    NOT NULL DEFAULT 15 CHECK (audit_percent BETWEEN 0 AND 100),
    closes_at      timestamptz,
    status         text        NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'live', 'closed')),
    published_at   timestamptz,
    published_by   uuid        REFERENCES users (id),
    closed_at      timestamptz,
    created_by     uuid        REFERENCES users (id),
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT farm_surveys_village_fk FOREIGN KEY (workspace_id, village_id)
        REFERENCES farm_villages (workspace_id, id),
    CONSTRAINT farm_surveys_currency_fk FOREIGN KEY (workspace_id, currency_id)
        REFERENCES currencies (workspace_id, id),
    CONSTRAINT farm_surveys_workspace_id_id_key UNIQUE (workspace_id, id)
);

CREATE INDEX farm_surveys_workspace_idx ON farm_surveys (workspace_id, status);
CREATE INDEX farm_surveys_currency_idx ON farm_surveys (workspace_id, currency_id);

-- The incentive and questions can't move under the farmers once live; status only moves
-- draft -> live -> closed. Who may author is checked in Rust.
CREATE FUNCTION farm_surveys_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        IF NEW.status <> 'draft' THEN
            RAISE EXCEPTION 'a survey must start as a draft' USING ERRCODE = 'check_violation';
        END IF;
        RETURN NEW;
    END IF;
    IF OLD.status <> 'draft' AND
       (NEW.village_id, NEW.title_en, NEW.title_sw, NEW.description_en, NEW.description_sw,
        NEW.reward_minor, NEW.currency_id, NEW.max_households, NEW.audit_percent, NEW.closes_at)
       IS DISTINCT FROM
       (OLD.village_id, OLD.title_en, OLD.title_sw, OLD.description_en, OLD.description_sw,
        OLD.reward_minor, OLD.currency_id, OLD.max_households, OLD.audit_percent, OLD.closes_at) THEN
        RAISE EXCEPTION 'a live survey cannot be edited' USING ERRCODE = 'check_violation';
    END IF;
    IF NEW.status IS DISTINCT FROM OLD.status
       AND NOT ((OLD.status = 'draft' AND NEW.status = 'live') OR (OLD.status = 'live' AND NEW.status = 'closed')) THEN
        RAISE EXCEPTION 'illegal survey transition % -> %', OLD.status, NEW.status USING ERRCODE = 'check_violation';
    END IF;
    RETURN NEW;
END
$$;

CREATE TRIGGER farm_surveys_guard BEFORE INSERT OR UPDATE ON farm_surveys
    FOR EACH ROW EXECUTE FUNCTION farm_surveys_guard();

CREATE TABLE farm_survey_questions (
    id           uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    survey_id    uuid        NOT NULL,
    position     integer     NOT NULL CHECK (position > 0),
    kind         text        NOT NULL CHECK (kind IN ('single_choice', 'multi_choice', 'number', 'text', 'yes_no')),
    prompt_en    text        NOT NULL CHECK (char_length(btrim(prompt_en)) BETWEEN 1 AND 500),
    prompt_sw    text        CHECK (char_length(prompt_sw) <= 500),
    required     boolean     NOT NULL DEFAULT true,
    -- [{value, label_en, label_sw}], choice kinds only.
    options      jsonb       NOT NULL DEFAULT '[]' CHECK (jsonb_typeof(options) = 'array'),
    deleted_at   timestamptz,
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT farm_survey_questions_survey_fk FOREIGN KEY (workspace_id, survey_id)
        REFERENCES farm_surveys (workspace_id, id)
);

CREATE INDEX farm_survey_questions_survey_idx ON farm_survey_questions (survey_id, position)
    WHERE deleted_at IS NULL;

CREATE FUNCTION farm_survey_questions_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'UPDATE' AND NEW.survey_id IS DISTINCT FROM OLD.survey_id THEN
        RAISE EXCEPTION 'a question cannot move to another survey' USING ERRCODE = 'check_violation';
    END IF;
    IF (SELECT status FROM farm_surveys WHERE id = NEW.survey_id) IS DISTINCT FROM 'draft' THEN
        RAISE EXCEPTION 'questions can only change while the survey is a draft' USING ERRCODE = 'check_violation';
    END IF;
    RETURN NEW;
END
$$;

CREATE TRIGGER farm_survey_questions_guard BEFORE INSERT OR UPDATE ON farm_survey_questions
    FOR EACH ROW EXECUTE FUNCTION farm_survey_questions_guard();

CREATE TABLE farm_survey_responses (
    id           uuid         PRIMARY KEY DEFAULT uuidv7(),
    workspace_id uuid         NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    village_id   uuid         NOT NULL,
    survey_id    uuid         NOT NULL,
    household_id uuid         NOT NULL,
    person_id    uuid         NOT NULL,
    -- Made once by the device: a retried submit returns the same voucher.
    client_ref   uuid         NOT NULL,
    submitted_at timestamptz  NOT NULL DEFAULT now(),
    source       text         NOT NULL DEFAULT 'farmer_reported' CHECK (source IN ('farmer_reported', 'field_verified',
                     'transaction_derived', 'sensor_derived', 'model_estimated')),
    captured_at  timestamptz  NOT NULL DEFAULT now(),
    captured_by  uuid         REFERENCES users (id),
    verification text         NOT NULL DEFAULT 'unverified'
                     CHECK (verification IN ('unverified', 'pending', 'verified', 'disputed')),
    verified_by  uuid         REFERENCES users (id),
    verified_at  timestamptz,
    confidence   text         CHECK (confidence IN ('low', 'medium', 'high')),
    evidence_ref text,
    created_at   timestamptz  NOT NULL DEFAULT now(),
    updated_at   timestamptz  NOT NULL DEFAULT now(),
    deleted_at   timestamptz,
    CONSTRAINT farm_survey_responses_survey_fk FOREIGN KEY (workspace_id, survey_id)
        REFERENCES farm_surveys (workspace_id, id),
    CONSTRAINT farm_survey_responses_household_fk FOREIGN KEY (workspace_id, village_id, household_id)
        REFERENCES farm_households (workspace_id, village_id, id),
    CONSTRAINT farm_survey_responses_person_fk FOREIGN KEY (workspace_id, village_id, person_id)
        REFERENCES farm_persons (workspace_id, village_id, id),
    CONSTRAINT farm_survey_responses_client_ref_key UNIQUE (workspace_id, client_ref),
    CONSTRAINT farm_survey_responses_one_per_household UNIQUE (survey_id, household_id)
);

CREATE INDEX farm_survey_responses_household_idx ON farm_survey_responses (household_id);
CREATE INDEX farm_survey_responses_village_idx ON farm_survey_responses (workspace_id, village_id);

CREATE TABLE farm_survey_answers (
    id           uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    response_id  uuid        NOT NULL REFERENCES farm_survey_responses (id),
    question_id  uuid        NOT NULL REFERENCES farm_survey_questions (id),
    value        jsonb       NOT NULL,
    created_at   timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT farm_survey_answers_one_per_question UNIQUE (response_id, question_id)
);

CREATE INDEX farm_survey_answers_question_idx ON farm_survey_answers (question_id);

CREATE TABLE farm_vouchers (
    id             uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id   uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    village_id     uuid        NOT NULL,
    response_id    uuid        NOT NULL UNIQUE REFERENCES farm_survey_responses (id),
    survey_id      uuid        NOT NULL,
    household_id   uuid        NOT NULL,
    -- 10 Crockford characters; only the household is ever shown it.
    code           text        NOT NULL CHECK (code ~ '^[0-9A-HJKMNP-TV-Z]{10}$'),
    -- Copied from the survey at issue.
    amount_minor   bigint      NOT NULL CHECK (amount_minor > 0),
    currency_id    uuid        NOT NULL,
    -- Expired is derived from expires_at.
    status         text        NOT NULL DEFAULT 'issued' CHECK (status IN ('issued', 'redeemed', 'void')),
    audit_required boolean     NOT NULL DEFAULT false,
    issued_at      timestamptz NOT NULL DEFAULT now(),
    expires_at     timestamptz NOT NULL,
    redeemed_by    uuid        REFERENCES users (id),
    redeemed_at    timestamptz,
    -- Which ID document the redeemer checked: never its number, never a photo.
    id_type_seen   text        CHECK (id_type_seen IN ('nida', 'voter', 'driving_licence', 'village_letter')),
    voided_by      uuid        REFERENCES users (id),
    voided_at      timestamptz,
    void_reason    text        CHECK (char_length(void_reason) <= 500),
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT farm_vouchers_survey_fk FOREIGN KEY (workspace_id, survey_id)
        REFERENCES farm_surveys (workspace_id, id),
    CONSTRAINT farm_vouchers_household_fk FOREIGN KEY (workspace_id, village_id, household_id)
        REFERENCES farm_households (workspace_id, village_id, id),
    CONSTRAINT farm_vouchers_currency_fk FOREIGN KEY (workspace_id, currency_id)
        REFERENCES currencies (workspace_id, id),
    CONSTRAINT farm_vouchers_code_key UNIQUE (workspace_id, code),
    CONSTRAINT farm_vouchers_redeemed_complete CHECK (
        status <> 'redeemed' OR (redeemed_by IS NOT NULL AND redeemed_at IS NOT NULL AND id_type_seen IS NOT NULL)),
    CONSTRAINT farm_vouchers_void_complete CHECK (
        status <> 'void' OR (voided_by IS NOT NULL AND voided_at IS NOT NULL AND nullif(btrim(void_reason), '') IS NOT NULL))
);

CREATE INDEX farm_vouchers_household_idx ON farm_vouchers (household_id);
CREATE INDEX farm_vouchers_survey_idx ON farm_vouchers (survey_id);
CREATE INDEX farm_vouchers_village_idx ON farm_vouchers (workspace_id, village_id, status);
CREATE INDEX farm_vouchers_redeemed_idx ON farm_vouchers (workspace_id, redeemed_at) WHERE status = 'redeemed';
CREATE INDEX farm_vouchers_currency_idx ON farm_vouchers (workspace_id, currency_id);

-- Every step a voucher took, with the actor's name and role as they were then.
CREATE TABLE farm_voucher_events (
    id           uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    voucher_id   uuid        NOT NULL REFERENCES farm_vouchers (id),
    kind         text        NOT NULL CHECK (kind IN ('issued', 'scanned', 'refused', 'redeemed', 'voided')),
    actor_id     uuid        REFERENCES users (id),
    actor_name   text        NOT NULL,
    actor_role   text        NOT NULL,
    occurred_at  timestamptz NOT NULL DEFAULT clock_timestamp(),
    detail       jsonb       NOT NULL DEFAULT '{}'
);

CREATE INDEX farm_voucher_events_voucher_idx ON farm_voucher_events (voucher_id, occurred_at);
-- The per-user lookup limit counts recent scans.
CREATE INDEX farm_voucher_events_actor_idx ON farm_voucher_events (actor_id, occurred_at)
    WHERE kind IN ('scanned', 'refused');

CREATE FUNCTION farm_voucher_events_append_only() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'voucher events cannot be changed' USING ERRCODE = 'restrict_violation';
END
$$;

CREATE TRIGGER farm_voucher_events_fixed BEFORE UPDATE OR DELETE ON farm_voucher_events
    FOR EACH ROW EXECUTE FUNCTION farm_voucher_events_append_only();

-- Every voucher lookup by an account, known code or not: the per-user lookup limit counts
-- these, so guessing codes is slow. Each insert prunes that account's rows older than an hour.
CREATE TABLE farm_voucher_lookups (
    id          bigint      GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    actor_id    uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    looked_up_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX farm_voucher_lookups_actor_idx ON farm_voucher_lookups (actor_id, looked_up_at);

-- An account's most senior live farm role in a workspace, as the voucher trail names actors.
CREATE FUNCTION farm_role_label(ws uuid, u uuid) RETURNS text LANGUAGE sql STABLE AS $$
    SELECT coalesce((
        SELECT m.role FROM farm_memberships m
        WHERE m.workspace_id = ws AND m.user_id = u AND m.revoked_at IS NULL
        ORDER BY CASE m.role WHEN 'admin' THEN 1 WHEN 'ops' THEN 2 WHEN 'field_officer' THEN 3 ELSE 4 END
        LIMIT 1), 'unknown')
$$;

CREATE TRIGGER farm_surveys_audit AFTER INSERT OR UPDATE OR DELETE ON farm_surveys
    FOR EACH ROW EXECUTE FUNCTION farm_write_audit();
CREATE TRIGGER farm_survey_questions_audit AFTER INSERT OR UPDATE OR DELETE ON farm_survey_questions
    FOR EACH ROW EXECUTE FUNCTION farm_write_audit();
CREATE TRIGGER farm_survey_responses_audit AFTER INSERT OR UPDATE OR DELETE ON farm_survey_responses
    FOR EACH ROW EXECUTE FUNCTION farm_write_audit();
CREATE TRIGGER farm_survey_answers_audit AFTER INSERT OR UPDATE OR DELETE ON farm_survey_answers
    FOR EACH ROW EXECUTE FUNCTION farm_write_audit();
CREATE TRIGGER farm_vouchers_audit AFTER INSERT OR UPDATE OR DELETE ON farm_vouchers
    FOR EACH ROW EXECUTE FUNCTION farm_write_audit();
