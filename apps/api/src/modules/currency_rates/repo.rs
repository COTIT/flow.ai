//! SQL for currency rates. Every statement takes `workspace_id`; rows are only ever inserted.

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use sqlx::PgExecutor;
use uuid::Uuid;

use crate::modules::currency_rates::dto::NewRate;

/// A currency as a rate shows it.
pub struct Side {
    pub id: Uuid,
    pub code: String,
    pub decimals: i16,
}

/// One stored rate with both currencies.
pub struct RateRow {
    pub id: Uuid,
    pub from_id: Uuid,
    pub from_code: String,
    pub from_decimals: i16,
    pub to_id: Uuid,
    pub to_code: String,
    pub to_decimals: i16,
    pub rate: Decimal,
    pub effective_on: NaiveDate,
    pub note: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// The rate in force for one direction on a day.
pub struct InForce {
    pub rate: Decimal,
    pub effective_on: NaiveDate,
}

/// Live currencies of `workspace_id` among `ids` (code upper-cased).
pub async fn sides(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    ids: &[Uuid],
) -> Result<Vec<Side>, sqlx::Error> {
    sqlx::query_as!(
        Side,
        r#"SELECT id, upper(asset_type) AS "code!", decimals FROM currencies
           WHERE workspace_id = $1 AND id = ANY($2) AND deleted_at IS NULL"#,
        workspace_id,
        ids,
    )
    .fetch_all(db)
    .await
}

/// Today in the workspace `general.timezone` (UTC when unset or unknown to Postgres).
pub async fn today(db: impl PgExecutor<'_>, workspace_id: Uuid) -> Result<NaiveDate, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT (now() AT TIME ZONE coalesce(
                      (SELECT z.name FROM pg_timezone_names z
                       WHERE z.name = (SELECT s.data -> 'general' ->> 'timezone'
                                       FROM settings s WHERE s.workspace_id = $1)),
                      'UTC'))::date AS "day!""#,
        workspace_id,
    )
    .fetch_one(db)
    .await
}

/// New rate; returns its id.
pub async fn insert(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    user_id: Uuid,
    r: &NewRate,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO currency_rates (workspace_id, from_currency_id, to_currency_id, rate,
                                     effective_on, note, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING id",
        workspace_id,
        r.from,
        r.to,
        r.rate,
        r.effective_on,
        r.note,
        user_id,
    )
    .fetch_one(db)
    .await
}

/// Latest rate `from` → `to` effective on or before `on` (the newer row wins on a tie).
pub async fn in_force(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    from: Uuid,
    to: Uuid,
    on: NaiveDate,
) -> Result<Option<InForce>, sqlx::Error> {
    sqlx::query_as!(
        InForce,
        "SELECT rate, effective_on FROM currency_rates
         WHERE workspace_id = $1 AND from_currency_id = $2 AND to_currency_id = $3
           AND effective_on <= $4
         ORDER BY effective_on DESC, created_at DESC, id DESC LIMIT 1",
        workspace_id,
        from,
        to,
        on,
    )
    .fetch_optional(db)
    .await
}

/// One page of the history, newest entry first, optionally one pair; plus the total.
pub async fn list(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    from: Option<Uuid>,
    to: Option<Uuid>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<RateRow>, i64), sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT r.id, r.rate, r.effective_on, r.note, r.created_at,
                  f.id AS from_id, upper(f.asset_type) AS "from_code!", f.decimals AS from_decimals,
                  t.id AS to_id, upper(t.asset_type) AS "to_code!", t.decimals AS to_decimals,
                  count(*) OVER () AS "total!"
           FROM currency_rates r
           JOIN currencies f ON f.workspace_id = r.workspace_id AND f.id = r.from_currency_id
           JOIN currencies t ON t.workspace_id = r.workspace_id AND t.id = r.to_currency_id
           WHERE r.workspace_id = $1
             AND ($2::uuid IS NULL OR r.from_currency_id = $2)
             AND ($3::uuid IS NULL OR r.to_currency_id = $3)
           ORDER BY r.created_at DESC, r.id DESC
           LIMIT $4 OFFSET $5"#,
        workspace_id,
        from,
        to,
        limit,
        offset,
    )
    .fetch_all(db)
    .await?;
    let total = rows.first().map_or(0, |r| r.total);
    let rows = rows
        .into_iter()
        .map(|r| RateRow {
            id: r.id,
            from_id: r.from_id,
            from_code: r.from_code,
            from_decimals: r.from_decimals,
            to_id: r.to_id,
            to_code: r.to_code,
            to_decimals: r.to_decimals,
            rate: r.rate,
            effective_on: r.effective_on,
            note: r.note,
            created_at: r.created_at,
        })
        .collect();
    Ok((rows, total))
}

/// Matching count, for a page past the end.
pub async fn count(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    from: Option<Uuid>,
    to: Option<Uuid>,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM currency_rates
           WHERE workspace_id = $1
             AND ($2::uuid IS NULL OR from_currency_id = $2)
             AND ($3::uuid IS NULL OR to_currency_id = $3)"#,
        workspace_id,
        from,
        to,
    )
    .fetch_one(db)
    .await
}
