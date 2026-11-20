//! Supplier bills (masterplan §11 BILLS / BILL MATCHING, §13 FX/TAX, §15 SUPPLIER BILLS).
//! Draft bills reserve receipt capacity (draft matches); posting freezes `supplier_bill_plan`
//! (SQL): DR GRNI at remaining carrying, DR recoverable tax, DR expense, ±realized FX,
//! ±pending purchase cost variance, CR AP open item. Reverse appends the mirror journal, a
//! supplier_credit twin applied to the AP item, and releases the matches. A bill never moves
//! stock. Lock order: command row → shared gate → bill FOR UPDATE → receipt lines FOR NO KEY
//! UPDATE (id) → landed components FOR SHARE (id) → accounts/period → effect/journal number →
//! open items → number counter last (create).

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
            repo as books,
            service::{created, not_configured, ok, stale, unsupported},
            settlement::{self, NewOpenItem},
        },
        payables::{
            common::{self, dependent, journal_preview, role_side},
            dto::{
                BillInput, BillLandedCandidates, BillLandedRef, BillLineResource,
                BillMatchCandidates, BillMatchResource, BillPayload, BillPostReview, BillTotals,
                LandedCandidate, LineTarget, ListQuery, MatchCandidate, MatchInput, MatchPayload,
                ReverseInput, ReversePayload, ReverseReview, ReviewLine, ReviewMatch,
                ReviewedPayload, SupplierBillResource, TaxComponent,
            },
        },
        purchasing::{
            dto::{RateSnapshot, SupplierRef},
            receipts::{rate, snapshot},
            repo::Rate,
            service::{amount, qty},
        },
    },
    numbering::{self, Series},
};

const SOURCE: &str = "supplier_bill";
const NOT_DRAFT: &str = "Only draft bills can change.";
const NOT_POSTED: &str = "Only a posted bill can be reversed.";
const NONREC_GOODS: &str = "Nonrecoverable purchase tax on goods or landed costs would change booked stock cost. Bill it on an expense line.";
const NO_DOC_VALUE: &str =
    "This receipt line was booked before supplier bills existed. It can't be billed or returned.";

#[derive(FromRow)]
struct BillRow {
    id: Uuid,
    number: i64,
    state: String,
    version: i64,
    supplier_id: Uuid,
    supplier_name: Option<String>,
    currency_id: Uuid,
    currency_code: String,
    currency_decimals: i16,
    functional_decimals: Option<i16>,
    supplier_invoice_number: String,
    corrects_bill_id: Option<Uuid>,
    invoice_date: NaiveDate,
    due_date: Option<NaiveDate>,
    accounting_date: NaiveDate,
    evidence_ref: Option<String>,
    note: Option<String>,
    net: i64,
    tax: Option<i64>,
    total: Option<i64>,
    functional_total: Option<i64>,
    rate: Option<Decimal>,
    rate_inverse: Option<bool>,
    rate_source: Option<String>,
    rate_date: Option<NaiveDate>,
    ap_item_id: Option<Uuid>,
    posted_effect_id: Option<Uuid>,
    posted_at: Option<chrono::DateTime<chrono::Utc>>,
    reversed_at: Option<chrono::DateTime<chrono::Utc>>,
    reversal_date: Option<NaiveDate>,
    reverse_reason: Option<String>,
    created_by: Uuid,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(FromRow)]
struct LineRow {
    id: Uuid,
    bill_id: Uuid,
    line_no: i32,
    kind: String,
    product_id: Option<Uuid>,
    product_name: Option<String>,
    landed_cost_id: Option<Uuid>,
    expense_account_id: Option<Uuid>,
    quantity: Decimal,
    unit_price: Decimal,
    extension: i64,
    tax_profile_id: Option<Uuid>,
    description: Option<String>,
    tax: Option<i64>,
    landed_po_id: Option<Uuid>,
    landed_po_number: Option<i64>,
    landed_receipt_id: Option<Uuid>,
    landed_receipt_number: Option<i64>,
    landed_evidence: Option<String>,
    landed_amount: Option<i64>,
    landed_decimals: Option<i16>,
}

#[derive(FromRow)]
struct MatchRow {
    id: Uuid,
    bill_line_id: Uuid,
    receipt_line_id: Option<Uuid>,
    receipt_id: Option<Uuid>,
    purchase_order_id: Option<Uuid>,
    landed_cost_id: Option<Uuid>,
    quantity: Decimal,
    state: String,
    grni_functional: Option<i64>,
    fx: Option<i64>,
    variance: Option<i64>,
}

const BILL_SELECT: &str = "
    SELECT b.id, b.number, b.state, b.version, b.supplier_id,
           coalesce(r.company_name, r.alias) AS supplier_name, b.currency_id,
           upper(c.asset_type) AS currency_code, c.decimals AS currency_decimals,
           f.decimals AS functional_decimals, b.supplier_invoice_number, b.corrects_bill_id,
           b.invoice_date, b.due_date, b.accounting_date, b.evidence_ref, b.note,
           coalesce(b.net, (SELECT coalesce(sum(l.extension), 0) FROM supplier_bill_lines l
                            WHERE l.bill_id = b.id))::bigint AS net,
           b.tax, b.total, b.functional_total, b.rate, b.rate_inverse, b.rate_source,
           b.rate_date, b.ap_item_id, b.posted_effect_id, b.posted_at, b.reversed_at,
           b.reversal_date, b.reverse_reason, b.created_by, b.created_at, b.updated_at,
           count(*) OVER () AS total_rows
    FROM supplier_bills b
    JOIN currencies c ON c.workspace_id = b.workspace_id AND c.id = b.currency_id
    JOIN relations r ON r.workspace_id = b.workspace_id AND r.id = b.supplier_id
    JOIN accounting_settings s ON s.workspace_id = b.workspace_id
    LEFT JOIN currencies f ON f.workspace_id = b.workspace_id AND f.id = s.functional_currency_id
    WHERE b.workspace_id = $1";

