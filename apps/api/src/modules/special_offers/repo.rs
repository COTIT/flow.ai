//! SQL for special offers, every query scoped to one workspace and live rows, plus the
//! per-product offer read the product prices and calculate-items use.

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde_json::Value;
use sqlx::{PgConnection, PgExecutor};
use uuid::Uuid;

/// One live special offer.
#[derive(Clone)]
pub struct OfferRow {
    pub id: Uuid,
    pub product_id: Option<Uuid>,
    pub from_date: Option<NaiveDate>,
    pub to_date: Option<NaiveDate>,
    pub always_active: bool,
    pub value: Decimal,
    pub kind: String,
    pub base_price: Option<Decimal>,
    pub discount_rule_type: Option<String>,
    pub condition_quantity: Option<i32>,
    pub discount_quantity: Option<i32>,
    pub condition_brand: Option<String>,
    pub condition_supplier_relation_id: Option<Uuid>,
    pub condition_category_id: Option<Uuid>,
    pub target_brand: Option<String>,
    pub target_supplier_relation_id: Option<Uuid>,
    pub target_category_id: Option<Uuid>,
    pub target_product_id: Option<Uuid>,
    pub discount_value: Option<Decimal>,
    pub discount_type: Option<String>,
    pub priority: i32,
    pub apply_to_cheapest: bool,
    pub meta: Option<Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Window count on list reads.
    pub total: Option<i64>,
}

/// Every stored column a write sets (the merged, validated offer).
#[derive(Default)]
pub struct OfferFields {
    pub product_id: Option<Uuid>,
    pub from_date: Option<NaiveDate>,
    pub to_date: Option<NaiveDate>,
    pub always_active: bool,
    pub value: Option<Decimal>,
    pub kind: Option<String>,
    pub base_price: Option<Decimal>,
    pub discount_rule_type: Option<String>,
    pub condition_quantity: Option<i32>,
    pub discount_quantity: Option<i32>,
    pub condition_brand: Option<String>,
    pub condition_supplier_relation_id: Option<Uuid>,
    pub condition_category_id: Option<Uuid>,
    pub target_brand: Option<String>,
    pub target_supplier_relation_id: Option<Uuid>,
    pub target_category_id: Option<Uuid>,
    pub target_product_id: Option<Uuid>,
    pub discount_value: Option<Decimal>,
    pub discount_type: Option<String>,
    pub priority: i32,
    pub apply_to_cheapest: bool,
    pub meta: Option<Value>,
}

impl From<OfferRow> for OfferFields {
    fn from(o: OfferRow) -> Self {
        Self {
            product_id: o.product_id,
            from_date: o.from_date,
            to_date: o.to_date,
            always_active: o.always_active,
            value: Some(o.value),
            kind: Some(o.kind),
            base_price: o.base_price,
            discount_rule_type: o.discount_rule_type,
            condition_quantity: o.condition_quantity,
            discount_quantity: o.discount_quantity,
            condition_brand: o.condition_brand,
            condition_supplier_relation_id: o.condition_supplier_relation_id,
            condition_category_id: o.condition_category_id,
            target_brand: o.target_brand,
            target_supplier_relation_id: o.target_supplier_relation_id,
            target_category_id: o.target_category_id,
            target_product_id: o.target_product_id,
            discount_value: o.discount_value,
            discount_type: o.discount_type,
            priority: o.priority,
            apply_to_cheapest: o.apply_to_cheapest,
            meta: o.meta,
        }
    }
}

/// New offer; returns its id. `value` and `kind` are set by the service.
pub async fn insert(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    o: &OfferFields,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO special_offers (
             workspace_id, product_id, from_date, to_date, always_active, value, type, base_price,
             discount_rule_type, condition_quantity, discount_quantity, condition_brand,
             condition_supplier_relation_id, condition_category_id, target_brand,
             target_supplier_relation_id, target_category_id, target_product_id, discount_value,
             discount_type, priority, apply_to_cheapest, meta)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18,
                 $19, $20, $21, $22, $23)
         RETURNING id",
        workspace_id,
        o.product_id,
        o.from_date,
        o.to_date,
        o.always_active,
        o.value,
        o.kind,
        o.base_price,
        o.discount_rule_type,
        o.condition_quantity,
        o.discount_quantity,
        o.condition_brand,
        o.condition_supplier_relation_id,
        o.condition_category_id,
        o.target_brand,
        o.target_supplier_relation_id,
        o.target_category_id,
        o.target_product_id,
        o.discount_value,
        o.discount_type,
        o.priority,
        o.apply_to_cheapest,
        o.meta,
    )
    .fetch_one(db)
    .await
}

