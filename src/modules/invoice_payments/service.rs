//! Payment rules (Laravel CreateInvoicePayment, DeleteInvoicePayment, MarkInvoiceAsCredited).
//! Every write is one transaction: Idempotency-Key, then the invoice rows it touches locked in
//! id order, then the ledger rows, then each invoice's paid total recomputed from its rows and
//! its status from that sum. Payments touch no stock and no counter.

use std::collections::BTreeMap;

use rust_decimal::Decimal;
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    idempotency::{self, Answer, Key},
    modules::{
        invoice_payments::{
            balance::{self, Balance},
            dto::{MarkCreditedInput, Method, Pay, PaymentInput},
            repo::{self, NewPayment, Target},
        },
        invoices,
    },
    numbering::Appendix,
};

const NOT_OPEN: &str = "Invoice must be pending or partially paid to create a payment";
const CREDIT_NOTE: &str =
    "A credit note is refunded with mark-as-credited or a settle, not a payment.";
const BAD_AMOUNT: &str = "The payment amount is invalid";
const NO_SETTLE: &str = "The selected settle invoice id is invalid.";
const BAD_SETTLE: &str =
    "The settle invoice id must be a pending credit note of the same relation and currency.";
const NOT_CREDITABLE: &str = "Only a pending credit note can be marked as credited.";
const OUT_OF_RANGE: &str = "The amount is out of range.";
/// Laravel MarkInvoiceAsCredited's description.
const REFUNDED: &str = "Refunded payment";
/// Live payments one invoice holds (each write re-sums them; the detail lists them).
const MAX_PAYMENTS: i64 = 500;

/// POST /invoices/{id}/payments: one payment, or a settle (two rows summing to zero).
pub async fn create(
    state: &AppState,
    workspace_id: Uuid,
    user_id: Uuid,
    invoice: &str,
    input: PaymentInput,
    key: Option<Key>,
) -> Result<Answer, AppError> {
    let id = parse(invoice)?;
    let mut tx = state.db.begin().await?;
    if let Some(answer) = idempotency::replay(&mut tx, workspace_id, user_id, key.as_ref()).await? {
        return Ok(answer);
    }
    let settle = match input.pay {
        Pay::Settle(note) => Some(note),
        Pay::Method(_) => None,
    };
    let mut locked = lock_all(&mut tx, workspace_id, &[Some(id), settle]).await?;
    let main = locked.remove(&id).ok_or(AppError::NotFound)?;
    if main.is_credit {
        return Err(AppError::Unprocessable(CREDIT_NOTE.into()));
    }
    if !matches!(main.status.as_str(), "pending" | "partially_paid") {
        return Err(AppError::Unprocessable(NOT_OPEN.into()));
    }
    if repo::count_live(&mut tx, workspace_id, id).await? >= MAX_PAYMENTS {
        return Err(AppError::Unprocessable(format!(
            "An invoice holds at most {MAX_PAYMENTS} payments."
        )));
    }
    let dp = dp(&mut tx, workspace_id, &main).await?;
    let amount = minor(input.amount, dp)?;
    // The paid total stays within i64 (its SUM would fail otherwise).
    main.paid_total
        .checked_add(amount)
        .ok_or_else(|| AppError::field("amount", OUT_OF_RANGE))?;
    let payment_id = Uuid::now_v7();
    let row = |method: &'static str, provider, settle_twin_id| NewPayment {
        id: payment_id,
        invoice_id: id,
        user_id,
        date: input.date,
        amount,
        description: input.description.as_deref(),
        payment_method: method,
        provider,
        provider_id: input.provider_id.as_deref(),
        meta: input.meta.as_ref(),
        settle_twin_id,
    };
    match (input.pay, settle) {
        (Pay::Method(Method { method, provider }), _) => {
            repo::insert(&mut tx, workspace_id, &[row(method, provider, None)]).await?;
            rebalance(&mut tx, workspace_id, &main, None).await?;
        }
        (Pay::Settle(_), Some(note_id)) => {
            let note = locked
                .remove(&note_id)
                .ok_or_else(|| AppError::field("settle_invoice_id", NO_SETTLE))?;
            check_settle(&mut tx, workspace_id, &main, &note, amount, dp).await?;
            let twin_id = Uuid::now_v7();
            let twin_text = format!("Payment for invoice {}", number(&main));
            let mut main_row = row("settled", None, Some(twin_id));
            main_row.provider_id = None;
            let twin = NewPayment {
                id: twin_id,
                invoice_id: note_id,
                user_id,
                date: input.date,
                amount: -amount,
                description: Some(&twin_text),
                payment_method: "settled",
                provider: None,
                provider_id: None,
                meta: None,
                settle_twin_id: Some(payment_id),
            };
            repo::insert(&mut tx, workspace_id, &[main_row, twin]).await?;
            rebalance(&mut tx, workspace_id, &main, None).await?;
            rebalance(&mut tx, workspace_id, &note, None).await?;
        }
        (Pay::Settle(_), None) => {
            return Err(AppError::Internal("settle without a credit note".into()));
        }
    }
    let answer = Answer::created(&json!({ "data": { "invoice_payment_id": payment_id } }));
    idempotency::finish(&mut tx, workspace_id, key.as_ref(), &answer).await?;
    tx.commit().await?;
    Ok(answer)
}

