-- Teams carry permissions; a member's permissions = union over their live teams.
-- Laravel mirrors each team into a spatie role 1:1; team_permissions replaces both.

CREATE TABLE teams (
    id           uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    -- Same caps as Laravel StoreTeamRequest.
    name         text        NOT NULL CHECK (char_length(name) <= 55),
    description  text        NOT NULL CHECK (char_length(description) <= 225),
    meta         jsonb,
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now(),
    deleted_at   timestamptz
);
CREATE INDEX teams_workspace_idx ON teams (workspace_id) WHERE deleted_at IS NULL;

-- Names checked against the catalog in code; stored lowercase so lookups are exact.
CREATE TABLE team_permissions (
    team_id    uuid NOT NULL REFERENCES teams (id) ON DELETE CASCADE,
    permission text NOT NULL CHECK (permission = lower(permission)),
    PRIMARY KEY (team_id, permission)
);

CREATE TABLE team_members (
    team_id    uuid        NOT NULL REFERENCES teams (id) ON DELETE CASCADE,
    user_id    uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (team_id, user_id)
);
-- /me and the permission guard look up a user's teams.
CREATE INDEX team_members_user_idx ON team_members (user_id);

-- Accepting the invite adds the user to this team (Laravel AnswerInviteController).
ALTER TABLE invites ADD COLUMN team_id uuid REFERENCES teams (id) ON DELETE SET NULL;
