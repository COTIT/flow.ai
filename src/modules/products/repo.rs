//! SQL for products, their default variant, price history and bundle children. Every
//! statement takes `workspace_id` and skips soft-deleted rows.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde_json::Value;
use sqlx::{PgConnection, PgExecutor};
use uuid::Uuid;

/// A product row, read and written whole (the service locks, merges and writes it back).
#[derive(Clone)]
pub struct ProductRow {
    pub id: Uuid,
    pub product_number: i64,
    pub product_nr_formatted: String,
    pub name: String,
    pub brand: Option<String>,
    pub kind: String,
    pub status: String,
    pub has_stock: bool,
    pub is_serialized: bool,
    pub warranty_default_mode: Option<String>,
    pub warranty_duration_value: Option<i32>,
    pub warranty_duration_unit: Option<String>,
    pub reference: Option<String>,
    pub reference_vendor: Option<String>,
    pub reference_vendor2: Option<String>,
    pub reference_vendor3: Option<String>,
    pub ean13: Option<String>,
    pub hs_code: Option<String>,
    pub barcode: Option<String>,
    pub barcode_vendor: Option<String>,
    pub supplier_discount_code: Option<String>,
    pub short_description: Option<String>,
    pub description: Option<String>,
    pub stock_level_supplier: Option<i32>,
    pub supplier_relation_id: Option<Uuid>,
    pub currency_id: Uuid,
    pub vat_group_id: Option<Uuid>,
    pub sales_group_id: Option<Uuid>,
    pub default_warehouse_id: Option<Uuid>,
    pub price: Decimal,
    pub gross_price: Decimal,
    pub wholesale_price: Option<Decimal>,
    pub supplier_wholesale_price: Option<Decimal>,
    pub impact_on_price: Option<Decimal>,
    pub impact_on_wholesale_price: Option<Decimal>,
    pub order_unit: Decimal,
    pub sale_unit: Decimal,
    pub weight: Option<Decimal>,
    pub unit: Option<String>,
    pub dimensions: Option<Value>,
    pub country_of_origin: Option<String>,
    pub url_vendor: Option<String>,
    pub url_vendor_override: Option<String>,
    pub exclude_price_corrections: bool,
    pub archived: bool,
    pub import_id: Option<String>,
    pub import_source: Option<String>,
    pub export_enabled: bool,
    pub export_price_sync: bool,
    pub export_product_data: bool,
    pub export_data: Option<Value>,
    pub meta: Option<Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// One price-history row as listed (with the changer's name and the variant).
pub struct HistoryRow {
    pub id: Uuid,
    pub field: String,
    pub old_value: Option<Decimal>,
    pub new_value: Option<Decimal>,
    pub changed_by_flow: String,
    pub first_name: Option<String>,
    pub middle_name: Option<String>,
    pub last_name: Option<String>,
    pub variant_id: Option<Uuid>,
    pub variant_sku: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// One changed price field: `(field, old, new)`.
pub type PriceChange = (&'static str, Option<Decimal>, Option<Decimal>);

/// List filter. `pattern` = escaped ILIKE value.
pub struct ListFilter<'a> {
    pub status: &'a str,
    pub pattern: Option<&'a str>,
    pub supplier: Option<Uuid>,
    pub ean13: Option<&'a str>,
    pub exclude: &'a [Uuid],
    pub limit: i64,
    pub offset: i64,
}

/// Next product number, serialized per workspace (Laravel MAX+1 had no lock). Deleted rows
/// count: numbers are never reused. `start` (settings) applies only to the first numbered
/// product, as in Laravel GetHighestNumber.
pub async fn next_number(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    start: i64,
) -> Result<i64, sqlx::Error> {
    // A waiter holds a pooled connection; give up instead of starving the pool.
    sqlx::query!("SET LOCAL lock_timeout = '5s'")
        .execute(&mut *conn)
        .await?;
    sqlx::query!(
        "SELECT pg_advisory_xact_lock(hashtext('products.product_number'), hashtext($1::text))",
        workspace_id.to_string(),
    )
    .execute(&mut *conn)
    .await?;
    let max = sqlx::query_scalar!(
        "SELECT max(product_number) FROM products WHERE workspace_id = $1",
        workspace_id,
    )
    .fetch_one(&mut *conn)
    .await?;
    max.map_or(Ok(start), |n| {
        n.checked_add(1)
            .ok_or_else(|| sqlx::Error::Protocol("product number overflow".into()))
    })
}

/// Insert `p` (its `id` and timestamps are ignored); returns the new id.
pub async fn insert(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    appendix: &Value,
    p: &ProductRow,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO products (workspace_id, product_number, product_appendix,
             product_nr_formatted, name, brand, type, status, has_stock, is_serialized,
             warranty_default_mode, warranty_duration_value, warranty_duration_unit, reference,
             reference_vendor, reference_vendor2, reference_vendor3, ean13, hs_code, barcode,
             barcode_vendor, supplier_discount_code, short_description, description,
             stock_level_supplier, supplier_relation_id, currency_id, vat_group_id,
             sales_group_id, default_warehouse_id, price, gross_price, wholesale_price,
             supplier_wholesale_price, impact_on_price, impact_on_wholesale_price, order_unit,
             sale_unit, weight, unit, dimensions, country_of_origin, url_vendor,
             url_vendor_override, exclude_price_corrections, import_id, import_source,
             export_enabled, export_price_sync, export_product_data, export_data, meta)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18,
                 $19, $20, $21, $22, $23, $24, $25, $26, $27, $28, $29, $30, $31, $32, $33, $34,
                 $35, $36, $37, $38, $39, $40, $41, $42, $43, $44, $45, $46, $47, $48, $49, $50,
                 $51, $52)
         RETURNING id",
        workspace_id,
        p.product_number,
        appendix,
        p.product_nr_formatted,
        p.name,
        p.brand,
        p.kind,
        p.status,
        p.has_stock,
        p.is_serialized,
        p.warranty_default_mode,
        p.warranty_duration_value,
        p.warranty_duration_unit,
        p.reference,
        p.reference_vendor,
        p.reference_vendor2,
        p.reference_vendor3,
        p.ean13,
        p.hs_code,
        p.barcode,
        p.barcode_vendor,
        p.supplier_discount_code,
        p.short_description,
        p.description,
        p.stock_level_supplier,
        p.supplier_relation_id,
        p.currency_id,
        p.vat_group_id,
        p.sales_group_id,
        p.default_warehouse_id,
        p.price,
        p.gross_price,
        p.wholesale_price,
        p.supplier_wholesale_price,
        p.impact_on_price,
        p.impact_on_wholesale_price,
        p.order_unit,
        p.sale_unit,
        p.weight,
        p.unit,
        p.dimensions,
        p.country_of_origin,
        p.url_vendor,
        p.url_vendor_override,
        p.exclude_price_corrections,
        p.import_id,
        p.import_source,
        p.export_enabled,
        p.export_price_sync,
        p.export_product_data,
        p.export_data,
        p.meta,
    )
    .fetch_one(&mut *conn)
    .await
}

