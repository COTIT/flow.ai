//! Supplier payments and refunds received (masterplan §11 SUPPLIER MONEY, §9 money places,
//! §13 FX, §15). A payment is real money out of one cash place or bank account: DR payables
//! / CR the place, opening a supplier_credit (debit) item in the payment currency (D23: a
//! monetary item; revaluation reads it later). A refund received is money in: DR the place /
//! CR payables, opening a payable (credit) item. Allocate applies the item to an opposite AP
//! item of the same supplier and currency; realized FX = difference of the functional carrying
//! relieved on both sides (SQL, the settlement trigger's formula). Reverse corrects an
//! erroneous record (no allocations left): mirror journal, cash-place counter row, item closed
//! by its twin. Lock order: command row → shared gate → payment FOR UPDATE → money place →
//! open items (id) → accounts/period → effect/journal number → number counter last.

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
            service::{created, ok, stale},
            settlement::{self, Located, MoneyLocation, NewOpenItem, Side},
        },
        cash::dto::Place,
        payables::{
            bills::{JournalRow, functional},
            common::{self, Relief, dependent, fp32, fx_journal, journal_preview},
            dto::{
                AllocateInput, AllocatePayload, AllocateReview, ListQuery,
                PaymentAllocationResource, PaymentInput, PaymentPayload, PaymentReview,
                PaymentTarget, ReverseInput, ReversePayload, ReverseReview,
                SupplierPaymentResource,
            },
        },
        purchasing::{
            dto::{RateSnapshot, SupplierRef},
            receipts::{rate, snapshot},
            repo::Rate,
            service::amount,
        },
        shipments::repo::book_today,
    },
    money::minor,
    numbering::{self, Series},
};

const SOURCE: &str = "supplier_payment";
const NO_SHIFT: &str = "The drawer has no open shift.";
const OTHER_CURRENCY: &str = "The money place holds another currency than the payment.";
const NOT_TODAY: &str = "Supplier money is recorded on the book's business day.";
const NOT_POSTED: &str = "Only a posted payment can change.";

fn rate_json(r: &Rate) -> serde_json::Value {
    json!([r.0.normalize().to_string(), r.1, r.2, r.3])
}

#[derive(FromRow)]
struct PaymentRow {
    id: Uuid,
    number: i64,
    direction: String,
    state: String,
    version: i64,
    supplier_id: Uuid,
    supplier_name: Option<String>,
    currency_id: Uuid,
    currency_code: String,
    currency_decimals: i16,
    functional_decimals: i16,
    amount: i64,
    unapplied: i64,
    date: NaiveDate,
    location_kind: String,
    register_id: Option<Uuid>,
    vault_id: Option<Uuid>,
    bank_account_id: Option<Uuid>,
    payment_reference: Option<String>,
    payment_evidence_ref: String,
    note: Option<String>,
    rate: Decimal,
    rate_inverse: bool,
    rate_source: String,
    rate_date: Option<NaiveDate>,
    functional_amount: i64,
    item_id: Uuid,
    effect_id: Uuid,
    reversed_at: Option<chrono::DateTime<chrono::Utc>>,
    reversal_date: Option<NaiveDate>,
    reverse_reason: Option<String>,
    created_by: Uuid,
    created_at: chrono::DateTime<chrono::Utc>,
}

const PAYMENT_SELECT: &str = "
    SELECT p.id, p.number, p.direction, p.state, p.version, p.supplier_id,
           coalesce(r.company_name, r.alias) AS supplier_name, p.currency_id,
           upper(c.asset_type) AS currency_code, c.decimals AS currency_decimals,
           f.decimals AS functional_decimals, p.amount, i.residual AS unapplied, p.date,
           p.location_kind, p.register_id, p.vault_id, p.bank_account_id, p.payment_reference,
           p.payment_evidence_ref, p.note, p.rate, p.rate_inverse, p.rate_source, p.rate_date,
           p.functional_amount, p.item_id, p.effect_id, p.reversed_at, p.reversal_date,
           p.reverse_reason, p.created_by, p.created_at, count(*) OVER () AS total_rows
    FROM supplier_payments p
    JOIN currencies c ON c.workspace_id = p.workspace_id AND c.id = p.currency_id
    JOIN relations r ON r.workspace_id = p.workspace_id AND r.id = p.supplier_id
    JOIN accounting_open_items i ON i.workspace_id = p.workspace_id AND i.id = p.item_id
    JOIN accounting_settings s ON s.workspace_id = p.workspace_id
    JOIN currencies f ON f.workspace_id = p.workspace_id AND f.id = s.functional_currency_id
    WHERE p.workspace_id = $1";

