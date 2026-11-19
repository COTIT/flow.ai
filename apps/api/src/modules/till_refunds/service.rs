//! Privileged Till returns and refunds (Flow ERP M4-C13, masterplan §12 TILL REFUND / CASH
//! ROUNDING, §15). Active books only. A return wraps the canonical order return (C11 credit +
//! optional restock); a refund pays a sale's customer credit back in cash (original drawer,
//! open shift) or records a card refund the terminal already made. Refundable = credit
//! residual − pending holds of other routes, in SQL under the credit lock. Lock order: command
//! row → shared book gate → order → recognition → credit item → register → effects.

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::list::{Page, Paginated},
    idempotency::{self, Answer, Command},
    modules::{
        accounting::{
            dto::Created,
            journals::{SourceHeader, SourceLine, post_source_lines},
            service::{created, stale},
            settlement::{self, NewAllocation, NewOpenItem, Side},
        },
        cash::{repo as cash, service::round_cash},
        orders::{self, dto::LinesQuantity},
        purchasing::{dto::PageQuery, service::amount},
        sales_returns::{dto::SalesReturnResource, service as returns},
        shipments::{self, recognition},
        till::privileged::PrivilegedTill,
        till_refunds::dto::{
            RefundInput, RefundJournal, RefundMethod, RefundPost, RefundReview, ReturnInput,
            ReturnReview, TillRefundResource,
        },
        till_sales,
    },
};

const SOURCE: &str = "till_refund";
const NOT_THIS_SALE: &str = "The selected credit is not a customer credit of this sale.";
const OTHER_CURRENCY: &str = "This register works in another currency than the credit.";
const NO_SHIFT: &str = "Open the drawer before paying a cash refund.";
const NOT_STEP: &str = "Cash refunds must be a multiple of the cash rounding step.";
const WAREHOUSE: &str =
    "Returned goods go back to the warehouse the sale left; its booked cost lives there.";
const BEFORE_BOOK: &str =
    "This sale was recorded before the accounting book became active; return it from the Office.";

/// The register a till sale `order` of `ws` was rung up on; 404 for anything else.
async fn sale_register(conn: &mut PgConnection, ws: Uuid, order: Uuid) -> Result<Uuid, AppError> {
    sqlx::query_scalar(
        "SELECT register_id FROM till_sales WHERE workspace_id = $1 AND order_id = $2",
    )
    .bind(ws)
    .bind(order)
    .fetch_optional(conn)
    .await?
    .ok_or(AppError::NotFound)
}

fn hash(v: &Value) -> [u8; 32] {
    Sha256::digest(v.to_string().as_bytes()).into()
}

// --- returns -----------------------------------------------------------------------------

/// Return `input` of till sale `order` (locks taken here); the posted credits and fingerprint.
async fn do_return(
    conn: &mut PgConnection,
    p: &PrivilegedTill,
    order: Uuid,
    input: &ReturnInput,
) -> Result<ReturnReview, AppError> {
    let ws = p.member.workspace_id;
    returns::require_active(&mut *conn, ws).await?;
    sale_register(&mut *conn, ws, order).await?;
    let o = orders::repo::lock(&mut *conn, ws, order)
        .await?
        .ok_or(AppError::NotFound)?;
    if input.warehouse.is_some_and(|w| w != o.warehouse_id) {
        return Err(AppError::field("destination_warehouse_id", WAREHOUSE));
    }
    let lines = LinesQuantity {
        lines: input.lines.clone(),
        restock: input.restock,
    };
    let ids = orders::service::return_locked(
        &mut *conn,
        (ws, p.member.user_id),
        &o,
        &lines,
        &input.reason,
    )
    .await?;
    if ids.is_empty() {
        return Err(AppError::Unprocessable(BEFORE_BOOK.into()));
    }
    let mut out: Vec<SalesReturnResource> = Vec::with_capacity(ids.len());
    for id in ids {
        let mut r = returns::get(&mut *conn, ws, id).await?;
        returns::hide_costs(&mut r, &p.member);
        out.push(r);
    }
    // Ids and timestamps are new on every run; the money and quantities are the facts.
    let facts: Vec<Value> = out
        .iter()
        .map(|r| {
            json!([
                r.recognition_id,
                r.net,
                r.tax,
                r.rounding,
                r.credit,
                r.lines
                    .iter()
                    .map(|l| json!([l.recognition_line_id, l.quantity]))
                    .collect::<Vec<_>>(),
                r.restocks.len()
            ])
        })
        .collect();
    let fp = hash(&json!({ "till_return": order, "restock": input.restock,
                           "reason": input.reason, "credits": facts }));
    Ok(ReturnReview {
        returns: out,
        review_fingerprint: hex::encode(fp),
    })
}