async fn resources(
    conn: &mut PgConnection,
    ws: Uuid,
    rows: Vec<BillRow>,
    accounting: bool,
) -> Result<Vec<SupplierBillResource>, AppError> {
    let ids: Vec<Uuid> = rows.iter().map(|b| b.id).collect();
    let lines: Vec<LineRow> = sqlx::query_as(
        "SELECT l.id, l.bill_id, l.line_no, l.kind, l.product_id, p.name AS product_name,
                l.landed_cost_id, l.expense_account_id, l.quantity, l.unit_price, l.extension,
                l.tax_profile_id, l.description, (l.tax_recoverable + l.tax_nonrecoverable) AS tax,
                lo.id AS landed_po_id, lo.number AS landed_po_number,
                lr.id AS landed_receipt_id, lr.number AS landed_receipt_number,
                lc.evidence_ref AS landed_evidence, lc.amount AS landed_amount,
                lcc.decimals AS landed_decimals
         FROM supplier_bill_lines l
         LEFT JOIN products p ON p.workspace_id = l.workspace_id AND p.id = l.product_id
         LEFT JOIN purchase_landed_costs lc
                ON lc.workspace_id = l.workspace_id AND lc.id = l.landed_cost_id
         LEFT JOIN purchase_receipts lr ON lr.workspace_id = lc.workspace_id AND lr.id = lc.receipt_id
         LEFT JOIN purchase_orders lo
                ON lo.workspace_id = lc.workspace_id AND lo.id = lc.purchase_order_id
         LEFT JOIN currencies lcc ON lcc.workspace_id = lc.workspace_id AND lcc.id = lc.currency_id
         WHERE l.workspace_id = $1 AND l.bill_id = ANY($2) ORDER BY l.bill_id, l.line_no",
    )
    .bind(ws)
    .bind(&ids)
    .fetch_all(&mut *conn)
    .await?;
    let matches: Vec<MatchRow> = sqlx::query_as(
        "SELECT m.id, m.bill_line_id, m.receipt_line_id, rl.receipt_id, r.purchase_order_id,
                m.landed_cost_id, m.quantity, m.state, m.grni_functional, m.fx, m.variance
         FROM supplier_bill_matches m
         LEFT JOIN purchase_receipt_lines rl
                ON rl.workspace_id = m.workspace_id AND rl.id = m.receipt_line_id
         LEFT JOIN purchase_receipts r ON r.workspace_id = rl.workspace_id AND r.id = rl.receipt_id
         WHERE m.workspace_id = $1 AND m.bill_id = ANY($2)
         ORDER BY m.id",
    )
    .bind(ws)
    .bind(&ids)
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|b| {
            let dp = b.currency_decimals;
            let fdp = b.functional_decimals.unwrap_or(2);
            let f = |v: Option<i64>| v.filter(|_| accounting).map(|v| amount(v, fdp));
            let allowed_actions = match b.state.as_str() {
                "draft" => vec!["update", "match", "post"],
                "posted" => vec!["reverse"],
                _ => vec![],
            };
            SupplierBillResource {
                id: b.id,
                workspace_id: ws,
                number: b.number,
                version: b.version,
                supplier: SupplierRef {
                    id: b.supplier_id,
                    name: b.supplier_name,
                },
                currency: CurrencyRef {
                    id: b.currency_id,
                    code: b.currency_code,
                    decimals: dp,
                },
                supplier_invoice_number: b.supplier_invoice_number,
                corrects_bill_id: b.corrects_bill_id,
                invoice_date: b.invoice_date,
                due_date: b.due_date,
                accounting_date: b.accounting_date,
                evidence_ref: b.evidence_ref,
                note: b.note,
                totals: BillTotals {
                    net: amount(b.net, dp),
                    tax: b.tax.map(|v| amount(v, dp)),
                    total: b.total.map(|v| amount(v, dp)),
                    functional_total: f(b.functional_total),
                },
                rate: b.rate.map(|r| RateSnapshot {
                    rate: r.normalize().to_string(),
                    inverse: b.rate_inverse.unwrap_or(false),
                    source: b.rate_source.clone().unwrap_or_default(),
                    date: b.rate_date,
                }),
                ap_item_id: b.ap_item_id,
                lines: lines
                    .iter()
                    .filter(|l| l.bill_id == b.id)
                    .map(|l| BillLineResource {
                        id: l.id,
                        line_no: l.line_no,
                        kind: l.kind.clone(),
                        product_id: l.product_id,
                        product_name: l.product_name.clone(),
                        landed_cost_id: l.landed_cost_id,
                        landed: landed_ref(l),
                        expense_account_id: l.expense_account_id,
                        quantity: qty(l.quantity),
                        unit_price: qty(l.unit_price),
                        extension: amount(l.extension, dp),
                        tax_profile_id: l.tax_profile_id,
                        description: l.description.clone(),
                        tax: l.tax.map(|v| amount(v, dp)),
                        matches: matches
                            .iter()
                            .filter(|m| m.bill_line_id == l.id)
                            .map(|m| BillMatchResource {
                                id: m.id,
                                receipt_line_id: m.receipt_line_id,
                                receipt_id: m.receipt_id,
                                purchase_order_id: m.purchase_order_id,
                                landed_cost_id: m.landed_cost_id,
                                quantity: qty(m.quantity),
                                state: m.state.clone(),
                                grni_functional: f(m.grni_functional),
                                fx: f(m.fx),
                                variance: f(m.variance),
                            })
                            .collect(),
                    })
                    .collect(),
                posted_at: b.posted_at,
                reversed_at: b.reversed_at,
                reversal_date: b.reversal_date,
                reverse_reason: b.reverse_reason,
                created_by: b.created_by,
                created_at: b.created_at,
                updated_at: b.updated_at,
                allowed_actions,
                state: b.state,
            }
        })
        .collect())
}

fn landed_ref(l: &LineRow) -> Option<BillLandedRef> {
    Some(BillLandedRef {
        purchase_order_id: l.landed_po_id?,
        purchase_order_number: l.landed_po_number?,
        receipt_id: l.landed_receipt_id?,
        receipt_number: l.landed_receipt_number?,
        evidence_ref: l.landed_evidence.clone()?,
        amount: amount(l.landed_amount?, l.landed_decimals?),
    })
}

async fn row(conn: &mut PgConnection, ws: Uuid, id: Uuid) -> Result<BillRow, AppError> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!("{BILL_SELECT} AND b.id = $2")))
        .bind(ws)
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(AppError::NotFound)
}

/// One bill; 404 outside the workspace.
pub async fn get(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
    accounting: bool,
) -> Result<SupplierBillResource, AppError> {
    let b = row(&mut *conn, ws, id).await?;
    resources(conn, ws, vec![b], accounting)
        .await?
        .pop()
        .ok_or(AppError::NotFound)
}

/// GET /supplier-bills: newest first.
pub async fn list(
    state: &AppState,
    ws: Uuid,
    q: &ListQuery,
    accounting: bool,
) -> Result<Paginated<SupplierBillResource>, AppError> {
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    let mut conn = state.db.acquire().await?;
    let rows: Vec<(BillRow, i64)> = sqlx::query(sqlx::AssertSqlSafe(format!(
        "{BILL_SELECT} AND ($2::uuid IS NULL OR b.supplier_id = $2)
         ORDER BY b.number DESC LIMIT $3 OFFSET $4"
    )))
    .bind(ws)
    .bind(q.supplier_id)
    .bind(page.size)
    .bind(page.offset())
    .fetch_all(&mut *conn)
    .await?
    .iter()
    .map(|r| {
        use sqlx::Row;
        Ok((BillRow::from_row(r)?, r.try_get::<i64, _>("total_rows")?))
    })
    .collect::<Result<_, sqlx::Error>>()?;
    let total = rows.first().map_or(0, |r| r.1);
    let data = resources(
        &mut conn,
        ws,
        rows.into_iter().map(|r| r.0).collect(),
        accounting,
    )
    .await?;
    Ok(Paginated::new(data, total, page))
}

#[derive(FromRow)]
struct CandidateRow {
    bill_line_id: Option<Uuid>,
    receipt_line_id: Option<Uuid>,
    receipt_id: Option<Uuid>,
    receipt_number: Option<i64>,
    purchase_order_id: Option<Uuid>,
    purchase_order_number: Option<i64>,
    booked_at: Option<chrono::DateTime<chrono::Utc>>,
    matchable: Option<Decimal>,
}

