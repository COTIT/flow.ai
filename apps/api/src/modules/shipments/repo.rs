//! Shipment SQL: draft rows and lines, planned capacity per order line, post/cancel stamps,
//! and the recognition rows of active books (amounts computed here, in `numeric`).

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use sqlx::{PgConnection, PgExecutor};
use uuid::Uuid;

/// A shipment header as read.
pub struct ShipmentRow {
    pub id: Uuid,
    pub order_id: Uuid,
    pub state: String,
    pub note: Option<String>,
    pub version: i64,
    pub accounting_date: Option<NaiveDate>,
    pub created_by: Uuid,
    pub posted_by: Option<Uuid>,
    pub posted_at: Option<DateTime<Utc>>,
    pub cancelled_at: Option<DateTime<Utc>>,
    pub cancel_reason: Option<String>,
    pub recognition_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A shipment line with its order line's product and title.
pub struct LineRow {
    pub id: Uuid,
    pub shipment_id: Uuid,
    pub order_line_id: Uuid,
    pub product_id: Uuid,
    pub title: String,
    pub quantity: Decimal,
}

/// Shipments of `order` (one when `id` is set), newest first, with the total count.
pub async fn shipments(
    conn: &mut PgConnection,
    ws: Uuid,
    order: Uuid,
    id: Option<Uuid>,
    (limit, offset): (i64, i64),
) -> Result<(Vec<ShipmentRow>, i64), sqlx::Error> {
    let rows = sqlx::query_as!(
        ShipmentRow,
        r#"SELECT s.id, s.order_id, s.state, s.note, s.version, s.accounting_date, s.created_by,
                  s.posted_by, s.posted_at, s.cancelled_at, s.cancel_reason,
                  (SELECT r.id FROM sales_recognitions r
                   WHERE r.workspace_id = s.workspace_id AND r.source_kind = 'shipment'
                     AND r.source_id = s.id) AS recognition_id,
                  s.created_at, s.updated_at
           FROM shipments s
           WHERE s.workspace_id = $1 AND s.order_id = $2 AND ($3::uuid IS NULL OR s.id = $3)
           ORDER BY s.created_at DESC, s.id DESC LIMIT $4 OFFSET $5"#,
        ws,
        order,
        id,
        limit,
        offset
    )
    .fetch_all(&mut *conn)
    .await?;
    let total = sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM shipments
           WHERE workspace_id = $1 AND order_id = $2 AND ($3::uuid IS NULL OR id = $3)"#,
        ws,
        order,
        id
    )
    .fetch_one(conn)
    .await?;
    Ok((rows, total))
}

/// Lines of `ids` in order-line display order.
pub async fn lines(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    ids: &[Uuid],
) -> Result<Vec<LineRow>, sqlx::Error> {
    sqlx::query_as!(
        LineRow,
        "SELECT sl.id, sl.shipment_id, sl.order_line_id, ol.product_id, ol.title, sl.quantity
         FROM shipment_lines sl
         JOIN order_lines ol ON ol.workspace_id = sl.workspace_id AND ol.id = sl.order_line_id
         WHERE sl.workspace_id = $1 AND sl.shipment_id = ANY($2)
         ORDER BY sl.shipment_id, ol.position, ol.id",
        ws,
        ids
    )
    .fetch_all(db)
    .await
}

/// Σ draft + posted shipped quantity per order line of `order`, leaving out `except`.
pub async fn planned(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    order: Uuid,
    except: Option<Uuid>,
) -> Result<Vec<(Uuid, Decimal)>, sqlx::Error> {
    Ok(sqlx::query!(
        r#"SELECT sl.order_line_id, sum(sl.quantity) AS "q!"
           FROM shipment_lines sl
           JOIN shipments s ON s.workspace_id = sl.workspace_id AND s.id = sl.shipment_id
           WHERE sl.workspace_id = $1 AND sl.order_id = $2 AND s.state IN ('draft', 'posted')
             AND ($3::uuid IS NULL OR s.id <> $3)
           GROUP BY sl.order_line_id"#,
        ws,
        order,
        except
    )
    .fetch_all(db)
    .await?
    .into_iter()
    .map(|r| (r.order_line_id, r.q))
    .collect())
}

