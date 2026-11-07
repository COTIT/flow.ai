-- One provider final-user event can approve only one action in its session.
ALTER TABLE assistant_actions ADD COLUMN approval_event_sha256 text CHECK (approval_event_sha256 ~ '^[0-9a-f]{64}$');
CREATE UNIQUE INDEX assistant_actions_approval_event_idx ON assistant_actions(session_id,approval_event_sha256) WHERE approval_event_sha256 IS NOT NULL;
