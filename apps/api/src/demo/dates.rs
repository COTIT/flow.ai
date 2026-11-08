//! One pass that spreads the demo over the last 90 days, so lists, charts and date filters have
//! history. Only mutable timestamps move (orders, invoices, stock movements, shifts, farm
//! records' capture and verification times). Append-only ledgers (passbook, payouts, cash,
//! voucher events, invoice payments) keep their real times: the seed keeps the rows they log in
//! the last 7 days. Every statement is scoped to its workspace.

use sqlx::{AssertSqlSafe, PgPool};
use uuid::Uuid;

use super::{sales::Orders, till::Tills};
use crate::error::AppError;

/// Farm record tables spread by capture order; verification follows two days later.
const FARM_RECORDS: &[&str] = &[
    "farm_persons",
    "farm_households",
    "farms",
    "farm_plots",
    "farm_crop_cycles",
    "farm_harvest_reports",
];

pub async fn backdate(
    db: &PgPool,
    shop: Uuid,
    farm: Uuid,
    orders: &Orders,
    tills: &Tills,
) -> Result<(), AppError> {
    let mut tx = db.begin().await?;
    let (ids, days): (Vec<Uuid>, Vec<i32>) = orders
        .dated
        .iter()
        .map(|(id, d)| (*id, i32::try_from(*d).unwrap_or(0)))
        .unzip();
    // Mid-morning on the day, then paid, finished or cancelled a little later (never ahead of now).
    sqlx::query!(
        "UPDATE orders o SET
            created_at = d.at,
            updated_at = d.at,
            -- sales reports read confirmed_at first; left as seed time, every sale landed today
            confirmed_at = CASE WHEN o.confirmed_at IS NOT NULL THEN d.at + interval '5 minutes' END,
            paid_at = CASE WHEN o.paid_at IS NOT NULL THEN d.at + interval '20 minutes' END,
            completed_at = CASE WHEN o.completed_at IS NOT NULL THEN least(d.at + interval '1 day', now()) END,
            cancelled_at = CASE WHEN o.cancelled_at IS NOT NULL THEN least(d.at + interval '2 days', now()) END
         FROM (SELECT id, date_trunc('day', now() - make_interval(days => n)) + interval '10 hours' AS at
               FROM unnest($2::uuid[], $3::int[]) AS t(id, n)) d
         WHERE o.workspace_id = $1 AND o.id = d.id",
        shop,
        &ids,
        &days
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query!(
        "UPDATE order_payments p SET created_at = o.created_at + interval '5 minutes'
         FROM orders o WHERE p.workspace_id = $1 AND o.workspace_id = $1 AND p.order_id = o.id
           AND o.id = ANY($2)",
        shop,
        &ids
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query!(
        "UPDATE stock_movements m SET created_at = o.created_at
         FROM orders o WHERE m.workspace_id = $1 AND o.workspace_id = $1 AND m.related_id = o.id
           AND o.id = ANY($2)",
        shop,
        &ids
    )
    .execute(&mut *tx)
    .await?;
    // Opening counts come before the first order.
    sqlx::query!(
        "UPDATE stock_movements SET created_at = now() - interval '95 days'
         WHERE workspace_id = $1 AND reason = 'DEMO opening count'",
        shop
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query!(
        "UPDATE orders SET expires_at = now() + interval '30 days'
         WHERE workspace_id = $1 AND id = ANY($2) AND status = 'awaiting_payment'",
        shop,
        &orders.waiting
    )
    .execute(&mut *tx)
    .await?;

    // Invoices with their order's day; sent the day after, due 30 days later.
    let (ids, days): (Vec<Uuid>, Vec<i32>) = orders
        .invoices
        .iter()
        .map(|(id, d, _)| (*id, i32::try_from(*d).unwrap_or(0)))
        .unzip();
    sqlx::query!(
        "UPDATE invoices i SET
            created_at = d.at,
            send_date = CASE WHEN i.send_date IS NOT NULL THEN d.at + interval '1 day' END,
            due_date = CASE WHEN i.send_date IS NOT NULL THEN d.at + interval '31 days' END
         FROM (SELECT id, date_trunc('day', now() - make_interval(days => n)) + interval '11 hours' AS at
               FROM unnest($2::uuid[], $3::int[]) AS t(id, n)) d
         WHERE i.workspace_id = $1 AND i.id = d.id",
        shop,
        &ids,
        &days
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query!(
        "UPDATE lots SET created_at = produced_on::timestamptz + interval '9 hours'
         WHERE workspace_id = $1 AND produced_on IS NOT NULL",
        shop
    )
    .execute(&mut *tx)
    .await?;

    // Closed shifts: opened 8:30, closed 18:15 on their day.
    let (ids, days): (Vec<Uuid>, Vec<i32>) = tills
        .shifts
        .iter()
        .map(|(id, d)| (*id, i32::try_from(*d).unwrap_or(0)))
        .unzip();
    sqlx::query!(
        "UPDATE drawer_shifts s SET
            opened_at = date_trunc('day', now() - make_interval(days => d.n)) + interval '8 hours 30 minutes',
            closed_at = least(date_trunc('day', now() - make_interval(days => d.n)) + interval '18 hours 15 minutes', now())
         FROM unnest($2::uuid[], $3::int[]) AS d(id, n)
         WHERE s.workspace_id = $1 AND s.id = d.id AND d.n > 0",
        shop,
        &ids,
        &days
    )
    .execute(&mut *tx)
    .await?;

    // Farm records: captured over 90 days in the order they were made, verified two days later.
    for table in FARM_RECORDS {
        let sql = format!(
            "WITH r AS (SELECT id, (85 * row_number() OVER (ORDER BY created_at, id) / count(*) OVER ())::int AS k
                        FROM {table} WHERE workspace_id = $1)
             UPDATE {table} t SET
                captured_at = now() - make_interval(days => 90 - r.k),
                verified_at = CASE WHEN t.verified_at IS NOT NULL THEN now() - make_interval(days => 88 - r.k) END
             FROM r WHERE t.workspace_id = $1 AND t.id = r.id"
        );
        sqlx::query(AssertSqlSafe(sql))
            .bind(farm)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query!(
        "WITH r AS (SELECT id, (5 * row_number() OVER (ORDER BY created_at, id))::int AS k
                    FROM farm_pue_requests WHERE workspace_id = $1)
         UPDATE farm_pue_requests p SET
            submitted_at = p.submitted_at - make_interval(days => r.k),
            decided_at = p.decided_at - make_interval(days => r.k / 2)
         FROM r WHERE p.workspace_id = $1 AND p.id = r.id",
        farm
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}