/// Overwrite every column of a live offer (the caller merged and validated it).
pub async fn update(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
    o: &OfferFields,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE special_offers SET
             product_id = $3, from_date = $4, to_date = $5, always_active = $6, value = $7,
             type = $8, base_price = $9, discount_rule_type = $10, condition_quantity = $11,
             discount_quantity = $12, condition_brand = $13, condition_supplier_relation_id = $14,
             condition_category_id = $15, target_brand = $16, target_supplier_relation_id = $17,
             target_category_id = $18, target_product_id = $19, discount_value = $20,
             discount_type = $21, priority = $22, apply_to_cheapest = $23, meta = $24,
             updated_at = now()
         WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
        id,
        workspace_id,
        o.product_id,
        o.from_date,
        o.to_date,
        o.always_active,
        o.value,
        o.kind,
        o.base_price,
        o.discount_rule_type,
        o.condition_quantity,
        o.discount_quantity,
        o.condition_brand,
        o.condition_supplier_relation_id,
        o.condition_category_id,
        o.target_brand,
        o.target_supplier_relation_id,
        o.target_category_id,
        o.target_product_id,
        o.discount_value,
        o.discount_type,
        o.priority,
        o.apply_to_cheapest,
        o.meta,
    )
    .execute(db)
    .await?;
    Ok(())
}

/// Live offer of the workspace (of `product` when given).
pub async fn find(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
    product: Option<Uuid>,
) -> Result<Option<OfferRow>, sqlx::Error> {
    sqlx::query_as!(
        OfferRow,
        r#"SELECT id, product_id, from_date, to_date, always_active, value, type AS kind,
                  base_price, discount_rule_type, condition_quantity, discount_quantity,
                  condition_brand, condition_supplier_relation_id, condition_category_id,
                  target_brand, target_supplier_relation_id, target_category_id,
                  target_product_id, discount_value, discount_type, priority, apply_to_cheapest,
                  meta, created_at, updated_at, NULL::bigint AS total
           FROM special_offers
           WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL
             AND ($3::uuid IS NULL OR product_id = $3)"#,
        id,
        workspace_id,
        product,
    )
    .fetch_optional(db)
    .await
}

/// `find`, locking the row for an update (caller's transaction).
pub async fn find_locked(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    id: Uuid,
    product: Option<Uuid>,
) -> Result<Option<OfferRow>, sqlx::Error> {
    sqlx::query_as!(
        OfferRow,
        r#"SELECT id, product_id, from_date, to_date, always_active, value, type AS kind,
                  base_price, discount_rule_type, condition_quantity, discount_quantity,
                  condition_brand, condition_supplier_relation_id, condition_category_id,
                  target_brand, target_supplier_relation_id, target_category_id,
                  target_product_id, discount_value, discount_type, priority, apply_to_cheapest,
                  meta, created_at, updated_at, NULL::bigint AS total
           FROM special_offers
           WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL
             AND ($3::uuid IS NULL OR product_id = $3)
           FOR NO KEY UPDATE"#,
        id,
        workspace_id,
        product,
    )
    .fetch_optional(conn)
    .await
}

/// List filter: one product's offers and/or one rule type (`legacy` = none).
pub struct ListFilter<'a> {
    pub product: Option<Uuid>,
    pub rule: Option<&'a str>,
}

/// One page (oldest first) with the window count.
pub async fn list(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    f: &ListFilter<'_>,
    limit: i64,
    offset: i64,
) -> Result<Vec<OfferRow>, sqlx::Error> {
    sqlx::query_as!(
        OfferRow,
        r#"SELECT id, product_id, from_date, to_date, always_active, value, type AS kind,
                  base_price, discount_rule_type, condition_quantity, discount_quantity,
                  condition_brand, condition_supplier_relation_id, condition_category_id,
                  target_brand, target_supplier_relation_id, target_category_id,
                  target_product_id, discount_value, discount_type, priority, apply_to_cheapest,
                  meta, created_at, updated_at, count(*) OVER () AS total
           FROM special_offers
           WHERE workspace_id = $1 AND deleted_at IS NULL
             AND ($2::uuid IS NULL OR product_id = $2)
             AND ($3::text IS NULL OR ($3 = 'legacy' AND discount_rule_type IS NULL)
                  OR discount_rule_type = $3)
           ORDER BY id LIMIT $4 OFFSET $5"#,
        workspace_id,
        f.product,
        f.rule,
        limit,
        offset,
    )
    .fetch_all(db)
    .await
}

