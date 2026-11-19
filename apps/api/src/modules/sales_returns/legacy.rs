//! Existing return and credit routes in active books (masterplan §12 "Existing order-return",
//! legacy credit note projection): order returns post canonical credits (and restocks), a
//! draft credit note aliases one canonical invoice-credit draft, issuing the note posts it,
//! deleting the draft note cancels it. Setup books never come here.

use chrono::NaiveDate;
use rust_decimal::Decimal;
use sqlx::{FromRow, PgConnection};
use uuid::Uuid;

use crate::{
    error::AppError,
    http::json::Rules,
    modules::{
        accounting::service::unsupported,
        customer_deposits,
        orders::repo::OrderRow,
        purchasing::service::{amount, qty},
        sales_returns::{
            dto::{RestockLine, ReturnInput},
            repo,
            service::{check_restock, create_locked, post_locked, restock_locked},
        },
        shipments,
    },
};

const BEFORE_BOOK: &str =
    "Credits of a sale recorded before the accounting book became active aren't supported yet.";
const MISMATCH: &str = "This invoice's lines no longer match its recognised sale.";
const ONE_SALE: &str = "A credit note can credit lines of one recognised sale only.";
const NO_ALIAS: &str = "Credit notes can't be issued in an active book yet.";

/// A recognition line of an order with what is left to credit.
#[derive(FromRow)]
struct Open {
    recognition_id: Uuid,
    id: Uuid,
    order_line_id: Uuid,
    left: Decimal,
}

/// One recognition line's part of an order return: (line, quantity, stock-tracked).
type Take = (Uuid, Decimal, bool);

/// Active `orders_returns`: credit `lines` (order line, quantity, stock-tracked) of the locked
/// order `o` through one posted canonical return per recognition (oldest first), each
/// restocked into the order's warehouse at the original booked cost when `restock`. False
/// when the order has no recognition (sold before activation): the caller returns it
/// physically only. The posted credits' ids (empty = no recognition).
pub async fn order_returns(
    conn: &mut PgConnection,
    ws: Uuid,
    o: &OrderRow,
    (lines, restock, reason): (&[(Uuid, Decimal, bool)], bool, &str),
    by: Uuid,
) -> Result<Vec<Uuid>, AppError> {
    let open: Vec<Open> = sqlx::query_as(concat!(
        "SELECT l.recognition_id, l.id, l.order_line_id, ",
        repo::credit_left!(),
        " AS left
         FROM sales_recognition_lines l
         JOIN sales_recognitions r ON r.workspace_id = l.workspace_id AND r.id = l.recognition_id
         WHERE l.workspace_id = $1 AND r.order_id = $2
         ORDER BY r.created_at, r.id, l.id"
    ))
    .bind(ws)
    .bind(o.id)
    .fetch_all(&mut *conn)
    .await?;
    if open.is_empty() {
        return Ok(Vec::new());
    }
    // Spread each line over its recognitions, oldest first.
    let mut per: Vec<(Uuid, Vec<Take>)> = Vec::new();
    let mut r = Rules::default();
    for (n, (line, q, tracked)) in lines.iter().enumerate() {
        let mut want = *q;
        for x in open
            .iter()
            .filter(|x| x.order_line_id == *line && x.left > Decimal::ZERO)
        {
            if want.is_zero() {
                break;
            }
            let take = want.min(x.left);
            want -= take;
            match per.iter_mut().find(|p| p.0 == x.recognition_id) {
                Some(p) => p.1.push((x.id, take, *tracked)),
                None => per.push((x.recognition_id, vec![(x.id, take, *tracked)])),
            }
        }
        if !want.is_zero() {
            r.add(
                &format!("lines.{n}.quantity"),
                format!("Only {} is left to credit on this line.", qty(*q - want)),
            );
        }
    }
    r.into_result()?;
    let date = shipments::repo::book_today(&mut *conn, ws).await?;
    let mut credits = Vec::new();
    for (rec, take) in per {
        let sale = super::service::lock_sale(&mut *conn, ws, rec).await?;
        let input = ReturnInput {
            accounting_date: date,
            reason: reason.to_owned(),
            lines: take.iter().map(|t| (t.0, t.1)).collect(),
        };
        let id = create_locked(
            &mut *conn,
            ws,
            &sale,
            (Uuid::now_v7(), None),
            (&input, "lines."),
            by,
        )
        .await?;
        post_locked(&mut *conn, ws, &sale, id, (date, by)).await?;
        credits.push(id);
        if !restock {
            continue;
        }
        let tracked: Vec<Uuid> = take.iter().filter(|t| t.2).map(|t| t.0).collect();
        let back: Vec<RestockLine> = repo::lines(&mut *conn, ws, &[id])
            .await?
            .into_iter()
            .filter(|l| tracked.contains(&l.recognition_line_id))
            .map(|l| RestockLine {
                return_line_id: l.id,
                quantity: l.quantity,
                warehouse_id: o.warehouse_id,
                legacy_cost_id: None,
            })
            .collect();
        if back.is_empty() {
            continue;
        }
        let (warehouse, backs) = check_restock(&mut *conn, ws, id, &back, "lines.").await?;
        restock_locked(
            &mut *conn,
            ws,
            &sale,
            (id, Uuid::now_v7()),
            (warehouse, &backs),
            by,
        )
        .await?;
    }
    Ok(credits)
}

