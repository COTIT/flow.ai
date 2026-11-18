//! Purchase returns (masterplan §11 SUPPLIER RETURN / BILL MATCHING, D08, §15): goods leave
//! at the current average C (stock layer `Valued::SupplierReturn`). An unbilled line relieves
//! the GRNI its receipt line still carries and spends the shared capacity (B − U − M − D); a
//! billed line names its posted bill match and opens a supplier credit at the original bill
//! basis (goods + recoverable tax). The difference to C is an explicit purchase-return
//! variance; FX is 0 at return (it realizes when the credit is applied). `purchase_return_plan`
//! / `purchase_return_journal` (SQL) do the money. Reverse (both components) brings the goods
//! back at their original C, undoes its own credit application, closes the credit and restores
//! capacity. Lock order: command row → shared gate → return FOR UPDATE → receipt lines FOR NO
//! KEY UPDATE (id) → bill matches FOR SHARE (id) → per line in (product, warehouse) order:
//! warehouse SHARE → pool → slot (stock layer) → open items → accounts/period → effect/journal.

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{FromRow, PgConnection};
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::{
        json::Rules,
        list::{Page, Paginated},
        workspace::WorkspaceAccess,
    },
    idempotency::{self, Answer, Command},
    modules::{
        accounting::{
            dto::{Action, Create, CurrencyRef},
            gate,
            journals::{SourceHeader, SourceLine, post_source_lines},
            service::{created, ok, stale, unsupported},
            settlement::{self, NewOpenItem},
        },
        payables::{
            bills::{JournalRow, functional, lock_receipt_lines, receipt_facts, require_valued},
            common::{self, dependent, fp32, journal_preview, role_side},
            dto::{
                ListQuery, PurchaseReturnResource, ReturnInput, ReturnLineResource, ReturnPayload,
                ReturnPostReview, ReturnReverseBody, ReturnReverseInput, ReturnReviewLine,
                ReverseReview, ReviewedPayload,
            },
        },
        purchasing::{
            dto::SupplierRef,
            service::{amount, qty},
        },
        stock::{
            repo::{self as stock, Bin, Cause, Floor, Mutated, Slot},
            value::Valued,
        },
    },
    numbering::{self, Series},
};

const SOURCE: &str = "purchase_return";

/// (line, product, warehouse, bin, shelf, lot, quantity, C).
type GoodsRow = (
    Uuid,
    Uuid,
    Uuid,
    Option<Uuid>,
    Option<Uuid>,
    Option<Uuid>,
    Decimal,
    Option<i64>,
);
const NOT_DRAFT: &str = "Only a draft return can be posted.";
const NOT_POSTED: &str = "Only a posted return can be reversed.";
const ONE_COMPONENT: &str = "Reversing only the goods or only the money side of a return is not supported yet. Reverse both together.";

#[derive(FromRow)]
struct ReturnRow {
    id: Uuid,
    number: i64,
    state: String,
    version: i64,
    purpose: String,
    receipt_id: Uuid,
    supplier_id: Uuid,
    supplier_name: Option<String>,
    currency_id: Uuid,
    currency_code: String,
    currency_decimals: i16,
    functional_decimals: Option<i16>,
    accounting_date: NaiveDate,
    reason: String,
    evidence_ref: Option<String>,
    apply_credit: bool,
    credit_item_id: Option<Uuid>,
    application_id: Option<Uuid>,
    posted_effect_id: Option<Uuid>,
    posted_at: Option<chrono::DateTime<chrono::Utc>>,
    reversed_at: Option<chrono::DateTime<chrono::Utc>>,
    reverse_reason: Option<String>,
    created_by: Uuid,
    created_at: chrono::DateTime<chrono::Utc>,
}

const RETURN_SELECT: &str = "
    SELECT x.id, x.number, x.state, x.version, x.purpose, x.receipt_id, x.supplier_id,
           coalesce(r.company_name, r.alias) AS supplier_name, x.currency_id,
           upper(c.asset_type) AS currency_code, c.decimals AS currency_decimals,
           f.decimals AS functional_decimals, x.accounting_date, x.reason, x.evidence_ref,
           x.apply_credit, x.credit_item_id, x.application_id, x.posted_effect_id, x.posted_at,
           x.reversed_at, x.reverse_reason, x.created_by, x.created_at,
           count(*) OVER () AS total_rows
    FROM purchase_returns x
    JOIN currencies c ON c.workspace_id = x.workspace_id AND c.id = x.currency_id
    JOIN relations r ON r.workspace_id = x.workspace_id AND r.id = x.supplier_id
    JOIN accounting_settings s ON s.workspace_id = x.workspace_id
    LEFT JOIN currencies f ON f.workspace_id = x.workspace_id AND f.id = s.functional_currency_id
    WHERE x.workspace_id = $1";

