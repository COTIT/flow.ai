-- Login-bound assistant sessions and once-only dispatch claims. No tokens or chat stored.
CREATE TABLE assistant_sessions (
    id uuid PRIMARY KEY DEFAULT uuidv7(),
    workspace_id uuid NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    login_id uuid NOT NULL,
    language text NOT NULL CHECK (language IN ('en','nl','sw')),
    state text NOT NULL DEFAULT 'active' CHECK (state IN ('active','closed','cleaned')),
    conversation_id text UNIQUE CHECK (conversation_id ~ '^[A-Za-z0-9_-]{1,200}$'),
    created_at timestamptz NOT NULL DEFAULT now(),
    last_seen_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL DEFAULT now() + interval '1 hour',
    closed_at timestamptz,
    cleaned_at timestamptz,
    UNIQUE (workspace_id, id)
);
CREATE INDEX assistant_sessions_owner_idx ON assistant_sessions(workspace_id,user_id,login_id,id);
CREATE INDEX assistant_sessions_cleanup_idx ON assistant_sessions(last_seen_at,expires_at) WHERE state <> 'cleaned';

CREATE TABLE assistant_actions (
    id uuid PRIMARY KEY DEFAULT uuidv7(),
    workspace_id uuid NOT NULL,
    session_id uuid NOT NULL,
    user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    login_id uuid NOT NULL,
    operation_id text NOT NULL CHECK (char_length(operation_id) BETWEEN 1 AND 100),
    step_index bigint NOT NULL CHECK (step_index > 0),
    request_hash bytea NOT NULL CHECK (octet_length(request_hash)=32),
    proposal jsonb CHECK (proposal IS NULL OR octet_length(proposal::text) <= 49152),
    state text NOT NULL DEFAULT 'proposed' CHECK (state IN ('proposed','approved','dispatching','completed','unknown','cancelled','expired','failed')),
    created_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL DEFAULT now() + interval '120 seconds',
    approved_at timestamptz,
    dispatched_at timestamptz,
    finished_at timestamptz,
    http_status integer CHECK (http_status BETWEEN 100 AND 599),
    evidence jsonb NOT NULL DEFAULT '{}'::jsonb CHECK (octet_length(evidence::text) <= 8192),
    FOREIGN KEY (workspace_id,session_id) REFERENCES assistant_sessions(workspace_id,id),
    UNIQUE (workspace_id,session_id,step_index),
    CHECK (state IN ('proposed','approved') OR proposal IS NULL)
);
CREATE INDEX assistant_actions_owner_idx ON assistant_actions(workspace_id,user_id,login_id,id);
CREATE INDEX assistant_actions_pending_idx ON assistant_actions(expires_at) WHERE state IN ('proposed','approved');
CREATE INDEX assistant_actions_retention_idx ON assistant_actions(finished_at) WHERE state IN ('completed','cancelled','expired','failed');
CREATE INDEX assistant_actions_unresolved_idx ON assistant_actions(dispatched_at) WHERE state = 'dispatching';

CREATE TABLE assistant_uploads (
    id uuid PRIMARY KEY DEFAULT uuidv7(),
    workspace_id uuid NOT NULL,
    session_id uuid NOT NULL,
    media_type text NOT NULL CHECK (media_type IN ('application/pdf','image/jpeg','image/png','image/webp','text/plain','text/csv','application/vnd.openxmlformats-officedocument.spreadsheetml.sheet')),
    size_bytes bigint NOT NULL CHECK (size_bytes BETWEEN 1 AND 20971520),
    file_id text UNIQUE CHECK (file_id ~ '^[A-Za-z0-9_-]{1,200}$'),
    state text NOT NULL DEFAULT 'reserved' CHECK (state IN ('reserved','uploaded','cleanup_pending','cancelled','cleaned')),
    created_at timestamptz NOT NULL DEFAULT now(),
    FOREIGN KEY (workspace_id,session_id) REFERENCES assistant_sessions(workspace_id,id)
);
CREATE INDEX assistant_uploads_session_idx ON assistant_uploads(workspace_id,session_id,state);