/// The oldest reviewed legacy cost of order line `line` with room for `q`, if any.
pub async fn legacy_cost_for(
    conn: &mut PgConnection,
    ws: Uuid,
    line: Uuid,
    q: Decimal,
) -> Result<Option<Uuid>, AppError> {
    Ok(sqlx::query_scalar(
        "SELECT id FROM legacy_return_costs
         WHERE workspace_id = $1 AND order_line_id = $2 AND quantity - consumed_quantity >= $3
         ORDER BY created_at, id LIMIT 1",
    )
    .bind(ws)
    .bind(line)
    .bind(q)
    .fetch_optional(conn)
    .await?)
}

/// An invoice item matched to its recognition line.
#[derive(FromRow)]
struct Matched {
    item: Option<Uuid>,
    line: Option<Uuid>,
    recognition_id: Option<Uuid>,
    fits: bool,
}

/// Item → recognition line of issued invoice `source`: a standalone sale by its own item ids;
/// a billed one by billing position (items in position order = recognition lines in billing
/// order, `shipments::billing::draft`), same count, quantity and product.
async fn matched(
    conn: &mut PgConnection,
    ws: Uuid,
    source: Uuid,
) -> Result<Vec<Matched>, AppError> {
    let own: Vec<Matched> = sqlx::query_as(
        "SELECT l.invoice_item_id AS item, l.id AS line, l.recognition_id, true AS fits
         FROM sales_recognition_lines l
         JOIN sales_recognitions r ON r.workspace_id = l.workspace_id AND r.id = l.recognition_id
         WHERE l.workspace_id = $1 AND r.source_kind = 'invoice' AND r.invoice_id = $2",
    )
    .bind(ws)
    .bind(source)
    .fetch_all(&mut *conn)
    .await?;
    if !own.is_empty() {
        return Ok(own);
    }
    Ok(sqlx::query_as(
        "WITH i AS (
             SELECT id, quantity, product_id, row_number() OVER (ORDER BY position, id) AS n
             FROM items WHERE workspace_id = $1 AND invoice_id = $2),
         r AS (
             SELECT l.id, l.recognition_id, l.quantity, l.product_id,
                    row_number() OVER (ORDER BY l.recognition_id, ol.position, ol.id) AS n
             FROM sales_recognition_lines l
             JOIN invoice_recognition_links k ON k.workspace_id = l.workspace_id
                                             AND k.recognition_id = l.recognition_id
                                             AND k.invoice_id = $2 AND k.state = 'issued'
             JOIN order_lines ol ON ol.workspace_id = l.workspace_id AND ol.id = l.order_line_id
             WHERE l.workspace_id = $1)
         SELECT i.id AS item, r.id AS line, r.recognition_id,
                coalesce(i.quantity = r.quantity
                         AND i.product_id IS NOT DISTINCT FROM r.product_id, false) AS fits
         FROM i FULL JOIN r ON r.n = i.n
         WHERE EXISTS (SELECT 1 FROM r)",
    )
    .bind(ws)
    .bind(source)
    .fetch_all(conn)
    .await?)
}

/// One credited item of a draft credit note.
pub struct Taken {
    pub index: usize,
    pub item: Uuid,
    pub quantity: Decimal,
    /// Laravel `return_amount` (incl. VAT) when one was sent and is not zero.
    pub return_amount: Option<Decimal>,
}