/// GET /supplier-bills/{id}/match-candidates: booked receipt lines of the bill's supplier,
/// currency and goods-line product with capacity left (`receipt_line_capacity`, this bill's own
/// reservations excluded). Read-only, one query; 404 outside the workspace.
// shortcut: first 500 candidates, page when a bill outgrows it.
pub async fn match_candidates(
    state: &AppState,
    ws: Uuid,
    id: Uuid,
) -> Result<BillMatchCandidates, AppError> {
    let rows: Vec<CandidateRow> = sqlx::query_as(
        "SELECT x.* FROM supplier_bills b
         LEFT JOIN LATERAL (
             SELECT bl.id AS bill_line_id, l.id AS receipt_line_id, r.id AS receipt_id,
                    r.number AS receipt_number, o.id AS purchase_order_id,
                    o.number AS purchase_order_number, l.booked_at, c.matchable, bl.line_no
             FROM supplier_bill_lines bl
             JOIN purchase_orders o ON o.workspace_id = b.workspace_id
                  AND o.supplier_id = b.supplier_id AND o.currency_id = b.currency_id
             JOIN purchase_receipts r ON r.workspace_id = o.workspace_id
                  AND r.purchase_order_id = o.id
             JOIN purchase_receipt_lines l ON l.workspace_id = r.workspace_id
                  AND l.receipt_id = r.id AND l.product_id = bl.product_id
             JOIN LATERAL receipt_line_capacity(b.workspace_id, l.id, b.id) c ON true
             WHERE bl.workspace_id = b.workspace_id AND bl.bill_id = b.id
               AND bl.kind = 'goods' AND c.matchable > 0
             ORDER BY bl.line_no, l.booked_at, l.id
             LIMIT 500) x ON true
         WHERE b.workspace_id = $1 AND b.id = $2",
    )
    .bind(ws)
    .bind(id)
    .fetch_all(&state.db)
    .await?;
    if rows.is_empty() {
        return Err(AppError::NotFound);
    }
    Ok(BillMatchCandidates {
        bill_id: id,
        candidates: rows
            .into_iter()
            .filter_map(|c| {
                Some(MatchCandidate {
                    bill_line_id: c.bill_line_id?,
                    receipt_line_id: c.receipt_line_id?,
                    receipt_id: c.receipt_id?,
                    receipt_number: c.receipt_number?,
                    purchase_order_id: c.purchase_order_id?,
                    purchase_order_number: c.purchase_order_number?,
                    booked_at: c.booked_at,
                    matchable: qty(c.matchable?),
                })
            })
            .collect(),
    })
}

#[derive(FromRow)]
struct LandedCandidateRow {
    landed_cost_id: Uuid,
    purchase_order_id: Uuid,
    purchase_order_number: i64,
    receipt_id: Uuid,
    receipt_number: i64,
    evidence_ref: String,
    amount: i64,
    decimals: i16,
}

/// GET /supplier-bills/landed-candidates: booked landed components in `currency` on no live bill
/// (draft or posted; `bill`'s own stay listed). Read-only, one query; workspace-scoped.
// shortcut: first 500 components, page when a workspace outgrows it.
pub async fn landed_candidates(
    state: &AppState,
    ws: Uuid,
    currency: Uuid,
    bill: Option<Uuid>,
) -> Result<BillLandedCandidates, AppError> {
    let rows: Vec<LandedCandidateRow> = sqlx::query_as(
        "SELECT lc.id AS landed_cost_id, o.id AS purchase_order_id,
                o.number AS purchase_order_number, r.id AS receipt_id, r.number AS receipt_number,
                lc.evidence_ref, lc.amount, c.decimals
         FROM purchase_landed_costs lc
         JOIN purchase_receipts r ON r.workspace_id = lc.workspace_id AND r.id = lc.receipt_id
         JOIN purchase_orders o ON o.workspace_id = lc.workspace_id AND o.id = lc.purchase_order_id
         JOIN currencies c ON c.workspace_id = lc.workspace_id AND c.id = lc.currency_id
         WHERE lc.workspace_id = $1 AND lc.currency_id = $2
           AND NOT EXISTS (SELECT 1 FROM supplier_bill_matches m
                           WHERE m.workspace_id = lc.workspace_id AND m.landed_cost_id = lc.id
                             AND m.state <> 'reversed' AND m.bill_id IS DISTINCT FROM $3)
         ORDER BY lc.created_at DESC, lc.id
         LIMIT 500",
    )
    .bind(ws)
    .bind(currency)
    .bind(bill)
    .fetch_all(&state.db)
    .await?;
    Ok(BillLandedCandidates {
        candidates: rows
            .into_iter()
            .map(|c| LandedCandidate {
                landed_cost_id: c.landed_cost_id,
                component: BillLandedRef {
                    purchase_order_id: c.purchase_order_id,
                    purchase_order_number: c.purchase_order_number,
                    receipt_id: c.receipt_id,
                    receipt_number: c.receipt_number,
                    evidence_ref: c.evidence_ref,
                    amount: amount(c.amount, c.decimals),
                },
            })
            .collect(),
    })
}

fn invoice_taken() -> AppError {
    AppError::field(
        "payload.supplier_invoice_number",
        "This supplier invoice number is already on a bill for this supplier and currency.",
    )
}

/// Check references; field errors under `payload.*`.
async fn check(
    conn: &mut PgConnection,
    ws: Uuid,
    bill: Option<Uuid>,
    input: &BillInput,
) -> Result<(), AppError> {
    let mut r = Rules::default();
    let supplier: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM relations
                        WHERE workspace_id = $1 AND id = $2 AND deleted_at IS NULL)",
    )
    .bind(ws)
    .bind(input.supplier)
    .fetch_one(&mut *conn)
    .await?;
    r.check(
        "payload.supplier_id",
        supplier,
        "The supplier was not found.",
    );
    let currency = books::currency(&mut *conn, ws, input.currency).await?;
    r.check(
        "payload.currency_id",
        currency.is_some(),
        "The currency was not found.",
    );
    // Duplicate number: a live bill, or a reversed one not named as corrected.
    let (live, reversed): (bool, Option<Uuid>) = sqlx::query_as(
        "SELECT EXISTS (SELECT 1 FROM supplier_bills
                        WHERE workspace_id = $1 AND supplier_id = $2 AND currency_id = $3
                          AND lower(btrim(supplier_invoice_number)) = lower(btrim($4))
                          AND state <> 'reversed' AND id IS DISTINCT FROM $5),
                (SELECT id FROM supplier_bills
                 WHERE workspace_id = $1 AND supplier_id = $2 AND currency_id = $3
                   AND lower(btrim(supplier_invoice_number)) = lower(btrim($4))
                   AND state = 'reversed' AND id = $6)",
    )
    .bind(ws)
    .bind(input.supplier)
    .bind(input.currency)
    .bind(&input.supplier_invoice_number)
    .bind(bill)
    .bind(input.corrects)
    .fetch_one(&mut *conn)
    .await?;
    let any_reversed: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM supplier_bills
                        WHERE workspace_id = $1 AND supplier_id = $2 AND currency_id = $3
                          AND lower(btrim(supplier_invoice_number)) = lower(btrim($4))
                          AND state = 'reversed')",
    )
    .bind(ws)
    .bind(input.supplier)
    .bind(input.currency)
    .bind(&input.supplier_invoice_number)
    .fetch_one(&mut *conn)
    .await?;
    if live || (any_reversed && reversed.is_none()) {
        r.add(
            "payload.supplier_invoice_number",
            "This supplier invoice number is already on a bill for this supplier and currency.",
        );
    }
    if input.corrects.is_some() && reversed.is_none() {
        r.add(
            "payload.corrects_bill_id",
            "Name a reversed bill of this supplier, currency and invoice number.",
        );
    }
    for (i, l) in input.lines.iter().enumerate() {
        let f = |name: &str| format!("payload.lines.{i}.{name}");
        match l.target {
            LineTarget::Goods(p) => {
                let found: bool = sqlx::query_scalar(
                    "SELECT EXISTS (SELECT 1 FROM products
                                    WHERE workspace_id = $1 AND id = $2 AND deleted_at IS NULL)",
                )
                .bind(ws)
                .bind(p)
                .fetch_one(&mut *conn)
                .await?;
                r.check(&f("product_id"), found, "The product was not found.");
            }
            LineTarget::Landed(c) => {
                let cur: Option<Uuid> = sqlx::query_scalar(
                    "SELECT currency_id FROM purchase_landed_costs
                     WHERE workspace_id = $1 AND id = $2",
                )
                .bind(ws)
                .bind(c)
                .fetch_optional(&mut *conn)
                .await?;
                match cur {
                    None => r.add(&f("landed_cost_id"), "The landed cost was not found."),
                    Some(cur) => r.check(
                        &f("landed_cost_id"),
                        cur == input.currency,
                        "A landed cost is billed in its own currency.",
                    ),
                }
                r.check(
                    &f("landed_cost_id"),
                    input.lines.iter().filter(|x| x.target == l.target).count() == 1,
                    "Each landed cost may appear once.",
                );
            }
            LineTarget::Expense(a) => {
                let ok: bool = sqlx::query_scalar(
                    "SELECT EXISTS (SELECT 1 FROM accounts a
                                    WHERE a.workspace_id = $1 AND a.id = $2 AND a.active
                                      AND a.type = 'expense'
                                      AND NOT EXISTS (SELECT 1 FROM account_mappings m
                                                      WHERE m.workspace_id = a.workspace_id
                                                        AND m.account_id = a.id))",
                )
                .bind(ws)
                .bind(a)
                .fetch_one(&mut *conn)
                .await?;
                r.check(
                    &f("expense_account_id"),
                    ok,
                    "Use an active expense account that is not a mapped control account.",
                );
            }
        }
        if let Some(p) = l.tax_profile {
            let found: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM tax_profiles WHERE workspace_id = $1 AND id = $2)",
            )
            .bind(ws)
            .bind(p)
            .fetch_one(&mut *conn)
            .await?;
            r.check(
                &f("tax_profile_id"),
                found,
                "The tax profile was not found.",
            );
        }
    }
    r.into_result()
}