#[derive(FromRow)]
struct LineRow {
    id: Uuid,
    return_id: Uuid,
    line_no: i32,
    receipt_line_id: Uuid,
    bill_match_id: Option<Uuid>,
    quantity: Decimal,
    warehouse_id: Uuid,
    bin_id: Option<Uuid>,
    lot_id: Option<Uuid>,
    issue_value: Option<i64>,
    grni_functional: Option<i64>,
    credit_nominal: Option<i64>,
    credit_functional: Option<i64>,
    tax_functional: Option<i64>,
    variance: Option<i64>,
    fx: Option<i64>,
}

async fn resources(
    conn: &mut PgConnection,
    ws: Uuid,
    rows: Vec<ReturnRow>,
    accounting: bool,
) -> Result<Vec<PurchaseReturnResource>, AppError> {
    let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
    let lines: Vec<LineRow> = sqlx::query_as(
        "SELECT id, return_id, line_no, receipt_line_id, bill_match_id, quantity, warehouse_id,
                bin_id, lot_id, issue_value, grni_functional, credit_nominal, credit_functional,
                tax_functional, variance, fx
         FROM purchase_return_lines WHERE workspace_id = $1 AND return_id = ANY($2)
         ORDER BY return_id, line_no",
    )
    .bind(ws)
    .bind(&ids)
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|x| {
            let dp = x.currency_decimals;
            let fdp = x.functional_decimals.unwrap_or(2);
            let f = |v: Option<i64>| v.filter(|_| accounting).map(|v| amount(v, fdp));
            let allowed_actions = match x.state.as_str() {
                "draft" => vec!["post"],
                "posted" => vec!["reverse"],
                _ => vec![],
            };
            PurchaseReturnResource {
                id: x.id,
                workspace_id: ws,
                number: x.number,
                version: x.version,
                purpose: x.purpose,
                receipt_id: x.receipt_id,
                supplier: SupplierRef {
                    id: x.supplier_id,
                    name: x.supplier_name,
                },
                currency: CurrencyRef {
                    id: x.currency_id,
                    code: x.currency_code,
                    decimals: dp,
                },
                accounting_date: x.accounting_date,
                reason: x.reason,
                evidence_ref: x.evidence_ref,
                apply_credit: x.apply_credit,
                credit_item_id: x.credit_item_id,
                lines: lines
                    .iter()
                    .filter(|l| l.return_id == x.id)
                    .map(|l| ReturnLineResource {
                        id: l.id,
                        line_no: l.line_no,
                        receipt_line_id: l.receipt_line_id,
                        bill_match_id: l.bill_match_id,
                        billed: l.bill_match_id.is_some(),
                        quantity: qty(l.quantity),
                        warehouse_id: l.warehouse_id,
                        bin_id: l.bin_id,
                        lot_id: l.lot_id,
                        issue_value: f(l.issue_value),
                        grni_functional: f(l.grni_functional),
                        credit_nominal: l.credit_nominal.map(|v| amount(v, dp)),
                        credit_functional: f(l.credit_functional),
                        tax_functional: f(l.tax_functional),
                        variance: f(l.variance),
                        fx: f(l.fx),
                    })
                    .collect(),
                posted_at: x.posted_at,
                reversed_at: x.reversed_at,
                reverse_reason: x.reverse_reason,
                created_by: x.created_by,
                created_at: x.created_at,
                allowed_actions,
                state: x.state,
            }
        })
        .collect())
}

async fn row(conn: &mut PgConnection, ws: Uuid, id: Uuid) -> Result<ReturnRow, AppError> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{RETURN_SELECT} AND x.id = $2"
    )))
    .bind(ws)
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(AppError::NotFound)
}

/// One return; 404 outside the workspace.
pub async fn get(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
    accounting: bool,
) -> Result<PurchaseReturnResource, AppError> {
    let x = row(&mut *conn, ws, id).await?;
    resources(conn, ws, vec![x], accounting)
        .await?
        .pop()
        .ok_or(AppError::NotFound)
}

/// GET /purchase-returns: newest first.
pub async fn list(
    state: &AppState,
    ws: Uuid,
    q: &ListQuery,
    accounting: bool,
) -> Result<Paginated<PurchaseReturnResource>, AppError> {
    use sqlx::Row;
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    let mut conn = state.db.acquire().await?;
    let raw = sqlx::query(sqlx::AssertSqlSafe(format!(
        "{RETURN_SELECT} AND ($2::uuid IS NULL OR x.supplier_id = $2)
         ORDER BY x.number DESC LIMIT $3 OFFSET $4"
    )))
    .bind(ws)
    .bind(q.supplier_id)
    .bind(page.size)
    .bind(page.offset())
    .fetch_all(&mut *conn)
    .await?;
    let total = raw
        .first()
        .map(|r| r.try_get::<i64, _>("total_rows"))
        .transpose()?
        .unwrap_or(0);
    let rows = raw
        .iter()
        .map(ReturnRow::from_row)
        .collect::<Result<Vec<_>, _>>()?;
    let data = resources(&mut conn, ws, rows, accounting).await?;
    Ok(Paginated::new(data, total, page))
}