/// Laravel's settle checks plus the ones it lacked: a sent, pending credit note of the same
/// relation, currency and decimals, and no more than both the note and the invoice have open
/// (credit is never burned on an overpayment).
async fn check_settle(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    main: &Target,
    note: &Target,
    amount: i64,
    dp: u32,
) -> Result<(), AppError> {
    let fits = note.is_credit
        && note.invoice_number.is_some()
        && note.status == "pending"
        && note.relation_id == main.relation_id
        && note.currency_id == main.currency_id
        && self::dp(conn, workspace_id, note).await? == dp;
    if !fits {
        return Err(AppError::field("settle_invoice_id", BAD_SETTLE));
    }
    let note_open = note.paid_total - note.total;
    let main_open = main.total - main.paid_total;
    if amount > note_open || amount > main_open {
        return Err(AppError::Unprocessable(BAD_AMOUNT.into()));
    }
    Ok(())
}

/// DELETE /invoices/{id}/payments/{pid}: void the payment (and its settle twin); 404 when it
/// isn't a live payment of that invoice.
pub async fn void(
    state: &AppState,
    workspace_id: Uuid,
    user_id: Uuid,
    invoice: &str,
    payment: &str,
    key: Option<Key>,
) -> Result<Answer, AppError> {
    let (id, payment) = (parse(invoice)?, parse(payment)?);
    let mut tx = state.db.begin().await?;
    if let Some(answer) = idempotency::replay(&mut tx, workspace_id, user_id, key.as_ref()).await? {
        return Ok(answer);
    }
    let twin_invoice = repo::find_live(&mut *tx, workspace_id, id, payment)
        .await?
        .ok_or(AppError::NotFound)?;
    let locked = lock_all(&mut tx, workspace_id, &[Some(id), twin_invoice]).await?;
    if !locked.contains_key(&id) || twin_invoice.is_some_and(|t| !locked.contains_key(&t)) {
        return Err(AppError::NotFound);
    }
    if repo::void(&mut tx, workspace_id, id, payment, user_id).await? == 0 {
        // Voided by a request that held the lock first.
        return Err(AppError::NotFound);
    }
    for t in locked.values() {
        rebalance(&mut tx, workspace_id, t, None).await?;
    }
    let answer = Answer::ok();
    idempotency::finish(&mut tx, workspace_id, key.as_ref(), &answer).await?;
    tx.commit().await?;
    Ok(answer)
}

/// POST /invoices/{id}/mark-as-credited: a pending credit note gets its open balance refunded
/// (a negative payment) and is credited; one already credited is a quiet ok.
pub async fn mark_as_credited(
    state: &AppState,
    workspace_id: Uuid,
    user_id: Uuid,
    invoice: &str,
    input: MarkCreditedInput,
    key: Option<Key>,
) -> Result<Answer, AppError> {
    let id = parse(invoice)?;
    let mut tx = state.db.begin().await?;
    if let Some(answer) = idempotency::replay(&mut tx, workspace_id, user_id, key.as_ref()).await? {
        return Ok(answer);
    }
    let note = repo::lock(&mut tx, workspace_id, id)
        .await?
        .ok_or(AppError::NotFound)?;
    let answer = Answer::ok();
    if note.is_credit && note.status == "credited" {
        idempotency::finish(&mut tx, workspace_id, key.as_ref(), &answer).await?;
        tx.commit().await?;
        return Ok(answer);
    }
    if !(note.is_credit && note.status == "pending") {
        return Err(AppError::BadRequest(NOT_CREDITABLE.into()));
    }
    let open = note.total - note.paid_total;
    if open != 0 {
        // Laravel kept the provider's transaction id in meta too; an empty meta is none.
        let meta = match (input.meta, input.provider_id.as_deref()) {
            (Some(Value::Object(mut m)), Some(t)) => {
                m.insert("transaction_id".into(), Value::from(t));
                Some(Value::Object(m))
            }
            (_, Some(t)) => Some(json!({ "transaction_id": t })),
            (meta, None) => meta.filter(|m| m.as_object().is_some_and(|o| !o.is_empty())),
        };
        repo::insert(
            &mut tx,
            workspace_id,
            &[NewPayment {
                id: Uuid::now_v7(),
                invoice_id: id,
                user_id,
                date: input.date,
                amount: open,
                description: Some(input.description.as_deref().unwrap_or(REFUNDED)),
                payment_method: input.method.method,
                provider: input.method.provider,
                provider_id: input.provider_id.as_deref(),
                meta: meta.as_ref(),
                settle_twin_id: None,
            }],
        )
        .await?;
    }
    rebalance(&mut tx, workspace_id, &note, Some("credited")).await?;
    idempotency::finish(&mut tx, workspace_id, key.as_ref(), &answer).await?;
    tx.commit().await?;
    Ok(answer)
}

