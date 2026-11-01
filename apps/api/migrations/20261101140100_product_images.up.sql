-- Product images: an ordered list of the workspace's image files per product, each optionally
-- for one variant. The storefront shows them as signed links.
CREATE TABLE product_images (
    id                 uuid        PRIMARY KEY DEFAULT uuidv7(),
    workspace_id       uuid        NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    product_id         uuid        NOT NULL,
    -- Null = the product's own image; else a variant of the same product (checked on write).
    product_variant_id uuid,
    file_id            uuid        NOT NULL,
    position           smallint    NOT NULL CHECK (position BETWEEN 1 AND 20),
    created_at         timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT product_images_product_fk FOREIGN KEY (workspace_id, product_id)
        REFERENCES products (workspace_id, id) ON DELETE CASCADE,
    CONSTRAINT product_images_variant_fk FOREIGN KEY (workspace_id, product_variant_id)
        REFERENCES product_variants (workspace_id, id) ON DELETE CASCADE,
    CONSTRAINT product_images_file_fk FOREIGN KEY (workspace_id, file_id)
        REFERENCES files (workspace_id, id) ON DELETE CASCADE,
    CONSTRAINT product_images_file_once UNIQUE (product_id, file_id),
    CONSTRAINT product_images_position_once UNIQUE (product_id, position)
);
-- A product's images in order (also a page of storefront products).
CREATE INDEX product_images_product_idx ON product_images (workspace_id, product_id, position);
-- FK checks when a file or variant row goes.
CREATE INDEX product_images_file_idx ON product_images (workspace_id, file_id);
CREATE INDEX product_images_variant_idx ON product_images (workspace_id, product_variant_id)
    WHERE product_variant_id IS NOT NULL;