/// Whether `order` has a draft shipment.
pub async fn has_draft(db: impl PgExecutor<'_>, ws: Uuid, order: Uuid) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM shipments
                          WHERE workspace_id = $1 AND order_id = $2 AND state = 'draft') AS "x!""#,
        ws,
        order
    )
    .fetch_one(db)
    .await
}

/// A new draft; its id.
pub async fn insert(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    order: Uuid,
    note: Option<&str>,
    by: Uuid,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO shipments (workspace_id, order_id, note, created_by)
         VALUES ($1, $2, $3, $4) RETURNING id",
        ws,
        order,
        note,
        by
    )
    .fetch_one(db)
    .await
}

/// Replace a draft's lines.
pub async fn replace_lines(
    conn: &mut PgConnection,
    ws: Uuid,
    (order, shipment): (Uuid, Uuid),
    lines: &[(Uuid, Decimal)],
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "DELETE FROM shipment_lines WHERE workspace_id = $1 AND shipment_id = $2",
        ws,
        shipment
    )
    .execute(&mut *conn)
    .await?;
    let ids: Vec<Uuid> = lines.iter().map(|l| l.0).collect();
    let qty: Vec<Decimal> = lines.iter().map(|l| l.1).collect();
    sqlx::query!(
        "INSERT INTO shipment_lines (workspace_id, shipment_id, order_id, order_line_id, quantity)
         SELECT $1, $2, $3, u.id, u.q FROM UNNEST($4::uuid[], $5::numeric[]) AS u(id, q)",
        ws,
        shipment,
        order,
        &ids,
        &qty
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// (state, version) of a shipment of `order` under a row lock.
pub async fn lock(
    conn: &mut PgConnection,
    ws: Uuid,
    order: Uuid,
    id: Uuid,
) -> Result<Option<(String, i64)>, sqlx::Error> {
    Ok(sqlx::query!(
        "SELECT state, version FROM shipments
         WHERE workspace_id = $1 AND order_id = $2 AND id = $3 FOR UPDATE",
        ws,
        order,
        id
    )
    .fetch_optional(conn)
    .await?
    .map(|r| (r.state, r.version)))
}

/// New note on a draft; version + 1.
pub async fn update_note(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    id: Uuid,
    note: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE shipments SET note = $3, version = version + 1, updated_at = now()
         WHERE workspace_id = $1 AND id = $2",
        ws,
        id,
        note
    )
    .execute(db)
    .await
    .map(|_| ())
}

/// draft → cancelled.
pub async fn cancel(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    id: Uuid,
    reason: &str,
    by: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE shipments SET state = 'cancelled', cancelled_at = now(), cancelled_by = $4,
                cancel_reason = $3, version = version + 1, updated_at = now()
         WHERE workspace_id = $1 AND id = $2",
        ws,
        id,
        reason,
        by
    )
    .execute(db)
    .await
    .map(|_| ())
}

/// draft → posted on `date`.
pub async fn mark_posted(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    id: Uuid,
    date: NaiveDate,
    by: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE shipments SET state = 'posted', accounting_date = $3, posted_at = now(),
                posted_by = $4, version = version + 1, updated_at = now()
         WHERE workspace_id = $1 AND id = $2",
        ws,
        id,
        date,
        by
    )
    .execute(db)
    .await
    .map(|_| ())
}

/// Today in the book's timezone (UTC before setup names one).
pub async fn book_today(db: impl PgExecutor<'_>, ws: Uuid) -> Result<NaiveDate, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT (now() AT TIME ZONE coalesce(timezone, 'UTC'))::date AS "d!"
           FROM accounting_settings WHERE workspace_id = $1"#,
        ws
    )
    .fetch_one(db)
    .await
}