/// Receipt-line facts a return line needs.
#[derive(FromRow)]
struct Source {
    receipt_id: Uuid,
    product_id: Uuid,
    warehouse_id: Uuid,
    supplier_id: Uuid,
    currency_id: Uuid,
}

async fn source(conn: &mut PgConnection, ws: Uuid, line: Uuid) -> Result<Source, AppError> {
    sqlx::query_as(
        "SELECT l.receipt_id, l.product_id, l.warehouse_id, o.supplier_id,
                o.currency_id
         FROM purchase_receipt_lines l
         JOIN purchase_receipts r ON r.workspace_id = l.workspace_id AND r.id = l.receipt_id
         JOIN purchase_orders o ON o.workspace_id = r.workspace_id AND o.id = r.purchase_order_id
         WHERE l.workspace_id = $1 AND l.id = $2",
    )
    .bind(ws)
    .bind(line)
    .fetch_optional(conn)
    .await?
    .ok_or(AppError::NotFound)
}

/// Capacity checks of every line of a return (create and post, under receipt-line locks).
async fn check_caps(
    conn: &mut PgConnection,
    ws: Uuid,
    lines: &[(Uuid, Option<Uuid>, Decimal)],
) -> Result<(), AppError> {
    let mut r = Rules::default();
    for (i, (receipt_line, bill_match, q)) in lines.iter().enumerate() {
        let f = |name: &str| format!("payload.lines.{i}.{name}");
        let facts = receipt_facts(&mut *conn, ws, *receipt_line, None).await?;
        require_valued(&facts)?;
        let (booked, returned): (Decimal, Decimal) =
            sqlx::query_as("SELECT booked, returned FROM receipt_line_capacity($1, $2, NULL)")
                .bind(ws)
                .bind(receipt_line)
                .fetch_one(&mut *conn)
                .await?;
        r.check(
            &f("quantity"),
            *q <= booked - returned,
            "More than is left of this receipt line to return.",
        );
        match bill_match {
            None => r.check(
                &f("quantity"),
                facts.matchable.is_some_and(|m| *q <= m),
                "More than this receipt line has unbilled (billed or reserved quantity needs a billed return).",
            ),
            Some(m) => {
                let left: Option<Decimal> = sqlx::query_scalar(
                    "SELECT m.quantity - coalesce((SELECT sum(x.quantity)
                         FROM purchase_return_lines x
                         JOIN purchase_returns h ON h.id = x.return_id AND h.state = 'posted'
                         WHERE x.bill_match_id = m.id), 0)
                     FROM supplier_bill_matches m
                     WHERE m.workspace_id = $1 AND m.id = $2 AND m.receipt_line_id = $3
                       AND m.state = 'posted'",
                )
                .bind(ws)
                .bind(m)
                .bind(receipt_line)
                .fetch_optional(&mut *conn)
                .await?;
                match left {
                    None => r.add(
                        &f("source_bill_allocation_id"),
                        "Name a posted bill match of this receipt line.",
                    ),
                    Some(left) => r.check(
                        &f("quantity"),
                        *q <= left,
                        "More than this bill match has left to return.",
                    ),
                }
            }
        }
    }
    r.into_result()
}

