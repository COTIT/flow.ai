-- Fixed reviewed batches retain original action ledger as execution authority.
CREATE TABLE assistant_batches (
 id uuid PRIMARY KEY DEFAULT uuidv7(),
 workspace_id uuid NOT NULL,
 session_id uuid NOT NULL,
 user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
 login_id uuid NOT NULL,
 protocol_version smallint NOT NULL DEFAULT 1 CHECK(protocol_version=1),
 fingerprint bytea NOT NULL CHECK(octet_length(fingerprint)=32),
 action_count smallint NOT NULL CHECK(action_count BETWEEN 2 AND 10),
 next_index smallint NOT NULL DEFAULT 0 CHECK(next_index BETWEEN 0 AND action_count),
 state text NOT NULL DEFAULT 'proposed' CHECK(state IN ('proposed','approved','running','paused','completed','cancelled','expired')),
 reason text CHECK(reason IN ('disconnected','changed_intent','unknown','failed','partial','queued','expired','cancelled','authorization','semantic_failure','interrupted')),
 approval_event_sha256 text CHECK(approval_event_sha256 ~ '^[0-9a-f]{64}$'),
 run_hash bytea CHECK(run_hash IS NULL OR octet_length(run_hash)=32),
 run_expires_at timestamptz,
 created_at timestamptz NOT NULL DEFAULT now(),
 expires_at timestamptz NOT NULL DEFAULT now()+interval '120 seconds',
 approved_at timestamptz,
 started_at timestamptz,
 finished_at timestamptz,
 FOREIGN KEY(workspace_id,session_id) REFERENCES assistant_sessions(workspace_id,id),
 UNIQUE(workspace_id,id),
 UNIQUE(session_id,approval_event_sha256)
);
CREATE INDEX assistant_batches_owner_idx ON assistant_batches(workspace_id,user_id,login_id,id);
CREATE INDEX assistant_batches_session_idx ON assistant_batches(workspace_id,session_id,created_at DESC);
CREATE INDEX assistant_batches_expiry_idx ON assistant_batches(expires_at) WHERE state IN ('proposed','approved','running');
CREATE INDEX assistant_batches_retention_idx ON assistant_batches(finished_at) WHERE state IN ('completed','cancelled','expired','paused');
ALTER TABLE assistant_actions ADD COLUMN batch_id uuid;
ALTER TABLE assistant_actions ADD COLUMN batch_index smallint CHECK(batch_index BETWEEN 0 AND 9);
ALTER TABLE assistant_actions ADD COLUMN batch_verdict text CHECK(batch_verdict IN ('completed','unknown','failed','partial','queued','cancelled'));
ALTER TABLE assistant_actions ADD CONSTRAINT assistant_actions_batch_fk FOREIGN KEY(workspace_id,batch_id) REFERENCES assistant_batches(workspace_id,id);
ALTER TABLE assistant_actions ADD CONSTRAINT assistant_actions_batch_position_check CHECK((batch_id IS NULL)=(batch_index IS NULL));
ALTER TABLE assistant_actions ADD CONSTRAINT assistant_actions_batch_verdict_scope_check CHECK(batch_verdict IS NULL OR batch_id IS NOT NULL);
CREATE UNIQUE INDEX assistant_actions_batch_idx ON assistant_actions(workspace_id,batch_id,batch_index) WHERE batch_id IS NOT NULL;
-- Old application versions never approve/claim batch_pending, even during rolling updates.
ALTER TABLE assistant_actions DROP CONSTRAINT assistant_actions_state_check;
ALTER TABLE assistant_actions ADD CONSTRAINT assistant_actions_state_check CHECK(state IN ('proposed','approved','batch_pending','dispatching','completed','unknown','cancelled','expired','failed','queued','partial'));
ALTER TABLE assistant_actions DROP CONSTRAINT assistant_actions_check;
ALTER TABLE assistant_actions ADD CONSTRAINT assistant_actions_proposal_lifetime_check CHECK(state IN ('proposed','approved','batch_pending') OR proposal IS NULL);