#[derive(FromRow)]
struct AllocRow {
    id: Uuid,
    item: Uuid,
    other: Uuid,
    amount: i64,
    accounting_date: NaiveDate,
    fx: i64,
    reversed: bool,
}

async fn resources(
    conn: &mut PgConnection,
    ws: Uuid,
    rows: Vec<PaymentRow>,
    accounting: bool,
) -> Result<Vec<SupplierPaymentResource>, AppError> {
    let items: Vec<Uuid> = rows.iter().map(|p| p.item_id).collect();
    let allocs: Vec<AllocRow> = sqlx::query_as(
        "SELECT a.id, i.item, CASE WHEN a.source_item_id = i.item THEN a.target_item_id
                                   ELSE a.source_item_id END AS other,
                a.amount, a.accounting_date,
                (CASE WHEN s.side = 'debit' THEN a.fx_difference ELSE -a.fx_difference END) AS fx,
                EXISTS (SELECT 1 FROM accounting_allocations x WHERE x.reversal_of = a.id) AS reversed
         FROM unnest($2::uuid[]) AS i(item)
         JOIN accounting_allocations a ON a.workspace_id = $1 AND a.reversal_of IS NULL
              AND a.kind = 'application' AND (a.source_item_id = i.item OR a.target_item_id = i.item)
         JOIN accounting_open_items s ON s.id = a.source_item_id
         ORDER BY a.recorded_at, a.id",
    )
    .bind(ws)
    .bind(&items)
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|p| {
            let dp = p.currency_decimals;
            let fdp = p.functional_decimals;
            let posted = p.state == "posted";
            let live = allocs.iter().any(|a| a.item == p.item_id && !a.reversed);
            let mut allowed_actions = Vec::new();
            if posted && p.unapplied > 0 {
                allowed_actions.push("allocate");
            }
            if posted && !live {
                allowed_actions.push("reverse");
            }
            SupplierPaymentResource {
                id: p.id,
                workspace_id: ws,
                number: p.number,
                version: p.version,
                supplier: SupplierRef {
                    id: p.supplier_id,
                    name: p.supplier_name,
                },
                currency: CurrencyRef {
                    id: p.currency_id,
                    code: p.currency_code,
                    decimals: dp,
                },
                amount: amount(p.amount, dp),
                unapplied: amount(p.unapplied, dp),
                date: p.date,
                location_kind: p.location_kind,
                register_id: p.register_id,
                vault_id: p.vault_id,
                bank_account_id: p.bank_account_id,
                reference: p.payment_reference,
                evidence_ref: p.payment_evidence_ref,
                note: p.note,
                rate: RateSnapshot {
                    rate: p.rate.normalize().to_string(),
                    inverse: p.rate_inverse,
                    source: p.rate_source,
                    date: p.rate_date,
                },
                functional_amount: accounting.then(|| amount(p.functional_amount, fdp)),
                open_item_id: p.item_id,
                allocations: allocs
                    .iter()
                    .filter(|a| a.item == p.item_id)
                    .map(|a| PaymentAllocationResource {
                        allocation_id: a.id,
                        target_open_item_id: a.other,
                        amount: amount(a.amount, dp),
                        accounting_date: a.accounting_date,
                        fx: accounting.then(|| amount(a.fx, fdp)),
                        reversed: a.reversed,
                    })
                    .collect(),
                reversed_at: p.reversed_at,
                reversal_date: p.reversal_date,
                reverse_reason: p.reverse_reason,
                created_by: p.created_by,
                created_at: p.created_at,
                allowed_actions,
                direction: p.direction,
                state: p.state,
            }
        })
        .collect())
}