/// POST /purchase-returns: a numbered draft.
pub async fn create(
    state: &AppState,
    access: &WorkspaceAccess,
    cmd: Command<Create<ReturnPayload>>,
) -> Result<Answer, AppError> {
    const OP: &str = "purchase_returns_create";
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, access.user_id, &cmd).await? {
        return Ok(a);
    }
    gate::shared(&mut tx, &[ws]).await?;
    let input: &ReturnInput = &cmd.input.payload;
    functional(&mut tx, ws).await?;
    let mut r = Rules::default();
    let mut placed = Vec::with_capacity(input.lines.len());
    let mut head: Option<(Uuid, Uuid)> = None;
    for (i, l) in input.lines.iter().enumerate() {
        let s = source(&mut tx, ws, l.receipt_line).await?;
        r.check(
            &format!("payload.lines.{i}.receipt_line_id"),
            s.receipt_id == input.receipt,
            "The line is from another receipt.",
        );
        head = Some((s.supplier_id, s.currency_id));
        let place = (s.product_id, l.warehouse.unwrap_or(s.warehouse_id));
        r.check(
            &format!("payload.lines.{i}.warehouse_id"),
            !placed.contains(&place),
            "One line per product and warehouse.",
        );
        placed.push(place);
        if input.purpose == "receipt_correction" {
            r.check(
                &format!("payload.lines.{i}.source_bill_allocation_id"),
                l.bill_match.is_none(),
                "A receipt correction returns unbilled goods only.",
            );
        }
    }
    if input.apply_credit {
        r.check(
            "payload.apply_credit",
            input.lines.iter().any(|l| l.bill_match.is_some()),
            "Only billed lines create a credit to apply.",
        );
    }
    r.into_result()?;
    let (supplier, currency) = head.ok_or(AppError::NotFound)?;
    let receipt_lines: Vec<Uuid> = input.lines.iter().map(|l| l.receipt_line).collect();
    lock_receipt_lines(&mut tx, ws, &receipt_lines).await?;
    let caps: Vec<(Uuid, Option<Uuid>, Decimal)> = input
        .lines
        .iter()
        .map(|l| (l.receipt_line, l.bill_match, l.quantity))
        .collect();
    check_caps(&mut tx, ws, &caps).await?;
    let number = numbering::next(&mut tx, ws, Series::PurchaseReturn, 1).await?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO purchase_returns (workspace_id, number, purpose, receipt_id, supplier_id,
             currency_id, accounting_date, reason, evidence_ref, apply_credit, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) RETURNING id",
    )
    .bind(ws)
    .bind(number)
    .bind(input.purpose)
    .bind(input.receipt)
    .bind(supplier)
    .bind(currency)
    .bind(input.accounting_date)
    .bind(&input.reason)
    .bind(&input.evidence_ref)
    .bind(input.apply_credit)
    .bind(access.user_id)
    .fetch_one(&mut *tx)
    .await?;
    for (n, l) in input.lines.iter().enumerate() {
        sqlx::query(
            "INSERT INTO purchase_return_lines (workspace_id, return_id, line_no, receipt_line_id,
                 bill_match_id, quantity, warehouse_id, bin_id, lot_id)
             SELECT $1, $2, $3, l.id, $4, $5, coalesce($6, l.warehouse_id),
                    CASE WHEN $6 IS NULL THEN coalesce($7, l.bin_id) ELSE $7 END,
                    coalesce($8, l.lot_id)
             FROM purchase_receipt_lines l WHERE l.workspace_id = $1 AND l.id = $9",
        )
        .bind(ws)
        .bind(id)
        .bind(i32::try_from(n + 1).unwrap_or(1))
        .bind(l.bill_match)
        .bind(l.quantity)
        .bind(l.warehouse)
        .bind(l.bin)
        .bind(l.lot)
        .bind(l.receipt_line)
        .execute(&mut *tx)
        .await
        .map_err(
            |e| match e.as_database_error().and_then(|d| d.code()).as_deref() {
                Some("23503") => {
                    AppError::field("payload.lines", "A warehouse, bin or lot was not found.")
                }
                _ => e.into(),
            },
        )?;
    }
    let answer = created(&get(&mut tx, ws, id, access.allows("accounting.read")).await?);
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[id]).await?;
    tx.commit().await?;
    Ok(answer)
}

/// One row of `purchase_return_plan`.
#[derive(FromRow, Clone)]
struct PlanRow {
    line_id: Uuid,
    receipt_line_id: Uuid,
    bill_match_id: Option<Uuid>,
    quantity: Decimal,
    pool_quantity: Option<Decimal>,
    pool_value: Option<i64>,
    pool_sequence: Option<i64>,
    issue_value: Option<i64>,
    grni_nominal: Option<i64>,
    grni_functional: Option<i64>,
    credit_nominal: Option<i64>,
    credit_functional: Option<i64>,
    tax_nominal: Option<i64>,
    tax_functional: Option<i64>,
}

struct PostPlan {
    review: ReturnPostReview,
    rows: Vec<PlanRow>,
    journal: Vec<JournalRow>,
}

async fn plan_rows(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
) -> Result<(Vec<PlanRow>, Vec<JournalRow>), AppError> {
    let rows: Vec<PlanRow> = sqlx::query_as(
        "SELECT line_id, receipt_line_id, bill_match_id, quantity, pool_quantity, pool_value,
                pool_sequence, issue_value, grni_nominal, grni_functional, credit_nominal,
                credit_functional, tax_nominal, tax_functional
         FROM purchase_return_plan($1, $2) ORDER BY line_id",
    )
    .bind(ws)
    .bind(id)
    .fetch_all(&mut *conn)
    .await?;
    let journal: Vec<JournalRow> = sqlx::query_as(
        "SELECT role, account_id, debit, credit FROM purchase_return_journal($1, $2)
         ORDER BY line_no",
    )
    .bind(ws)
    .bind(id)
    .fetch_all(&mut *conn)
    .await?;
    Ok((rows, journal))
}

