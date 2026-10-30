//! SQL for discounts, plus the two lookups that apply them: the purchase discount (internal
//! supplier groups) and a customer relation's discount (its non-internal groups). Both take
//! the highest live percentage whose date window holds today, UTC as Laravel (user decisions
//! 2026-10-21). Codes match ignoring case, as Laravel's MySQL collation did.

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde_json::Value;
use sqlx::{PgConnection, PgExecutor};
use uuid::Uuid;

use crate::modules::discounts::dto::DiscountInput;

/// One live discount.
pub struct DiscountRow {
    pub id: Uuid,
    pub name: String,
    pub active: bool,
    pub discount_product_code: String,
    pub percentage: Decimal,
    pub valid_from: Option<NaiveDate>,
    pub valid_to: Option<NaiveDate>,
    pub monthly: Option<Value>,
    pub yearly: Option<Value>,
    pub seasonal: Option<Value>,
    pub meta: Option<Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Window count on list reads.
    pub total: Option<i64>,
}

/// New discount (create input: required fields are set); returns its id.
pub async fn insert(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    group: Uuid,
    d: &DiscountInput,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO discounts (workspace_id, discount_group_id, name, active, discount_product_code,
                                percentage, valid_from, valid_to, monthly, yearly, seasonal, meta)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12) RETURNING id",
        workspace_id,
        group,
        d.name,
        d.active,
        d.discount_product_code,
        d.percentage,
        d.valid_from.flatten(),
        d.valid_to.flatten(),
        d.monthly.clone().flatten(),
        d.yearly.clone().flatten(),
        d.seasonal.clone().flatten(),
        d.meta.clone().flatten(),
    )
    .fetch_one(db)
    .await
}

