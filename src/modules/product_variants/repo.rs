//! SQL for product_variants and option types/values. Callers resolve the live product first;
//! every statement takes `workspace_id` (+ `product_id`) and skips soft-deleted rows.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde_json::Value;
use sqlx::{PgConnection, PgExecutor};
use uuid::Uuid;

/// A variant row, read and written whole (the service locks, merges and writes it back).
#[derive(Clone)]
pub struct VariantRow {
    pub id: Uuid,
    pub product_id: Uuid,
    pub is_default: bool,
    pub sku: Option<String>,
    pub barcode: Option<String>,
    pub ean13: Option<String>,
    pub price: Decimal,
    pub gross_price: Decimal,
    pub original_price: Option<Decimal>,
    pub wholesale_price: Option<Decimal>,
    pub original_wholesale_price: Option<Decimal>,
    pub supplier_wholesale_price: Option<Decimal>,
    pub impact_on_price: Option<Decimal>,
    pub impact_on_wholesale_price: Option<Decimal>,
    pub weight: Option<Decimal>,
    pub reference_vendor: Option<String>,
    pub reference_vendor2: Option<String>,
    pub reference_vendor3: Option<String>,
    pub meta: Option<Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// An option value of a variant, with its type.
pub struct OptionValueRow {
    pub variant_id: Uuid,
    pub id: Uuid,
    pub option_type_id: Uuid,
    pub value: String,
    pub display_value: String,
    pub sort_order: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub type_name: String,
    pub type_display_name: String,
}

/// Live variants of a product, oldest first.
pub async fn list(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    product_id: Uuid,
) -> Result<Vec<VariantRow>, sqlx::Error> {
    sqlx::query_as!(
        VariantRow,
        "SELECT id, product_id, is_default, sku, barcode, ean13, price, gross_price, original_price,
                wholesale_price, original_wholesale_price, supplier_wholesale_price,
                impact_on_price, impact_on_wholesale_price, weight, reference_vendor,
                reference_vendor2, reference_vendor3, meta, created_at, updated_at
         FROM product_variants
         WHERE workspace_id = $1 AND product_id = $2 AND deleted_at IS NULL
         ORDER BY id",
        workspace_id,
        product_id,
    )
    .fetch_all(db)
    .await
}

/// Live variant of this product under a row lock. None = 404.
pub async fn find_locked(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    product_id: Uuid,
    id: Uuid,
) -> Result<Option<VariantRow>, sqlx::Error> {
    sqlx::query_as!(
        VariantRow,
        "SELECT id, product_id, is_default, sku, barcode, ean13, price, gross_price, original_price,
                wholesale_price, original_wholesale_price, supplier_wholesale_price,
                impact_on_price, impact_on_wholesale_price, weight, reference_vendor,
                reference_vendor2, reference_vendor3, meta, created_at, updated_at
         FROM product_variants
         WHERE id = $1 AND workspace_id = $2 AND product_id = $3 AND deleted_at IS NULL
         FOR UPDATE",
        id,
        workspace_id,
        product_id,
    )
    .fetch_optional(&mut *conn)
    .await
}

/// Insert `v` (id and timestamps ignored); returns the new id.
pub async fn insert(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    v: &VariantRow,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO product_variants (workspace_id, product_id, is_default, sku, barcode, ean13,
             price, gross_price, original_price, wholesale_price, original_wholesale_price,
             supplier_wholesale_price, impact_on_price, impact_on_wholesale_price, weight,
             reference_vendor, reference_vendor2, reference_vendor3, meta)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18,
                 $19)
         RETURNING id",
        workspace_id,
        v.product_id,
        v.is_default,
        v.sku,
        v.barcode,
        v.ean13,
        v.price,
        v.gross_price,
        v.original_price,
        v.wholesale_price,
        v.original_wholesale_price,
        v.supplier_wholesale_price,
        v.impact_on_price,
        v.impact_on_wholesale_price,
        v.weight,
        v.reference_vendor,
        v.reference_vendor2,
        v.reference_vendor3,
        v.meta,
    )
    .fetch_one(&mut *conn)
    .await
}