/// POST /till/sales/{order}/returns/review: runs the return and rolls it back.
pub async fn return_review(
    state: &AppState,
    p: &PrivilegedTill,
    order: Uuid,
    input: &ReturnInput,
) -> Result<ReturnReview, AppError> {
    let mut tx = state.db.begin().await?;
    let review = do_return(&mut tx, p, order, input).await?;
    tx.rollback().await?;
    Ok(review)
}

/// POST /till/sales/{order}/returns: post the reviewed return once.
pub async fn return_create(
    state: &AppState,
    p: &PrivilegedTill,
    order: Uuid,
    cmd: Command<crate::modules::accounting::dto::Create<super::dto::ReturnPayload>>,
) -> Result<Answer, AppError> {
    const OP: &str = "till_sales_return";
    let ws = p.member.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, p.member.user_id, &cmd).await? {
        return Ok(a);
    }
    let (input, fingerprint) = &cmd.input.payload;
    let review = do_return(&mut tx, p, order, input).await?;
    if &review.review_fingerprint != fingerprint {
        return Err(stale());
    }
    let ids: Vec<Uuid> = review.returns.iter().map(|r| r.id).collect();
    let answer = created(&review.returns);
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &ids).await?;
    tx.commit().await?;
    Ok(answer)
}

// --- refunds -----------------------------------------------------------------------------

/// Facts of a refund under the locks; `fingerprint` binds them for the post.
struct Facts {
    review: RefundReview,
    recognition_id: Uuid,
    relation_id: Option<Uuid>,
    currency_id: Uuid,
    fingerprint: [u8; 32],
}

#[derive(sqlx::FromRow)]
struct Credit {
    recognition_id: Uuid,
    relation_id: Option<Uuid>,
    currency_id: Uuid,
}

async fn facts(
    conn: &mut PgConnection,
    p: &PrivilegedTill,
    order: Uuid,
    input: &RefundInput,
) -> Result<Facts, AppError> {
    let ws = p.member.workspace_id;
    returns::require_active(&mut *conn, ws).await?;
    let original_register = sale_register(&mut *conn, ws, order).await?;
    let credit: Credit = sqlx::query_as(
        "SELECT recognition_id, relation_id, currency_id FROM accounting_open_items
         WHERE workspace_id = $1 AND id = $2 AND order_id = $3 AND classification = 'customer_credit'
           AND source_kind = 'sales_return' AND recognition_id IS NOT NULL",
    )
    .bind(ws)
    .bind(input.item)
    .bind(order)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or_else(|| AppError::field("credit_open_item_id", NOT_THIS_SALE))?;
    let sale = returns::lock_sale(&mut *conn, ws, credit.recognition_id).await?;
    let (residual, refundable): (i64, i64) = sqlx::query_as(
        "SELECT i.residual,
                (i.residual - coalesce((SELECT sum(r.amount) FROM credit_refund_reservations r
                                        WHERE r.workspace_id = i.workspace_id
                                          AND r.credit_item_id = i.id AND r.state = 'pending'), 0)
                )::int8
         FROM accounting_open_items i WHERE i.workspace_id = $1 AND i.id = $2
         FOR NO KEY UPDATE",
    )
    .bind(ws)
    .bind(input.item)
    .fetch_one(&mut *conn)
    .await?;
    if input.amount > refundable {
        return Err(AppError::field(
            "amount",
            format!(
                "Only {} is left to refund on this credit.",
                amount(refundable.max(0), sale.decimals)
            ),
        ));
    }
    if p.till.currency_id != credit.currency_id {
        return Err(AppError::field("method", OTHER_CURRENCY));
    }
    let (shift_id, drawer_expected, credit_role) = match input.method {
        RefundMethod::Cash => {
            if p.till.register_id != original_register {
                return Err(AppError::NotFound);
            }
            cash::lock_register(&mut *conn, ws, original_register).await?;
            let shift = cash::open_shift(&mut *conn, ws, original_register)
                .await?
                .ok_or(AppError::Conflict(NO_SHIFT))?;
            let step =
                till_sales::service::cash_step(&mut *conn, ws, &p.till.currency_code).await?;
            if round_cash(input.amount, step) != input.amount {
                return Err(AppError::field("amount", NOT_STEP));
            }
            if shift.expected < input.amount {
                return Err(AppError::field(
                    "amount",
                    format!(
                        "The drawer holds only {}.",
                        amount(shift.expected.max(0), sale.decimals)
                    ),
                ));
            }
            (Some(shift.id), Some(shift.expected), "cash")
        }
        RefundMethod::CardRecorded => (None, None, "provider_clearing"),
    };
    let date = shipments::repo::book_today(&mut *conn, ws).await?;
    let fingerprint = hash(&json!({
        "order": order, "item": input.item, "residual": residual, "refundable": refundable,
        "amount": input.amount, "method": input.method.as_str(), "register": p.till.register_id,
        "shift": shift_id, "expected": drawer_expected, "date": date,
        "reference": input.reference, "confirmed": input.confirmed,
    }));
    Ok(Facts {
        recognition_id: sale.id,
        relation_id: credit.relation_id,
        currency_id: credit.currency_id,
        fingerprint,
        review: RefundReview {
            order_id: order,
            credit_open_item_id: input.item,
            method: input.method.as_str(),
            amount: input.amount,
            residual,
            refundable,
            register_id: p.till.register_id,
            shift_id,
            drawer_expected,
            accounting_date: date,
            journal: RefundJournal {
                debit_role: "receivables",
                credit_role,
                amount: input.amount,
            },
            review_fingerprint: hex::encode(fingerprint),
        },
    })
}