/// Unique violations of the duplicate-number or landed-once indexes as field errors.
fn unique(e: sqlx::Error) -> AppError {
    let db = e.as_database_error();
    let constraint = db.and_then(|d| d.constraint()).map(str::to_owned);
    let code = db.and_then(|d| d.code()).map(|c| c.into_owned());
    match (constraint.as_deref(), code.as_deref()) {
        (Some("supplier_bills_invoice_key"), _) => invoice_taken(),
        (Some("supplier_bill_matches_landed_key"), _) => AppError::field(
            "payload.lines",
            "A landed cost on these lines is already on another bill.",
        ),
        (_, Some("22003")) => AppError::field("payload.lines", "A line total is too large."),
        _ => e.into(),
    }
}

/// Write the draft's lines (extension in SQL) and the landed reservations.
async fn write_lines(
    conn: &mut PgConnection,
    ws: Uuid,
    bill: Uuid,
    input: &BillInput,
) -> Result<(), AppError> {
    sqlx::query("DELETE FROM supplier_bill_lines WHERE workspace_id = $1 AND bill_id = $2")
        .bind(ws)
        .bind(bill)
        .execute(&mut *conn)
        .await?;
    let kind = |t: &LineTarget| match t {
        LineTarget::Goods(_) => "goods",
        LineTarget::Landed(_) => "landed_cost",
        LineTarget::Expense(_) => "expense",
    };
    let pick = |f: fn(&LineTarget) -> Option<Uuid>| {
        input.lines.iter().map(|l| f(&l.target)).collect::<Vec<_>>()
    };
    sqlx::query(
        "INSERT INTO supplier_bill_lines (workspace_id, bill_id, line_no, kind, product_id,
             landed_cost_id, expense_account_id, quantity, unit_price, extension,
             tax_profile_id, description)
         SELECT $1, $2, n::int, x.kind, x.product, x.landed, x.account, x.q, x.price,
                money_round_div(x.q * x.price * 10::numeric ^ c.decimals, 1)::bigint,
                x.tax, x.descr
         FROM unnest($3::text[], $4::uuid[], $5::uuid[], $6::uuid[], $7::numeric[],
                     $8::numeric[], $9::uuid[], $10::text[])
              WITH ORDINALITY AS x(kind, product, landed, account, q, price, tax, descr, n)
         JOIN supplier_bills b ON b.workspace_id = $1 AND b.id = $2
         JOIN currencies c ON c.workspace_id = b.workspace_id AND c.id = b.currency_id",
    )
    .bind(ws)
    .bind(bill)
    .bind(
        input
            .lines
            .iter()
            .map(|l| kind(&l.target))
            .collect::<Vec<_>>(),
    )
    .bind(pick(|t| match t {
        LineTarget::Goods(p) => Some(*p),
        _ => None,
    }))
    .bind(pick(|t| match t {
        LineTarget::Landed(c) => Some(*c),
        _ => None,
    }))
    .bind(pick(|t| match t {
        LineTarget::Expense(a) => Some(*a),
        _ => None,
    }))
    .bind(input.lines.iter().map(|l| l.quantity).collect::<Vec<_>>())
    .bind(input.lines.iter().map(|l| l.unit_price).collect::<Vec<_>>())
    .bind(
        input
            .lines
            .iter()
            .map(|l| l.tax_profile)
            .collect::<Vec<_>>(),
    )
    .bind(
        input
            .lines
            .iter()
            .map(|l| l.description.clone())
            .collect::<Vec<_>>(),
    )
    .execute(&mut *conn)
    .await
    .map_err(unique)?;
    sqlx::query(
        "INSERT INTO supplier_bill_matches (workspace_id, bill_id, bill_line_id, landed_cost_id,
             quantity)
         SELECT $1, $2, id, landed_cost_id, 1 FROM supplier_bill_lines
         WHERE workspace_id = $1 AND bill_id = $2 AND kind = 'landed_cost'",
    )
    .bind(ws)
    .bind(bill)
    .execute(&mut *conn)
    .await
    .map_err(unique)?;
    Ok(())
}

/// POST /supplier-bills: a numbered draft.
pub async fn create(
    state: &AppState,
    access: &WorkspaceAccess,
    cmd: Command<Create<BillPayload>>,
) -> Result<Answer, AppError> {
    const OP: &str = "supplier_bills_create";
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, access.user_id, &cmd).await? {
        return Ok(a);
    }
    gate::shared(&mut tx, &[ws]).await?;
    let input = &cmd.input.payload;
    check(&mut tx, ws, None, input).await?;
    let number = numbering::next(&mut tx, ws, Series::SupplierBill, 1).await?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO supplier_bills (workspace_id, number, supplier_id, currency_id,
             supplier_invoice_number, corrects_bill_id, invoice_date, due_date, accounting_date,
             evidence_ref, note, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12) RETURNING id",
    )
    .bind(ws)
    .bind(number)
    .bind(input.supplier)
    .bind(input.currency)
    .bind(&input.supplier_invoice_number)
    .bind(input.corrects)
    .bind(input.invoice_date)
    .bind(input.due_date)
    .bind(input.accounting_date)
    .bind(&input.evidence_ref)
    .bind(&input.note)
    .bind(access.user_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(unique)?;
    write_lines(&mut tx, ws, id, input).await?;
    let answer = created(&get(&mut tx, ws, id, access.allows("accounting.read")).await?);
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[id]).await?;
    tx.commit().await?;
    Ok(answer)
}

/// Shared gate, then the bill FOR UPDATE; (state, version). 404 outside `ws`.
async fn lock(conn: &mut PgConnection, ws: Uuid, id: Uuid) -> Result<(String, i64), AppError> {
    gate::shared(&mut *conn, &[ws]).await?;
    sqlx::query_as(
        "SELECT state, version FROM supplier_bills WHERE workspace_id = $1 AND id = $2
         FOR UPDATE",
    )
    .bind(ws)
    .bind(id)
    .fetch_optional(conn)
    .await?
    .ok_or(AppError::NotFound)
}

async fn lock_draft(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
    expected: i64,
) -> Result<(), AppError> {
    let (state, version) = lock(&mut *conn, ws, id).await?;
    if version != expected {
        return Err(stale());
    }
    if state != "draft" {
        return Err(AppError::Unprocessable(NOT_DRAFT.into()));
    }
    Ok(())
}

async fn touch(conn: &mut PgConnection, ws: Uuid, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE supplier_bills SET version = version + 1, updated_at = now()
         WHERE workspace_id = $1 AND id = $2",
    )
    .bind(ws)
    .bind(id)
    .execute(conn)
    .await?;
    Ok(())
}

