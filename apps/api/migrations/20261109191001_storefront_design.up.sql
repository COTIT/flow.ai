-- Home designs: relational identities/pointers and immutable presentation documents.
CREATE TABLE storefronts (
 id uuid PRIMARY KEY DEFAULT uuidv7(),
 workspace_id uuid NOT NULL UNIQUE REFERENCES workspaces(id) ON DELETE CASCADE,
 created_at timestamptz NOT NULL DEFAULT now(),
 UNIQUE(workspace_id,id)
);
CREATE TABLE storefront_pages (
 id uuid PRIMARY KEY DEFAULT uuidv7(),
 workspace_id uuid NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
 storefront_id uuid NOT NULL,
 page_key text NOT NULL CHECK(page_key='home'),
 version bigint NOT NULL DEFAULT 0 CHECK(version>=0),
 draft_revision_id uuid,
 published_revision_id uuid,
 updated_at timestamptz NOT NULL DEFAULT now(),
 published_at timestamptz,
 published_by uuid,
 UNIQUE(workspace_id,page_key),
 UNIQUE(workspace_id,id),
 CONSTRAINT storefront_pages_storefront_fk FOREIGN KEY(workspace_id,storefront_id) REFERENCES storefronts(workspace_id,id) ON DELETE CASCADE
);
CREATE TABLE storefront_design_revisions (
 id uuid PRIMARY KEY DEFAULT uuidv7(),
 workspace_id uuid NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
 page_id uuid NOT NULL,
 parent_revision_id uuid,
 restored_from_revision_id uuid,
 schema_version smallint NOT NULL CHECK(schema_version=1),
 -- 64 KiB backstop on jsonb text form; Rust compact 32 KiB check stays authoritative.
 document jsonb NOT NULL CHECK(jsonb_typeof(document)='object' AND octet_length(document::text)<=65536),
 -- Actor/login snapshots deliberately survive user and expired-token cleanup.
 created_by uuid NOT NULL,
 login_id uuid NOT NULL,
 created_at timestamptz NOT NULL DEFAULT now(),
 UNIQUE(workspace_id,page_id,id),
 CONSTRAINT storefront_design_revisions_page_fk FOREIGN KEY(workspace_id,page_id) REFERENCES storefront_pages(workspace_id,id) ON DELETE CASCADE,
 CONSTRAINT storefront_design_revisions_parent_fk FOREIGN KEY(workspace_id,page_id,parent_revision_id) REFERENCES storefront_design_revisions(workspace_id,page_id,id) DEFERRABLE INITIALLY DEFERRED,
 CONSTRAINT storefront_design_revisions_restored_from_fk FOREIGN KEY(workspace_id,page_id,restored_from_revision_id) REFERENCES storefront_design_revisions(workspace_id,page_id,id) DEFERRABLE INITIALLY DEFERRED
);
ALTER TABLE storefront_pages
 ADD CONSTRAINT storefront_pages_draft_revision_fk FOREIGN KEY(workspace_id,id,draft_revision_id) REFERENCES storefront_design_revisions(workspace_id,page_id,id) DEFERRABLE INITIALLY DEFERRED,
 ADD CONSTRAINT storefront_pages_published_revision_fk FOREIGN KEY(workspace_id,id,published_revision_id) REFERENCES storefront_design_revisions(workspace_id,page_id,id) DEFERRABLE INITIALLY DEFERRED;
CREATE INDEX storefront_design_revisions_history_idx ON storefront_design_revisions(workspace_id,page_id,created_at DESC,id DESC);
CREATE TABLE storefront_design_events (
 id uuid PRIMARY KEY DEFAULT uuidv7(),
 workspace_id uuid NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
 page_id uuid NOT NULL,
 revision_id uuid NOT NULL,
 previous_draft_revision_id uuid,
 previous_published_revision_id uuid,
 page_version bigint NOT NULL CHECK(page_version>0),
 kind text NOT NULL CHECK(kind IN ('saved','restored','published')),
 actor_id uuid NOT NULL,
 login_id uuid NOT NULL,
 created_at timestamptz NOT NULL DEFAULT now(),
 UNIQUE(workspace_id,page_id,page_version),
 CONSTRAINT storefront_design_events_page_fk FOREIGN KEY(workspace_id,page_id) REFERENCES storefront_pages(workspace_id,id) ON DELETE CASCADE,
 CONSTRAINT storefront_design_events_revision_fk FOREIGN KEY(workspace_id,page_id,revision_id) REFERENCES storefront_design_revisions(workspace_id,page_id,id) DEFERRABLE INITIALLY DEFERRED
);
CREATE INDEX storefront_design_events_revision_idx ON storefront_design_events(workspace_id,page_id,revision_id,kind);
CREATE FUNCTION storefront_design_fixed() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 -- Preserve existing workspace cascade deletion; individual history edits stay forbidden.
 IF TG_OP='DELETE' THEN
  IF NOT EXISTS(SELECT 1 FROM workspaces WHERE id=OLD.workspace_id) THEN
   RETURN OLD;
  END IF;
 END IF;
 RAISE EXCEPTION 'Storefront design history is immutable.' USING ERRCODE='23514';
END $$;
CREATE TRIGGER storefront_design_revisions_fixed BEFORE UPDATE OR DELETE ON storefront_design_revisions
 FOR EACH ROW EXECUTE FUNCTION storefront_design_fixed();
CREATE TRIGGER storefront_design_events_fixed BEFORE UPDATE OR DELETE ON storefront_design_events
 FOR EACH ROW EXECUTE FUNCTION storefront_design_fixed();
CREATE TRIGGER storefront_design_revisions_no_truncate BEFORE TRUNCATE ON storefront_design_revisions
 FOR EACH STATEMENT EXECUTE FUNCTION storefront_design_fixed();
CREATE TRIGGER storefront_design_events_no_truncate BEFORE TRUNCATE ON storefront_design_events
 FOR EACH STATEMENT EXECUTE FUNCTION storefront_design_fixed();
