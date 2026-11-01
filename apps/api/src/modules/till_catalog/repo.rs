//! Till catalog SQL: one statement per page. Prices become integer minor units of the
//! register's currency; stock sums the register warehouse's usable rows.

use chrono::{DateTime, Utc};
use serde::Deserialize;
use sqlx::{PgExecutor, types::Json};
use uuid::Uuid;

use crate::modules::storefront::repo::OptionRow;

/// One product as the till sees it.
pub struct ProductRow {
    pub id: Uuid,
    pub name: String,
    pub r#type: String,
    pub ean13: Option<String>,
    pub barcode: Option<String>,
    pub store_only: bool,
    pub has_stock: bool,
    pub sellable: bool,
    pub vat_rate: String,
    pub unit_price: i64,
    pub stock: Option<String>,
    pub variants: Json<Vec<VariantRow>>,
    pub updated_at: DateTime<Utc>,
    pub total: i64,
}

/// A live variant, built in SQL as JSON.
#[derive(Deserialize)]
pub struct VariantRow {
    pub id: Uuid,
    pub sku: Option<String>,
    pub ean13: Option<String>,
    pub barcode: Option<String>,
    pub is_default: bool,
    pub unit_price: i64,
    pub options: Vec<OptionRow>,
}

/// What to read: the register's workspace, currency and warehouse, plus the filters.
pub struct Filter<'a> {
    pub workspace_id: Uuid,
    pub currency_id: Uuid,
    pub warehouse_id: Uuid,
    /// None = sellable products only; Some = everything changed after it, sellable or not.
    pub since: Option<DateTime<Utc>>,
    /// Exact product or variant EAN-13 or barcode, any case.
    pub code: Option<&'a str>,
}

/// A page of the register's catalog by id. Products in another currency are left out (a sale
/// refuses them). `total` on each row counts every match.
pub async fn products(
    db: impl PgExecutor<'_>,
    f: &Filter<'_>,
    limit: i64,
    offset: i64,
) -> Result<Vec<ProductRow>, sqlx::Error> {
    sqlx::query_as!(
        ProductRow,
        r#"WITH page AS (
               SELECT p.id, p.name, p.type, p.ean13, p.barcode, p.store_only, p.has_stock,
                      p.price, p.vat_group_id, p.updated_at,
                      (p.deleted_at IS NULL AND p.status = 'active' AND NOT p.archived)
                        AS sellable,
                      count(*) OVER () AS total
               FROM products p
               WHERE p.workspace_id = $1 AND p.currency_id = $2
                 AND CASE WHEN $4::timestamptz IS NULL
                          THEN p.deleted_at IS NULL AND p.status = 'active' AND NOT p.archived
                          ELSE p.updated_at > $4
                               OR EXISTS (SELECT 1 FROM product_variants v
                                          WHERE v.workspace_id = $1 AND v.product_id = p.id
                                            AND v.updated_at > $4)
                               OR EXISTS (SELECT 1 FROM vat_groups g
                                          WHERE g.workspace_id = $1 AND g.id = p.vat_group_id
                                            AND g.updated_at > $4)
                               OR EXISTS (SELECT 1 FROM stocks s
                                          LEFT JOIN lots l ON l.id = s.lot_id
                                          WHERE s.workspace_id = $1 AND s.product_id = p.id
                                            AND s.warehouse_id = $3
                                            AND (s.updated_at > $4 OR l.updated_at > $4))
                     END
                 AND ($5::text IS NULL OR lower(p.ean13) = lower($5)
                      OR lower(p.barcode) = lower($5)
                      OR EXISTS (SELECT 1 FROM product_variants v
                                 WHERE v.workspace_id = $1 AND v.product_id = p.id
                                   AND v.deleted_at IS NULL
                                   AND (lower(v.ean13) = lower($5)
                                        OR lower(v.barcode) = lower($5))))
               ORDER BY p.id
               LIMIT $6 OFFSET $7
           )
           SELECT pg.id, pg.name, pg.type, pg.ean13, pg.barcode, pg.store_only, pg.has_stock,
                  pg.sellable AS "sellable!",
                  trim_scale(coalesce(g.vat_amount, 0))::text AS "vat_rate!",
                  round(pg.price * 10::numeric ^ c.decimals)::bigint AS "unit_price!",
                  CASE WHEN pg.has_stock THEN (
                      SELECT coalesce(sum(s.stock_lvl - s.reserved), 0.00::numeric(19, 2))::text
                      FROM stocks s
                      LEFT JOIN lots l ON l.id = s.lot_id
                      WHERE s.workspace_id = $1 AND s.product_id = pg.id
                        AND s.warehouse_id = $3 AND s.deleted_at IS NULL
                        AND (s.lot_id IS NULL OR (l.status = 'released'
                             AND (l.expires_on IS NULL OR l.expires_on >= current_date)))
                  ) END AS stock,
                  (SELECT coalesce(json_agg(json_build_object(
                              'id', v.id, 'sku', v.sku, 'ean13', v.ean13, 'barcode', v.barcode,
                              'is_default', v.is_default,
                              'unit_price', round(v.price * 10::numeric ^ c.decimals)::bigint,
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
                  pg.updated_at,
                  pg.total AS "total!"
           FROM page pg
           JOIN currencies c ON c.workspace_id = $1 AND c.id = $2
           LEFT JOIN vat_groups g ON g.workspace_id = $1 AND g.id = pg.vat_group_id
           ORDER BY pg.id"#,
        f.workspace_id,
        f.currency_id,
        f.warehouse_id,
        f.since,
        f.code,
        limit,
        offset,
    )
    .fetch_all(db)
    .await
}

/// Where the next delta read starts: a minute before now, so a write that committed late
/// with an earlier `updated_at` is read again (the till upserts by id, so repeats are harmless).
pub async fn next_since(db: impl PgExecutor<'_>) -> Result<DateTime<Utc>, sqlx::Error> {
    sqlx::query_scalar!(r#"SELECT now() - interval '1 minute' AS "at!""#)
        .fetch_one(db)
        .await
}