/// PATCH /supplier-bills/{id}: replace a draft's fields and lines (its goods reservations go
/// with the old lines).
pub async fn update(
    state: &AppState,
    access: &WorkspaceAccess,
    id: Uuid,
    cmd: Command<Action<BillPayload>>,
) -> Result<Answer, AppError> {
    const OP: &str = "supplier_bills_update";
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, access.user_id, &cmd).await? {
        return Ok(a);
    }
    lock_draft(&mut tx, ws, id, cmd.input.expected_version).await?;
    let input = &cmd.input.payload;
    check(&mut tx, ws, Some(id), input).await?;
    sqlx::query(
        "UPDATE supplier_bills SET supplier_id = $3, currency_id = $4,
             supplier_invoice_number = $5, corrects_bill_id = $6, invoice_date = $7,
             due_date = $8, accounting_date = $9, evidence_ref = $10, note = $11
         WHERE workspace_id = $1 AND id = $2",
    )
    .bind(ws)
    .bind(id)
    .bind(input.supplier)
    .bind(input.currency)
    .bind(&input.supplier_invoice_number)
    .bind(input.corrects)
    .bind(input.invoice_date)
    .bind(input.due_date)
    .bind(input.accounting_date)
    .bind(&input.evidence_ref)
    .bind(&input.note)
    .execute(&mut *tx)
    .await
    .map_err(unique)?;
    write_lines(&mut tx, ws, id, input).await?;
    touch(&mut tx, ws, id).await?;
    let answer = ok(&get(&mut tx, ws, id, access.allows("accounting.read")).await?);
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[id]).await?;
    tx.commit().await?;
    Ok(answer)
}

/// Lock `lines` (receipt lines of `ws`) FOR NO KEY UPDATE in id order: every capacity reader
/// and writer of a receipt line passes here first.
pub(crate) async fn lock_receipt_lines(
    conn: &mut PgConnection,
    ws: Uuid,
    lines: &[Uuid],
) -> Result<(), AppError> {
    let mut ids = lines.to_vec();
    ids.sort_unstable();
    ids.dedup();
    let found: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM purchase_receipt_lines WHERE workspace_id = $1 AND id = ANY($2)
         ORDER BY id FOR NO KEY UPDATE",
    )
    .bind(ws)
    .bind(&ids)
    .fetch_all(conn)
    .await?;
    if found.len() == ids.len() {
        Ok(())
    } else {
        Err(AppError::NotFound)
    }
}

/// Facts of a receipt line a bill or return may use: (product, PO supplier, PO currency,
/// capacity (B, U, M, D, matchable, returned)), None when not booked with values.
#[derive(FromRow)]
pub(crate) struct ReceiptFacts {
    pub product_id: Uuid,
    pub supplier_id: Uuid,
    pub currency_id: Uuid,
    pub valued: bool,
    pub matchable: Option<Decimal>,
}

pub(crate) async fn receipt_facts(
    conn: &mut PgConnection,
    ws: Uuid,
    line: Uuid,
    except_bill: Option<Uuid>,
) -> Result<ReceiptFacts, AppError> {
    sqlx::query_as(
        "SELECT l.product_id, o.supplier_id, o.currency_id, c.matchable IS NOT NULL AS valued,
                c.matchable
         FROM purchase_receipt_lines l
         JOIN purchase_receipts r ON r.workspace_id = l.workspace_id AND r.id = l.receipt_id
         JOIN purchase_orders o ON o.workspace_id = r.workspace_id AND o.id = r.purchase_order_id
         LEFT JOIN LATERAL receipt_line_capacity($1, l.id, $3) c ON true
         WHERE l.workspace_id = $1 AND l.id = $2",
    )
    .bind(ws)
    .bind(line)
    .bind(except_bill)
    .fetch_optional(conn)
    .await?
    .ok_or(AppError::NotFound)
}

/// Refuse a receipt line that isn't booked with values (or predates doc values).
pub(crate) fn require_valued(f: &ReceiptFacts) -> Result<(), AppError> {
    if f.valued {
        Ok(())
    } else {
        Err(unsupported(NO_DOC_VALUE))
    }
}

/// POST /supplier-bills/{id}/match: replace the draft's goods reservations.
pub async fn match_lines(
    state: &AppState,
    access: &WorkspaceAccess,
    id: Uuid,
    cmd: Command<Action<MatchPayload>>,
) -> Result<Answer, AppError> {
    const OP: &str = "supplier_bills_match";
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, access.user_id, &cmd).await? {
        return Ok(a);
    }
    lock_draft(&mut tx, ws, id, cmd.input.expected_version).await?;
    let input: &MatchInput = &cmd.input.payload;
    let (supplier, currency): (Uuid, Uuid) = sqlx::query_as(
        "SELECT supplier_id, currency_id FROM supplier_bills WHERE workspace_id = $1 AND id = $2",
    )
    .bind(ws)
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    let lines: Vec<(Uuid, String, Option<Uuid>, Decimal)> = sqlx::query_as(
        "SELECT id, kind, product_id, quantity FROM supplier_bill_lines
         WHERE workspace_id = $1 AND bill_id = $2",
    )
    .bind(ws)
    .bind(id)
    .fetch_all(&mut *tx)
    .await?;
    let receipt_lines: Vec<Uuid> = input.iter().map(|m| m.1).collect();
    lock_receipt_lines(&mut tx, ws, &receipt_lines).await?;
    sqlx::query(
        "DELETE FROM supplier_bill_matches
         WHERE workspace_id = $1 AND bill_id = $2 AND receipt_line_id IS NOT NULL",
    )
    .bind(ws)
    .bind(id)
    .execute(&mut *tx)
    .await?;
    let mut r = Rules::default();
    for (i, (line, receipt_line, q)) in input.iter().enumerate() {
        let f = |name: &str| format!("payload.allocations.{i}.{name}");
        let Some(bl) = lines.iter().find(|l| l.0 == *line) else {
            return Err(AppError::NotFound);
        };
        if bl.1 != "goods" {
            r.add(&f("bill_line_id"), "Only goods lines match receipt lines.");
            continue;
        }
        let facts = receipt_facts(&mut tx, ws, *receipt_line, Some(id)).await?;
        require_valued(&facts)?;
        r.check(
            &f("receipt_line_id"),
            Some(facts.product_id) == bl.2,
            "The receipt line is for another product.",
        );
        r.check(
            &f("receipt_line_id"),
            facts.supplier_id == supplier && facts.currency_id == currency,
            "The receipt is from another supplier or in another currency.",
        );
        r.check(
            &f("quantity"),
            facts.matchable.is_some_and(|m| *q <= m),
            "More than this receipt line still has to bill.",
        );
    }
    for l in lines.iter().filter(|l| l.1 == "goods") {
        let sum: Decimal = input.iter().filter(|m| m.0 == l.0).map(|m| m.2).sum();
        r.check(
            "payload.allocations",
            sum <= l.3,
            "A line can't match more than its quantity.",
        );
    }
    r.into_result()?;
    for (line, receipt_line, q) in input {
        sqlx::query(
            "INSERT INTO supplier_bill_matches (workspace_id, bill_id, bill_line_id,
                 receipt_line_id, quantity)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(ws)
        .bind(id)
        .bind(line)
        .bind(receipt_line)
        .bind(q)
        .execute(&mut *tx)
        .await?;
    }
    touch(&mut tx, ws, id).await?;
    let answer = ok(&get(&mut tx, ws, id, access.allows("accounting.read")).await?);
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[id]).await?;
    tx.commit().await?;
    Ok(answer)
}

