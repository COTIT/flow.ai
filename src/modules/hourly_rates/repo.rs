//! SQL for hourly_rates. Every statement takes `workspace_id` and skips soft-deleted rows;
//! reads bring the live VAT group along in the same query.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde_json::Value;
use sqlx::PgExecutor;
use uuid::Uuid;

use crate::modules::{
    hourly_rates::dto::{HourlyRatePatch, NewHourlyRate},
    vat_groups::repo::VatGroupRow,
};

/// Live rate with its VAT group (None when unset or soft-deleted, like Laravel's belongsTo).
pub struct HourlyRateRow {
    pub id: Uuid,
    pub title: String,
    pub description: String,
    pub consumer_price: Decimal,
    pub business_price: Decimal,
    pub purchase_price: Option<Decimal>,
    pub ledger_number: Option<String>,
    pub meta: Option<Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub vat_group: Option<VatGroupRow>,
}

/// New row; returns its id.
pub async fn insert(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    r: &NewHourlyRate,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO hourly_rates (workspace_id, title, description, consumer_price,
                                   business_price, purchase_price, ledger_number, vat_group_id, meta)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
         RETURNING id",
        workspace_id,
        r.title,
        r.description,
        r.consumer_price,
        r.business_price,
        r.purchase_price,
        r.ledger_number,
        r.vat_group_id,
        r.meta,
    )
    .fetch_one(db)
    .await
}

/// Page of live rates (or one rate when `id` is set), oldest first, plus the total
/// matching count (0 when the page is empty). One query, VAT groups joined.
pub async fn select(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Option<Uuid>,
    pattern: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<HourlyRateRow>, i64), sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT h.id, h.title, h.description, h.consumer_price, h.business_price,
                  h.purchase_price, h.ledger_number, h.meta, h.created_at, h.updated_at,
                  v.id AS "v_id?", v.name AS "v_name?", v.vat_amount AS "v_vat_amount?",
                  v.default_group AS "v_default_group?", v.country AS "v_country?",
                  v.meta AS "v_meta?", v.created_at AS "v_created_at?",
                  v.updated_at AS "v_updated_at?",
                  count(*) OVER () AS "total!"
           FROM hourly_rates h
           LEFT JOIN vat_groups v
             ON v.workspace_id = h.workspace_id AND v.id = h.vat_group_id
            AND v.deleted_at IS NULL
           WHERE h.workspace_id = $1 AND h.deleted_at IS NULL
             AND ($2::uuid IS NULL OR h.id = $2)
             AND ($3::text IS NULL OR h.title ILIKE $3)
           ORDER BY h.id
           LIMIT $4 OFFSET $5"#,
        workspace_id,
        id,
        pattern,
        limit,
        offset,
    )
    .fetch_all(db)
    .await?;
    let total = rows.first().map_or(0, |r| r.total);
    let rows = rows
        .into_iter()
        .map(|r| HourlyRateRow {
            vat_group: match (
                r.v_id,
                r.v_name,
                r.v_vat_amount,
                r.v_default_group,
                r.v_created_at,
                r.v_updated_at,
            ) {
                (Some(id), Some(name), Some(vat_amount), Some(default_group), Some(c), Some(u)) => {
                    Some(VatGroupRow {
                        id,
                        name,
                        vat_amount,
                        default_group,
                        country: r.v_country,
                        meta: r.v_meta,
                        created_at: c,
                        updated_at: u,
                    })
                }
                _ => None,
            },
            id: r.id,
            title: r.title,
            description: r.description,
            consumer_price: r.consumer_price,
            business_price: r.business_price,
            purchase_price: r.purchase_price,
            ledger_number: r.ledger_number,
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
    pattern: Option<&str>,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM hourly_rates
           WHERE workspace_id = $1 AND deleted_at IS NULL
             AND ($2::text IS NULL OR title ILIKE $2)"#,
        workspace_id,
        pattern,
    )
    .fetch_one(db)
    .await
}

/// Prices always set (Laravel update rules require them); the rest is a patch.
/// False = no such row.
pub async fn update(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
    p: &HourlyRatePatch,
) -> Result<bool, sqlx::Error> {
    sqlx::query!(
        "UPDATE hourly_rates
         SET title = COALESCE($3, title),
             description = COALESCE($4, description),
             consumer_price = $5,
             business_price = $6,
             purchase_price = CASE WHEN $7 THEN $8 ELSE purchase_price END,
             ledger_number = CASE WHEN $9 THEN $10 ELSE ledger_number END,
             vat_group_id = CASE WHEN $11 THEN $12 ELSE vat_group_id END,
             meta = CASE WHEN $13 THEN $14 ELSE meta END,
             updated_at = now()
         WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
        id,
        workspace_id,
        p.title,
        p.description,
        p.consumer_price,
        p.business_price,
        p.purchase_price.is_some(),
        p.purchase_price.flatten(),
        p.ledger_number.is_some(),
        p.ledger_number.as_ref().and_then(Option::as_deref),
        p.vat_group_id.is_some(),
        p.vat_group_id.flatten(),
        p.meta.is_some(),
        p.meta.as_ref().and_then(Option::as_ref),
    )
    .execute(db)
    .await
    .map(|done| done.rows_affected() == 1)
}

/// Soft delete. False = no such live row.
pub async fn soft_delete(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query!(
        "UPDATE hourly_rates SET deleted_at = now(), updated_at = now()
         WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
        id,
        workspace_id,
    )
    .execute(db)
    .await
    .map(|done| done.rows_affected() == 1)
}
