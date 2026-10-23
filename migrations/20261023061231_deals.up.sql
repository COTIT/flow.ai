-- Deals (Laravel CRM/Deal): one card on a sales board. A deal sits in one column of one board;
-- the composite FK keeps that column on the deal's board. `position` is a gap rank inside the
-- column (spaced 65536 apart): a drag writes one row, and only a closed gap renumbers the
-- column. Rank writers hold the board row FOR SHARE, then the column rows in id order.

CREATE TABLE deals (
    id                      uuid          PRIMARY KEY DEFAULT uuidv7(),
    workspace_id            uuid          NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    deal_pipeline_id        uuid          NOT NULL,
    deal_pipeline_status_id uuid          NOT NULL,
    currency_id             uuid          NOT NULL,
    relation_id             uuid,
    sales_agent_id          uuid          REFERENCES users (id) ON DELETE SET NULL,
    created_by              uuid          REFERENCES users (id) ON DELETE SET NULL,
    title                   text          NOT NULL CHECK (char_length(title) <= 255),
    description             text          CHECK (char_length(description) <= 50000),
    deal_value              numeric(24,8) NOT NULL DEFAULT 0,
    deal_priority           text          NOT NULL CHECK (deal_priority IN ('high', 'low', 'medium')),
    expected_close_date     date,
    date                    date,
    position                bigint        NOT NULL,
    meta                    jsonb         CHECK (meta IS NULL OR jsonb_typeof(meta) = 'object'),
    created_at              timestamptz   NOT NULL DEFAULT now(),
    updated_at              timestamptz   NOT NULL DEFAULT now(),
    deleted_at              timestamptz,
    CONSTRAINT deals_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT deals_status_fk FOREIGN KEY (workspace_id, deal_pipeline_id, deal_pipeline_status_id)
        REFERENCES deal_pipeline_statuses (workspace_id, deal_pipeline_id, id),
    CONSTRAINT deals_currency_fk FOREIGN KEY (workspace_id, currency_id)
        REFERENCES currencies (workspace_id, id),
    CONSTRAINT deals_relation_fk FOREIGN KEY (workspace_id, relation_id)
        REFERENCES relations (workspace_id, id)
);
-- One board column in rank order (kanban read, neighbour lookup, renumber, append).
CREATE INDEX deals_column ON deals (deal_pipeline_status_id, position, id) WHERE deleted_at IS NULL;
-- Same column by deal date: a-z scans it forward, z-a backward (MySQL put NULL dates first).
CREATE INDEX deals_column_date ON deals (deal_pipeline_status_id, date NULLS FIRST, id)
    WHERE deleted_at IS NULL;
-- List (newest first) and its board/column filters; the pipeline summary.
CREATE INDEX deals_live_idx ON deals (workspace_id, id DESC) WHERE deleted_at IS NULL;
CREATE INDEX deals_pipeline_idx ON deals (workspace_id, deal_pipeline_id, deal_pipeline_status_id)
    WHERE deleted_at IS NULL;
-- Relation Deals tab, and the referencing side of the relation FK.
CREATE INDEX deals_relation_idx ON deals (workspace_id, relation_id) WHERE relation_id IS NOT NULL;
-- Referencing side of the currency FK (currency delete probes).
CREATE INDEX deals_currency_idx ON deals (workspace_id, currency_id);