/// Write every writable column of `p` back. The caller holds the row lock (`find_locked`).
pub async fn update(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    p: &ProductRow,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE products SET name = $3, brand = $4, type = $5, status = $6, has_stock = $7,
             is_serialized = $8, warranty_default_mode = $9, warranty_duration_value = $10,
             warranty_duration_unit = $11, reference = $12, reference_vendor = $13,
             reference_vendor2 = $14, reference_vendor3 = $15, ean13 = $16, hs_code = $17,
             barcode = $18, barcode_vendor = $19, supplier_discount_code = $20,
             short_description = $21, description = $22, stock_level_supplier = $23,
             supplier_relation_id = $24, currency_id = $25, vat_group_id = $26,
             sales_group_id = $27, default_warehouse_id = $28, price = $29, gross_price = $30,
             wholesale_price = $31, supplier_wholesale_price = $32, impact_on_price = $33,
             impact_on_wholesale_price = $34, order_unit = $35, sale_unit = $36, weight = $37,
             unit = $38, dimensions = $39, country_of_origin = $40, url_vendor = $41,
             url_vendor_override = $42, exclude_price_corrections = $43, import_id = $44,
             import_source = $45, export_enabled = $46, export_price_sync = $47,
             export_product_data = $48, export_data = $49, meta = $50, updated_at = now()
         WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
        p.id,
        workspace_id,
        p.name,
        p.brand,
        p.kind,
        p.status,
        p.has_stock,
        p.is_serialized,
        p.warranty_default_mode,
        p.warranty_duration_value,
        p.warranty_duration_unit,
        p.reference,
        p.reference_vendor,
        p.reference_vendor2,
        p.reference_vendor3,
        p.ean13,
        p.hs_code,
        p.barcode,
        p.barcode_vendor,
        p.supplier_discount_code,
        p.short_description,
        p.description,
        p.stock_level_supplier,
        p.supplier_relation_id,
        p.currency_id,
        p.vat_group_id,
        p.sales_group_id,
        p.default_warehouse_id,
        p.price,
        p.gross_price,
        p.wholesale_price,
        p.supplier_wholesale_price,
        p.impact_on_price,
        p.impact_on_wholesale_price,
        p.order_unit,
        p.sale_unit,
        p.weight,
        p.unit,
        p.dimensions,
        p.country_of_origin,
        p.url_vendor,
        p.url_vendor_override,
        p.exclude_price_corrections,
        p.import_id,
        p.import_source,
        p.export_enabled,
        p.export_price_sync,
        p.export_product_data,
        p.export_data,
        p.meta,
    )
    .execute(&mut *conn)
    .await
    .map(|_| ())
}