/// Matching rows when the page came back empty.
pub async fn count(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    f: &ListFilter<'_>,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM special_offers
           WHERE workspace_id = $1 AND deleted_at IS NULL
             AND ($2::uuid IS NULL OR product_id = $2)
             AND ($3::text IS NULL OR ($3 = 'legacy' AND discount_rule_type IS NULL)
                  OR discount_rule_type = $3)"#,
        workspace_id,
        f.product,
        f.rule,
    )
    .fetch_one(db)
    .await
}

/// Soft delete a live offer (of `product` when given). True when it was live.
pub async fn soft_delete(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
    product: Option<Uuid>,
) -> Result<bool, sqlx::Error> {
    let n = sqlx::query!(
        "UPDATE special_offers SET deleted_at = now()
         WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL
           AND ($3::uuid IS NULL OR product_id = $3)",
        id,
        workspace_id,
        product,
    )
    .execute(db)
    .await?
    .rows_affected();
    Ok(n > 0)
}

/// Serialise offer writes of one product (overlap check, then write) in the caller's
/// transaction.
pub async fn lock_product(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    product: Uuid,
) -> Result<(), sqlx::Error> {
    // A waiter holds a pooled connection; give up instead of starving the pool.
    sqlx::query!("SET LOCAL lock_timeout = '5s'")
        .execute(&mut *conn)
        .await?;
    sqlx::query!(
        "SELECT pg_advisory_xact_lock(hashtext('special_offers'), hashtext($1::text))",
        format!("{workspace_id}:{product}"),
    )
    .execute(&mut *conn)
    .await
    .map(|_| ())
}

/// Laravel `checkOverlappingDates`: another live offer of the product that is always active
/// or whose window touches `from..=to`.
pub async fn overlaps(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    product: Uuid,
    from: NaiveDate,
    to: NaiveDate,
    except: Option<Uuid>,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (
               SELECT 1 FROM special_offers
               WHERE workspace_id = $1 AND product_id = $2 AND deleted_at IS NULL
                 AND ($5::uuid IS NULL OR id <> $5)
                 AND (always_active
                      OR from_date BETWEEN $3 AND $4 OR to_date BETWEEN $3 AND $4
                      OR $3 BETWEEN from_date AND to_date OR $4 BETWEEN from_date AND to_date)
           ) AS "e!""#,
        workspace_id,
        product,
        from,
        to,
        except,
    )
    .fetch_one(db)
    .await
}

/// Live offers of one product (the per-product cap).
pub async fn count_for_product(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    product: Uuid,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM special_offers
           WHERE workspace_id = $1 AND product_id = $2 AND deleted_at IS NULL"#,
        workspace_id,
        product,
    )
    .fetch_one(db)
    .await
}

/// Net price of a live product of the workspace (None: no such product).
pub async fn product_price(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<Option<Decimal>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT price FROM products WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
        id,
        workspace_id,
    )
    .fetch_optional(db)
    .await
}

/// True when `id` is a live relation of the workspace.
pub async fn relation_live(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM relations
                          WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL) AS "e!""#,
        id,
        workspace_id,
    )
    .fetch_one(db)
    .await
}

/// True when `id` is a live product category of the workspace.
pub async fn category_live(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM product_categories
                          WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL) AS "e!""#,
        id,
        workspace_id,
    )
    .fetch_one(db)
    .await
}

/// Live offers of these products, best first per product (priority, then newest); with
/// `active_on`, only those active that day.
pub async fn for_products(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    products: &[Uuid],
    active_on: Option<NaiveDate>,
) -> Result<Vec<OfferRow>, sqlx::Error> {
    sqlx::query_as!(
        OfferRow,
        r#"SELECT id, product_id, from_date, to_date, always_active, value, type AS kind,
                  base_price, discount_rule_type, condition_quantity, discount_quantity,
                  condition_brand, condition_supplier_relation_id, condition_category_id,
                  target_brand, target_supplier_relation_id, target_category_id,
                  target_product_id, discount_value, discount_type, priority, apply_to_cheapest,
                  meta, created_at, updated_at, NULL::bigint AS total
           FROM special_offers
           WHERE workspace_id = $1 AND product_id = ANY($2) AND deleted_at IS NULL
             AND ($3::date IS NULL OR always_active OR $3 BETWEEN from_date AND to_date)
           ORDER BY product_id, priority DESC, created_at DESC, id DESC"#,
        workspace_id,
        products,
        active_on,
    )
    .fetch_all(db)
    .await
}