async fn row(conn: &mut PgConnection, ws: Uuid, id: Uuid) -> Result<PaymentRow, AppError> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{PAYMENT_SELECT} AND p.id = $2"
    )))
    .bind(ws)
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(AppError::NotFound)
}

/// One payment; 404 outside the workspace.
pub async fn get(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
    accounting: bool,
) -> Result<SupplierPaymentResource, AppError> {
    let p = row(&mut *conn, ws, id).await?;
    resources(conn, ws, vec![p], accounting)
        .await?
        .pop()
        .ok_or(AppError::NotFound)
}

/// GET /supplier-payments: newest first.
pub async fn list(
    state: &AppState,
    ws: Uuid,
    q: &ListQuery,
    accounting: bool,
) -> Result<Paginated<SupplierPaymentResource>, AppError> {
    use sqlx::Row;
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    let mut conn = state.db.acquire().await?;
    let raw = sqlx::query(sqlx::AssertSqlSafe(format!(
        "{PAYMENT_SELECT} AND ($2::uuid IS NULL OR p.supplier_id = $2)
         ORDER BY p.number DESC LIMIT $3 OFFSET $4"
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
        .map(PaymentRow::from_row)
        .collect::<Result<Vec<_>, _>>()?;
    let data = resources(&mut conn, ws, rows, accounting).await?;
    Ok(Paginated::new(data, total, page))
}

struct CreatePlan {
    review: PaymentReview,
    minor: i64,
    functional: i64,
    rate: Rate,
    located: Located,
}

/// Validate and price a new payment; no writes (the place is locked when `lock` is set).
async fn prepare_create(
    conn: &mut PgConnection,
    ws: Uuid,
    input: &PaymentInput,
    accounting: bool,
) -> Result<CreatePlan, AppError> {
    let fc = functional(&mut *conn, ws).await?;
    let supplier: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM relations
                        WHERE workspace_id = $1 AND id = $2 AND deleted_at IS NULL)",
    )
    .bind(ws)
    .bind(input.supplier)
    .fetch_one(&mut *conn)
    .await?;
    if !supplier {
        return Err(AppError::field(
            "payload.supplier_id",
            "The supplier was not found.",
        ));
    }
    let currency = books::currency(&mut *conn, ws, input.currency)
        .await?
        .ok_or_else(|| AppError::field("payload.currency_id", "The currency was not found."))?;
    let u32dp = u32::try_from(currency.decimals).unwrap_or(0);
    let minor_amount = minor(input.amount, u32dp)
        .map_err(|_| AppError::field("payload.amount", "The amount doesn't fit the currency."))?;
    if input.date != book_today(&mut *conn, ws).await? {
        return Err(AppError::field("payload.date", NOT_TODAY));
    }
    let located = input.location.lock(&mut *conn, ws, input.currency).await?;
    if located.currency_id != input.currency {
        return Err(AppError::field("payload.location", OTHER_CURRENCY));
    }
    if matches!(input.location, MoneyLocation::CashPlace(Place::Drawer(_)))
        && located.shift_id.is_none()
    {
        return Err(AppError::field("payload.location", NO_SHIFT));
    }
    let rate_of = rate(&mut *conn, ws, &currency, &fc, input.date, "payload.date").await?;
    let functional_amount: i64 = sqlx::query_scalar("SELECT money_convert($1, $2, $3, $4, $5)")
        .bind(minor_amount)
        .bind(rate_of.0)
        .bind(i32::from(currency.decimals))
        .bind(i32::from(fc.decimals))
        .bind(rate_of.1)
        .fetch_one(&mut *conn)
        .await?;
    if functional_amount <= 0 {
        return Err(AppError::field(
            "payload.amount",
            "The amount is too small to book.",
        ));
    }
    let place = side_name(located.side);
    let (debit, credit) = if input.direction == "payment" {
        (("payables", None), place)
    } else {
        (place, ("payables", None))
    };
    let journal = vec![
        JournalRow {
            role: Some(debit.0.to_owned()).filter(|r| !r.is_empty()),
            account_id: debit.1,
            debit: functional_amount,
            credit: 0,
            relation_id: None,
        },
        JournalRow {
            role: Some(credit.0.to_owned()).filter(|r| !r.is_empty()),
            account_id: credit.1,
            debit: 0,
            credit: functional_amount,
            relation_id: None,
        },
    ];
    let c = input.location.columns();
    let fp: [u8; 32] = Sha256::digest(
        json!({
            "action": "pay", "supplier": input.supplier, "currency": input.currency,
            "direction": input.direction, "amount": minor_amount, "date": input.date,
            "location": [c.location_kind, c.register_id, c.vault_id, c.bank_account_id,
                         c.reference, c.evidence_ref, c.confirmation_method],
            "reference": input.reference, "evidence": input.evidence_ref, "note": input.note,
            "rate": rate_json(&rate_of), "functional": functional_amount,
        })
        .to_string(),
    )
    .into();
    Ok(CreatePlan {
        review: PaymentReview {
            supplier_id: input.supplier,
            direction: input.direction.to_owned(),
            amount: amount(minor_amount, currency.decimals),
            date: input.date,
            location_kind: c.location_kind.to_owned(),
            functional_amount: accounting.then(|| amount(functional_amount, fc.decimals)),
            journal: accounting.then(|| journal_preview(&journal, fc.decimals)),
            rate: snapshot(&rate_of),
            currency,
            functional_currency: fc,
            review_fingerprint: hex::encode(fp),
        },
        minor: minor_amount,
        functional: functional_amount,
        rate: rate_of,
        located,
    })
}

/// (role, account) of a money place's journal side.
fn side_name(s: Side) -> (&'static str, Option<Uuid>) {
    match s {
        Side::Role(r) => (r, None),
        Side::Account(a) => ("", Some(a)),
    }
}

/// POST /supplier-payments/review: no writes.
pub async fn create_review(
    state: &AppState,
    access: &WorkspaceAccess,
    input: &PaymentInput,
) -> Result<PaymentReview, AppError> {
    let mut tx = state.db.begin().await?;
    gate::shared(&mut tx, &[access.workspace_id]).await?;
    let plan = prepare_create(
        &mut tx,
        access.workspace_id,
        input,
        access.allows("accounting.read"),
    )
    .await?;
    tx.rollback().await?;
    Ok(plan.review)
}

/// Cash ledger row of a money-place movement (signed into the place).
async fn cash_row(
    conn: &mut PgConnection,
    ws: Uuid,
    location: &MoneyLocation,
    located: &Located,
    out: bool,
    minor_amount: i64,
    (id, effect, by): (Uuid, Uuid, Uuid),
) -> Result<(), AppError> {
    if !matches!(location, MoneyLocation::CashPlace(_)) {
        return Ok(());
    }
    let c = location.columns();
    sqlx::query(
        "INSERT INTO cash_ledger (workspace_id, register_id, vault_id, shift_id, kind, amount,
             actor_kind, member_user_id, source_kind, source_id, effect_id)
         VALUES ($1, $2, $3, $4, $5, CASE WHEN $6 THEN -$7::bigint ELSE $7::bigint END,
                 'member', $8, $9, $10, $11)",
    )
    .bind(ws)
    .bind(c.register_id)
    .bind(c.vault_id)
    .bind(located.shift_id)
    .bind(if out { "payment" } else { "receipt" })
    .bind(out)
    .bind(minor_amount)
    .bind(by)
    .bind(SOURCE)
    .bind(id)
    .bind(effect)
    .execute(conn)
    .await?;
    Ok(())
}