/// Live product of the workspace. None = 404.
pub async fn find(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<Option<ProductRow>, sqlx::Error> {
    sqlx::query_as!(
        ProductRow,
        "SELECT " + r#"id, product_number, product_nr_formatted, name, brand, type AS kind, status, has_stock,
           is_serialized, warranty_default_mode, warranty_duration_value, warranty_duration_unit,
           reference, reference_vendor, reference_vendor2, reference_vendor3, ean13, hs_code,
           barcode, barcode_vendor, supplier_discount_code, short_description, description,
           stock_level_supplier, supplier_relation_id, currency_id, vat_group_id, sales_group_id,
           default_warehouse_id, price, gross_price, wholesale_price, supplier_wholesale_price,
           impact_on_price, impact_on_wholesale_price, order_unit, sale_unit, weight, unit,
           dimensions, country_of_origin, url_vendor, url_vendor_override,
           exclude_price_corrections, archived, import_id, import_source, export_enabled,
           export_price_sync, export_product_data, export_data, meta, created_at, updated_at"# + " FROM products
         WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
        id,
        workspace_id,
    )
    .fetch_optional(db)
    .await
}

/// `find` under a row lock, for read-merge-write updates.
pub async fn find_locked(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<Option<ProductRow>, sqlx::Error> {
    sqlx::query_as!(
        ProductRow,
        "SELECT " + r#"id, product_number, product_nr_formatted, name, brand, type AS kind, status, has_stock,
           is_serialized, warranty_default_mode, warranty_duration_value, warranty_duration_unit,
           reference, reference_vendor, reference_vendor2, reference_vendor3, ean13, hs_code,
           barcode, barcode_vendor, supplier_discount_code, short_description, description,
           stock_level_supplier, supplier_relation_id, currency_id, vat_group_id, sales_group_id,
           default_warehouse_id, price, gross_price, wholesale_price, supplier_wholesale_price,
           impact_on_price, impact_on_wholesale_price, order_unit, sale_unit, weight, unit,
           dimensions, country_of_origin, url_vendor, url_vendor_override,
           exclude_price_corrections, archived, import_id, import_source, export_enabled,
           export_price_sync, export_product_data, export_data, meta, created_at, updated_at"# + " FROM products
         WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL FOR NO KEY UPDATE",
        id,
        workspace_id,
    )
    .fetch_optional(&mut *conn)
    .await
}

/// Live products among `ids` (bundle children).
pub async fn find_many(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    ids: &[Uuid],
) -> Result<Vec<ProductRow>, sqlx::Error> {
    sqlx::query_as!(
        ProductRow,
        "SELECT " + r#"id, product_number, product_nr_formatted, name, brand, type AS kind, status, has_stock,
           is_serialized, warranty_default_mode, warranty_duration_value, warranty_duration_unit,
           reference, reference_vendor, reference_vendor2, reference_vendor3, ean13, hs_code,
           barcode, barcode_vendor, supplier_discount_code, short_description, description,
           stock_level_supplier, supplier_relation_id, currency_id, vat_group_id, sales_group_id,
           default_warehouse_id, price, gross_price, wholesale_price, supplier_wholesale_price,
           impact_on_price, impact_on_wholesale_price, order_unit, sale_unit, weight, unit,
           dimensions, country_of_origin, url_vendor, url_vendor_override,
           exclude_price_corrections, archived, import_id, import_source, export_enabled,
           export_price_sync, export_product_data, export_data, meta, created_at, updated_at"# + " FROM products
         WHERE id = ANY($1) AND workspace_id = $2 AND deleted_at IS NULL",
        ids,
        workspace_id,
    )
    .fetch_all(db)
    .await
}

/// Exact, case-insensitive barcode/EAN/vendor-barcode or id match; several matches prefer an
/// active product, then the oldest, so a later copy never shadows it at the till (Laravel
/// `first()`, plus hard-coded warehouses 7 and 8).
pub async fn by_code(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    code: &str,
) -> Result<Option<ProductRow>, sqlx::Error> {
    sqlx::query_as!(
        ProductRow,
        "SELECT " + r#"id, product_number, product_nr_formatted, name, brand, type AS kind, status, has_stock,
           is_serialized, warranty_default_mode, warranty_duration_value, warranty_duration_unit,
           reference, reference_vendor, reference_vendor2, reference_vendor3, ean13, hs_code,
           barcode, barcode_vendor, supplier_discount_code, short_description, description,
           stock_level_supplier, supplier_relation_id, currency_id, vat_group_id, sales_group_id,
           default_warehouse_id, price, gross_price, wholesale_price, supplier_wholesale_price,
           impact_on_price, impact_on_wholesale_price, order_unit, sale_unit, weight, unit,
           dimensions, country_of_origin, url_vendor, url_vendor_override,
           exclude_price_corrections, archived, import_id, import_source, export_enabled,
           export_price_sync, export_product_data, export_data, meta, created_at, updated_at"# + " FROM products
         WHERE workspace_id = $1 AND deleted_at IS NULL
           AND (lower(barcode) = lower($2) OR lower(ean13) = lower($2)
                OR lower(barcode_vendor) = lower($2) OR id = $3)
         ORDER BY status = 'active' DESC, id
         LIMIT 1",
        workspace_id,
        code,
        // nil never matches: keeps `id = $3` a plain index lookup next to the code indexes.
        Uuid::parse_str(code).unwrap_or(Uuid::nil()),
    )
    .fetch_optional(db)
    .await
}

/// One page, newest first, plus the total (0 when the page is empty).
pub async fn list(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    f: &ListFilter<'_>,
) -> Result<(Vec<ProductRow>, i64), sqlx::Error> {
    let rows = sqlx::query!(
        "SELECT " + r#"id, product_number, product_nr_formatted, name, brand, type AS kind, status, has_stock,
           is_serialized, warranty_default_mode, warranty_duration_value, warranty_duration_unit,
           reference, reference_vendor, reference_vendor2, reference_vendor3, ean13, hs_code,
           barcode, barcode_vendor, supplier_discount_code, short_description, description,
           stock_level_supplier, supplier_relation_id, currency_id, vat_group_id, sales_group_id,
           default_warehouse_id, price, gross_price, wholesale_price, supplier_wholesale_price,
           impact_on_price, impact_on_wholesale_price, order_unit, sale_unit, weight, unit,
           dimensions, country_of_origin, url_vendor, url_vendor_override,
           exclude_price_corrections, archived, import_id, import_source, export_enabled,
           export_price_sync, export_product_data, export_data, meta, created_at, updated_at"# + r#", count(*) OVER () AS "total!" FROM products
         WHERE workspace_id = $1 AND deleted_at IS NULL AND status = $2
           AND ($3::text IS NULL OR name ILIKE $3 OR reference ILIKE $3
                OR reference_vendor ILIKE $3 OR ean13 ILIKE $3 OR barcode ILIKE $3
                OR product_nr_formatted ILIKE $3)
           AND ($4::uuid IS NULL OR supplier_relation_id = $4)
           AND ($5::text IS NULL OR lower(ean13) = lower($5))
           AND NOT (id = ANY($6))
         ORDER BY id DESC
         LIMIT $7 OFFSET $8"#,
        workspace_id,
        f.status,
        f.pattern,
        f.supplier,
        f.ean13,
        f.exclude,
        f.limit,
        f.offset,
    )
    .fetch_all(db)
    .await?;
    let total = rows.first().map_or(0, |r| r.total);
    let rows = rows
        .into_iter()
        .map(|r| ProductRow {
            id: r.id,
            product_number: r.product_number,
            product_nr_formatted: r.product_nr_formatted,
            name: r.name,
            brand: r.brand,
            kind: r.kind,
            status: r.status,
            has_stock: r.has_stock,
            is_serialized: r.is_serialized,
            warranty_default_mode: r.warranty_default_mode,
            warranty_duration_value: r.warranty_duration_value,
            warranty_duration_unit: r.warranty_duration_unit,
            reference: r.reference,
            reference_vendor: r.reference_vendor,
            reference_vendor2: r.reference_vendor2,
            reference_vendor3: r.reference_vendor3,
            ean13: r.ean13,
            hs_code: r.hs_code,
            barcode: r.barcode,
            barcode_vendor: r.barcode_vendor,
            supplier_discount_code: r.supplier_discount_code,
            short_description: r.short_description,
            description: r.description,
            stock_level_supplier: r.stock_level_supplier,
            supplier_relation_id: r.supplier_relation_id,
            currency_id: r.currency_id,
            vat_group_id: r.vat_group_id,
            sales_group_id: r.sales_group_id,
            default_warehouse_id: r.default_warehouse_id,
            price: r.price,
            gross_price: r.gross_price,
            wholesale_price: r.wholesale_price,
            supplier_wholesale_price: r.supplier_wholesale_price,
            impact_on_price: r.impact_on_price,
            impact_on_wholesale_price: r.impact_on_wholesale_price,
            order_unit: r.order_unit,
            sale_unit: r.sale_unit,
            weight: r.weight,
            unit: r.unit,
            dimensions: r.dimensions,
            country_of_origin: r.country_of_origin,
            url_vendor: r.url_vendor,
            url_vendor_override: r.url_vendor_override,
            exclude_price_corrections: r.exclude_price_corrections,
            archived: r.archived,
            import_id: r.import_id,
            import_source: r.import_source,
            export_enabled: r.export_enabled,
            export_price_sync: r.export_price_sync,
            export_product_data: r.export_product_data,
            export_data: r.export_data,
            meta: r.meta,
            created_at: r.created_at,
            updated_at: r.updated_at,
        })
        .collect();
    Ok((rows, total))
}

/// Matching count, for a page past the end.
pub async fn count(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    f: &ListFilter<'_>,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM products
           WHERE workspace_id = $1 AND deleted_at IS NULL AND status = $2
             AND ($3::text IS NULL OR name ILIKE $3 OR reference ILIKE $3
                  OR reference_vendor ILIKE $3 OR ean13 ILIKE $3 OR barcode ILIKE $3
                  OR product_nr_formatted ILIKE $3)
             AND ($4::uuid IS NULL OR supplier_relation_id = $4)
             AND ($5::text IS NULL OR lower(ean13) = lower($5))
             AND NOT (id = ANY($6))"#,
        workspace_id,
        f.status,
        f.pattern,
        f.supplier,
        f.ean13,
        f.exclude,
    )
    .fetch_one(db)
    .await
}

/// Soft delete. False = no such live row.
pub async fn soft_delete(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query!(
        "UPDATE products SET deleted_at = now(), updated_at = now()
         WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
        id,
        workspace_id,
    )
    .execute(db)
    .await
    .map(|done| done.rows_affected() == 1)
}

/// The default variant of a new product (Laravel ProductCreated): codes and prices copied from
/// `p`, which carries the variant's normalised price; `original_price` is the product's raw
/// price. Returns the variant id.
pub async fn insert_default_variant(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    p: &ProductRow,
    original_price: Decimal,
    impact_on_price: Option<Decimal>,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO product_variants (workspace_id, product_id, is_default, barcode, ean13,
             price, original_price, wholesale_price, original_wholesale_price, gross_price,
             supplier_wholesale_price, impact_on_price, impact_on_wholesale_price, weight,
             reference_vendor, reference_vendor2, reference_vendor3)
         VALUES ($1, $2, true, $3, $4, $5, $15, $6, $6, $7, $8, $9, $10, $11, $12, $13, $14)
         RETURNING id",
        workspace_id,
        p.id,
        p.barcode,
        p.ean13,
        p.price,
        p.wholesale_price,
        p.gross_price,
        p.supplier_wholesale_price,
        impact_on_price,
        p.impact_on_wholesale_price,
        p.weight,
        p.reference_vendor,
        p.reference_vendor2,
        p.reference_vendor3,
        original_price,
    )
    .fetch_one(&mut *conn)
    .await
}

