ALTER TABLE storefront_pages DROP CONSTRAINT storefront_pages_draft_revision_fk;
ALTER TABLE storefront_pages DROP CONSTRAINT storefront_pages_published_revision_fk;
DROP TABLE storefront_design_events;
DROP TABLE storefront_design_revisions;
DROP FUNCTION storefront_design_fixed();
DROP TABLE storefront_pages;
DROP TABLE storefronts;