/// Lock the live invoices `ids` in id order (so two writes over the same pair never
/// deadlock); the ones found, by id.
async fn lock_all(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    ids: &[Option<Uuid>],
) -> Result<BTreeMap<Uuid, Target>, AppError> {
    let mut sorted: Vec<Uuid> = ids.iter().flatten().copied().collect();
    sorted.sort_unstable();
    sorted.dedup();
    let mut found = BTreeMap::new();
    for id in sorted {
        if let Some(t) = repo::lock(conn, workspace_id, id).await? {
            found.insert(id, t);
        }
    }
    Ok(found)
}

/// Recompute the invoice's paid total from its ledger rows and its status from that sum
/// (`status` forces one, as mark-as-credited does).
async fn rebalance(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    t: &Target,
    status: Option<&'static str>,
) -> Result<(), AppError> {
    let (paid, cash) = repo::sums(conn, workspace_id, t.id).await?;
    let dp = dp(conn, workspace_id, t).await?;
    let status = status.or_else(|| {
        balance::status(
            &t.status,
            t.is_credit,
            Balance {
                total: t.total,
                paid,
                cash,
                dp,
            },
        )
    });
    repo::set_balance(conn, workspace_id, t.id, paid, status).await?;
    Ok(())
}

/// Decimals the invoice was priced at (frozen once sent), else its currency's.
async fn dp(conn: &mut PgConnection, workspace_id: Uuid, t: &Target) -> Result<u32, AppError> {
    let decimals = match t.dp {
        Some(dp) => dp,
        None => i32::from(
            invoices::repo::currency_decimals(&mut *conn, workspace_id, t.currency_id).await?,
        ),
    };
    u32::try_from(decimals).map_err(|_| AppError::Internal("negative currency decimals".into()))
}

/// `amount` in minor units at `dp`; 422 past the currency's decimals or i64.
fn minor(amount: Decimal, dp: u32) -> Result<i64, AppError> {
    if amount.normalize().scale() > dp {
        return Err(AppError::field(
            "amount",
            format!("The amount must have at most {dp} decimal places."),
        ));
    }
    10_i64
        .checked_pow(dp)
        .and_then(|scale| amount.checked_mul(Decimal::from(scale)))
        .and_then(|m| i64::try_from(m).ok())
        .ok_or_else(|| AppError::field("amount", OUT_OF_RANGE))
}

/// The invoice's formatted number, as its PDF prints it.
fn number(t: &Target) -> String {
    t.invoice_number
        .map(|n| {
            Appendix::from_section(t.invoice_appendix.as_ref().unwrap_or(&Value::Null)).format(n)
        })
        .unwrap_or_default()
}

fn parse(id: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(id).map_err(|_| AppError::NotFound)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn amounts_turn_into_minor_units_exactly() {
        let m = |s: &str, dp| minor(Decimal::from_str(s).unwrap(), dp).ok();
        assert_eq!(m("39.98", 2), Some(3_998));
        assert_eq!(m("100", 2), Some(10_000));
        assert_eq!(
            m("1.50", 2),
            Some(150),
            "trailing zeros are no extra decimals"
        );
        assert_eq!(m("1.234", 2), None);
        assert_eq!(m("1.234", 3), Some(1_234));
        assert_eq!(m("12", 0), Some(12));
        assert_eq!(m("99999999999999999", 2), None, "past i64");
    }
}