/// Record price changes in one statement. `variant_id` set = the variant's own history.
pub async fn insert_history(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    product_id: Uuid,
    variant_id: Option<Uuid>,
    changes: &[PriceChange],
    user_id: Option<Uuid>,
    flow: &str,
) -> Result<(), sqlx::Error> {
    if changes.is_empty() {
        return Ok(());
    }
    let fields: Vec<String> = changes.iter().map(|c| c.0.to_owned()).collect();
    let old: Vec<Option<Decimal>> = changes.iter().map(|c| c.1).collect();
    let new: Vec<Option<Decimal>> = changes.iter().map(|c| c.2).collect();
    sqlx::query!(
        "INSERT INTO product_price_histories (workspace_id, product_id, product_variant_id,
             field, old_value, new_value, changed_by_user_id, changed_by_flow, created_at)
         SELECT $1, $2, $3, f, o, n, $7, $8, clock_timestamp()
         FROM unnest($4::text[], $5::numeric[], $6::numeric[]) WITH ORDINALITY AS t(f, o, n, i)
         ORDER BY i",
        workspace_id,
        product_id,
        variant_id,
        &fields,
        &old as &[Option<Decimal>],
        &new as &[Option<Decimal>],
        user_id,
        flow,
    )
    .execute(&mut *conn)
    .await
    .map(|_| ())
}

