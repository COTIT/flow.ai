-- Deal pipelines (Laravel CRM/DealPipeline): sales boards, and their columns. A column is a
-- `deal_pipeline` custom status at a position, with lane rules and won/lost flags. Laravel kept
-- rules and properties as JSON; here they are columns so the one-won/one-lost rule is an index.

CREATE TABLE deal_pipelines (
    id           uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    name         text        NOT NULL CHECK (char_length(name) <= 255),
    type         text        CHECK (char_length(type) <= 255),
    -- Order of deals in a column: a-z / z-a by deal date, pos by deal position (SortingEnum).
    sorting      text        NOT NULL DEFAULT 'a-z' CHECK (sorting IN ('a-z', 'pos', 'z-a')),
    meta         jsonb       CHECK (meta IS NULL OR jsonb_typeof(meta) = 'object'),
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now(),
    deleted_at   timestamptz,
    CONSTRAINT deal_pipelines_workspace_id_id_key UNIQUE (workspace_id, id)
);

CREATE TABLE deal_pipeline_statuses (
    id                   uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id         uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    deal_pipeline_id     uuid        NOT NULL,
    custom_status_id     uuid        NOT NULL,
    -- 0-based column order; writers keep it distinct per pipeline under the pipeline row lock.
    position             integer     NOT NULL CHECK (position >= 0),
    max_items_per_lane   integer     NOT NULL,
    max_days_in_lane     integer     NOT NULL,
    auto_archive_in_days integer     NOT NULL,
    lane_color           text        NOT NULL CHECK (char_length(lane_color) <= 255),
    deal_probability     text        NOT NULL CHECK (char_length(deal_probability) <= 255),
    is_won               boolean     NOT NULL,
    is_lost              boolean     NOT NULL,
    meta                 jsonb       CHECK (meta IS NULL OR jsonb_typeof(meta) = 'object'),
    created_at           timestamptz NOT NULL DEFAULT now(),
    updated_at           timestamptz NOT NULL DEFAULT now(),
    deleted_at           timestamptz,
    CONSTRAINT deal_pipeline_statuses_workspace_id_id_key UNIQUE (workspace_id, id),
    -- Target of the deals FK that keeps a deal's status inside the deal's pipeline.
    CONSTRAINT deal_pipeline_statuses_pipeline_key UNIQUE (workspace_id, deal_pipeline_id, id),
    CONSTRAINT deal_pipeline_statuses_won_or_lost CHECK (NOT (is_won AND is_lost)),
    CONSTRAINT deal_pipeline_statuses_pipeline_fk FOREIGN KEY (workspace_id, deal_pipeline_id)
        REFERENCES deal_pipelines (workspace_id, id),
    CONSTRAINT deal_pipeline_statuses_custom_status_fk FOREIGN KEY (workspace_id, custom_status_id)
        REFERENCES custom_statuses (workspace_id, id)
);
-- A board's columns in order (pipeline reads, position shifts).
CREATE INDEX deal_pipeline_statuses_by_pipeline
    ON deal_pipeline_statuses (deal_pipeline_id, position, id) WHERE deleted_at IS NULL;
-- Laravel StatusExistInDealPipeline and ValidateStatusProperties, as constraints.
CREATE UNIQUE INDEX deal_pipeline_statuses_custom_status_once
    ON deal_pipeline_statuses (deal_pipeline_id, custom_status_id) WHERE deleted_at IS NULL;
CREATE UNIQUE INDEX deal_pipeline_statuses_one_won
    ON deal_pipeline_statuses (deal_pipeline_id) WHERE is_won AND deleted_at IS NULL;
CREATE UNIQUE INDEX deal_pipeline_statuses_one_lost
    ON deal_pipeline_statuses (deal_pipeline_id) WHERE is_lost AND deleted_at IS NULL;
-- Referencing side of the custom status FK.
CREATE INDEX deal_pipeline_statuses_custom_status_idx
    ON deal_pipeline_statuses (workspace_id, custom_status_id);
