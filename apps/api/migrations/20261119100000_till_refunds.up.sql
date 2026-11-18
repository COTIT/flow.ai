-- Flow ERP M4-C13: refund holds on customer credits (shared cap across Till/Office/provider
-- refunds) and privileged Till refunds (cash out of the original drawer, or a card refund
-- recorded with its terminal reference). Refundable = credit residual − pending holds.

CREATE TABLE credit_refund_reservations (
    id             uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id   uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    credit_item_id uuid        NOT NULL,
    source_kind    text        NOT NULL
        CHECK (source_kind IN ('till_refund', 'office_refund', 'provider_refund')),
    source_id      uuid        NOT NULL,
    amount         bigint      NOT NULL CHECK (amount > 0),
    state          text        NOT NULL DEFAULT 'pending'
        CHECK (state IN ('pending', 'consumed', 'released')),
    -- The refund's effect once consumed.
    effect_id      uuid,
    created_at     timestamptz NOT NULL DEFAULT clock_timestamp(),
    updated_at     timestamptz NOT NULL DEFAULT clock_timestamp(),
    CONSTRAINT credit_refund_reservations_source_key UNIQUE (workspace_id, source_kind, source_id),
    CONSTRAINT credit_refund_reservations_item_fk FOREIGN KEY (workspace_id, credit_item_id)
        REFERENCES accounting_open_items (workspace_id, id),
    CONSTRAINT credit_refund_reservations_effect_fk FOREIGN KEY (workspace_id, effect_id)
        REFERENCES accounting_effects (workspace_id, id),
    CONSTRAINT credit_refund_reservations_effect_check CHECK (state = 'consumed' OR effect_id IS NULL)
);
CREATE INDEX credit_refund_reservations_item_idx
    ON credit_refund_reservations (workspace_id, credit_item_id);
CREATE INDEX credit_refund_reservations_effect_idx
    ON credit_refund_reservations (workspace_id, effect_id) WHERE effect_id IS NOT NULL;

-- Only pending → consumed | released (with the consumed effect); no delete.
CREATE FUNCTION credit_refund_reservations_transition() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP <> 'UPDATE' OR OLD.state <> 'pending' OR NEW.state = 'pending'
       OR (NEW.id, NEW.workspace_id, NEW.credit_item_id, NEW.source_kind, NEW.source_id,
           NEW.amount, NEW.created_at)
          IS DISTINCT FROM (OLD.id, OLD.workspace_id, OLD.credit_item_id, OLD.source_kind,
                            OLD.source_id, OLD.amount, OLD.created_at) THEN
        RAISE EXCEPTION 'credit_refund_reservations: only pending -> consumed|released (%)', TG_OP
            USING ERRCODE = 'restrict_violation';
    END IF;
    NEW.updated_at := clock_timestamp();
    RETURN NEW;
END
$$;
CREATE TRIGGER credit_refund_reservations_state BEFORE UPDATE OR DELETE
    ON credit_refund_reservations
    FOR EACH ROW EXECUTE FUNCTION credit_refund_reservations_transition();
CREATE TRIGGER credit_refund_reservations_no_truncate BEFORE TRUNCATE ON credit_refund_reservations
    FOR EACH STATEMENT EXECUTE FUNCTION cash_ledger_append_only();

