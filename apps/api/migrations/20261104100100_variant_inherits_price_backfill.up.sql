-- Default sizes whose price was only the copy made with the product follow it from now on, and
-- a stale copy takes the product's current price: the product's only live variant, or a default
-- still at the product price. A default priced apart next to other sizes keeps its own.
UPDATE product_variants v
SET inherits_price = true,
    price = round(p.price, 2),
    original_price = p.price,
    impact_on_price = CASE WHEN p.price > 0 THEN 0 ELSE v.impact_on_price END,
    updated_at = now()
FROM products p
WHERE p.workspace_id = v.workspace_id AND p.id = v.product_id AND p.deleted_at IS NULL
  AND v.is_default AND v.deleted_at IS NULL
  AND (round(v.price, 2) = round(p.price, 2)
       OR NOT EXISTS (SELECT 1 FROM product_variants o
                      WHERE o.workspace_id = v.workspace_id AND o.product_id = v.product_id
                        AND o.id <> v.id AND o.deleted_at IS NULL));