/// POST /supplier-payments: record real supplier money once.
pub async fn create(
    state: &AppState,
    access: &WorkspaceAccess,
    cmd: Command<Create<PaymentPayload>>,
) -> Result<Answer, AppError> {
    const OP: &str = "supplier_payments_create";
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, access.user_id, &cmd).await? {
        return Ok(a);
    }
    gate::shared(&mut tx, &[ws]).await?;
    let input = &cmd.input.payload;
    let plan = prepare_create(&mut tx, ws, input, false).await?;
    if input.review_fingerprint.as_deref() != Some(plan.review.review_fingerprint.as_str()) {
        return Err(stale());
    }
    let id = Uuid::now_v7();
    let fp = fp32(&plan.review.review_fingerprint)?;
    let pay = input.direction == "payment";
    let payables = settlement::role_account(&mut tx, ws, "payables").await?;
    let place = plan.located.side;
    let bank = match &input.location {
        MoneyLocation::BankAccount { id, .. } => Some(*id),
        _ => None,
    };
    let line = |side: Side, debit: i64, credit: i64| SourceLine {
        side,
        debit,
        credit,
        relation_id: (side == Side::Account(payables)).then_some(input.supplier),
        bank_account_id: bank,
    };
    let lines = if pay {
        [
            line(Side::Account(payables), plan.functional, 0),
            line(place, 0, plan.functional),
        ]
    } else {
        [
            line(place, plan.functional, 0),
            line(Side::Account(payables), 0, plan.functional),
        ]
    };
    let (_, effect) = post_source_lines(
        &mut tx,
        ws,
        &SourceHeader {
            source_kind: SOURCE,
            source_id: id,
            effect_kind: if pay { "paid" } else { "received" },
            date: input.date,
            reason: if pay {
                "supplier payment"
            } else {
                "supplier refund received"
            },
            fingerprint: &fp,
            by: access.user_id,
        },
        &lines,
    )
    .await?;
    let item = settlement::open_item(
        &mut tx,
        ws,
        &NewOpenItem {
            classification: if pay { "supplier_credit" } else { "payable" },
            side: if pay { "debit" } else { "credit" },
            account_role: "payables",
            account_id: payables,
            source_kind: SOURCE,
            source_id: id,
            relation_id: Some(input.supplier),
            order_id: None,
            invoice_id: None,
            recognition_id: None,
            currency_id: input.currency,
            issue_date: input.date,
            due_date: None,
            origin: "posted",
            amount: plan.minor,
            functional_amount: plan.functional,
            effect_id: effect,
            by: access.user_id,
        },
    )
    .await?;
    cash_row(
        &mut tx,
        ws,
        &input.location,
        &plan.located,
        pay,
        plan.minor,
        (id, effect, access.user_id),
    )
    .await?;
    let number = numbering::next(&mut tx, ws, Series::SupplierPayment, 1).await?;
    let c = input.location.columns();
    sqlx::query(
        "INSERT INTO supplier_payments (id, workspace_id, number, direction, supplier_id,
             currency_id, amount, date, payment_reference, payment_evidence_ref, note,
             location_kind, register_id, vault_id, bank_account_id, reference, evidence_ref,
             confirmation_method, rate, rate_inverse, rate_source, rate_date, functional_amount,
             item_id, effect_id, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17,
                 $18, $19, $20, $21, $22, $23, $24, $25, $26)",
    )
    .bind(id)
    .bind(ws)
    .bind(number)
    .bind(input.direction)
    .bind(input.supplier)
    .bind(input.currency)
    .bind(plan.minor)
    .bind(input.date)
    .bind(&input.reference)
    .bind(&input.evidence_ref)
    .bind(&input.note)
    .bind(c.location_kind)
    .bind(c.register_id)
    .bind(c.vault_id)
    .bind(c.bank_account_id)
    .bind(c.reference)
    .bind(c.evidence_ref)
    .bind(c.confirmation_method)
    .bind(plan.rate.0)
    .bind(plan.rate.1)
    .bind(plan.rate.2)
    .bind(plan.rate.3)
    .bind(plan.functional)
    .bind(item)
    .bind(effect)
    .bind(access.user_id)
    .execute(&mut *tx)
    .await?;
    let answer = created(&get(&mut tx, ws, id, access.allows("accounting.read")).await?);
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[id]).await?;
    tx.commit().await?;
    Ok(answer)
}

