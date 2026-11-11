-- A public Shop URL owns one immutable workspace and one stable, fixed-scope actor.
CREATE TABLE storefront_connections (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL UNIQUE REFERENCES workspaces(id) ON DELETE CASCADE,
    active boolean NOT NULL DEFAULT true,
    created_by uuid REFERENCES users(id) ON DELETE SET NULL,
    updated_by uuid REFERENCES users(id) ON DELETE SET NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE(workspace_id, id),
    FOREIGN KEY(workspace_id, id) REFERENCES app_keys(workspace_id, id) ON DELETE CASCADE
);

CREATE FUNCTION storefront_connection_identity() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.id IS DISTINCT FROM OLD.id OR NEW.workspace_id IS DISTINCT FROM OLD.workspace_id THEN
        RAISE EXCEPTION 'A Shop connection cannot change its workspace or actor'
            USING ERRCODE = 'restrict_violation';
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER storefront_connection_identity BEFORE UPDATE ON storefront_connections
    FOR EACH ROW EXECUTE FUNCTION storefront_connection_identity();

CREATE TABLE storefront_connection_events (
    id uuid PRIMARY KEY DEFAULT uuidv7(),
    workspace_id uuid NOT NULL,
    connection_id uuid NOT NULL,
    active boolean NOT NULL,
    user_id uuid REFERENCES users(id) ON DELETE SET NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    FOREIGN KEY(workspace_id, connection_id) REFERENCES storefront_connections(workspace_id, id)
        ON DELETE CASCADE
);
CREATE INDEX storefront_connection_events_connection_idx
    ON storefront_connection_events(workspace_id, connection_id, created_at);
