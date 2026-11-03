//! Storefront SQL: one statement per page or product; store-only products never show. Prices become integer minor units of the
//! product's currency; available stock sums every live stock row.

use serde::Deserialize;
use sqlx::{PgExecutor, types::Json};
use uuid::Uuid;

/// A sellable product with its variants, as one row.
pub struct ProductRow {
    pub id: Uuid,
    pub name: String,
    pub brand: Option<String>,
    pub r#type: String,
    pub short_description: Option<String>,
    pub description: Option<String>,
    pub ean13: Option<String>,
    pub barcode: Option<String>,
    pub price_gross: i64,
    pub price_net: i64,
    pub currency: String,
    pub currency_decimals: i16,
    pub available_stock: Option<String>,
    pub labels: Vec<String>,
    /// Public code of the published lot FEFO sells next, if any.
    pub passport_code: Option<String>,
    pub variants: Json<Vec<VariantRow>>,
    pub total: i64,
}

/// A live variant, built in SQL as JSON.
#[derive(Deserialize)]
pub struct VariantRow {
    pub id: Uuid,
    pub sku: Option<String>,
    pub barcode: Option<String>,
    pub ean13: Option<String>,
    pub is_default: bool,
    pub price_gross: i64,
    pub price_net: i64,
    pub options: Vec<OptionRow>,
}

/// One option of a variant (`Size: 500 g`).
#[derive(Deserialize)]
pub struct OptionRow {
    pub r#type: String,
    pub value: String,
}

/// Sellable web products of the workspace (active, not archived, not deleted, not store-only),
/// by id; `only` narrows to one. `total` on each row counts every match.
pub async fn products(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    only: Option<Uuid>,
    limit: i64,
    offset: i64,
) -> Result<Vec<ProductRow>, sqlx::Error> {
    sqlx::query_as!(
        ProductRow,
        r#"WITH page AS (
               SELECT p.id, p.name, p.brand, p.type, p.short_description, p.description,
                      p.ean13, p.barcode, p.price, p.gross_price, p.has_stock, p.currency_id,
                      p.labels,
                      count(*) OVER () AS total
               FROM products p
               WHERE p.workspace_id = $1 AND p.deleted_at IS NULL
                 AND p.status = 'active' AND NOT p.archived AND NOT p.store_only
                 AND ($2::uuid IS NULL OR p.id = $2)
               ORDER BY p.id
               LIMIT $3 OFFSET $4
           )
           SELECT pg.id, pg.name, pg.brand, pg.type, pg.short_description, pg.description,
                  pg.ean13, pg.barcode,
                  round(pg.gross_price * 10::numeric ^ c.decimals)::bigint AS "price_gross!",
                  round(pg.price * 10::numeric ^ c.decimals)::bigint AS "price_net!",
                  upper(c.asset_type) AS "currency!", c.decimals AS currency_decimals,
                  CASE WHEN pg.has_stock THEN (
                      SELECT coalesce(sum(s.stock_lvl - s.reserved), 0.00::numeric(19, 2))::text
                      FROM stocks s
                      LEFT JOIN lots l ON l.id = s.lot_id
                      WHERE s.workspace_id = $1 AND s.product_id = pg.id AND s.deleted_at IS NULL
                        AND (s.lot_id IS NULL OR (l.status = 'released'
                             AND (l.expires_on IS NULL OR l.expires_on >= current_date)))
                  ) END AS available_stock,
                  pg.labels,
                  (SELECT l.public_code FROM lots l
                   WHERE l.workspace_id = $1 AND l.product_id = pg.id
                     AND l.published_at IS NOT NULL AND l.status = 'released'
                     AND (l.expires_on IS NULL OR l.expires_on >= current_date)
                     AND EXISTS (SELECT 1 FROM stocks s
                                 WHERE s.workspace_id = $1 AND s.lot_id = l.id
                                   AND s.deleted_at IS NULL AND s.stock_lvl - s.reserved > 0)
                   ORDER BY least(l.expires_on, l.best_before) NULLS LAST, l.id
                   LIMIT 1) AS passport_code,
                  (SELECT coalesce(json_agg(json_build_object(
                              'id', v.id, 'sku', v.sku, 'barcode', v.barcode, 'ean13', v.ean13,
                              'is_default', v.is_default,
                              'price_gross', round(v.gross_price * 10::numeric ^ c.decimals)::bigint,
                              'price_net', round(v.price * 10::numeric ^ c.decimals)::bigint,
                              'options', (
                                  SELECT coalesce(json_agg(json_build_object(
                                             'type', ot.display_name, 'value', ov.display_value)
                                         ORDER BY ot.display_name, ov.sort_order, ov.id), '[]')
                                  FROM variant_option_values vov
                                  JOIN option_values ov ON ov.id = vov.option_value_id
                                       AND ov.workspace_id = $1 AND ov.deleted_at IS NULL
                                  JOIN option_types ot ON ot.id = ov.option_type_id
                                       AND ot.workspace_id = $1 AND ot.deleted_at IS NULL
                                  WHERE vov.workspace_id = $1 AND vov.product_variant_id = v.id))
                          ORDER BY v.is_default DESC, v.id), '[]')
                   FROM product_variants v
                   WHERE v.workspace_id = $1 AND v.product_id = pg.id AND v.deleted_at IS NULL
                  ) AS "variants!: Json<Vec<VariantRow>>",
                  pg.total AS "total!"
           FROM page pg
           JOIN currencies c ON c.workspace_id = $1 AND c.id = pg.currency_id
           ORDER BY pg.id"#,
        workspace_id,
        only,
        limit,
        offset,
    )
    .fetch_all(db)
    .await
}

/// Sellable products of the workspace (page past the end).
pub async fn count(db: impl PgExecutor<'_>, workspace_id: Uuid) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM products
           WHERE workspace_id = $1 AND deleted_at IS NULL AND status = 'active' AND NOT archived
             AND NOT store_only"#,
        workspace_id,
    )
    .fetch_one(db)
    .await
}