async fn prepare_post(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
    accounting: bool,
) -> Result<PostPlan, AppError> {
    let x = row(&mut *conn, ws, id).await?;
    if x.state != "draft" {
        return Err(AppError::Unprocessable(NOT_DRAFT.into()));
    }
    let fc = functional(&mut *conn, ws).await?;
    let caps: Vec<(Uuid, Option<Uuid>, Decimal)> = sqlx::query_as(
        "SELECT receipt_line_id, bill_match_id, quantity FROM purchase_return_lines
         WHERE workspace_id = $1 AND return_id = $2 ORDER BY line_no",
    )
    .bind(ws)
    .bind(id)
    .fetch_all(&mut *conn)
    .await?;
    check_caps(&mut *conn, ws, &caps).await?;
    let (rows, journal) = plan_rows(&mut *conn, ws, id).await?;
    if rows.iter().any(|p| p.issue_value.is_none()) {
        return Err(crate::modules::stock::value::insufficient());
    }
    // The variance per line, in SQL.
    let variances: Vec<(Uuid, i64)> = sqlx::query_as(
        "SELECT line_id, (coalesce(issue_value, 0) - coalesce(grni_functional, 0)
                          - coalesce(credit_functional, 0))::bigint
         FROM purchase_return_plan($1, $2)",
    )
    .bind(ws)
    .bind(id)
    .fetch_all(&mut *conn)
    .await?;
    let fdp = fc.decimals;
    let dp = x.currency_decimals;
    let f = |v: Option<i64>| v.filter(|_| accounting).map(|v| amount(v, fdp));
    let lines = rows
        .iter()
        .map(|p| ReturnReviewLine {
            line_id: p.line_id,
            receipt_line_id: p.receipt_line_id,
            billed: p.bill_match_id.is_some(),
            quantity: qty(p.quantity),
            issue_value: f(p.issue_value),
            grni_nominal: p.grni_nominal.map(|v| amount(v, dp)),
            grni_functional: f(p.grni_functional),
            credit_nominal: p.credit_nominal.map(|v| amount(v, dp)),
            credit_functional: f(p.credit_functional),
            tax_nominal: p.tax_nominal.map(|v| amount(v, dp)),
            tax_functional: f(p.tax_functional),
            variance: f(variances.iter().find(|v| v.0 == p.line_id).map(|v| v.1)),
            fx: f(Some(0)),
        })
        .collect();
    let fp: [u8; 32] = Sha256::digest(
        json!({
            "action": "post", "return": id, "version": x.version, "date": x.accounting_date,
            "rows": rows.iter().map(|p| json!([p.line_id, qty(p.quantity), p.pool_quantity.map(qty),
                p.pool_value, p.pool_sequence, p.issue_value, p.grni_nominal, p.grni_functional,
                p.credit_nominal, p.credit_functional, p.tax_nominal, p.tax_functional]))
                .collect::<Vec<_>>(),
            "journal": journal.iter().map(|j| json!([j.role, j.debit, j.credit]))
                .collect::<Vec<_>>(),
        })
        .to_string(),
    )
    .into();
    Ok(PostPlan {
        review: ReturnPostReview {
            return_id: id,
            version: x.version,
            accounting_date: x.accounting_date,
            lines,
            journal: accounting.then(|| journal_preview(&journal, fdp)),
            review_fingerprint: hex::encode(fp),
        },
        rows,
        journal,
    })
}

/// POST /purchase-returns/{id}/post/review: no writes.
pub async fn post_review(
    state: &AppState,
    access: &WorkspaceAccess,
    id: Uuid,
) -> Result<ReturnPostReview, AppError> {
    let mut tx = state.db.begin().await?;
    let plan = prepare_post(
        &mut tx,
        access.workspace_id,
        id,
        access.allows("accounting.read"),
    )
    .await?;
    tx.rollback().await?;
    Ok(plan.review)
}

/// Shared gate, the return FOR UPDATE, its receipt lines and bill matches; (state, version).
async fn lock(conn: &mut PgConnection, ws: Uuid, id: Uuid) -> Result<(String, i64), AppError> {
    gate::shared(&mut *conn, &[ws]).await?;
    let found: (String, i64) = sqlx::query_as(
        "SELECT state, version FROM purchase_returns WHERE workspace_id = $1 AND id = $2
         FOR UPDATE",
    )
    .bind(ws)
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(AppError::NotFound)?;
    let lines: Vec<(Uuid, Option<Uuid>)> = sqlx::query_as(
        "SELECT receipt_line_id, bill_match_id FROM purchase_return_lines
         WHERE workspace_id = $1 AND return_id = $2",
    )
    .bind(ws)
    .bind(id)
    .fetch_all(&mut *conn)
    .await?;
    let receipt_lines: Vec<Uuid> = lines.iter().map(|l| l.0).collect();
    lock_receipt_lines(&mut *conn, ws, &receipt_lines).await?;
    let mut matches: Vec<Uuid> = lines.iter().filter_map(|l| l.1).collect();
    matches.sort_unstable();
    sqlx::query(
        "SELECT id FROM supplier_bill_matches WHERE workspace_id = $1 AND id = ANY($2)
         ORDER BY id FOR SHARE",
    )
    .bind(ws)
    .bind(&matches)
    .execute(&mut *conn)
    .await?;
    Ok(found)
}