/// Shared gate, payment FOR UPDATE, version check, posted.
async fn lock_posted(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
    expected: Option<i64>,
) -> Result<(), AppError> {
    gate::shared(&mut *conn, &[ws]).await?;
    let (state, version): (String, i64) = sqlx::query_as(
        "SELECT state, version FROM supplier_payments WHERE workspace_id = $1 AND id = $2
         FOR UPDATE",
    )
    .bind(ws)
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(AppError::NotFound)?;
    if expected.is_some_and(|e| e != version) {
        return Err(stale());
    }
    if state != "posted" {
        return Err(AppError::Unprocessable(NOT_POSTED.into()));
    }
    Ok(())
}

struct AllocatePlan {
    review: AllocateReview,
    relief: Relief,
    currency: Uuid,
    supplier: Uuid,
    item: Uuid,
    date: NaiveDate,
}

async fn prepare_allocate(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
    input: &AllocateInput,
    accounting: bool,
) -> Result<AllocatePlan, AppError> {
    let p = row(&mut *conn, ws, id).await?;
    if p.state != "posted" {
        return Err(AppError::Unprocessable(NOT_POSTED.into()));
    }
    let minor_amount = minor(
        input.amount,
        u32::try_from(p.currency_decimals).unwrap_or(0),
    )
    .map_err(|_| AppError::field("payload.amount", "The amount doesn't fit the currency."))?;
    let date = book_today(&mut *conn, ws).await?;
    let target: Option<(String, Option<Uuid>, Uuid, i64, NaiveDate)> = sqlx::query_as(
        "SELECT classification, relation_id, currency_id, residual, issue_date
         FROM accounting_open_items WHERE workspace_id = $1 AND id = $2",
    )
    .bind(ws)
    .bind(input.target)
    .fetch_optional(&mut *conn)
    .await?;
    let Some((class, relation, currency, residual, _)) = target else {
        return Err(AppError::NotFound);
    };
    let want = if p.direction == "payment" {
        "payable"
    } else {
        "supplier_credit"
    };
    if class != want || relation != Some(p.supplier_id) || currency != p.currency_id {
        return Err(AppError::field(
            "payload.target_open_item_id",
            "Apply a payment to this supplier's bills in its currency, a refund to their credits.",
        ));
    }
    if minor_amount > residual || minor_amount > p.unapplied {
        return Err(AppError::field(
            "payload.amount",
            "More than the payment or the target still has open.",
        ));
    }
    let relief = common::relief(&mut *conn, ws, p.item_id, input.target, minor_amount).await?;
    let journal = fx_journal(&relief);
    let fp: [u8; 32] = Sha256::digest(
        json!({
            "action": "allocate", "payment": id, "version": p.version, "target": input.target,
            "amount": minor_amount, "date": date, "relief": [relief.source_functional,
            relief.target_functional, relief.loss],
        })
        .to_string(),
    )
    .into();
    let fdp = p.functional_decimals;
    Ok(AllocatePlan {
        review: AllocateReview {
            payment_id: id,
            version: p.version,
            target_open_item_id: input.target,
            amount: amount(minor_amount, p.currency_decimals),
            source_functional: accounting.then(|| amount(relief.source_functional, fdp)),
            target_functional: accounting.then(|| amount(relief.target_functional, fdp)),
            fx: accounting.then(|| amount(relief.loss, fdp)),
            journal: accounting.then(|| journal_preview(&journal, fdp)),
            review_fingerprint: hex::encode(fp),
        },
        relief,
        currency: p.currency_id,
        supplier: p.supplier_id,
        item: p.item_id,
        date,
    })
}

