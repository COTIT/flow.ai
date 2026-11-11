-- "Your Flow": a member's own display settings per scene. Display config only, never
-- business data; vocabulary validated in Rust. At most 20 rows per member per workspace
-- (service, advisory lock).
CREATE TABLE flow_scenes (
    id uuid PRIMARY KEY DEFAULT uuidv7(),
    workspace_id uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    user_id uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    scene text NOT NULL CHECK (scene ~ '^[a-z][a-z0-9_]{0,31}$'),
    position smallint NOT NULL CHECK (position BETWEEN 0 AND 19),
    hidden boolean NOT NULL DEFAULT false,
    schema_version smallint NOT NULL DEFAULT 1 CHECK (schema_version = 1),
    config jsonb NOT NULL CHECK (pg_column_size(config) <= 8192),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (workspace_id, user_id, scene)
);