/// One row of `supplier_bill_plan`.
#[derive(FromRow, Clone)]
pub(crate) struct PlanRow {
    row_kind: String,
    id: Uuid,
    line_id: Option<Uuid>,
    pub nominal: Option<i64>,
    pub tax_recoverable: Option<i64>,
    pub tax_nonrecoverable: Option<i64>,
    tax_functional: Option<i64>,
    grni_nominal: Option<i64>,
    grni_functional: Option<i64>,
    share_functional: Option<i64>,
    fx: Option<i64>,
    variance: Option<i64>,
    functional: Option<i64>,
}

/// One row of `supplier_bill_journal`.
#[derive(FromRow, Clone)]
pub(crate) struct JournalRow {
    pub role: Option<String>,
    pub account_id: Option<Uuid>,
    pub debit: i64,
    pub credit: i64,
    /// Party of a control line (mirrors keep the original's).
    #[sqlx(default)]
    pub relation_id: Option<Uuid>,
}

/// `tax_line` refusals (22023 with a message key) as a field error.
fn tax_refused(e: sqlx::Error) -> AppError {
    match e
        .as_database_error()
        .map(|d| (d.code(), d.message().to_owned()))
    {
        Some((Some(c), m)) if c == "22023" => AppError::field(
            "tax_profile_id",
            match m.as_str() {
                "tax_profile_not_verified" => "The tax profile is not verified.",
                "tax_profile_not_effective" => {
                    "The tax profile is not in force on the invoice date."
                }
                "tax_profile_wrong_transaction" => "The tax profile is not for purchases.",
                _ => "The tax profile doesn't apply to this line.",
            },
        ),
        _ => e.into(),
    }
}

struct PostPlan {
    review: BillPostReview,
    rows: Vec<PlanRow>,
    journal: Vec<JournalRow>,
    rate: Rate,
    supplier: Uuid,
    due_date: Option<NaiveDate>,
}

/// The functional currency of an active book; 422 `accounting_not_configured` otherwise.
pub(crate) async fn functional(conn: &mut PgConnection, ws: Uuid) -> Result<CurrencyRef, AppError> {
    let s = books::settings(&mut *conn, ws, false).await?;
    let id = s
        .functional_currency_id
        .filter(|_| s.state == "active")
        .ok_or_else(not_configured)?;
    books::currency(&mut *conn, ws, id)
        .await?
        .ok_or_else(not_configured)
}

/// What posting the draft bill books now; no writes.
async fn prepare_post(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
    accounting: bool,
) -> Result<PostPlan, AppError> {
    let b = row(&mut *conn, ws, id).await?;
    if b.state != "draft" {
        return Err(AppError::Unprocessable(NOT_DRAFT.into()));
    }
    let fc = functional(&mut *conn, ws).await?;
    let currency = CurrencyRef {
        id: b.currency_id,
        code: b.currency_code.clone(),
        decimals: b.currency_decimals,
    };
    // Every goods line matched in full; every reserved receipt line still has room.
    let open: Vec<(i32, Decimal, Decimal)> = sqlx::query_as(
        "SELECT l.line_no, l.quantity, coalesce(sum(m.quantity), 0)
         FROM supplier_bill_lines l
         LEFT JOIN supplier_bill_matches m ON m.bill_line_id = l.id
         WHERE l.workspace_id = $1 AND l.bill_id = $2 AND l.kind = 'goods'
         GROUP BY l.id, l.line_no, l.quantity ORDER BY l.line_no",
    )
    .bind(ws)
    .bind(id)
    .fetch_all(&mut *conn)
    .await?;
    let mut r = Rules::default();
    for (no, q, matched) in &open {
        r.check(
            &format!("lines.{}.quantity", no - 1),
            q == matched,
            "Match this goods line to receipt lines for its full quantity first.",
        );
    }
    let matched: Vec<(Uuid, Decimal)> = sqlx::query_as(
        "SELECT receipt_line_id, quantity FROM supplier_bill_matches
         WHERE workspace_id = $1 AND bill_id = $2 AND receipt_line_id IS NOT NULL",
    )
    .bind(ws)
    .bind(id)
    .fetch_all(&mut *conn)
    .await?;
    for (rl, q) in &matched {
        let facts = receipt_facts(&mut *conn, ws, *rl, Some(id)).await?;
        require_valued(&facts)?;
        r.check(
            "lines",
            facts.matchable.is_some_and(|m| *q <= m),
            "A matched receipt line no longer has this much to bill.",
        );
    }
    r.into_result()?;
    let rate_of = rate(
        &mut *conn,
        ws,
        &currency,
        &fc,
        b.accounting_date,
        "accounting_date",
    )
    .await?;
    let rows: Vec<PlanRow> =
        sqlx::query_as("SELECT * FROM supplier_bill_plan($1, $2, $3, $4) ORDER BY row_kind, id")
            .bind(ws)
            .bind(id)
            .bind(rate_of.0)
            .bind(rate_of.1)
            .fetch_all(&mut *conn)
            .await
            .map_err(tax_refused)?;
    let journal: Vec<JournalRow> = sqlx::query_as(
        "SELECT role, account_id, debit, credit FROM supplier_bill_journal($1, $2, $3, $4)
         ORDER BY line_no",
    )
    .bind(ws)
    .bind(id)
    .bind(rate_of.0)
    .bind(rate_of.1)
    .fetch_all(&mut *conn)
    .await
    .map_err(tax_refused)?;
    let head = rows
        .iter()
        .find(|p| p.row_kind == "bill")
        .ok_or_else(|| AppError::Internal("bill plan without header".into()))?;
    let kinds: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT id, kind FROM supplier_bill_lines WHERE workspace_id = $1 AND bill_id = $2",
    )
    .bind(ws)
    .bind(id)
    .fetch_all(&mut *conn)
    .await?;
    let mfacts: Vec<(Uuid, Option<Uuid>, Option<Uuid>, Decimal)> = sqlx::query_as(
        "SELECT id, receipt_line_id, landed_cost_id, quantity FROM supplier_bill_matches
         WHERE workspace_id = $1 AND bill_id = $2",
    )
    .bind(ws)
    .bind(id)
    .fetch_all(&mut *conn)
    .await?;
    let kind_of = |line: Uuid| {
        kinds
            .iter()
            .find(|k| k.0 == line)
            .map_or("", |k| k.1.as_str())
    };
    if rows.iter().any(|p| {
        p.row_kind == "line" && kind_of(p.id) != "expense" && p.tax_nonrecoverable.unwrap_or(0) != 0
    }) {
        return Err(unsupported(NONREC_GOODS));
    }
    if head.functional.unwrap_or(0) <= 0 || head.nominal.unwrap_or(0) < 0 {
        return Err(AppError::Unprocessable(
            "A bill needs a total above zero.".into(),
        ));
    }
    let taxes: Vec<(Uuid, String, Decimal, i64, i64, bool, bool)> = sqlx::query_as(
        "SELECT l.id, x.code, x.rate, x.base, x.tax, x.recoverable, x.exempt
         FROM supplier_bill_lines l
         JOIN supplier_bills b ON b.id = l.bill_id
         LEFT JOIN products p ON p.workspace_id = l.workspace_id AND p.id = l.product_id
         CROSS JOIN LATERAL tax_line($1, l.tax_profile_id, b.invoice_date, 'purchase',
                                     l.extension, false, p.category_id, b.supplier_id) x
         WHERE l.workspace_id = $1 AND l.bill_id = $2 AND l.tax_profile_id IS NOT NULL
         ORDER BY l.line_no, x.line_no",
    )
    .bind(ws)
    .bind(id)
    .fetch_all(&mut *conn)
    .await
    .map_err(tax_refused)?;
    let dp = b.currency_decimals;
    let fdp = fc.decimals;
    let f = |v: Option<i64>| v.filter(|_| accounting).map(|v| amount(v, fdp));
    let lines: Vec<ReviewLine> = rows
        .iter()
        .filter(|p| p.row_kind == "line")
        .map(|p| ReviewLine {
            line_id: p.id,
            kind: kind_of(p.id).to_owned(),
            extension: amount(p.nominal.unwrap_or(0), dp),
            tax_recoverable: amount(p.tax_recoverable.unwrap_or(0), dp),
            tax_nonrecoverable: amount(p.tax_nonrecoverable.unwrap_or(0), dp),
            functional: f(p.functional),
            taxes: taxes
                .iter()
                .filter(|t| t.0 == p.id)
                .map(|t| TaxComponent {
                    code: t.1.clone(),
                    rate: t.2.normalize().to_string(),
                    base: amount(t.3, dp),
                    tax: amount(t.4, dp),
                    recoverable: t.5,
                    exempt: t.6,
                })
                .collect(),
            matches: rows
                .iter()
                .filter(|m| m.row_kind == "match" && m.line_id == Some(p.id))
                .map(|m| {
                    let facts = mfacts.iter().find(|x| x.0 == m.id);
                    ReviewMatch {
                        match_id: m.id,
                        receipt_line_id: facts.and_then(|x| x.1),
                        landed_cost_id: facts.and_then(|x| x.2),
                        quantity: facts.map(|x| qty(x.3)).unwrap_or_default(),
                        grni_nominal: amount(m.grni_nominal.unwrap_or(0), dp),
                        grni_functional: f(m.grni_functional),
                        share_functional: f(m.share_functional),
                        fx: f(m.fx),
                        variance: f(m.variance),
                    }
                })
                .collect(),
        })
        .collect();
    let review_journal = journal_preview(&journal, fdp);
    let rate_json = json!([
        rate_of.0.normalize().to_string(),
        rate_of.1,
        rate_of.2,
        rate_of.3
    ]);
    let fp: [u8; 32] = Sha256::digest(
        json!({
            "action": "post", "bill": id, "version": b.version, "date": b.accounting_date,
            "rate": rate_json,
            "rows": rows.iter().map(|p| json!([p.row_kind, p.id, p.nominal, p.tax_recoverable,
                p.tax_nonrecoverable, p.tax_functional, p.grni_nominal, p.grni_functional,
                p.share_functional, p.fx, p.variance, p.functional])).collect::<Vec<_>>(),
            "journal": journal.iter().map(|j| json!([j.role, j.account_id, j.debit, j.credit]))
                .collect::<Vec<_>>(),
        })
        .to_string(),
    )
    .into();
    let (tax, total) = common::bill_totals(&mut *conn, head).await?;
    let review = BillPostReview {
        bill_id: id,
        version: b.version,
        accounting_date: b.accounting_date,
        currency,
        functional_currency: fc,
        rate: snapshot(&rate_of),
        net: amount(head.nominal.unwrap_or(0), dp),
        tax: amount(tax, dp),
        total: amount(total, dp),
        functional_total: f(head.functional),
        lines,
        journal: accounting.then_some(review_journal),
        review_fingerprint: hex::encode(fp),
    };
    Ok(PostPlan {
        review,
        rows,
        journal,
        rate: rate_of,
        supplier: b.supplier_id,
        due_date: b.due_date,
    })
}