/// One page of a product's price history, newest first, plus the total.
pub async fn history(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    product_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<(Vec<HistoryRow>, i64), sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT h.id, h.field, h.old_value, h.new_value, h.changed_by_flow, h.created_at,
                  u.first_name AS "first_name?", u.middle_name, u.last_name AS "last_name?",
                  v.id AS "variant_id?", v.sku AS "variant_sku?",
                  count(*) OVER () AS "total!"
           FROM product_price_histories h
           LEFT JOIN users u ON u.id = h.changed_by_user_id
           LEFT JOIN product_variants v ON v.id = h.product_variant_id
           WHERE h.workspace_id = $1 AND h.product_id = $2
           ORDER BY h.created_at DESC, h.id DESC
           LIMIT $3 OFFSET $4"#,
        workspace_id,
        product_id,
        limit,
        offset,
    )
    .fetch_all(db)
    .await?;
    let total = rows.first().map_or(0, |r| r.total);
    let rows = rows
        .into_iter()
        .map(|r| HistoryRow {
            id: r.id,
            field: r.field,
            old_value: r.old_value,
            new_value: r.new_value,
            changed_by_flow: r.changed_by_flow,
            first_name: r.first_name,
            middle_name: r.middle_name,
            last_name: r.last_name,
            variant_id: r.variant_id,
            variant_sku: r.variant_sku,
            created_at: r.created_at,
        })
        .collect();
    Ok((rows, total))
}