/// Goods of `id`'s lines out (`back` false, current average) or back in at their C.
async fn move_goods(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
    date: NaiveDate,
    back: bool,
    by: Uuid,
) -> Result<(), AppError> {
    let lines: Vec<GoodsRow> = sqlx::query_as(
        "SELECT l.id, rl.product_id, l.warehouse_id, l.bin_id, b.shelf_id, l.lot_id,
                    l.quantity, l.issue_value
             FROM purchase_return_lines l
             JOIN purchase_receipt_lines rl ON rl.id = l.receipt_line_id
             LEFT JOIN warehouse_section_row_bins b ON b.id = l.bin_id
             WHERE l.workspace_id = $1 AND l.return_id = $2
             ORDER BY rl.product_id, l.warehouse_id, l.id",
    )
    .bind(ws)
    .bind(id)
    .fetch_all(&mut *conn)
    .await?;
    for (line, product, warehouse, bin, shelf, lot, q, c) in lines {
        let slot = Slot {
            product_id: product,
            warehouse_id: warehouse,
            bin: bin
                .zip(shelf)
                .map(|(bin_id, shelf_id)| Bin { shelf_id, bin_id }),
            lot_id: lot,
        };
        let cause = Cause {
            reason: Some(if back {
                "purchase return reversed"
            } else {
                "purchase return"
            }),
            user_id: Some(by),
            related: Some(("purchase_return_line", line)),
            value: Valued::SupplierReturn {
                value: if back { c } else { None },
                date,
            },
            ..Cause::default()
        };
        let change = if back { q } else { -q };
        match stock::mutate(&mut *conn, ws, slot, change, Floor::Zero, &cause).await? {
            Mutated::Done { .. } => {}
            _ => return Err(crate::modules::stock::value::insufficient()),
        }
        if !back {
            sqlx::query(
                "UPDATE purchase_return_lines l SET movement_id = m.id, issue_value = -e.value_delta
                 FROM stock_movements m
                 JOIN inventory_value_entries e ON e.movement_id = m.id
                 WHERE l.workspace_id = $1 AND l.id = $2 AND m.workspace_id = $1
                   AND m.related_domain = 'purchase_return_line' AND m.related_id = $2
                   AND e.classification = 'supplier_return'",
            )
            .bind(ws)
            .bind(line)
            .execute(&mut *conn)
            .await?;
        }
    }
    Ok(())
}

fn source_lines(journal: &[JournalRow], supplier: Uuid) -> Vec<SourceLine> {
    journal
        .iter()
        .map(|j| SourceLine {
            side: role_side(j.role.as_deref(), j.account_id),
            debit: j.debit,
            credit: j.credit,
            relation_id: j
                .relation_id
                .or((j.role.as_deref() == Some("payables")).then_some(supplier)),
            bank_account_id: None,
        })
        .collect()
}