-- One money refund at the Till against a sale's customer credit. Append-only.
CREATE TABLE till_refunds (
    id                      uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id            uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    order_id                uuid        NOT NULL,
    credit_item_id          uuid        NOT NULL,
    -- Register of the approving Till session; cash: the sale's own register.
    register_id             uuid        NOT NULL,
    shift_id                uuid,
    -- Cashier context; the member is the authority.
    till_user_id            uuid        NOT NULL,
    member_user_id          uuid        NOT NULL,
    amount                  bigint      NOT NULL CHECK (amount > 0),
    currency_id             uuid        NOT NULL,
    method                  text        NOT NULL CHECK (method IN ('cash', 'card_recorded')),
    external_reference      text        CHECK (char_length(btrim(external_reference)) BETWEEN 1 AND 255),
    actual_refund_confirmed boolean     NOT NULL,
    reason                  text        NOT NULL CHECK (char_length(btrim(reason)) BETWEEN 1 AND 1000),
    accounting_date         date        NOT NULL,
    refund_item_id          uuid        NOT NULL,
    reservation_id          uuid        NOT NULL,
    effect_id               uuid        NOT NULL,
    journal_entry_id        uuid        NOT NULL,
    created_at              timestamptz NOT NULL DEFAULT clock_timestamp(),
    CONSTRAINT till_refunds_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT till_refunds_effect_key UNIQUE (effect_id),
    CONSTRAINT till_refunds_method_fields_check CHECK (CASE method
        WHEN 'cash' THEN shift_id IS NOT NULL AND external_reference IS NULL
        ELSE shift_id IS NULL AND external_reference IS NOT NULL AND actual_refund_confirmed END),
    CONSTRAINT till_refunds_order_fk FOREIGN KEY (workspace_id, order_id)
        REFERENCES orders (workspace_id, id),
    CONSTRAINT till_refunds_credit_item_fk FOREIGN KEY (workspace_id, credit_item_id)
        REFERENCES accounting_open_items (workspace_id, id),
    CONSTRAINT till_refunds_refund_item_fk FOREIGN KEY (workspace_id, refund_item_id)
        REFERENCES accounting_open_items (workspace_id, id),
    CONSTRAINT till_refunds_register_fk FOREIGN KEY (workspace_id, register_id)
        REFERENCES till_registers (workspace_id, id),
    CONSTRAINT till_refunds_shift_fk FOREIGN KEY (workspace_id, shift_id)
        REFERENCES drawer_shifts (workspace_id, id),
    CONSTRAINT till_refunds_till_user_fk FOREIGN KEY (workspace_id, till_user_id)
        REFERENCES till_users (workspace_id, id),
    CONSTRAINT till_refunds_currency_fk FOREIGN KEY (workspace_id, currency_id)
        REFERENCES currencies (workspace_id, id),
    CONSTRAINT till_refunds_reservation_fk FOREIGN KEY (reservation_id)
        REFERENCES credit_refund_reservations (id),
    CONSTRAINT till_refunds_effect_fk FOREIGN KEY (workspace_id, effect_id)
        REFERENCES accounting_effects (workspace_id, id),
    CONSTRAINT till_refunds_journal_fk FOREIGN KEY (workspace_id, journal_entry_id)
        REFERENCES journal_entries (workspace_id, id)
);
CREATE INDEX till_refunds_order_idx ON till_refunds (workspace_id, order_id, created_at, id);
CREATE INDEX till_refunds_credit_item_idx ON till_refunds (workspace_id, credit_item_id);
CREATE INDEX till_refunds_refund_item_idx ON till_refunds (workspace_id, refund_item_id);
CREATE INDEX till_refunds_register_idx ON till_refunds (workspace_id, register_id);
CREATE INDEX till_refunds_shift_idx ON till_refunds (workspace_id, shift_id) WHERE shift_id IS NOT NULL;
CREATE INDEX till_refunds_till_user_idx ON till_refunds (workspace_id, till_user_id);
CREATE INDEX till_refunds_currency_idx ON till_refunds (workspace_id, currency_id);
CREATE INDEX till_refunds_reservation_idx ON till_refunds (reservation_id);
CREATE INDEX till_refunds_journal_idx ON till_refunds (workspace_id, journal_entry_id);

CREATE TRIGGER till_refunds_fixed BEFORE UPDATE OR DELETE ON till_refunds
    FOR EACH ROW EXECUTE FUNCTION cash_ledger_append_only();
CREATE TRIGGER till_refunds_no_truncate BEFORE TRUNCATE ON till_refunds
    FOR EACH STATEMENT EXECUTE FUNCTION cash_ledger_append_only();
