//! Active-book invoice payment = canonical receipt (Flow ERP M4, masterplan §8/§9/§12).
//! One transaction, §6 order: permanent command row → book gate (taken by the caller) →
//! invoice → money location (drawer/vault/bank) → receivable + new deposit item → accounts and
//! period (`post_source`) → effects/journals → legacy row, cash place row, allocations, link.
//! Journals: receive Dr cash|bank|provider_clearing / Cr customer_deposits; apply
//! Dr customer_deposits / Cr receivables; explicit cash rounding Dr rounding / Cr receivables
//! (or Dr customer_deposits / Cr rounding when the customer paid the rounded-up sum).
//! Identities: (`invoice_payment`, payment id, `received` | `applied` | `rounded`).

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::{
    error::AppError,
    modules::{
        accounting::{
            journals::{SourceJournal, post_source},
            repo as books,
            service::unsupported,
            settlement::{self, Located, MoneyLocation, NewAllocation, NewOpenItem, Side},
        },
        cash::repo::entry_effect,
        invoice_payments::{dto::Method, repo::Target},
    },
    money,
};

const SOURCE: &str = "invoice_payment";
const NO_LOCATION: &str =
    "Say where the money is: a cash place, a bank account or the provider clearing account.";
const WRONG_LOCATION: &str = "This payment method doesn't fit that money location.";
const NO_SHIFT: &str = "The drawer has no open shift.";
const OTHER_CURRENCY: &str = "The money location holds another currency than the invoice.";
const FOREIGN: &str = "Payments in another currency than the book's can't be booked yet.";
const NO_RECEIVABLE: &str = "This invoice has no open receivable in the book yet.";
const NOT_CASH: &str = "Cash rounding needs a cash place.";
const BAD_ROUNDING: &str = "Cash due must be a multiple of 0.05 and the cash rounding (at most 0.05) must settle the open balance exactly.";

/// What the receipt books.
pub struct Receipt<'a> {
    pub id: Uuid,
    pub invoice: &'a Target,
    pub date: NaiveDate,
    /// Minor units, > 0.
    pub amount: i64,
    pub dp: u32,
    pub method: Method,
    pub location: Option<&'a MoneyLocation>,
    pub cash_rounding: Option<Decimal>,
    pub user: Uuid,
}

/// Payment method ↔ location (masterplan §9: legacy labels alone are not attribution).
fn fits(method: Method, location: &MoneyLocation) -> bool {
    match (method.method, location) {
        ("cash", MoneyLocation::CashPlace(_)) => true,
        ("pin" | "manual", MoneyLocation::BankAccount { .. }) => true,
        ("provider", MoneyLocation::BankAccount { .. }) => method.provider == Some("afas"),
        // globadyme: provider observations (C12) book confirmed gateway collections here.
        ("provider", MoneyLocation::ProviderClearing(_)) => {
            matches!(method.provider, Some("mollie" | "globadyme"))
        }
        _ => false,
    }
}

fn fingerprint(r: &Receipt<'_>) -> [u8; 32] {
    let columns = r
        .location
        .map(|l| serde_json::to_value(l.columns()).unwrap_or_default());
    Sha256::digest(
        json!({"invoice": r.invoice.id, "amount": r.amount, "date": r.date, "location": columns,
               "rounding": r.cash_rounding.map(|d| d.to_string())})
        .to_string(),
    )
    .into()
}