/// POST /purchase-returns/{id}/post: goods out at C, GRNI / supplier credit, explicit
/// variance, once.
pub async fn post(
    state: &AppState,
    access: &WorkspaceAccess,
    id: Uuid,
    cmd: Command<Action<ReviewedPayload>>,
) -> Result<Answer, AppError> {
    const OP: &str = "purchase_returns_post";
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, access.user_id, &cmd).await? {
        return Ok(a);
    }
    let (state_now, version) = lock(&mut tx, ws, id).await?;
    if version != cmd.input.expected_version {
        return Err(stale());
    }
    if state_now != "draft" {
        return Err(AppError::Unprocessable(NOT_DRAFT.into()));
    }
    let plan = prepare_post(&mut tx, ws, id, false).await?;
    if plan.review.review_fingerprint != cmd.input.payload {
        return Err(stale());
    }
    let x = row(&mut tx, ws, id).await?;
    move_goods(&mut tx, ws, id, x.accounting_date, false, access.user_id).await?;
    // C is now the value entry's; one line per pool keeps it equal to the reviewed preview.
    let (after, journal) = plan_rows(&mut tx, ws, id).await?;
    if after.iter().map(|p| p.issue_value).collect::<Vec<_>>()
        != plan.rows.iter().map(|p| p.issue_value).collect::<Vec<_>>()
        || journal
            .iter()
            .map(|j| (j.debit, j.credit))
            .collect::<Vec<_>>()
            != plan
                .journal
                .iter()
                .map(|j| (j.debit, j.credit))
                .collect::<Vec<_>>()
    {
        return Err(AppError::Internal(
            "return value differs from its review".into(),
        ));
    }
    sqlx::query(
        "UPDATE purchase_return_lines l SET grni_nominal = p.grni_nominal,
             grni_functional = p.grni_functional, credit_nominal = p.credit_nominal,
             credit_functional = p.credit_functional, tax_nominal = p.tax_nominal,
             tax_functional = p.tax_functional, fx = 0,
             variance = coalesce(p.issue_value, 0) - coalesce(p.grni_functional, 0)
                        - coalesce(p.credit_functional, 0)
         FROM purchase_return_plan($1, $2) p WHERE l.id = p.line_id",
    )
    .bind(ws)
    .bind(id)
    .execute(&mut *tx)
    .await?;
    let fp = fp32(&plan.review.review_fingerprint)?;
    let (_, effect) = post_source_lines(
        &mut tx,
        ws,
        &SourceHeader {
            source_kind: SOURCE,
            source_id: id,
            effect_kind: "posted",
            date: x.accounting_date,
            reason: "purchase return",
            fingerprint: &fp,
            by: access.user_id,
        },
        &source_lines(&journal, x.supplier_id),
    )
    .await?;
    // Billed lines: one supplier credit at the original basis.
    let (credit_n, credit_f): (i64, i64) = sqlx::query_as(
        "SELECT coalesce(sum(credit_nominal + tax_nominal), 0)::bigint,
                coalesce(sum(credit_functional + tax_functional), 0)::bigint
         FROM purchase_return_lines
         WHERE workspace_id = $1 AND return_id = $2 AND bill_match_id IS NOT NULL",
    )
    .bind(ws)
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    let mut credit_item = None;
    let mut application = None;
    if credit_n > 0 && credit_f > 0 {
        let payables = settlement::role_account(&mut tx, ws, "payables").await?;
        let item = settlement::open_item(
            &mut tx,
            ws,
            &NewOpenItem {
                classification: "supplier_credit",
                side: "debit",
                account_role: "payables",
                account_id: payables,
                source_kind: SOURCE,
                source_id: id,
                relation_id: Some(x.supplier_id),
                order_id: None,
                invoice_id: None,
                recognition_id: None,
                currency_id: x.currency_id,
                issue_date: x.accounting_date,
                due_date: None,
                origin: "posted",
                amount: credit_n,
                functional_amount: credit_f,
                effect_id: effect,
                by: access.user_id,
            },
        )
        .await?;
        credit_item = Some(item);
        if x.apply_credit {
            // The original bill's AP (one bill), as much as both still have open.
            let target: Option<(Uuid, i64)> = sqlx::query_as(
                "SELECT b.ap_item_id, LEAST(i.residual, $3)
                 FROM purchase_return_lines l
                 JOIN supplier_bill_matches m ON m.id = l.bill_match_id
                 JOIN supplier_bills b ON b.id = m.bill_id AND b.state = 'posted'
                 JOIN accounting_open_items i ON i.id = b.ap_item_id
                 WHERE l.workspace_id = $1 AND l.return_id = $2 AND i.residual > 0
                 GROUP BY b.ap_item_id, i.residual
                 HAVING count(DISTINCT b.id) = 1
                 ORDER BY b.ap_item_id LIMIT 1",
            )
            .bind(ws)
            .bind(id)
            .bind(credit_n)
            .fetch_optional(&mut *tx)
            .await?;
            if let Some((ap, minor)) = target {
                let r = common::relief(&mut tx, ws, item, ap, minor).await?;
                let (allocation, _) = common::apply(
                    &mut tx,
                    ws,
                    (item, ap),
                    &r,
                    (x.accounting_date, x.currency_id, x.supplier_id),
                    (SOURCE, id, "applied"),
                    &fp,
                    access.user_id,
                )
                .await?;
                application = Some(allocation);
            }
        }
    }
    sqlx::query(
        "UPDATE purchase_returns SET state = 'posted', credit_item_id = $3, application_id = $4,
             posted_effect_id = $5, posted_by = $6, posted_at = now(), version = version + 1,
             updated_at = now()
         WHERE workspace_id = $1 AND id = $2",
    )
    .bind(ws)
    .bind(id)
    .bind(credit_item)
    .bind(application)
    .bind(effect)
    .bind(access.user_id)
    .execute(&mut *tx)
    .await?;
    let answer = ok(&get(&mut tx, ws, id, access.allows("accounting.read")).await?);
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[id]).await?;
    tx.commit().await?;
    Ok(answer)
}

struct ReversePlan {
    review: ReverseReview,
    journal: Vec<JournalRow>,
}