/// Live discount of the workspace.
pub async fn find(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<Option<DiscountRow>, sqlx::Error> {
    sqlx::query_as!(
        DiscountRow,
        r#"SELECT id, name, active, discount_product_code, percentage, valid_from,
                  valid_to, monthly, yearly, seasonal, meta, created_at, updated_at,
                  NULL::bigint AS total
           FROM discounts WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL"#,
        id,
        workspace_id,
    )
    .fetch_optional(db)
    .await
}

/// One page (oldest first), optionally one group's, with the window count.
pub async fn list(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    group: Option<Uuid>,
    limit: i64,
    offset: i64,
) -> Result<Vec<DiscountRow>, sqlx::Error> {
    sqlx::query_as!(
        DiscountRow,
        r#"SELECT id, name, active, discount_product_code, percentage, valid_from,
                  valid_to, monthly, yearly, seasonal, meta, created_at, updated_at,
                  count(*) OVER () AS total
           FROM discounts
           WHERE workspace_id = $1 AND deleted_at IS NULL
             AND ($2::uuid IS NULL OR discount_group_id = $2)
           ORDER BY id LIMIT $3 OFFSET $4"#,
        workspace_id,
        group,
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
    group: Option<Uuid>,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM discounts
           WHERE workspace_id = $1 AND deleted_at IS NULL AND ($2::uuid IS NULL OR discount_group_id = $2)"#,
        workspace_id,
        group,
    )
    .fetch_one(db)
    .await
}

/// Patch a live discount. True when a row matched.
pub async fn update(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
    d: &DiscountInput,
) -> Result<bool, sqlx::Error> {
    let n = sqlx::query!(
        "UPDATE discounts SET
             name = COALESCE($3, name), active = COALESCE($4, active),
             discount_product_code = COALESCE($5, discount_product_code),
             percentage = COALESCE($6, percentage),
             valid_from = CASE WHEN $7 THEN $8 ELSE valid_from END,
             valid_to = CASE WHEN $9 THEN $10 ELSE valid_to END,
             monthly = CASE WHEN $11 THEN $12 ELSE monthly END,
             yearly = CASE WHEN $13 THEN $14 ELSE yearly END,
             seasonal = CASE WHEN $15 THEN $16 ELSE seasonal END,
             meta = CASE WHEN $17 THEN $18 ELSE meta END,
             updated_at = now()
         WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
        id,
        workspace_id,
        d.name,
        d.active,
        d.discount_product_code,
        d.percentage,
        d.valid_from.is_some(),
        d.valid_from.flatten(),
        d.valid_to.is_some(),
        d.valid_to.flatten(),
        d.monthly.is_some(),
        d.monthly.clone().flatten(),
        d.yearly.is_some(),
        d.yearly.clone().flatten(),
        d.seasonal.is_some(),
        d.seasonal.clone().flatten(),
        d.meta.is_some(),
        d.meta.clone().flatten(),
    )
    .execute(db)
    .await?
    .rows_affected();
    Ok(n > 0)
}

/// Lock a live discount for an update and read its window. None when it isn't live.
pub async fn lock_window(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<Option<(Option<NaiveDate>, Option<NaiveDate>)>, sqlx::Error> {
    let row = sqlx::query!(
        "SELECT valid_from, valid_to FROM discounts
         WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL FOR NO KEY UPDATE",
        id,
        workspace_id,
    )
    .fetch_optional(conn)
    .await?;
    Ok(row.map(|r| (r.valid_from, r.valid_to)))
}

/// Soft delete a live discount. True when it was live.
pub async fn soft_delete(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    let n = sqlx::query!(
        "UPDATE discounts SET deleted_at = now() WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
        id,
        workspace_id,
    )
    .execute(db)
    .await?
    .rows_affected();
    Ok(n > 0)
}

/// Highest live purchase discount per product: discounts of the product's supplier in active,
/// internal groups, matching its supplier discount code (Laravel Product::buildPurchasePrice).
pub async fn purchase_discounts(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    product_ids: &[Uuid],
) -> Result<Vec<(Uuid, Decimal)>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT p.id, max(d.percentage) AS "pct!"
           FROM products p
           JOIN discount_groups g ON g.workspace_id = p.workspace_id
                AND g.supplier_relation_id = p.supplier_relation_id
                AND g.internal AND g.active AND g.deleted_at IS NULL
           JOIN discounts d ON d.workspace_id = g.workspace_id AND d.discount_group_id = g.id
                AND lower(d.discount_product_code) = lower(p.supplier_discount_code)
                AND d.active AND d.deleted_at IS NULL
                AND (d.valid_from IS NULL OR d.valid_from <= (now() AT TIME ZONE 'UTC')::date)
                AND (d.valid_to IS NULL OR d.valid_to >= (now() AT TIME ZONE 'UTC')::date)
           WHERE p.id = ANY($1) AND p.workspace_id = $2
           GROUP BY p.id"#,
        product_ids,
        workspace_id,
    )
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(|r| (r.id, r.pct)).collect())
}

/// Highest live discount of customer `relation_id` per product: discounts of the product's
/// supplier in active, non-internal groups attached to the relation (Laravel GetProductPrice).
pub async fn relation_discounts(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    relation_id: Uuid,
    product_ids: &[Uuid],
) -> Result<Vec<(Uuid, Decimal)>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT p.id, max(d.percentage) AS "pct!"
           FROM products p
           JOIN discount_group_relation gr ON gr.workspace_id = p.workspace_id
                AND gr.relation_id = $3
           JOIN discount_groups g ON g.workspace_id = gr.workspace_id AND g.id = gr.discount_group_id
                AND g.supplier_relation_id = p.supplier_relation_id
                AND NOT g.internal AND g.active AND g.deleted_at IS NULL
           JOIN discounts d ON d.workspace_id = g.workspace_id AND d.discount_group_id = g.id
                AND lower(d.discount_product_code) = lower(p.supplier_discount_code)
                AND d.active AND d.deleted_at IS NULL
                AND (d.valid_from IS NULL OR d.valid_from <= (now() AT TIME ZONE 'UTC')::date)
                AND (d.valid_to IS NULL OR d.valid_to >= (now() AT TIME ZONE 'UTC')::date)
           WHERE p.id = ANY($1) AND p.workspace_id = $2
           GROUP BY p.id"#,
        product_ids,
        workspace_id,
        relation_id,
    )
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(|r| (r.id, r.pct)).collect())
}