/// History rows of a product, for a page past the end.
pub async fn history_count(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    product_id: Uuid,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM product_price_histories
           WHERE workspace_id = $1 AND product_id = $2"#,
        workspace_id,
        product_id,
    )
    .fetch_one(db)
    .await
}

/// Replace a bundle's children.
pub async fn set_children(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    product_id: Uuid,
    children: &[(Uuid, Decimal)],
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "DELETE FROM product_sets WHERE workspace_id = $1 AND product_id = $2",
        workspace_id,
        product_id,
    )
    .execute(&mut *conn)
    .await?;
    let ids: Vec<Uuid> = children.iter().map(|c| c.0).collect();
    let qty: Vec<Decimal> = children.iter().map(|c| c.1).collect();
    sqlx::query!(
        "INSERT INTO product_sets (workspace_id, product_id, child_product_id, quantity)
         SELECT $1, $2, c, q FROM unnest($3::uuid[], $4::numeric[]) AS t(c, q)",
        workspace_id,
        product_id,
        &ids,
        &qty,
    )
    .execute(&mut *conn)
    .await
    .map(|_| ())
}

/// `(parent, child, quantity)` for live children of any of `parents`.
pub async fn set_children_of(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    parents: &[Uuid],
) -> Result<Vec<(Uuid, Uuid, Decimal)>, sqlx::Error> {
    let rows = sqlx::query!(
        "SELECT s.product_id, s.child_product_id, s.quantity FROM product_sets s
         JOIN products c ON c.id = s.child_product_id AND c.deleted_at IS NULL
         WHERE s.workspace_id = $1 AND s.product_id = ANY($2)
         ORDER BY s.product_id, c.id",
        workspace_id,
        parents,
    )
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| (r.product_id, r.child_product_id, r.quantity))
        .collect())
}