/// POST /till/sales/{order}/refunds/review: no writes.
pub async fn refund_review(
    state: &AppState,
    p: &PrivilegedTill,
    order: Uuid,
    input: &RefundInput,
) -> Result<RefundReview, AppError> {
    let mut tx = state.db.begin().await?;
    let f = facts(&mut tx, p, order, input).await?;
    tx.rollback().await?;
    Ok(f.review)
}

/// Columns of a `TillRefundResource` (static SQL text).
macro_rules! columns {
    () => {
        "id, order_id, credit_item_id AS credit_open_item_id,
         refund_item_id AS refund_open_item_id, method, amount, currency_id, register_id,
         shift_id, till_user_id, member_user_id, external_reference, actual_refund_confirmed,
         reason, accounting_date, journal_entry_id, created_at"
    };
}

/// POST /till/sales/{order}/refunds: pay out the reviewed refund once. Journal DR receivables
/// (the credit's party) / CR cash or provider_clearing; a receivable item for the money paid
/// out applied to the credit; cash: a drawer refund row in the open shift.
pub async fn refund_create<T>(
    state: &AppState,
    p: &PrivilegedTill,
    order: Uuid,
    cmd: Command<T>,
) -> Result<Answer, AppError>
where
    T: crate::http::json::Validate<Valid = Created<RefundPost>>,
{
    const OP: &str = "till_refunds_create";
    let ws = p.member.workspace_id;
    let by = p.member.user_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, by, &cmd).await? {
        return Ok(a);
    }
    let post = &cmd.input.payload;
    let f = facts(&mut tx, p, order, &post.input).await?;
    if f.review.review_fingerprint != post.fingerprint {
        return Err(stale());
    }
    let r = &f.review;
    let id = Uuid::now_v7();
    let reservation: Uuid = sqlx::query_scalar(
        "INSERT INTO credit_refund_reservations (workspace_id, credit_item_id, source_kind,
             source_id, amount)
         VALUES ($1, $2, $3, $4, $5) RETURNING id",
    )
    .bind(ws)
    .bind(r.credit_open_item_id)
    .bind(SOURCE)
    .bind(id)
    .bind(r.amount)
    .fetch_one(&mut *tx)
    .await?;
    let header = SourceHeader {
        source_kind: SOURCE,
        source_id: id,
        effect_kind: "posted",
        date: r.accounting_date,
        reason: &post.reason,
        fingerprint: &f.fingerprint,
        by,
    };
    let lines = [
        SourceLine {
            side: Side::Role("receivables"),
            debit: r.amount,
            credit: 0,
            relation_id: f.relation_id,
            bank_account_id: None,
        },
        SourceLine {
            side: Side::Role(r.journal.credit_role),
            debit: 0,
            credit: r.amount,
            relation_id: None,
            bank_account_id: None,
        },
    ];
    let (entry, effect) = post_source_lines(&mut tx, ws, &header, &lines).await?;
    let account_id = recognition::role_account(&mut tx, ws, "receivables").await?;
    let refund_item = settlement::open_item(
        &mut tx,
        ws,
        &NewOpenItem {
            classification: "receivable",
            side: "debit",
            account_role: "receivables",
            account_id,
            source_kind: SOURCE,
            source_id: id,
            relation_id: f.relation_id,
            order_id: Some(order),
            invoice_id: None,
            recognition_id: Some(f.recognition_id),
            currency_id: f.currency_id,
            issue_date: r.accounting_date,
            due_date: None,
            origin: "posted",
            amount: r.amount,
            functional_amount: r.amount,
            effect_id: effect,
            by,
        },
    )
    .await?;
    settlement::allocate(
        &mut tx,
        ws,
        &NewAllocation {
            kind: "application",
            source_item_id: Some(r.credit_open_item_id),
            target_item_id: refund_item,
            date: r.accounting_date,
            currency_id: f.currency_id,
            amount: r.amount,
            effect_id: effect,
            by,
        },
    )
    .await?;
    sqlx::query(
        "UPDATE credit_refund_reservations SET state = 'consumed', effect_id = $3
         WHERE workspace_id = $1 AND id = $2",
    )
    .bind(ws)
    .bind(reservation)
    .bind(effect)
    .execute(&mut *tx)
    .await?;
    settlement::link_source(
        &mut tx,
        ws,
        SOURCE,
        id,
        Some(effect),
        "posted",
        &f.fingerprint,
    )
    .await?;
    if let Some(shift) = r.shift_id {
        sqlx::query(
            "INSERT INTO cash_ledger (workspace_id, register_id, shift_id, kind, amount, actor_kind,
                 member_user_id, till_user_id, source_kind, source_id, effect_id)
             VALUES ($1, $2, $3, 'refund', -$4::int8, 'member', $5, $6, $7, $8, $9)",
        )
        .bind(ws)
        .bind(r.register_id)
        .bind(shift)
        .bind(r.amount)
        .bind(by)
        .bind(p.till.till_user_id)
        .bind(SOURCE)
        .bind(id)
        .bind(effect)
        .execute(&mut *tx)
        .await?;
    }
    let row: TillRefundResource = sqlx::query_as(concat!(
        "INSERT INTO till_refunds (id, workspace_id, order_id, credit_item_id, register_id,
             shift_id, till_user_id, member_user_id, amount, currency_id, method,
             external_reference, actual_refund_confirmed, reason, accounting_date,
             refund_item_id, reservation_id, effect_id, journal_entry_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17,
                 $18, $19)
         RETURNING ",
        columns!()
    ))
    .bind(id)
    .bind(ws)
    .bind(order)
    .bind(r.credit_open_item_id)
    .bind(r.register_id)
    .bind(r.shift_id)
    .bind(p.till.till_user_id)
    .bind(by)
    .bind(r.amount)
    .bind(f.currency_id)
    .bind(r.method)
    .bind(&post.input.reference)
    .bind(post.input.confirmed)
    .bind(&post.reason)
    .bind(r.accounting_date)
    .bind(refund_item)
    .bind(reservation)
    .bind(effect)
    .bind(entry)
    .fetch_one(&mut *tx)
    .await?;
    let answer = created(&row);
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[id]).await?;
    tx.commit().await?;
    Ok(answer)
}