/// POST /supplier-bills/{id}/post/review: no writes.
pub async fn post_review(
    state: &AppState,
    access: &WorkspaceAccess,
    id: Uuid,
) -> Result<BillPostReview, AppError> {
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

/// POST /supplier-bills/{id}/post: book the reviewed bill once.
pub async fn post(
    state: &AppState,
    access: &WorkspaceAccess,
    id: Uuid,
    cmd: Command<Action<ReviewedPayload>>,
) -> Result<Answer, AppError> {
    const OP: &str = "supplier_bills_post";
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, access.user_id, &cmd).await? {
        return Ok(a);
    }
    lock_draft(&mut tx, ws, id, cmd.input.expected_version).await?;
    let reserved: Vec<Uuid> = sqlx::query_scalar(
        "SELECT receipt_line_id FROM supplier_bill_matches
         WHERE workspace_id = $1 AND bill_id = $2 AND receipt_line_id IS NOT NULL",
    )
    .bind(ws)
    .bind(id)
    .fetch_all(&mut *tx)
    .await?;
    lock_receipt_lines(&mut tx, ws, &reserved).await?;
    sqlx::query(
        "SELECT id FROM purchase_landed_costs WHERE workspace_id = $1 AND id IN
             (SELECT landed_cost_id FROM supplier_bill_matches WHERE bill_id = $2)
         ORDER BY id FOR SHARE",
    )
    .bind(ws)
    .bind(id)
    .execute(&mut *tx)
    .await?;
    let plan = prepare_post(&mut tx, ws, id, false).await?;
    if plan.review.review_fingerprint != cmd.input.payload {
        return Err(stale());
    }
    let b = row(&mut tx, ws, id).await?;
    let fp = hex::decode(&plan.review.review_fingerprint)
        .ok()
        .and_then(|v| <[u8; 32]>::try_from(v).ok())
        .ok_or_else(|| AppError::Internal("fingerprint".into()))?;
    let lines: Vec<SourceLine> = plan
        .journal
        .iter()
        .map(|j| SourceLine {
            side: role_side(j.role.as_deref(), j.account_id),
            debit: j.debit,
            credit: j.credit,
            relation_id: (j.role.as_deref() == Some("payables")).then_some(plan.supplier),
            bank_account_id: None,
        })
        .collect();
    let (_, effect) = post_source_lines(
        &mut tx,
        ws,
        &SourceHeader {
            source_kind: SOURCE,
            source_id: id,
            effect_kind: "posted",
            date: b.accounting_date,
            reason: "supplier bill",
            fingerprint: &fp,
            by: access.user_id,
        },
        &lines,
    )
    .await?;
    let head = plan
        .rows
        .iter()
        .find(|p| p.row_kind == "bill")
        .cloned()
        .ok_or_else(|| AppError::Internal("bill plan without header".into()))?;
    let (tax, total) = common::bill_totals(&mut tx, &head).await?;
    let payables = settlement::role_account(&mut tx, ws, "payables").await?;
    let ap = settlement::open_item(
        &mut tx,
        ws,
        &NewOpenItem {
            classification: "payable",
            side: "credit",
            account_role: "payables",
            account_id: payables,
            source_kind: SOURCE,
            source_id: id,
            relation_id: Some(plan.supplier),
            order_id: None,
            invoice_id: None,
            recognition_id: None,
            currency_id: b.currency_id,
            issue_date: b.accounting_date,
            due_date: plan.due_date,
            origin: "posted",
            amount: total,
            functional_amount: head.functional.unwrap_or(0),
            effect_id: effect,
            by: access.user_id,
        },
    )
    .await?;
    // Freeze taxes, lines and matches as reviewed.
    sqlx::query(
        "INSERT INTO supplier_bill_line_taxes (workspace_id, bill_line_id, component_no, code,
             rate, base, tax, recoverable, exempt)
         SELECT $1, l.id, x.line_no, x.code, x.rate, x.base, x.tax, x.recoverable, x.exempt
         FROM supplier_bill_lines l
         JOIN supplier_bills b ON b.id = l.bill_id
         LEFT JOIN products p ON p.workspace_id = l.workspace_id AND p.id = l.product_id
         CROSS JOIN LATERAL tax_line($1, l.tax_profile_id, b.invoice_date, 'purchase',
                                     l.extension, false, p.category_id, b.supplier_id) x
         WHERE l.workspace_id = $1 AND l.bill_id = $2 AND l.tax_profile_id IS NOT NULL",
    )
    .bind(ws)
    .bind(id)
    .execute(&mut *tx)
    .await?;
    let pick = |kind: &str, f: fn(&PlanRow) -> Option<i64>| {
        plan.rows
            .iter()
            .filter(|p| p.row_kind == kind)
            .map(f)
            .collect::<Vec<_>>()
    };
    let ids = |kind: &str| {
        plan.rows
            .iter()
            .filter(|p| p.row_kind == kind)
            .map(|p| p.id)
            .collect::<Vec<_>>()
    };
    sqlx::query(
        "UPDATE supplier_bill_lines l SET tax_recoverable = x.rec,
             tax_nonrecoverable = x.nonrec, functional = x.f
         FROM unnest($2::uuid[], $3::bigint[], $4::bigint[], $5::bigint[]) AS x(id, rec, nonrec, f)
         WHERE l.workspace_id = $1 AND l.id = x.id",
    )
    .bind(ws)
    .bind(ids("line"))
    .bind(pick("line", |p| p.tax_recoverable))
    .bind(pick("line", |p| p.tax_nonrecoverable))
    .bind(pick("line", |p| p.functional))
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE supplier_bill_matches m SET state = 'posted', grni_nominal = x.gn,
             grni_functional = x.gf, share_functional = x.s, fx = x.fx, variance = x.v
         FROM unnest($2::uuid[], $3::bigint[], $4::bigint[], $5::bigint[], $6::bigint[],
                     $7::bigint[]) AS x(id, gn, gf, s, fx, v)
         WHERE m.workspace_id = $1 AND m.id = x.id",
    )
    .bind(ws)
    .bind(ids("match"))
    .bind(pick("match", |p| p.grni_nominal))
    .bind(pick("match", |p| p.grni_functional))
    .bind(pick("match", |p| p.share_functional))
    .bind(pick("match", |p| p.fx))
    .bind(pick("match", |p| p.variance))
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE supplier_bills SET state = 'posted', rate = $3, rate_inverse = $4,
             rate_source = $5, rate_date = $6, net = $7, tax = $8, total = $9,
             functional_total = $10, ap_item_id = $11, posted_effect_id = $12, posted_by = $13,
             posted_at = now(), version = version + 1, updated_at = now()
         WHERE workspace_id = $1 AND id = $2",
    )
    .bind(ws)
    .bind(id)
    .bind(plan.rate.0)
    .bind(plan.rate.1)
    .bind(plan.rate.2)
    .bind(plan.rate.3)
    .bind(head.nominal)
    .bind(tax)
    .bind(total)
    .bind(head.functional)
    .bind(ap)
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