/// Write every column of `v` back. The caller holds the row lock.
pub async fn update(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    v: &VariantRow,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE product_variants SET is_default = $3, sku = $4, barcode = $5, ean13 = $6,
             price = $7, gross_price = $8, original_price = $9, wholesale_price = $10,
             original_wholesale_price = $11, supplier_wholesale_price = $12,
             impact_on_price = $13, impact_on_wholesale_price = $14, weight = $15,
             reference_vendor = $16, reference_vendor2 = $17, reference_vendor3 = $18,
             meta = $19, updated_at = now()
         WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
        v.id,
        workspace_id,
        v.is_default,
        v.sku,
        v.barcode,
        v.ean13,
        v.price,
        v.gross_price,
        v.original_price,
        v.wholesale_price,
        v.original_wholesale_price,
        v.supplier_wholesale_price,
        v.impact_on_price,
        v.impact_on_wholesale_price,
        v.weight,
        v.reference_vendor,
        v.reference_vendor2,
        v.reference_vendor3,
        v.meta,
    )
    .execute(&mut *conn)
    .await
    .map(|_| ())
}

/// Unset the product's default variant, except `keep`.
pub async fn clear_default(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    product_id: Uuid,
    keep: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE product_variants SET is_default = false, updated_at = now()
         WHERE workspace_id = $1 AND product_id = $2 AND id <> $3 AND is_default
           AND deleted_at IS NULL",
        workspace_id,
        product_id,
        keep,
    )
    .execute(&mut *conn)
    .await
    .map(|_| ())
}

/// Soft delete.
pub async fn soft_delete(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE product_variants SET deleted_at = now(), updated_at = now()
         WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
        id,
        workspace_id,
    )
    .execute(&mut *conn)
    .await
    .map(|_| ())
}

/// Live option values of any of `variant_ids`, with their type, in sort order.
pub async fn option_values(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    variant_ids: &[Uuid],
) -> Result<Vec<OptionValueRow>, sqlx::Error> {
    sqlx::query_as!(
        OptionValueRow,
        "SELECT l.product_variant_id AS variant_id, v.id, v.option_type_id, v.value,
                v.display_value, v.sort_order, v.created_at, v.updated_at,
                t.name AS type_name, t.display_name AS type_display_name
         FROM variant_option_values l
         JOIN option_values v ON v.id = l.option_value_id AND v.deleted_at IS NULL
         JOIN option_types t ON t.id = v.option_type_id AND t.deleted_at IS NULL
         WHERE l.workspace_id = $1 AND l.product_variant_id = ANY($2)
         ORDER BY l.product_variant_id, v.sort_order NULLS LAST, v.id",
        workspace_id,
        variant_ids,
    )
    .fetch_all(db)
    .await
}

/// Laravel OptionType::firstOrCreate by lowercased name; returns its id.
pub async fn option_type(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    display_name: &str,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO option_types (workspace_id, name, display_name) VALUES ($1, lower($2), $2)
         ON CONFLICT (workspace_id, name) WHERE deleted_at IS NULL
         DO UPDATE SET name = EXCLUDED.name
         RETURNING id",
        workspace_id,
        display_name,
    )
    .fetch_one(&mut *conn)
    .await
}

/// Attach a type to a product, or update its sort order (Laravel syncWithoutDetaching).
pub async fn attach_type(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    product_id: Uuid,
    option_type_id: Uuid,
    sort_order: i32,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO product_option_types (workspace_id, product_id, option_type_id, sort_order)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (product_id, option_type_id) DO UPDATE SET sort_order = EXCLUDED.sort_order",
        workspace_id,
        product_id,
        option_type_id,
        sort_order,
    )
    .execute(&mut *conn)
    .await
    .map(|_| ())
}

/// Laravel OptionValue::firstOrCreate by (type, lowercased value): an existing value keeps its
/// display text and sort order. Returns its id.
pub async fn option_value(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    option_type_id: Uuid,
    display_value: &str,
    sort_order: i32,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO option_values (workspace_id, option_type_id, value, display_value, sort_order)
         VALUES ($1, $2, lower($3), $3, $4)
         ON CONFLICT (option_type_id, value) WHERE deleted_at IS NULL
         DO UPDATE SET value = EXCLUDED.value
         RETURNING id",
        workspace_id,
        option_type_id,
        display_value,
        sort_order,
    )
    .fetch_one(&mut *conn)
    .await
}

/// Link option values to a variant.
pub async fn link_values(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    variant_id: Uuid,
    value_ids: &[Uuid],
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO variant_option_values (workspace_id, product_variant_id, option_value_id)
         SELECT $1, $2, v FROM unnest($3::uuid[]) AS t(v)
         ON CONFLICT DO NOTHING",
        workspace_id,
        variant_id,
        value_ids,
    )
    .execute(&mut *conn)
    .await
    .map(|_| ())
}