/// Refunds of till sale `order`, oldest first.
pub async fn list(
    state: &AppState,
    ws: Uuid,
    order: Uuid,
    q: &PageQuery,
) -> Result<Paginated<TillRefundResource>, AppError> {
    let mut conn = state.db.acquire().await?;
    sale_register(&mut conn, ws, order).await?;
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    let rows: Vec<TillRefundResource> = sqlx::query_as(concat!(
        "SELECT ",
        columns!(),
        " FROM till_refunds WHERE workspace_id = $1 AND order_id = $2
         ORDER BY created_at, id LIMIT $3 OFFSET $4"
    ))
    .bind(ws)
    .bind(order)
    .bind(page.size)
    .bind(page.offset())
    .fetch_all(&mut *conn)
    .await?;
    let total: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM till_refunds WHERE workspace_id = $1 AND order_id = $2",
    )
    .bind(ws)
    .bind(order)
    .fetch_one(&mut *conn)
    .await?;
    Ok(Paginated::new(rows, total, page))
}

/// One refund of till sale `order`.
pub async fn get(
    state: &AppState,
    ws: Uuid,
    order: Uuid,
    id: Uuid,
) -> Result<TillRefundResource, AppError> {
    sqlx::query_as(concat!(
        "SELECT ",
        columns!(),
        " FROM till_refunds WHERE workspace_id = $1 AND order_id = $2 AND id = $3"
    ))
    .bind(ws)
    .bind(order)
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)
}