/// Book `r` (checks before any effect); the invoice's paid projection is the caller's.
pub async fn book(conn: &mut PgConnection, ws: Uuid, r: &Receipt<'_>) -> Result<(), AppError> {
    let location = r
        .location
        .ok_or_else(|| AppError::field("attribution", NO_LOCATION))?;
    if !fits(r.method, location) {
        return Err(AppError::field("attribution.kind", WRONG_LOCATION));
    }
    let functional = books::settings(&mut *conn, ws, false)
        .await?
        .functional_currency_id;
    if functional != Some(r.invoice.currency_id) {
        return Err(unsupported(FOREIGN));
    }
    let located: Located = location.lock(conn, ws, r.invoice.currency_id).await?;
    if located.currency_id != r.invoice.currency_id {
        return Err(AppError::field("attribution", OTHER_CURRENCY));
    }
    let drawer = matches!(
        location,
        MoneyLocation::CashPlace(crate::modules::cash::dto::Place::Drawer(_))
    );
    if drawer && located.shift_id.is_none() {
        return Err(AppError::field("attribution.cash_place", NO_SHIFT));
    }
    // The invoice's oldest open receivable; applied = least(amount, residual) in SQL.
    let receivable: Option<(Uuid, Uuid, i64, i64)> = sqlx::query_as(
        "SELECT id, account_id, residual, LEAST(residual, $3) FROM accounting_open_items
         WHERE workspace_id = $1 AND invoice_id = $2 AND classification = 'receivable'
           AND residual > 0
         ORDER BY issue_date, id LIMIT 1 FOR NO KEY UPDATE",
    )
    .bind(ws)
    .bind(r.invoice.id)
    .bind(r.amount)
    .fetch_optional(&mut *conn)
    .await?;
    let (ar, ar_account, residual, applied) =
        receivable.ok_or_else(|| unsupported(NO_RECEIVABLE))?;
    let rounding = match r.cash_rounding {
        None => 0,
        Some(_) if !matches!(location, MoneyLocation::CashPlace(_)) => {
            return Err(AppError::field("cash_rounding", NOT_CASH));
        }
        Some(d) => {
            let minor = money::minor(d, r.dp)
                .map_err(|_| AppError::field("cash_rounding", BAD_ROUNDING))?;
            // Cash due is a multiple of 0.05, the explicit rounding at most one 0.05 step,
            // and together they settle the open balance exactly (masterplan §12 R = D − G).
            let ok: bool = sqlx::query_scalar(
                "SELECT $4 >= 2 AND $3 <> 0
                        AND $2 % (5 * 10::numeric ^ ($4 - 2)) = 0
                        AND abs($3) <= 5 * 10::numeric ^ ($4 - 2)
                        AND $2::numeric + $3 = $1",
            )
            .bind(residual)
            .bind(r.amount)
            .bind(minor)
            .bind(i32::try_from(r.dp).unwrap_or(0))
            .fetch_one(&mut *conn)
            .await?;
            if !ok {
                return Err(AppError::field("cash_rounding", BAD_ROUNDING));
            }
            minor
        }
    };
    let deposits = settlement::role_account(conn, ws, "customer_deposits").await?;
    let fp = fingerprint(r);
    let journal = |effect_kind, debit, credit, amount, bank| SourceJournal {
        source_kind: SOURCE,
        source_id: r.id,
        effect_kind,
        date: r.date,
        reason: "Invoice payment",
        debit,
        credit,
        relation_id: Some(r.invoice.relation_id),
        bank_account_id: bank,
        amount,
        fingerprint: &fp,
        by: r.user,
    };
    let bank = match location {
        MoneyLocation::BankAccount { id, .. } => Some(*id),
        _ => None,
    };
    let entry = post_source(
        conn,
        ws,
        &journal(
            "received",
            located.side,
            Side::Account(deposits),
            r.amount,
            bank,
        ),
    )
    .await?;
    let received = entry_effect(conn, ws, entry).await?;
    // Location columns on the legacy row (inserted by the caller just before).
    let c = location.columns();
    sqlx::query(
        "UPDATE invoice_payments SET location_kind = $3, register_id = $4, vault_id = $5,
             bank_account_id = $6, reference = $7, evidence_ref = $8, confirmation_method = $9,
             effect_id = $10
         WHERE workspace_id = $1 AND id = $2",
    )
    .bind(ws)
    .bind(r.id)
    .bind(c.location_kind)
    .bind(c.register_id)
    .bind(c.vault_id)
    .bind(c.bank_account_id)
    .bind(c.reference)
    .bind(c.evidence_ref)
    .bind(c.confirmation_method)
    .bind(received)
    .execute(&mut *conn)
    .await?;
    if matches!(location, MoneyLocation::CashPlace(_)) {
        sqlx::query(
            "INSERT INTO cash_ledger (workspace_id, register_id, vault_id, shift_id, kind, amount,
                 actor_kind, member_user_id, source_kind, source_id, effect_id)
             VALUES ($1, $2, $3, $4, 'receipt', $5, 'member', $6, $7, $8, $9)",
        )
        .bind(ws)
        .bind(c.register_id)
        .bind(c.vault_id)
        .bind(located.shift_id)
        .bind(r.amount)
        .bind(r.user)
        .bind(SOURCE)
        .bind(r.id)
        .bind(received)
        .execute(&mut *conn)
        .await?;
    }
    let deposit = settlement::open_item(
        conn,
        ws,
        &NewOpenItem {
            classification: "customer_deposit",
            side: "credit",
            account_role: "customer_deposits",
            account_id: deposits,
            source_kind: SOURCE,
            source_id: r.id,
            relation_id: Some(r.invoice.relation_id),
            order_id: None,
            invoice_id: Some(r.invoice.id),
            recognition_id: None,
            currency_id: r.invoice.currency_id,
            issue_date: r.date,
            due_date: None,
            origin: "posted",
            amount: r.amount,
            functional_amount: r.amount,
            effect_id: received,
            by: r.user,
        },
    )
    .await?;
    let entry = post_source(
        conn,
        ws,
        &journal(
            "applied",
            Side::Account(deposits),
            Side::Account(ar_account),
            applied,
            None,
        ),
    )
    .await?;
    let effect_id = entry_effect(conn, ws, entry).await?;
    let alloc = |kind, source_item_id, target_item_id, amount, effect_id| NewAllocation {
        kind,
        source_item_id,
        target_item_id,
        date: r.date,
        currency_id: r.invoice.currency_id,
        amount,
        effect_id,
        by: r.user,
    };
    settlement::allocate(
        conn,
        ws,
        &alloc("application", Some(deposit), ar, applied, effect_id),
    )
    .await?;
    if rounding != 0 {
        // Positive: the receivable keeps |R| open; negative: the deposit keeps |R| left over.
        let (debit, credit, target) = if rounding > 0 {
            (Side::Role("rounding"), Side::Account(ar_account), ar)
        } else {
            (Side::Account(deposits), Side::Role("rounding"), deposit)
        };
        let size = rounding
            .checked_abs()
            .ok_or_else(|| AppError::field("cash_rounding", BAD_ROUNDING))?;
        let entry = post_source(conn, ws, &journal("rounded", debit, credit, size, None)).await?;
        let effect_id = entry_effect(conn, ws, entry).await?;
        settlement::allocate(conn, ws, &alloc("rounding", None, target, size, effect_id)).await?;
    }
    settlement::link_source(conn, ws, SOURCE, r.id, Some(received), "posted", &fp).await?;
    Ok(())
}
