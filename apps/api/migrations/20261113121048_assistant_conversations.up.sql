-- Durable dialogue belongs to member, separate from login-bound execution controls.
ALTER TABLE assistant_sessions ADD COLUMN cleanup_attempted_at timestamptz;
CREATE INDEX assistant_sessions_cleanup_fair_idx ON assistant_sessions(cleanup_attempted_at NULLS FIRST,last_seen_at,id) WHERE state<>'cleaned';
CREATE TABLE assistant_conversations (
 id uuid PRIMARY KEY DEFAULT uuidv7(),
 workspace_id uuid NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
 user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
 title text NOT NULL DEFAULT '' CHECK (octet_length(title)<=200),
 state text NOT NULL DEFAULT 'active' CHECK (state IN ('active','ended','deleted')),
 revision bigint NOT NULL DEFAULT 1 CHECK (revision>0),
 created_at timestamptz NOT NULL DEFAULT now(),
 last_activity_at timestamptz NOT NULL DEFAULT now(),
 expires_at timestamptz NOT NULL DEFAULT now()+interval '90 days',
 deleted_at timestamptz,
 UNIQUE(workspace_id,id)
);
CREATE INDEX assistant_conversations_owner_idx ON assistant_conversations(workspace_id,user_id,last_activity_at DESC,id DESC) WHERE state<>'deleted';
CREATE INDEX assistant_conversations_expiry_idx ON assistant_conversations(expires_at) WHERE state<>'deleted';
CREATE TABLE assistant_conversation_sources (
 id uuid PRIMARY KEY DEFAULT uuidv7(),
 workspace_id uuid NOT NULL,
 conversation_id uuid NOT NULL,
 session_id uuid NOT NULL UNIQUE,
 login_id uuid NOT NULL,
 provider_conversation_id text NOT NULL UNIQUE CHECK (provider_conversation_id ~ '^[A-Za-z0-9_-]{1,200}$'),
 agent_id text NOT NULL CHECK (agent_id ~ '^[A-Za-z0-9_-]{1,200}$'),
 text_only boolean NOT NULL,
 ordinal bigint NOT NULL CHECK (ordinal>0),
 revision bigint NOT NULL DEFAULT 1,
 state text NOT NULL DEFAULT 'provisional' CHECK (state IN ('provisional','final','partial')),
 summary text CHECK (octet_length(summary)<=16384),
 final_sha256 bytea CHECK (octet_length(final_sha256)=32),
 remote_deleted_at timestamptz,
 last_reconcile_attempt_at timestamptz,
 created_at timestamptz NOT NULL DEFAULT now(),
 FOREIGN KEY(workspace_id,conversation_id) REFERENCES assistant_conversations(workspace_id,id) ON DELETE CASCADE,
 UNIQUE(workspace_id,conversation_id,id),
 UNIQUE(workspace_id,conversation_id,ordinal)
);
CREATE INDEX assistant_conversation_sources_cleanup_idx ON assistant_conversation_sources(workspace_id,session_id) WHERE remote_deleted_at IS NULL;
CREATE INDEX assistant_conversation_sources_reconcile_idx ON assistant_conversation_sources(last_reconcile_attempt_at NULLS FIRST,created_at,id) WHERE state<>'final' AND remote_deleted_at IS NULL;
CREATE TABLE assistant_conversation_turns (
 id uuid PRIMARY KEY DEFAULT uuidv7(),
 workspace_id uuid NOT NULL,
 conversation_id uuid NOT NULL,
 source_id uuid NOT NULL,
 key text NOT NULL CHECK (octet_length(key) BETWEEN 1 AND 256),
 ordinal bigint NOT NULL CHECK (ordinal>=0),
 role text NOT NULL CHECK (role IN ('user','agent')),
 text text NOT NULL CHECK (octet_length(text)<=16384),
 modality text NOT NULL CHECK (modality IN ('text','voice')),
 status text NOT NULL CHECK (status IN ('queued','sent_unconfirmed','complete')),
 interrupted boolean NOT NULL DEFAULT false,
 provenance text NOT NULL CHECK (provenance IN ('provisional','final')),
 FOREIGN KEY(workspace_id,conversation_id,source_id) REFERENCES assistant_conversation_sources(workspace_id,conversation_id,id) ON DELETE CASCADE,
 UNIQUE(workspace_id,source_id,key,provenance),
 UNIQUE(workspace_id,source_id,ordinal,provenance)
);
CREATE INDEX assistant_conversation_turns_page_idx ON assistant_conversation_turns(workspace_id,conversation_id,source_id,ordinal,id);
CREATE INDEX assistant_conversation_turns_search_idx ON assistant_conversation_turns USING gin(text gin_trgm_ops);
