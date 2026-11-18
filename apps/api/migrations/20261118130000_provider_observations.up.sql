-- Flow ERP M4 C12: confirmed provider facts (masterplan D16, §12 EXTERNAL OBSERVATIONS).
-- One immutable observation per confirmed collection/refund/chargeback; its accounting state is
-- its accounting_source_links row (source_kind 'provider_observation'). An observation that
-- can't post keeps a pending exception; an excess outflow keeps a discrepancy exception.

CREATE TABLE provider_observations (
    id                  uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id        uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    provider            text        NOT NULL CHECK (provider IN ('globadyme', 'stripe', 'fake')),
    environment         text,
    merchant_id         text,
    operation_id        uuid        NOT NULL REFERENCES order_payments (id),
    parent_operation_id uuid        REFERENCES order_payments (id),
    economic_kind       text        NOT NULL CHECK (economic_kind IN ('collection', 'refund', 'chargeback')),
    external_id         text        NOT NULL CHECK (length(external_id) BETWEEN 1 AND 255),
    invoice_id          uuid,
    credit_note_id      uuid,
    order_id            uuid,
    relation_id         uuid,
    amount              bigint      NOT NULL CHECK (amount > 0),
    currency            text        NOT NULL,
    decimals            smallint    NOT NULL CHECK (decimals BETWEEN 0 AND 8),
    occurred_at         timestamptz NOT NULL,
    recorded_at         timestamptz NOT NULL DEFAULT clock_timestamp(),
    fingerprint         bytea       NOT NULL CHECK (octet_length(fingerprint) = 32),
    CONSTRAINT provider_observations_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT provider_observations_identity_key
        UNIQUE (workspace_id, provider, economic_kind, external_id)
);
CREATE INDEX provider_observations_operation_idx ON provider_observations (workspace_id, operation_id);

CREATE TABLE accounting_exceptions (
    id                   uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id         uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    observation_id       uuid        NOT NULL,
    kind                 text        NOT NULL CHECK (kind IN ('pending_posting', 'discrepancy')),
    reason_code          text        NOT NULL CHECK (reason_code IN ('no_receivable',
                             'foreign_currency', 'closed_period', 'missing_mapping', 'not_payable',
                             'posting_failed', 'excess_refund', 'chargeback')),
    message              text        NOT NULL CHECK (length(message) BETWEEN 1 AND 1000),
    state                text        NOT NULL DEFAULT 'pending' CHECK (state IN ('pending', 'resolved')),
    recovery_item_id     uuid,
    amount               bigint      CHECK (amount > 0),
    resolution           text        CHECK (resolution IN ('posted', 'assign_receivable', 'write_off')),
    resolution_effect_id uuid,
    relation_id          uuid,
    reason               text        CHECK (length(reason) BETWEEN 1 AND 1000),
    evidence_ref         text        CHECK (length(evidence_ref) BETWEEN 1 AND 255),
    attempts             integer     NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    last_error           text,
    version              bigint      NOT NULL DEFAULT 1 CHECK (version >= 1),
    resolved_by          uuid,
    resolved_at          timestamptz,
    created_at           timestamptz NOT NULL DEFAULT clock_timestamp(),
    CONSTRAINT accounting_exceptions_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT accounting_exceptions_observation_fk FOREIGN KEY (workspace_id, observation_id)
        REFERENCES provider_observations (workspace_id, id),
    CONSTRAINT accounting_exceptions_recovery_fk FOREIGN KEY (workspace_id, recovery_item_id)
        REFERENCES accounting_open_items (workspace_id, id),
    CONSTRAINT accounting_exceptions_resolved_check CHECK (
        (state = 'resolved') = (resolution IS NOT NULL AND resolved_at IS NOT NULL)),
    CONSTRAINT accounting_exceptions_discrepancy_check CHECK (
        (kind = 'discrepancy') = (recovery_item_id IS NOT NULL AND amount IS NOT NULL)
        AND (resolution IS NULL OR (kind = 'discrepancy') = (resolution <> 'posted')))
);
CREATE UNIQUE INDEX accounting_exceptions_open_key
    ON accounting_exceptions (workspace_id, observation_id, kind) WHERE state = 'pending';
CREATE INDEX accounting_exceptions_list_idx ON accounting_exceptions (workspace_id, state, id);

-- Observations never change; exceptions only move pending -> resolved (attempts/last_error
-- while pending). Workspace cascade allowed.
CREATE FUNCTION provider_observation_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'TRUNCATE' THEN
        RAISE EXCEPTION '% is append-only (TRUNCATE)', TG_TABLE_NAME USING ERRCODE = '23514';
    END IF;
    IF TG_OP = 'DELETE' THEN
        IF NOT EXISTS (SELECT 1 FROM workspaces WHERE id = OLD.workspace_id) THEN
            RETURN OLD;
        END IF;
        RAISE EXCEPTION '% is append-only (DELETE)', TG_TABLE_NAME USING ERRCODE = '23514';
    END IF;
    IF TG_TABLE_NAME = 'accounting_exceptions' AND to_jsonb(OLD) ->> 'state' = 'pending'
       AND (to_jsonb(NEW) - ARRAY['state', 'resolution', 'resolution_effect_id', 'relation_id',
            'reason', 'evidence_ref', 'attempts', 'last_error', 'version', 'resolved_by',
            'resolved_at'])
         = (to_jsonb(OLD) - ARRAY['state', 'resolution', 'resolution_effect_id', 'relation_id',
            'reason', 'evidence_ref', 'attempts', 'last_error', 'version', 'resolved_by',
            'resolved_at']) THEN
        RETURN NEW;
    END IF;
    RAISE EXCEPTION '% is append-only (UPDATE)', TG_TABLE_NAME USING ERRCODE = '23514';
END $$;
CREATE TRIGGER provider_observations_fixed BEFORE UPDATE OR DELETE ON provider_observations
    FOR EACH ROW EXECUTE FUNCTION provider_observation_guard();
CREATE TRIGGER provider_observations_no_truncate BEFORE TRUNCATE ON provider_observations
    FOR EACH STATEMENT EXECUTE FUNCTION provider_observation_guard();
CREATE TRIGGER accounting_exceptions_fixed BEFORE UPDATE OR DELETE ON accounting_exceptions
    FOR EACH ROW EXECUTE FUNCTION provider_observation_guard();
CREATE TRIGGER accounting_exceptions_no_truncate BEFORE TRUNCATE ON accounting_exceptions
    FOR EACH STATEMENT EXECUTE FUNCTION provider_observation_guard();

-- ==== SHARED CHECK WIDENING (C12): accounting_allocations kind/items. Base = main; adds
-- refund, write_off, assignment. No trigger function change. Exact down in .down.sql. ====
-- Target-only allocations: refund (provider outflow relieves a customer credit/deposit),
-- write_off (recovery item to financial_loss), assignment (recovery item moved to a receivable).
ALTER TABLE accounting_allocations
    DROP CONSTRAINT accounting_allocations_kind_check,
    ADD CONSTRAINT accounting_allocations_kind_check CHECK (kind IN ('application',
        'reclassification', 'rounding', 'refund', 'write_off', 'assignment')),
    DROP CONSTRAINT accounting_allocations_items_check,
    ADD CONSTRAINT accounting_allocations_items_check CHECK (
        (kind IN ('rounding', 'refund', 'write_off', 'assignment')) = (source_item_id IS NULL)
        AND source_item_id IS DISTINCT FROM target_item_id);
-- ==== END SHARED CHECK WIDENING (C12) ====