/// Active `invoices/{id}/credit`: alias draft credit note `credit` of issued invoice `source`
/// (locked by the caller) as one canonical invoice-credit draft over the same quantities.
/// A `return_amount` must be the exact proportional gross of its quantity (no repricing).
/// 422 before anything stays written (the caller's transaction rolls back).
pub async fn credit_note(
    conn: &mut PgConnection,
    ws: Uuid,
    (source, credit): (Uuid, Uuid),
    taken: &[Taken],
    by: Uuid,
) -> Result<(), AppError> {
    let map = matched(&mut *conn, ws, source).await?;
    if map.is_empty() {
        return Err(unsupported(BEFORE_BOOK));
    }
    if map
        .iter()
        .any(|m| !m.fits || m.item.is_none() || m.line.is_none())
    {
        return Err(AppError::field("items", MISMATCH));
    }
    let mut lines: Vec<(usize, Uuid, Decimal, Option<Decimal>)> = Vec::new();
    let mut rec: Option<Uuid> = None;
    for t in taken {
        let Some(m) = map.iter().find(|m| m.item == Some(t.item)) else {
            return Err(AppError::field("items", MISMATCH));
        };
        if rec.is_some_and(|r| Some(r) != m.recognition_id) {
            return Err(unsupported(ONE_SALE));
        }
        rec = m.recognition_id;
        if let Some(line) = m.line {
            lines.push((t.index, line, t.quantity, t.return_amount));
        }
    }
    let Some(rec) = rec else {
        return Err(AppError::field("items", MISMATCH));
    };
    super::service::require_active(&mut *conn, ws).await?;
    let sale = repo::lock_sale(&mut *conn, ws, rec).await?;
    let mut r = Rules::default();
    for (index, line, q, ret) in &lines {
        let Some(ret) = ret else { continue };
        let expected: i64 = sqlx::query_scalar(
            "WITH b AS (
                 SELECT l.quantity AS whole, l.net, l.tax,
                        coalesce((SELECT sum(x.quantity) FROM sales_return_lines x
                                  JOIN sales_returns s ON s.id = x.return_id
                                  WHERE x.workspace_id = l.workspace_id
                                    AND x.recognition_line_id = l.id
                                    AND s.state IN ('draft', 'posted')), 0) AS before
                 FROM sales_recognition_lines l WHERE l.workspace_id = $1 AND l.id = $2)
             SELECT (money_round_div(net * (before + $3), whole) - money_round_div(net * before, whole)
                     + money_round_div(tax * (before + $3), whole)
                     - money_round_div(tax * before, whole))::int8 FROM b",
        )
        .bind(ws)
        .bind(line)
        .bind(q)
        .fetch_one(&mut *conn)
        .await?;
        let field = format!("items.{index}.return_amount");
        let given = customer_deposits::service::minor(&mut *conn, (*ret, sale.decimals), &field)
            .await
            .ok();
        r.check(
            &field,
            given == Some(expected),
            format!(
                "In an active book a credit gives back exactly what the sale charged for this quantity: {}.",
                amount(expected, sale.decimals)
            ),
        );
    }
    r.into_result()?;
    let date = shipments::repo::book_today(&mut *conn, ws).await?;
    let input = ReturnInput {
        accounting_date: date,
        reason: "credit note".into(),
        lines: lines.iter().map(|l| (l.1, l.2)).collect(),
    };
    create_locked(
        conn,
        ws,
        &sale,
        (Uuid::now_v7(), Some(credit)),
        (&input, "items."),
        by,
    )
    .await?;
    Ok(())
}

/// The canonical draft aliased by credit note `credit`: (return, recognition, state).
async fn alias(
    conn: &mut PgConnection,
    ws: Uuid,
    credit: Uuid,
) -> Result<Option<(Uuid, Uuid, String)>, AppError> {
    Ok(sqlx::query_as(
        "SELECT id, recognition_id, state FROM sales_returns
         WHERE workspace_id = $1 AND credit_invoice_id = $2",
    )
    .bind(ws)
    .bind(credit)
    .fetch_optional(conn)
    .await?)
}

/// 422 `unsupported_active_erp_operation` unless credit note `credit` aliases a draft credit.
pub async fn check_credit_note(
    conn: &mut PgConnection,
    ws: Uuid,
    credit: Uuid,
) -> Result<(), AppError> {
    match alias(conn, ws, credit).await? {
        Some((_, _, st)) if st == "draft" => Ok(()),
        _ => Err(unsupported(NO_ALIAS)),
    }
}

/// Issue of credit note `credit` (locked by the caller) in an active book on `date`: post
/// its aliased canonical credit once. The note's own number and document follow as before.
pub async fn issue_credit_note(
    conn: &mut PgConnection,
    ws: Uuid,
    credit: Uuid,
    (date, by): (NaiveDate, Uuid),
) -> Result<(), AppError> {
    let Some((id, rec, st)) = alias(&mut *conn, ws, credit).await? else {
        return Err(unsupported(NO_ALIAS));
    };
    if st != "draft" {
        return Err(unsupported(NO_ALIAS));
    }
    let sale = repo::lock_sale(&mut *conn, ws, rec).await?;
    repo::lock(&mut *conn, ws, id).await?;
    post_locked(conn, ws, &sale, id, (date, by)).await
}

/// Deleting draft credit note `credit` cancels its canonical draft (capacity frees).
pub async fn release_credit_note(
    conn: &mut PgConnection,
    ws: Uuid,
    credit: Uuid,
) -> Result<(), AppError> {
    sqlx::query(
        "UPDATE sales_returns SET state = 'cancelled', cancelled_by = created_by,
                cancelled_at = now(), cancel_reason = 'Credit note deleted.',
                version = version + 1, updated_at = now()
         WHERE workspace_id = $1 AND credit_invoice_id = $2 AND state = 'draft'",
    )
    .bind(ws)
    .bind(credit)
    .execute(conn)
    .await?;
    Ok(())
}
