//! SQL for shipment_methods. Every statement takes `workspace_id` and skips soft-deleted rows.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde_json::Value;
use sqlx::PgExecutor;
use uuid::Uuid;

use crate::modules::shipment_methods::dto::{NewShipmentMethod, ShipmentMethodPatch};

/// Live shipment method.
pub struct ShipmentMethodRow {
    pub id: Uuid,
    pub name: String,
    pub carrier: String,
    pub shipping_costs: Decimal,
    pub min_weight: Decimal,
    pub max_weight: Decimal,
    pub price_impact_business: Decimal,
    pub price_impact_consumer: Decimal,
    pub countries: Option<Value>,
    pub provider: Option<String>,
    pub provider_id: Option<String>,
    pub can_be_selected: bool,
    pub meta: Option<Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// List filter. `can_be_selected` only comes from /searches (Laravel ScoutFilterHelper).
pub struct ListFilter<'a> {
    pub pattern: Option<&'a str>,
    pub can_be_selected: Option<bool>,
    pub limit: i64,
    pub offset: i64,
}

pub async fn insert(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    m: &NewShipmentMethod,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO shipment_methods (workspace_id, name, carrier, shipping_costs, min_weight,
                                       max_weight, price_impact_business, price_impact_consumer,
                                       countries, can_be_selected, meta)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
         RETURNING id",
        workspace_id,
        m.name,
        m.carrier,
        m.shipping_costs,
        m.min_weight,
        m.max_weight,
        m.price_impact_business,
        m.price_impact_consumer,
        m.countries,
        m.can_be_selected,
        m.meta,
    )
    .fetch_one(db)
    .await
}

/// Live method of `workspace_id`. None = unknown, deleted or another workspace's.
pub async fn find(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<Option<ShipmentMethodRow>, sqlx::Error> {
    sqlx::query_as!(
        ShipmentMethodRow,
        "SELECT id, name, carrier, shipping_costs, min_weight, max_weight, price_impact_business,
                price_impact_consumer, countries, provider, provider_id, can_be_selected, meta,
                created_at, updated_at
         FROM shipment_methods
         WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
        id,
        workspace_id,
    )
    .fetch_optional(db)
    .await
}

/// One page, oldest first, plus the total matching count (0 when the page is empty).
pub async fn list(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    f: &ListFilter<'_>,
) -> Result<(Vec<ShipmentMethodRow>, i64), sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT id, name, carrier, shipping_costs, min_weight, max_weight,
                  price_impact_business, price_impact_consumer, countries, provider, provider_id,
                  can_be_selected, meta, created_at, updated_at, count(*) OVER () AS "total!"
           FROM shipment_methods
           WHERE workspace_id = $1 AND deleted_at IS NULL
             AND ($2::text IS NULL OR name ILIKE $2)
             AND ($3::bool IS NULL OR can_be_selected = $3)
           ORDER BY id
           LIMIT $4 OFFSET $5"#,
        workspace_id,
        f.pattern,
        f.can_be_selected,
        f.limit,
        f.offset,
    )
    .fetch_all(db)
    .await?;
    let total = rows.first().map_or(0, |r| r.total);
    let rows = rows
        .into_iter()
        .map(|r| ShipmentMethodRow {
            id: r.id,
            name: r.name,
            carrier: r.carrier,
            shipping_costs: r.shipping_costs,
            min_weight: r.min_weight,
            max_weight: r.max_weight,
            price_impact_business: r.price_impact_business,
            price_impact_consumer: r.price_impact_consumer,
            countries: r.countries,
            provider: r.provider,
            provider_id: r.provider_id,
            can_be_selected: r.can_be_selected,
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
        r#"SELECT count(*) AS "n!" FROM shipment_methods
           WHERE workspace_id = $1 AND deleted_at IS NULL
             AND ($2::text IS NULL OR name ILIKE $2)
             AND ($3::bool IS NULL OR can_be_selected = $3)"#,
        workspace_id,
        f.pattern,
        f.can_be_selected,
    )
    .fetch_one(db)
    .await
}

/// Price impacts and countries always set (Laravel update rules require them); the rest
/// patches. The weight-range CHECK sees the merged row. False = no such row.
pub async fn update(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
    p: &ShipmentMethodPatch,
) -> Result<bool, sqlx::Error> {
    sqlx::query!(
        "UPDATE shipment_methods
         SET name = COALESCE($3, name),
             carrier = COALESCE($4, carrier),
             shipping_costs = COALESCE($5, shipping_costs),
             min_weight = COALESCE($6, min_weight),
             max_weight = COALESCE($7, max_weight),
             price_impact_business = $8,
             price_impact_consumer = $9,
             countries = $10,
             can_be_selected = COALESCE($11, can_be_selected),
             meta = CASE WHEN $12 THEN $13 ELSE meta END,
             updated_at = now()
         WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
        id,
        workspace_id,
        p.name,
        p.carrier,
        p.shipping_costs,
        p.min_weight,
        p.max_weight,
        p.price_impact_business,
        p.price_impact_consumer,
        p.countries,
        p.can_be_selected,
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
        "UPDATE shipment_methods SET deleted_at = now(), updated_at = now()
         WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
        id,
        workspace_id,
    )
    .execute(db)
    .await
    .map(|done| done.rows_affected() == 1)
}
