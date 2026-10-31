-- Farm tenancy (RUAHA, phase 4). A RUAHA project is a workspace; villages are scopes inside
-- it. A farm membership is a role plus, for farmers and field officers, one village. Staff
-- roles count only while the user is an active workspace member (checked in Rust); farmers
-- are never workspace members, so the rest of the workspace stays closed to them.

CREATE TABLE farm_villages (
    id               uuid         PRIMARY KEY DEFAULT uuidv7(),
    workspace_id     uuid         NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    code             text         NOT NULL CHECK (char_length(code) BETWEEN 1 AND 32),
    name             text         NOT NULL CHECK (char_length(name) BETWEEN 1 AND 120),
    -- GPS point only; polygons wait until someone needs them.
    latitude         numeric(9,6) CHECK (latitude BETWEEN -90 AND 90),
    longitude        numeric(9,6) CHECK (longitude BETWEEN -180 AND 180),
    primary_language text         NOT NULL DEFAULT 'sw' CHECK (primary_language IN ('sw', 'en')),
    created_at       timestamptz  NOT NULL DEFAULT now(),
    updated_at       timestamptz  NOT NULL DEFAULT now(),
    CONSTRAINT farm_villages_code_key UNIQUE (workspace_id, code),
    -- Target of every (workspace_id, village_id) foreign key below and in later farm tables.
    CONSTRAINT farm_villages_workspace_id_id_key UNIQUE (workspace_id, id)
);

CREATE TABLE farm_memberships (
    id           uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    user_id      uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    role         text        NOT NULL CHECK (role IN ('farmer', 'field_officer', 'ops', 'admin')),
    village_id   uuid,
    created_by   uuid        REFERENCES users (id) ON DELETE SET NULL,
    created_at   timestamptz NOT NULL DEFAULT now(),
    -- Never deleted: access ends, the record of it does not.
    revoked_at   timestamptz,
    -- The village must belong to this workspace.
    CONSTRAINT farm_memberships_village_fk FOREIGN KEY (workspace_id, village_id)
        REFERENCES farm_villages (workspace_id, id),
    -- ops and admin cover the whole workspace; farmers and officers exactly one village.
    CONSTRAINT farm_memberships_scope CHECK ((role IN ('ops', 'admin')) = (village_id IS NULL)),
    CONSTRAINT farm_memberships_key UNIQUE NULLS NOT DISTINCT (workspace_id, user_id, role, village_id)
);

-- The guard's lookup on every farm request.
CREATE INDEX farm_memberships_user_idx ON farm_memberships (user_id, workspace_id)
    WHERE revoked_at IS NULL;
CREATE INDEX farm_memberships_village_idx ON farm_memberships (workspace_id, village_id)
    WHERE village_id IS NOT NULL;
CREATE INDEX farm_memberships_created_by_idx ON farm_memberships (created_by)
    WHERE created_by IS NOT NULL;

-- One row per insert/update/delete on a farm table, written by trigger so no write path can
-- skip it. The actor comes from `genesy.actor`, set per transaction by the service.
CREATE TABLE farm_audit_log (
    id           bigint      GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    workspace_id uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    table_name   text        NOT NULL,
    record_id    uuid        NOT NULL,
    action       text        NOT NULL CHECK (action IN ('insert', 'update', 'delete')),
    -- No foreign key: the trail outlives the user.
    actor_id     uuid,
    before       jsonb,
    after        jsonb,
    occurred_at  timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX farm_audit_log_record_idx
    ON farm_audit_log (workspace_id, table_name, record_id, occurred_at DESC);

CREATE FUNCTION farm_audit_log_append_only() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'the farm audit trail cannot be changed'
        USING ERRCODE = 'restrict_violation';
END
$$;

CREATE TRIGGER farm_audit_log_fixed BEFORE UPDATE OR DELETE ON farm_audit_log
    FOR EACH ROW EXECUTE FUNCTION farm_audit_log_append_only();

-- OLD is unassigned on INSERT and NEW on DELETE, so each branch reads only the row that exists.
CREATE FUNCTION farm_write_audit() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    actor uuid := nullif(current_setting('genesy.actor', true), '')::uuid;
BEGIN
    IF TG_OP = 'INSERT' THEN
        INSERT INTO farm_audit_log (workspace_id, table_name, record_id, action, actor_id, after)
        VALUES (NEW.workspace_id, TG_TABLE_NAME, NEW.id, 'insert', actor, to_jsonb(NEW));
        RETURN NEW;
    ELSIF TG_OP = 'UPDATE' THEN
        INSERT INTO farm_audit_log (workspace_id, table_name, record_id, action, actor_id, before, after)
        VALUES (NEW.workspace_id, TG_TABLE_NAME, NEW.id, 'update', actor, to_jsonb(OLD), to_jsonb(NEW));
        RETURN NEW;
    END IF;
    INSERT INTO farm_audit_log (workspace_id, table_name, record_id, action, actor_id, before)
    VALUES (OLD.workspace_id, TG_TABLE_NAME, OLD.id, 'delete', actor, to_jsonb(OLD));
    RETURN OLD;
END
$$;

CREATE TRIGGER farm_villages_audit AFTER INSERT OR UPDATE OR DELETE ON farm_villages
    FOR EACH ROW EXECUTE FUNCTION farm_write_audit();
CREATE TRIGGER farm_memberships_audit AFTER INSERT OR UPDATE OR DELETE ON farm_memberships
    FOR EACH ROW EXECUTE FUNCTION farm_write_audit();
