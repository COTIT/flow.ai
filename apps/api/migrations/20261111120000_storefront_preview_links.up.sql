-- Expiring full-page preview links: one unpublished revision each, token stored as sha256 only.
CREATE TABLE storefront_preview_links (
 id uuid PRIMARY KEY DEFAULT uuidv7(),
 workspace_id uuid NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
 page_id uuid NOT NULL,
 revision_id uuid NOT NULL,
 token_hash bytea NOT NULL UNIQUE CHECK(octet_length(token_hash)=32),
 -- Actor/login snapshots survive user and token cleanup: the row is the audit record.
 created_by uuid NOT NULL,
 login_id uuid NOT NULL,
 created_at timestamptz NOT NULL DEFAULT now(),
 expires_at timestamptz NOT NULL,
 revoked_at timestamptz,
 revoked_by uuid,
 revoked_login_id uuid,
 CHECK(expires_at > created_at AND expires_at <= created_at + interval '168 hours'),
 CHECK((revoked_at IS NULL) = (revoked_by IS NULL) AND (revoked_at IS NULL) = (revoked_login_id IS NULL)),
 CONSTRAINT storefront_preview_links_revision_fk FOREIGN KEY(workspace_id,page_id,revision_id) REFERENCES storefront_design_revisions(workspace_id,page_id,id)
);
CREATE INDEX storefront_preview_links_revision_idx ON storefront_preview_links(workspace_id,revision_id,created_at DESC,id DESC);
CREATE FUNCTION storefront_preview_link_fixed() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='DELETE' THEN
  -- Workspace cascade only; single links are revoked, never deleted.
  IF NOT EXISTS(SELECT 1 FROM workspaces WHERE id=OLD.workspace_id) THEN
   RETURN OLD;
  END IF;
 ELSIF OLD.revoked_at IS NULL AND NEW.revoked_at IS NOT NULL
  AND (NEW.id,NEW.workspace_id,NEW.page_id,NEW.revision_id,NEW.token_hash,NEW.created_by,NEW.login_id,NEW.created_at,NEW.expires_at)
   IS NOT DISTINCT FROM (OLD.id,OLD.workspace_id,OLD.page_id,OLD.revision_id,OLD.token_hash,OLD.created_by,OLD.login_id,OLD.created_at,OLD.expires_at) THEN
  RETURN NEW;
 END IF;
 RAISE EXCEPTION 'Storefront preview links only change by one revoke.' USING ERRCODE='23514';
END $$;
CREATE TRIGGER storefront_preview_links_fixed BEFORE UPDATE OR DELETE ON storefront_preview_links
 FOR EACH ROW EXECUTE FUNCTION storefront_preview_link_fixed();
