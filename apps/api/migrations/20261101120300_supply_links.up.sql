-- Phase 4b: consented supply links between two workspaces. The farm workspace grants with an
-- allow-list of what the shop may show publicly; the shop accepts; either side revokes. A
-- revoked link never comes back (grant a new one). Links carry stock later, never records.

CREATE TABLE supply_links (
    id                uuid        PRIMARY KEY DEFAULT uuidv7(),
    from_workspace_id uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    to_workspace_id   uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    -- Public fields the receiving side may show (checked against a fixed list in Rust).
    fields            text[]      NOT NULL DEFAULT '{}',
    granted_by        uuid        NOT NULL REFERENCES users (id),
    granted_at        timestamptz NOT NULL DEFAULT now(),
    accepted_by       uuid        REFERENCES users (id),
    accepted_at       timestamptz,
    revoked_by        uuid        REFERENCES users (id),
    revoked_at        timestamptz,
    CHECK (from_workspace_id <> to_workspace_id),
    CHECK ((accepted_by IS NULL) = (accepted_at IS NULL)),
    CHECK ((revoked_by IS NULL) = (revoked_at IS NULL))
);
-- One open (pending or live) link per pair.
CREATE UNIQUE INDEX supply_links_open_pair ON supply_links (from_workspace_id, to_workspace_id)
    WHERE revoked_at IS NULL;
CREATE INDEX supply_links_to_idx ON supply_links (to_workspace_id, granted_at DESC);
CREATE INDEX supply_links_from_idx ON supply_links (from_workspace_id, granted_at DESC);
CREATE INDEX supply_links_granted_by_idx ON supply_links (granted_by);
CREATE INDEX supply_links_accepted_by_idx ON supply_links (accepted_by) WHERE accepted_by IS NOT NULL;
CREATE INDEX supply_links_revoked_by_idx ON supply_links (revoked_by) WHERE revoked_by IS NOT NULL;