/// POST /supplier-payments/{id}/allocate/review: no writes.
pub async fn allocate_review(
    state: &AppState,
    access: &WorkspaceAccess,
    id: Uuid,
    input: &AllocateInput,
) -> Result<AllocateReview, AppError> {
    let mut tx = state.db.begin().await?;
    let plan = prepare_allocate(
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

/// POST /supplier-payments/{id}/allocate: apply part or all of the payment.
pub async fn allocate(
    state: &AppState,
    access: &WorkspaceAccess,
    id: Uuid,
    cmd: Command<Action<AllocatePayload>>,
) -> Result<Answer, AppError> {
    const OP: &str = "supplier_payments_allocate";
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, access.user_id, &cmd).await? {
        return Ok(a);
    }
    lock_posted(&mut tx, ws, id, Some(cmd.input.expected_version)).await?;
    let input = &cmd.input.payload;
    let item: Uuid = sqlx::query_scalar(
        "SELECT item_id FROM supplier_payments WHERE workspace_id = $1 AND id = $2",
    )
    .bind(ws)
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    let mut items = [item, input.target];
    items.sort_unstable();
    sqlx::query(
        "SELECT id FROM accounting_open_items WHERE workspace_id = $1 AND id = ANY($2)
         ORDER BY id FOR NO KEY UPDATE",
    )
    .bind(ws)
    .bind(&items[..])
    .execute(&mut *tx)
    .await?;
    let plan = prepare_allocate(&mut tx, ws, id, input, false).await?;
    if input.review_fingerprint.as_deref() != Some(plan.review.review_fingerprint.as_str()) {
        return Err(stale());
    }
    let fp = fp32(&plan.review.review_fingerprint)?;
    common::apply(
        &mut tx,
        ws,
        (plan.item, input.target),
        &plan.relief,
        (plan.date, plan.currency, plan.supplier),
        ("supplier_payment_allocation", Uuid::now_v7(), "allocated"),
        &fp,
        access.user_id,
    )
    .await?;
    sqlx::query(
        "UPDATE supplier_payments SET version = version + 1, updated_at = now()
         WHERE workspace_id = $1 AND id = $2",
    )
    .bind(ws)
    .bind(id)
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
    input: &ReverseInput,
    accounting: bool,
) -> Result<ReversePlan, AppError> {
    let p = row(&mut *conn, ws, id).await?;
    if p.state != "posted" {
        return Err(AppError::Unprocessable(NOT_POSTED.into()));
    }
    if p.unapplied != p.amount {
        return Err(dependent(
            "This payment is applied. Only an unapplied payment record can be reversed.",
        ));
    }
    if input.date != book_today(&mut *conn, ws).await? {
        return Err(AppError::field("payload.accounting_date", NOT_TODAY));
    }
    let journal = common::mirror(&mut *conn, ws, Some(p.effect_id)).await?;
    let fp: [u8; 32] = Sha256::digest(
        json!({
            "action": "reverse", "payment": id, "version": p.version, "date": input.date,
            "reason": input.reason,
            "journal": journal.iter().map(|j| json!([j.account_id, j.debit, j.credit]))
                .collect::<Vec<_>>(),
        })
        .to_string(),
    )
    .into();
    Ok(ReversePlan {
        review: ReverseReview {
            id,
            version: p.version,
            accounting_date: input.date,
            reason: input.reason.clone(),
            journal: accounting.then(|| journal_preview(&journal, p.functional_decimals)),
            review_fingerprint: hex::encode(fp),
        },
        journal,
    })
}

/// POST /supplier-payments/{id}/reverse/review: no writes.
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

/// POST /supplier-payments/{id}/reverse: correct an erroneous, unapplied record. This is
/// no proof the money came back (a refund received is its own record).
pub async fn reverse(
    state: &AppState,
    access: &WorkspaceAccess,
    id: Uuid,
    cmd: Command<Action<ReversePayload>>,
) -> Result<Answer, AppError> {
    const OP: &str = "supplier_payments_reverse";
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, access.user_id, &cmd).await? {
        return Ok(a);
    }
    lock_posted(&mut tx, ws, id, Some(cmd.input.expected_version)).await?;
    let p = row(&mut tx, ws, id).await?;
    let location = match (p.register_id, p.vault_id, p.bank_account_id) {
        (Some(r), _, _) => Some(MoneyLocation::CashPlace(Place::Drawer(r))),
        (_, Some(v), _) => Some(MoneyLocation::CashPlace(Place::Vault(v))),
        _ => None,
    };
    let located = match &location {
        Some(l) => Some(l.lock(&mut tx, ws, p.currency_id).await?),
        None => None,
    };
    sqlx::query(
        "SELECT id FROM accounting_open_items WHERE workspace_id = $1 AND id = $2
         FOR NO KEY UPDATE",
    )
    .bind(ws)
    .bind(p.item_id)
    .execute(&mut *tx)
    .await?;
    let (input, fingerprint) = &cmd.input.payload;
    let plan = prepare_reverse(&mut tx, ws, id, input, false).await?;
    if &plan.review.review_fingerprint != fingerprint {
        return Err(stale());
    }
    let effect = common::post_mirror(
        &mut tx,
        ws,
        (SOURCE, id),
        input,
        &plan.journal,
        Some(p.supplier_id),
        &plan.review.review_fingerprint,
        access.user_id,
    )
    .await?;
    if let (Some(l), Some(located)) = (&location, &located) {
        if matches!(l, MoneyLocation::CashPlace(Place::Drawer(_))) && located.shift_id.is_none() {
            return Err(AppError::field("payload", NO_SHIFT));
        }
        // The counter-movement: money back into the place for a reversed payment.
        cash_row(
            &mut tx,
            ws,
            l,
            located,
            p.direction != "payment",
            p.amount,
            (id, effect, access.user_id),
        )
        .await?;
    }
    common::close_item(
        &mut tx,
        ws,
        p.item_id,
        SOURCE,
        id,
        input.date,
        effect,
        access.user_id,
    )
    .await?;
    sqlx::query(
        "UPDATE supplier_payments SET state = 'reversed', reversal_date = $3, reverse_reason = $4,
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

#[derive(FromRow)]
struct TargetRow {
    decimals: i16,
    open_item_id: Option<Uuid>,
    classification: Option<String>,
    source_kind: Option<String>,
    source_id: Option<Uuid>,
    source_number: Option<i64>,
    supplier_invoice_number: Option<String>,
    issue_date: Option<NaiveDate>,
    due_date: Option<NaiveDate>,
    amount: Option<i64>,
    residual: Option<i64>,
    total_rows: Option<i64>,
}

/// GET /supplier-payments/{id}/targets: open items `allocate` accepts for this record (a
/// payment's supplier payables, a refund's supplier credits; same supplier and currency,
/// residual left), oldest due first. Read-only, one query; 404 outside the workspace.
pub async fn targets(
    state: &AppState,
    ws: Uuid,
    id: Uuid,
    q: &ListQuery,
) -> Result<Paginated<PaymentTarget>, AppError> {
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    let rows: Vec<TargetRow> = sqlx::query_as(
        "SELECT c.decimals, t.* FROM supplier_payments p
         JOIN currencies c ON c.workspace_id = p.workspace_id AND c.id = p.currency_id
         LEFT JOIN LATERAL (
             SELECT i.id AS open_item_id, i.classification, i.source_kind, i.source_id,
                    coalesce(sb.number, pr.number, sp.number) AS source_number,
                    sb.supplier_invoice_number, i.issue_date, i.due_date, i.amount, i.residual,
                    count(*) OVER () AS total_rows
             FROM accounting_open_items i
             LEFT JOIN supplier_bills sb ON i.source_kind = 'supplier_bill'
                  AND sb.workspace_id = i.workspace_id AND sb.id = i.source_id
             LEFT JOIN purchase_returns pr ON i.source_kind = 'purchase_return'
                  AND pr.workspace_id = i.workspace_id AND pr.id = i.source_id
             LEFT JOIN supplier_payments sp ON i.source_kind = 'supplier_payment'
                  AND sp.workspace_id = i.workspace_id AND sp.id = i.source_id
             WHERE i.workspace_id = p.workspace_id AND i.relation_id = p.supplier_id
               AND i.classification = CASE WHEN p.direction = 'payment' THEN 'payable'
                                           ELSE 'supplier_credit' END
               AND i.currency_id = p.currency_id AND i.residual > 0
             ORDER BY coalesce(i.due_date, i.issue_date), i.id
             LIMIT $3 OFFSET $4) t ON true
         WHERE p.workspace_id = $1 AND p.id = $2",
    )
    .bind(ws)
    .bind(id)
    .bind(page.size)
    .bind(page.offset())
    .fetch_all(&state.db)
    .await?;
    let dp = rows.first().ok_or(AppError::NotFound)?.decimals;
    let total = rows.first().and_then(|r| r.total_rows).unwrap_or(0);
    let data = rows
        .into_iter()
        .filter_map(|t| {
            Some(PaymentTarget {
                open_item_id: t.open_item_id?,
                classification: t.classification?,
                source_kind: t.source_kind?,
                source_id: t.source_id?,
                source_number: t.source_number,
                supplier_invoice_number: t.supplier_invoice_number,
                issue_date: t.issue_date?,
                due_date: t.due_date,
                amount: amount(t.amount?, dp),
                residual: amount(t.residual?, dp),
            })
        })
        .collect();
    Ok(Paginated::new(data, total, page))
}