/// What reversing posted bill `id` appends; no writes.
async fn prepare_reverse(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
    input: &ReverseInput,
    accounting: bool,
) -> Result<ReversePlan, AppError> {
    let b = row(&mut *conn, ws, id).await?;
    if b.state != "posted" {
        return Err(AppError::Unprocessable(NOT_POSTED.into()));
    }
    let fc = functional(&mut *conn, ws).await?;
    if input.date < b.accounting_date {
        return Err(AppError::field(
            "payload.accounting_date",
            "A reversal can't be dated before the bill.",
        ));
    }
    let (allocated, returned, adjusted): (bool, bool, bool) = sqlx::query_as(
        "SELECT EXISTS (SELECT 1 FROM accounting_allocations a
                        WHERE a.workspace_id = $1 AND a.reversal_of IS NULL
                          AND (a.target_item_id = $3 OR a.source_item_id = $3)
                          AND NOT EXISTS (SELECT 1 FROM accounting_allocations x
                                          WHERE x.reversal_of = a.id)),
                EXISTS (SELECT 1 FROM purchase_return_lines l
                        JOIN purchase_returns r ON r.id = l.return_id AND r.state = 'posted'
                        JOIN supplier_bill_matches m ON m.id = l.bill_match_id
                        WHERE m.workspace_id = $1 AND m.bill_id = $2),
                EXISTS (SELECT 1 FROM purchase_cost_adjustments
                        WHERE workspace_id = $1 AND bill_id = $2 AND state = 'posted')",
    )
    .bind(ws)
    .bind(id)
    .bind(b.ap_item_id)
    .fetch_one(&mut *conn)
    .await?;
    if allocated || returned || adjusted {
        return Err(dependent(
            "This bill has payments, credits, returns or cost adjustments. Reverse those first.",
        ));
    }
    let journal = common::mirror(&mut *conn, ws, b.posted_effect_id).await?;
    let fp: [u8; 32] = Sha256::digest(
        json!({
            "action": "reverse", "bill": id, "version": b.version, "date": input.date,
            "reason": input.reason,
            "journal": journal.iter().map(|j| json!([j.role, j.account_id, j.debit, j.credit]))
                .collect::<Vec<_>>(),
        })
        .to_string(),
    )
    .into();
    Ok(ReversePlan {
        review: ReverseReview {
            id,
            version: b.version,
            accounting_date: input.date,
            reason: input.reason.clone(),
            journal: accounting.then(|| journal_preview(&journal, fc.decimals)),
            review_fingerprint: hex::encode(fp),
        },
        journal,
    })
}

/// POST /supplier-bills/{id}/reverse/review: no writes.
pub async fn reverse_review(
    state: &AppState,
    access: &WorkspaceAccess,
    id: Uuid,
    input: &ReverseInput,
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

/// POST /supplier-bills/{id}/reverse: typed reversal (§11): mirror journal, AP closed by a
/// supplier_credit twin, matches released. No stock effect.
pub async fn reverse(
    state: &AppState,
    access: &WorkspaceAccess,
    id: Uuid,
    cmd: Command<Action<ReversePayload>>,
) -> Result<Answer, AppError> {
    const OP: &str = "supplier_bills_reverse";
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, access.user_id, &cmd).await? {
        return Ok(a);
    }
    let (_, version) = lock(&mut tx, ws, id).await?;
    if version != cmd.input.expected_version {
        return Err(stale());
    }
    let (input, fingerprint) = &cmd.input.payload;
    let reserved: Vec<Uuid> = sqlx::query_scalar(
        "SELECT receipt_line_id FROM supplier_bill_matches
         WHERE workspace_id = $1 AND bill_id = $2 AND receipt_line_id IS NOT NULL",
    )
    .bind(ws)
    .bind(id)
    .fetch_all(&mut *tx)
    .await?;
    lock_receipt_lines(&mut tx, ws, &reserved).await?;
    let plan = prepare_reverse(&mut tx, ws, id, input, false).await?;
    if &plan.review.review_fingerprint != fingerprint {
        return Err(stale());
    }
    let b = row(&mut tx, ws, id).await?;
    let ap = b
        .ap_item_id
        .ok_or_else(|| AppError::Internal("posted bill without AP".into()))?;
    let effect = common::post_mirror(
        &mut tx,
        ws,
        (SOURCE, id),
        input,
        &plan.journal,
        Some(b.supplier_id),
        &plan.review.review_fingerprint,
        access.user_id,
    )
    .await?;
    common::close_item(
        &mut tx,
        ws,
        ap,
        SOURCE,
        id,
        input.date,
        effect,
        access.user_id,
    )
    .await?;
    sqlx::query(
        "UPDATE supplier_bill_matches SET state = 'reversed'
         WHERE workspace_id = $1 AND bill_id = $2 AND state = 'posted'",
    )
    .bind(ws)
    .bind(id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE supplier_bills SET state = 'reversed', reversal_date = $3, reverse_reason = $4,
             reversal_effect_id = $5, reversed_by = $6, reversed_at = now(),
             version = version + 1, updated_at = now()
         WHERE workspace_id = $1 AND id = $2",
    )
    .bind(ws)
    .bind(id)
    .bind(input.date)
    .bind(&input.reason)
    .bind(effect)
    .bind(access.user_id)
    .execute(&mut *tx)
    .await?;
    let answer = ok(&get(&mut tx, ws, id, access.allows("accounting.read")).await?);
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[id]).await?;
    tx.commit().await?;
    Ok(answer)
}