async fn prepare_reverse(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
    input: &ReturnReverseInput,
    accounting: bool,
) -> Result<ReversePlan, AppError> {
    let x = row(&mut *conn, ws, id).await?;
    if x.state != "posted" {
        return Err(AppError::Unprocessable(NOT_POSTED.into()));
    }
    if !(input.financial && input.physical) {
        return Err(unsupported(ONE_COMPONENT));
    }
    let fc = functional(&mut *conn, ws).await?;
    if input.reverse.date < x.accounting_date {
        return Err(AppError::field(
            "payload.accounting_date",
            "A reversal can't be dated before the return.",
        ));
    }
    if let Some(item) = x.credit_item_id {
        let other: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM accounting_allocations a
                            WHERE a.workspace_id = $1 AND a.reversal_of IS NULL
                              AND (a.source_item_id = $2 OR a.target_item_id = $2)
                              AND a.id IS DISTINCT FROM $3
                              AND NOT EXISTS (SELECT 1 FROM accounting_allocations r
                                              WHERE r.reversal_of = a.id))",
        )
        .bind(ws)
        .bind(item)
        .bind(x.application_id)
        .fetch_one(&mut *conn)
        .await?;
        if other {
            return Err(dependent(
                "This return's supplier credit is applied or refunded. Reverse that first.",
            ));
        }
    }
    let mut journal = common::mirror(&mut *conn, ws, x.posted_effect_id).await?;
    if let Some(a) = x.application_id {
        let applied: Option<Uuid> = sqlx::query_scalar(
            "SELECT effect_id FROM accounting_allocations WHERE workspace_id = $1 AND id = $2",
        )
        .bind(ws)
        .bind(a)
        .fetch_optional(&mut *conn)
        .await?;
        // A bare application effect has no journal: nothing to add.
        journal.extend(common::mirror(&mut *conn, ws, applied).await?);
    }
    let fp: [u8; 32] = Sha256::digest(
        json!({
            "action": "reverse", "return": id, "version": x.version, "date": input.reverse.date,
            "reason": input.reverse.reason, "evidence": input.evidence_ref,
            "journal": journal.iter().map(|j| json!([j.account_id, j.debit, j.credit]))
                .collect::<Vec<_>>(),
        })
        .to_string(),
    )
    .into();
    Ok(ReversePlan {
        review: ReverseReview {
            id,
            version: x.version,
            accounting_date: input.reverse.date,
            reason: input.reverse.reason.clone(),
            journal: accounting.then(|| journal_preview(&journal, fc.decimals)),
            review_fingerprint: hex::encode(fp),
        },
        journal,
    })
}

/// POST /purchase-returns/{id}/reverse/review: no writes.
pub async fn reverse_review(
    state: &AppState,
    access: &WorkspaceAccess,
    id: Uuid,
    input: &ReturnReverseInput,
) -> Result<ReverseReview, AppError> {
    let mut tx = state.db.begin().await?;
    let plan = prepare_reverse(
        &mut tx,
        access.workspace_id,
        id,
        input,
        access.allows("accounting.read"),
    )
    .await?;
    tx.rollback().await?;
    Ok(plan.review)
}

/// POST /purchase-returns/{id}/reverse: goods back at their original C, own credit
/// application undone, credit closed, capacity restored.
pub async fn reverse(
    state: &AppState,
    access: &WorkspaceAccess,
    id: Uuid,
    cmd: Command<Action<ReturnReverseBody>>,
) -> Result<Answer, AppError> {
    const OP: &str = "purchase_returns_reverse";
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, access.user_id, &cmd).await? {
        return Ok(a);
    }
    let (_, version) = lock(&mut tx, ws, id).await?;
    if version != cmd.input.expected_version {
        return Err(stale());
    }
    let input = &cmd.input.payload;
    let x = row(&mut tx, ws, id).await?;
    if let Some(item) = x.credit_item_id {
        sqlx::query(
            "SELECT id FROM accounting_open_items WHERE workspace_id = $1 AND id = $2
             FOR NO KEY UPDATE",
        )
        .bind(ws)
        .bind(item)
        .execute(&mut *tx)
        .await?;
    }
    let plan = prepare_reverse(&mut tx, ws, id, input, false).await?;
    if input.review_fingerprint.as_deref() != Some(plan.review.review_fingerprint.as_str()) {
        return Err(stale());
    }
    move_goods(&mut tx, ws, id, input.reverse.date, true, access.user_id).await?;
    let effect = common::post_mirror(
        &mut tx,
        ws,
        (SOURCE, id),
        &input.reverse,
        &plan.journal,
        Some(x.supplier_id),
        &plan.review.review_fingerprint,
        access.user_id,
    )
    .await?;
    if let Some(a) = x.application_id {
        settlement::reverse(&mut tx, ws, a, input.reverse.date, effect, access.user_id).await?;
    }
    if let Some(item) = x.credit_item_id {
        common::close_item(
            &mut tx,
            ws,
            item,
            SOURCE,
            id,
            input.reverse.date,
            effect,
            access.user_id,
        )
        .await?;
    }
    sqlx::query(
        "UPDATE purchase_returns SET state = 'reversed', reversal_date = $3, reverse_reason = $4,
             reversal_effect_id = $5, reversed_by = $6, reversed_at = now(),
             version = version + 1, updated_at = now()
         WHERE workspace_id = $1 AND id = $2",
    )
    .bind(ws)
    .bind(id)
    .bind(input.reverse.date)
    .bind(&input.reverse.reason)
    .bind(effect)
    .bind(access.user_id)
    .execute(&mut *tx)
    .await?;
    let answer = ok(&get(&mut tx, ws, id, access.allows("accounting.read")).await?);
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[id]).await?;
    tx.commit().await?;
    Ok(answer)
}
