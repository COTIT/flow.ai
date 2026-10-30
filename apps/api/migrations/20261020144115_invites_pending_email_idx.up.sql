-- GET /v1/me lists pending invites by lower(email) across all workspaces; the existing
-- invites_one_pending_per_email index leads with workspace_id, so it can't serve that lookup.
CREATE INDEX invites_pending_email_idx ON invites (lower(email)) WHERE status = 'pending';