/// Which sent references are live rows of this workspace: `(supplier, vat group, sales group,
/// warehouse, currency)`. A None id counts as fine. One query.
pub async fn refs_ok(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    ids: [Option<Uuid>; 5],
) -> Result<[bool; 5], sqlx::Error> {
    let [supplier, vat, sales, warehouse, currency] = ids;
    let r = sqlx::query!(
        r#"SELECT
             ($2::uuid IS NULL OR EXISTS (SELECT 1 FROM relations
                 WHERE id = $2 AND workspace_id = $1 AND deleted_at IS NULL)) AS "supplier!",
             ($3::uuid IS NULL OR EXISTS (SELECT 1 FROM vat_groups
                 WHERE id = $3 AND workspace_id = $1 AND deleted_at IS NULL)) AS "vat!",
             ($4::uuid IS NULL OR EXISTS (SELECT 1 FROM sales_groups
                 WHERE id = $4 AND workspace_id = $1 AND deleted_at IS NULL)) AS "sales!",
             ($5::uuid IS NULL OR EXISTS (SELECT 1 FROM warehouses
                 WHERE id = $5 AND workspace_id = $1 AND deleted_at IS NULL)) AS "warehouse!",
             ($6::uuid IS NULL OR EXISTS (SELECT 1 FROM currencies
                 WHERE id = $6 AND workspace_id = $1 AND deleted_at IS NULL)) AS "currency!""#,
        workspace_id,
        supplier,
        vat,
        sales,
        warehouse,
        currency,
    )
    .fetch_one(db)
    .await?;
    Ok([r.supplier, r.vat, r.sales, r.warehouse, r.currency])
}

/// The workspace's default currency, else its oldest live one (Laravel "main currency").
pub async fn main_currency(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT id FROM currencies WHERE workspace_id = $1 AND deleted_at IS NULL
         ORDER BY default_currency DESC, id LIMIT 1",
        workspace_id,
    )
    .fetch_optional(db)
    .await
}

/// The workspace's default VAT group, else its oldest live one (user decision; Laravel took the
/// oldest only).
pub async fn fallback_vat_group(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT id FROM vat_groups WHERE workspace_id = $1 AND deleted_at IS NULL
         ORDER BY default_group DESC, id LIMIT 1",
        workspace_id,
    )
    .fetch_optional(db)
    .await
}

/// Ids among `ids` that are live products of this workspace.
pub async fn live_ids(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    ids: &[Uuid],
) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT id FROM products WHERE id = ANY($1) AND workspace_id = $2 AND deleted_at IS NULL",
        ids,
        workspace_id,
    )
    .fetch_all(db)
    .await
}

/// Point the product at another default warehouse (Laravel SetProductStockRequirements).
pub async fn set_default_warehouse(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    id: Uuid,
    warehouse_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE products SET default_warehouse_id = $3, updated_at = now()
         WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
        id,
        workspace_id,
        warehouse_id,
    )
    .execute(&mut *conn)
    .await
    .map(|_| ())
}
