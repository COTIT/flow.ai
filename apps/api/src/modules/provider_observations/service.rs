//! Confirmed provider facts in active books (Flow ERP M4 C12; masterplan D16, §12 EXTERNAL
//! OBSERVATIONS). `ingest` runs in the caller's confirming tx after gate → order → invoices →
//! operation locks: persist the observation once, post its canonical effect inside a savepoint,
//! else keep it with a pending exception. Excess refund/chargeback lands on a recovery item
//! (`financial_discrepancy`) with a discrepancy exception, resolved by assign or write-off.
//! Identities: link + effects (`provider_observation`, observation id, `received` | `refunded`
//! | `charged_back`); invoice collections reuse the receipt (`invoice_payment`, operation id).

use chrono::NaiveDate;
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{Connection, PgConnection};
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::workspace::WorkspaceAccess,
    idempotency::{self, Answer, Command},
    modules::{
        accounting::{
            dto::Action,
            journals::{SourceHeader, SourceJournal, SourceLine, post_source, post_source_lines},
            service::{ok, stale, unsupported},
            settlement::{self, MoneyLocation, NewAllocation, NewOpenItem, Proof, Side},
        },
        cash::repo::entry_effect,
        gateway::repo::Operation,
        invoice_payments::{self, dto::Method, receipt},
        provider_observations::dto::{
            ExceptionResource, ResolveInput, ResolvePayload, ResolveReview, RetryPayload,
        },
        stock::value,
    },
};

pub const SOURCE: &str = "provider_observation";
/// Legacy operation flag while an observation waits for the book.
pub const PENDING: &str = "Confirmed by the provider; posting to the book is pending.";
const FOREIGN: &str = "Provider money in another currency than the book's can't be booked yet.";
/// System actor of provider-driven effects (as shipments recognition).
const SYSTEM: Uuid = Uuid::nil();

/// Persist one confirmed fact and post it; caller holds the locks and checked the book is active.
/// Replays of the same fact are no-ops.
pub async fn ingest(
    conn: &mut PgConnection,
    ws: Uuid,
    op: &Operation,
    kind: &str,
    external_id: &str,
    amount: i64,
) -> Result<(), AppError> {
    let fp: [u8; 32] = Sha256::digest(
        json!({"operation": op.id, "kind": kind, "external": external_id, "amount": amount,
               "currency": op.currency})
        .to_string(),
    )
    .into();
    // Covered opening callback: no-op only for the exact reviewed historical fact (§12); a new
    // refund or chargeback is a new event even when its parent was covered.
    let fact = crate::modules::accounting::opening::provider_fact(kind, amount, &op.currency);
    if crate::modules::accounting::opening::covered_exactly(conn, ws, op.id, &fact).await? {
        return Ok(());
    }
    let parent = match kind {
        "collection" => None,
        "chargeback" => Some(op.id),
        _ => op.parent_id,
    };
    let id: Option<Uuid> = sqlx::query_scalar(
        "INSERT INTO provider_observations (workspace_id, provider, environment, merchant_id,
             operation_id, parent_operation_id, economic_kind, external_id, invoice_id,
             credit_note_id, order_id, relation_id, amount, currency, decimals, occurred_at,
             fingerprint)
         SELECT p.workspace_id, p.provider, p.environment, p.merchant_id, p.id, $3, $4, $5,
                coalesce(p.invoice_id, o.invoice_id), p.credit_note_id, p.order_id,
                coalesce(i.relation_id, o.relation_id), $6, p.currency, p.decimals, now(), $7
         FROM order_payments p
         LEFT JOIN orders o ON o.workspace_id = p.workspace_id AND o.id = p.order_id
         LEFT JOIN invoices i ON i.workspace_id = p.workspace_id
              AND i.id = coalesce(p.credit_note_id, p.invoice_id, o.invoice_id)
         WHERE p.workspace_id = $1 AND p.id = $2
         ON CONFLICT (workspace_id, provider, economic_kind, external_id) DO NOTHING
         RETURNING id",
    )
    .bind(ws)
    .bind(op.id)
    .bind(parent)
    .bind(kind)
    .bind(external_id)
    .bind(amount)
    .bind(fp.as_slice())
    .fetch_optional(&mut *conn)
    .await?;
    let Some(id) = id else { return Ok(()) };
    match attempt(conn, ws, id).await? {
        Ok(effect) => {
            settlement::link_source(conn, ws, SOURCE, id, Some(effect), "posted", &fp).await?;
        }
        Err(e) => {
            settlement::link_source(conn, ws, SOURCE, id, None, "pending_observation", &fp).await?;
            sqlx::query(
                "INSERT INTO accounting_exceptions (workspace_id, observation_id, kind,
                     reason_code, message, attempts, last_error)
                 VALUES ($1, $2, 'pending_posting', $3, $4, 1, $4)",
            )
            .bind(ws)
            .bind(id)
            .bind(reason_code(&e))
            .bind(message(&e))
            .execute(&mut *conn)
            .await?;
            flag(conn, ws, op.id, true).await?;
        }
    }
    Ok(())
}

/// Post observation `id` inside a savepoint: Ok(Ok(effect)) posted, Ok(Err) rolled back.
async fn attempt(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
) -> Result<Result<Uuid, AppError>, AppError> {
    let mut sp = conn.begin().await?;
    match post(&mut sp, ws, id).await {
        Ok(effect) => {
            sp.commit().await?;
            Ok(Ok(effect))
        }
        Err(e) => {
            sp.rollback().await?;
            Ok(Err(e))
        }
    }
}

#[derive(sqlx::FromRow)]
struct Obs {
    id: Uuid,
    operation_id: Uuid,
    parent_operation_id: Option<Uuid>,
    economic_kind: String,
    external_id: String,
    invoice_id: Option<Uuid>,
    credit_note_id: Option<Uuid>,
    order_id: Option<Uuid>,
    relation_id: Option<Uuid>,
    amount: i64,
    decimals: i16,
    date: NaiveDate,
    currency_id: Option<Uuid>,
    functional: Option<bool>,
}

async fn load(conn: &mut PgConnection, ws: Uuid, id: Uuid) -> Result<Obs, AppError> {
    sqlx::query_as(
        "SELECT b.id, b.operation_id, b.parent_operation_id, b.economic_kind, b.external_id,
                b.invoice_id, b.credit_note_id, b.order_id, b.relation_id, b.amount, b.decimals,
                b.occurred_at::date AS date, coalesce(i.currency_id, o.currency_id) AS currency_id,
                coalesce(i.currency_id, o.currency_id) = s.functional_currency_id AS functional
         FROM provider_observations b
         LEFT JOIN invoices i ON i.workspace_id = b.workspace_id
              AND i.id = coalesce(b.credit_note_id, b.invoice_id)
         LEFT JOIN orders o ON o.workspace_id = b.workspace_id AND o.id = b.order_id
         LEFT JOIN accounting_settings s ON s.workspace_id = b.workspace_id
         WHERE b.workspace_id = $1 AND b.id = $2",
    )
    .bind(ws)
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(AppError::NotFound)
}

/// Canonical effect of one observation; its main effect id.
async fn post(conn: &mut PgConnection, ws: Uuid, id: Uuid) -> Result<Uuid, AppError> {
    let o = load(conn, ws, id).await?;
    let currency = o.currency_id.ok_or(AppError::NotFound)?;
    if o.functional.is_none_or(|f| !f) {
        return Err(unsupported(FOREIGN));
    }
    let fp: [u8; 32] = Sha256::digest(o.id.as_bytes()).into();
    match (o.economic_kind.as_str(), o.invoice_id, o.credit_note_id) {
        ("collection", Some(invoice), _) => {
            let target = invoice_payments::repo::lock(&mut *conn, ws, invoice)
                .await?
                .ok_or(AppError::NotFound)?;
            let location = MoneyLocation::ProviderClearing(Proof {
                reference: o.external_id.clone(),
                evidence_ref: o.id.to_string(),
                confirmation_method: "provider_callback",
            });
            receipt::book(
                conn,
                ws,
                &receipt::Receipt {
                    id: o.operation_id,
                    invoice: &target,
                    date: o.date,
                    amount: o.amount,
                    dp: u32::try_from(o.decimals).unwrap_or(2),
                    method: Method {
                        method: "provider",
                        provider: Some("globadyme"),
                    },
                    location: Some(&location),
                    cash_rounding: None,
                    user: SYSTEM,
                },
            )
            .await?;
            Ok(sqlx::query_scalar(
                "SELECT id FROM accounting_effects WHERE workspace_id = $1
                 AND source_kind = 'invoice_payment' AND source_id = $2 AND effect_kind = 'received'",
            )
            .bind(ws)
            .bind(o.operation_id)
            .fetch_one(&mut *conn)
            .await?)
        }
        ("collection", None, _) => {
            // Shop payment before any invoice: unapplied customer funds of the order.
            let journal = SourceJournal {
                source_kind: SOURCE,
                source_id: o.id,
                effect_kind: "received",
                date: o.date,
                reason: "Provider collection",
                debit: Side::Role("provider_clearing"),
                credit: Side::Role("customer_deposits"),
                relation_id: o.relation_id,
                bank_account_id: None,
                amount: o.amount,
                fingerprint: &fp,
                by: SYSTEM,
            };
            let entry = post_source(conn, ws, &journal).await?;
            let effect = entry_effect(conn, ws, entry).await?;
            let account_id = settlement::role_account(conn, ws, "customer_deposits").await?;
            settlement::open_item(
                conn,
                ws,
                &item(
                    &o,
                    "customer_deposit",
                    "credit",
                    "customer_deposits",
                    account_id,
                    currency,
                    effect,
                ),
            )
            .await?;
            Ok(effect)
        }
        (kind, _, note) => outflow(conn, ws, &o, kind, note, currency, &fp).await,
    }
}

fn item<'a>(
    o: &Obs,
    classification: &'a str,
    side: &'a str,
    role: &'a str,
    account_id: Uuid,
    currency_id: Uuid,
    effect_id: Uuid,
) -> NewOpenItem<'a> {
    NewOpenItem {
        classification,
        side,
        account_role: role,
        account_id,
        source_kind: SOURCE,
        source_id: o.id,
        relation_id: o.relation_id,
        order_id: o.order_id,
        invoice_id: o.invoice_id,
        recognition_id: None,
        currency_id,
        issue_date: o.date,
        due_date: None,
        origin: "posted",
        amount: o.amount,
        functional_amount: o.amount,
        effect_id,
        by: SYSTEM,
    }
}

/// Open credit-note credit eligible for a refund, oldest first, locked.
pub async fn eligible_credit(
    conn: &mut PgConnection,
    ws: Uuid,
    credit_note: Uuid,
) -> Result<Vec<(Uuid, Uuid, i64)>, AppError> {
    Ok(sqlx::query_as(
        "SELECT id, account_id, residual FROM accounting_open_items
         WHERE workspace_id = $1 AND classification = 'customer_credit' AND invoice_id = $2
           AND residual > 0
         ORDER BY issue_date, id FOR UPDATE",
    )
    .bind(ws)
    .bind(credit_note)
    .fetch_all(conn)
    .await?)
}

/// Unapplied deposit of the source payment (invoice receipt or shop collection), locked.
async fn source_deposit(
    conn: &mut PgConnection,
    ws: Uuid,
    payment: Uuid,
) -> Result<Vec<(Uuid, Uuid, i64)>, AppError> {
    Ok(sqlx::query_as(
        "SELECT i.id, i.account_id, i.residual FROM accounting_open_items i
         WHERE i.workspace_id = $1 AND i.classification = 'customer_deposit' AND i.residual > 0
           AND ((i.source_kind = 'invoice_payment' AND i.source_id = $2)
             OR (i.source_kind = 'provider_observation' AND i.source_id IN (
                 SELECT id FROM provider_observations WHERE workspace_id = $1
                 AND operation_id = $2 AND economic_kind = 'collection')))
         ORDER BY i.issue_date, i.id FOR UPDATE",
    )
    .bind(ws)
    .bind(payment)
    .fetch_all(conn)
    .await?)
}

/// Refund/chargeback: Dr eligible credit (residual) + Dr financial_discrepancy (rest) /
/// Cr provider_clearing; the rest opens a recovery item with a discrepancy exception.
async fn outflow(
    conn: &mut PgConnection,
    ws: Uuid,
    o: &Obs,
    kind: &str,
    note: Option<Uuid>,
    currency: Uuid,
    fp: &[u8; 32],
) -> Result<Uuid, AppError> {
    let items = match (kind, note) {
        ("refund", Some(note)) => eligible_credit(conn, ws, note).await?,
        _ => source_deposit(conn, ws, o.parent_operation_id.unwrap_or(o.operation_id)).await?,
    };
    // Per item applied = least(residual, what is left), in SQL.
    let ids: Vec<Uuid> = items.iter().map(|i| i.0).collect();
    let applied: Vec<(Uuid, Uuid, i64)> = sqlx::query_as(
        "SELECT id, account_id, applied FROM (
             SELECT id, account_id, least(residual, greatest($2 - coalesce(sum(residual) OVER (
                 ORDER BY ord ROWS BETWEEN UNBOUNDED PRECEDING AND 1 PRECEDING), 0), 0))::bigint AS applied,
                 ord
             FROM accounting_open_items JOIN unnest($1::uuid[]) WITH ORDINALITY AS u(uid, ord)
               ON u.uid = id) a
         WHERE applied > 0 ORDER BY ord",
    )
    .bind(&ids)
    .bind(o.amount)
    .fetch_all(&mut *conn)
    .await?;
    let rest: i64 =
        sqlx::query_scalar("SELECT $1 - coalesce(sum(x), 0)::bigint FROM unnest($2::bigint[]) x")
            .bind(o.amount)
            .bind(applied.iter().map(|a| a.2).collect::<Vec<_>>())
            .fetch_one(&mut *conn)
            .await?;
    let line = |side, debit, credit| SourceLine {
        side,
        debit,
        credit,
        relation_id: o.relation_id,
        bank_account_id: None,
    };
    let mut lines: Vec<SourceLine> = applied
        .iter()
        .map(|a| line(Side::Account(a.1), a.2, 0))
        .collect();
    if rest > 0 {
        lines.push(line(Side::Role("financial_discrepancy"), rest, 0));
    }
    lines.push(line(Side::Role("provider_clearing"), 0, o.amount));
    let effect_kind = if kind == "refund" {
        "refunded"
    } else {
        "charged_back"
    };
    let header = SourceHeader {
        source_kind: SOURCE,
        source_id: o.id,
        effect_kind,
        date: o.date,
        reason: if kind == "refund" {
            "Provider refund"
        } else {
            "Provider chargeback"
        },
        fingerprint: fp,
        by: SYSTEM,
    };
    let (_, effect) = post_source_lines(conn, ws, &header, &lines).await?;
    for a in &applied {
        settlement::allocate(
            conn,
            ws,
            &NewAllocation {
                kind: "refund",
                source_item_id: None,
                target_item_id: a.0,
                date: o.date,
                currency_id: currency,
                amount: a.2,
                effect_id: effect,
                by: SYSTEM,
            },
        )
        .await?;
    }
    if rest > 0 {
        let account_id = settlement::role_account(conn, ws, "financial_discrepancy").await?;
        let mut recovery = item(
            o,
            "recovery_receivable",
            "debit",
            "financial_discrepancy",
            account_id,
            currency,
            effect,
        );
        recovery.amount = rest;
        recovery.functional_amount = rest;
        let recovery = settlement::open_item(conn, ws, &recovery).await?;
        sqlx::query(
            "INSERT INTO accounting_exceptions (workspace_id, observation_id, kind, reason_code,
                 message, recovery_item_id, amount)
             VALUES ($1, $2, 'discrepancy', $3, $4, $5, $6)",
        )
        .bind(ws)
        .bind(o.id)
        .bind(if kind == "refund" { "excess_refund" } else { "chargeback" })
        .bind("Provider paid out more than the customer's open credit; recover or write off the rest.")
        .bind(recovery)
        .bind(rest)
        .execute(&mut *conn)
        .await?;
    }
    Ok(effect)
}

fn reason_code(e: &AppError) -> &'static str {
    match e {
        AppError::Reason {
            code: "accounting_period_closed",
            ..
        } => "closed_period",
        AppError::Reason {
            code: "accounting_not_configured",
            ..
        } => "missing_mapping",
        AppError::Reason { message, .. } if message == FOREIGN => "foreign_currency",
        AppError::Reason { message, .. } if message.contains("no open receivable") => {
            "no_receivable"
        }
        _ => "posting_failed",
    }
}

fn message(e: &AppError) -> String {
    let m = e.to_string();
    let m: String = m.chars().take(1000).collect();
    if m.is_empty() {
        "Posting failed.".into()
    } else {
        m
    }
}

/// Set or clear the legacy PENDING flag on the provider operation.
async fn flag(conn: &mut PgConnection, ws: Uuid, op: Uuid, on: bool) -> Result<(), AppError> {
    sqlx::query(
        "UPDATE order_payments SET attention = $3,
             attention_reason = CASE WHEN $3 THEN $4 ELSE NULL END, updated_at = now()
         WHERE workspace_id = $1 AND id = $2
           AND ($3 OR attention_reason = $4)",
    )
    .bind(ws)
    .bind(op)
    .bind(on)
    .bind(PENDING)
    .execute(conn)
    .await?;
    Ok(())
}

/// Whether the book takes canonical provider effects (shared gate taken here, re-entrant).
pub async fn active(conn: &mut PgConnection, ws: Uuid) -> Result<bool, AppError> {
    value::active(conn, ws).await
}

// ---- exceptions API ----

const NOT_PENDING: &str = "This exception is already resolved.";
const RETRY_ONLY: &str = "A pending posting is retried, not written off or reassigned.";
const RESOLVE_ONLY: &str = "A discrepancy is resolved by assigning or writing it off, not retried.";
const NO_RELATION: &str = "Say which customer owes the recovered amount.";

fn conflict(m: &str) -> AppError {
    AppError::Reason {
        status: axum::http::StatusCode::CONFLICT,
        code: "exception_state",
        message: m.into(),
    }
}

pub(crate) async fn resource(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
) -> Result<ExceptionResource, AppError> {
    sqlx::query_as(
        "SELECT e.id, e.kind, e.reason_code, e.message, e.state, e.resolution, e.attempts,
                e.last_error, e.version, e.created_at, e.resolved_at, e.relation_id,
                e.amount, r.residual AS recovery_residual, b.id AS observation_id, b.provider,
                b.economic_kind, b.external_id, b.amount AS observed_amount, b.currency,
                b.decimals, b.invoice_id, b.credit_note_id, b.order_id, b.operation_id,
                b.occurred_at, l.disposition,
                CASE WHEN e.state <> 'pending' THEN ARRAY[]::text[]
                     WHEN e.kind = 'pending_posting' THEN ARRAY['retry']
                     ELSE ARRAY['resolve'] END AS allowed_actions
         FROM accounting_exceptions e
         JOIN provider_observations b ON b.workspace_id = e.workspace_id AND b.id = e.observation_id
         LEFT JOIN accounting_open_items r ON r.workspace_id = e.workspace_id AND r.id = e.recovery_item_id
         LEFT JOIN accounting_source_links l ON l.workspace_id = e.workspace_id
              AND l.source_kind = 'provider_observation' AND l.source_id = b.id
         WHERE e.workspace_id = $1 AND e.id = $2",
    )
    .bind(ws)
    .bind(id)
    .fetch_optional(conn)
    .await?
    .ok_or(AppError::NotFound)
}

/// GET /accounting/exceptions: newest first, keyset by id.
pub async fn list(
    state: &AppState,
    ws: Uuid,
    pending_only: bool,
    before: Option<Uuid>,
    limit: i64,
) -> Result<Vec<ExceptionResource>, AppError> {
    let mut conn = state.db.acquire().await?;
    let ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM accounting_exceptions WHERE workspace_id = $1
           AND (NOT $2 OR state = 'pending') AND ($3::uuid IS NULL OR id < $3)
         ORDER BY id DESC LIMIT $4",
    )
    .bind(ws)
    .bind(pending_only)
    .bind(before)
    .bind(limit)
    .fetch_all(&mut *conn)
    .await?;
    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        // shortcut: one indexed lookup per row, page ≤ 100; join list query if pages grow.
        out.push(resource(&mut conn, ws, id).await?);
    }
    Ok(out)
}

pub async fn get(state: &AppState, ws: Uuid, id: Uuid) -> Result<ExceptionResource, AppError> {
    let mut conn = state.db.acquire().await?;
    resource(&mut conn, ws, id).await
}

#[derive(sqlx::FromRow)]
struct Locked {
    observation_id: Uuid,
    kind: String,
    state: String,
    version: i64,
    operation_id: Uuid,
    invoice_id: Option<Uuid>,
    credit_note_id: Option<Uuid>,
    order_id: Option<Uuid>,
    relation_id: Option<Uuid>,
    recovery_item_id: Option<Uuid>,
    currency_id: Option<Uuid>,
}

/// Operation, order, invoice and credit note of an observation.
type Owner = (Uuid, Option<Uuid>, Option<Uuid>, Option<Uuid>);

/// Gate → order → invoices → operation → exception, as ingestion.
async fn lock(conn: &mut PgConnection, ws: Uuid, id: Uuid) -> Result<Locked, AppError> {
    if !active(conn, ws).await? {
        return Err(crate::modules::accounting::service::not_configured());
    }
    let owner: Option<Owner> = sqlx::query_as(
        "SELECT b.operation_id, b.order_id, b.invoice_id, b.credit_note_id
         FROM accounting_exceptions e JOIN provider_observations b
           ON b.workspace_id = e.workspace_id AND b.id = e.observation_id
         WHERE e.workspace_id = $1 AND e.id = $2",
    )
    .bind(ws)
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?;
    let (op, order, invoice, note) = owner.ok_or(AppError::NotFound)?;
    if let Some(order) = order {
        crate::modules::orders::repo::lock(&mut *conn, ws, order).await?;
    }
    let mut ids: Vec<Uuid> = [invoice, note].into_iter().flatten().collect();
    ids.sort_unstable();
    for i in ids {
        invoice_payments::repo::lock(&mut *conn, ws, i).await?;
    }
    sqlx::query("SELECT 1 FROM order_payments WHERE workspace_id = $1 AND id = $2 FOR UPDATE")
        .bind(ws)
        .bind(op)
        .execute(&mut *conn)
        .await?;
    sqlx::query_as(
        "SELECT e.observation_id, e.kind, e.state, e.version, b.operation_id, b.invoice_id,
                b.credit_note_id, b.order_id, b.relation_id, e.recovery_item_id,
                r.currency_id
         FROM accounting_exceptions e JOIN provider_observations b
           ON b.workspace_id = e.workspace_id AND b.id = e.observation_id
         LEFT JOIN accounting_open_items r ON r.workspace_id = e.workspace_id
              AND r.id = e.recovery_item_id
         WHERE e.workspace_id = $1 AND e.id = $2 FOR UPDATE OF e",
    )
    .bind(ws)
    .bind(id)
    .fetch_one(&mut *conn)
    .await
    .map_err(Into::into)
}

/// Paid projections of the observation's invoices after a late posting.
async fn reproject(conn: &mut PgConnection, ws: Uuid, l: &Locked) -> Result<(), AppError> {
    for id in [l.invoice_id, l.credit_note_id].into_iter().flatten() {
        let target = invoice_payments::repo::lock(&mut *conn, ws, id)
            .await?
            .ok_or(AppError::NotFound)?;
        invoice_payments::service::rebalance(conn, ws, &target, None).await?;
    }
    Ok(())
}

/// POST /accounting/exceptions/{id}/retry: rerun the source adapter; no provider call.
pub async fn retry(
    state: &AppState,
    access: &WorkspaceAccess,
    id: Uuid,
    cmd: Command<Action<RetryPayload>>,
) -> Result<Answer, AppError> {
    const OP: &str = "accounting_exceptions_retry";
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, access.user_id, &cmd).await? {
        return Ok(a);
    }
    let l = lock(&mut tx, ws, id).await?;
    if l.state != "pending" {
        return Err(conflict(NOT_PENDING));
    }
    if l.kind != "pending_posting" {
        return Err(conflict(RESOLVE_ONLY));
    }
    if l.version != cmd.input.expected_version {
        return Err(stale());
    }
    match attempt(&mut tx, ws, l.observation_id).await? {
        Ok(effect) => {
            sqlx::query(
                "UPDATE accounting_source_links SET disposition = 'posted', effect_id = $3
                 WHERE workspace_id = $1 AND source_kind = $4 AND source_id = $2",
            )
            .bind(ws)
            .bind(l.observation_id)
            .bind(effect)
            .bind(SOURCE)
            .execute(&mut *tx)
            .await?;
            sqlx::query(
                "UPDATE accounting_exceptions SET state = 'resolved', resolution = 'posted',
                     resolution_effect_id = $3, attempts = attempts + 1, last_error = NULL,
                     version = version + 1, resolved_by = $4, resolved_at = clock_timestamp()
                 WHERE workspace_id = $1 AND id = $2",
            )
            .bind(ws)
            .bind(id)
            .bind(effect)
            .bind(access.user_id)
            .execute(&mut *tx)
            .await?;
            reproject(&mut tx, ws, &l).await?;
            flag(&mut tx, ws, l.operation_id, false).await?;
            let answer = ok(&resource(&mut tx, ws, id).await?);
            idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[id]).await?;
            tx.commit().await?;
            Ok(answer)
        }
        Err(e) => {
            // The failed attempt is a fact too: count it, then refuse.
            sqlx::query(
                "UPDATE accounting_exceptions SET attempts = attempts + 1, last_error = $3,
                     reason_code = reason_code
                 WHERE workspace_id = $1 AND id = $2",
            )
            .bind(ws)
            .bind(id)
            .bind(message(&e))
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            Err(e)
        }
    }
}

/// Facts of a resolve, checked; review fingerprint over them.
struct Prepared {
    l: Locked,
    relation: Option<Uuid>,
    date: NaiveDate,
    review: ResolveReview,
}

async fn prepare(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
    i: &ResolveInput,
) -> Result<Prepared, AppError> {
    let l = lock(conn, ws, id).await?;
    if l.state != "pending" {
        return Err(conflict(NOT_PENDING));
    }
    if l.kind != "discrepancy" {
        return Err(AppError::field("payload.resolution", RETRY_ONLY));
    }
    let relation = match i.resolution {
        "assign_receivable" => {
            let r = i.relation_id.or(l.relation_id);
            let found: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM relations WHERE workspace_id = $1 AND id = $2)",
            )
            .bind(ws)
            .bind(r)
            .fetch_one(&mut *conn)
            .await?;
            if !found {
                return Err(AppError::field("payload.relation_id", NO_RELATION));
            }
            r
        }
        _ => None,
    };
    let residual = match l.recovery_item_id {
        Some(item) => settlement::residual(conn, ws, item).await?.residual,
        None => 0,
    };
    let date = crate::modules::shipments::repo::book_today(&mut *conn, ws).await?;
    let dp: i16 = sqlx::query_scalar(
        "SELECT decimals::int2 FROM currencies WHERE workspace_id = $1 AND id = $2",
    )
    .bind(ws)
    .bind(l.currency_id)
    .fetch_one(&mut *conn)
    .await?;
    let digest: [u8; 32] = Sha256::digest(
        json!({"id": id, "version": l.version, "resolution": i.resolution, "relation": relation,
               "residual": residual, "date": date})
        .to_string(),
    )
    .into();
    let review = ResolveReview {
        resolution: i.resolution.into(),
        relation_id: relation,
        amount: crate::modules::purchasing::service::amount(residual, dp),
        accounting_date: date,
        debit: if i.resolution == "write_off" {
            "financial_loss"
        } else {
            "receivables"
        }
        .into(),
        credit: "financial_discrepancy".into(),
        review_fingerprint: hex::encode(digest),
    };
    Ok(Prepared {
        l,
        relation,
        date,
        review,
    })
}

/// POST /accounting/exceptions/{id}/resolve/review: no writes.
pub async fn resolve_review(
    state: &AppState,
    ws: Uuid,
    id: Uuid,
    i: &ResolveInput,
) -> Result<ResolveReview, AppError> {
    let mut tx = state.db.begin().await?;
    let p = prepare(&mut tx, ws, id, i).await?;
    tx.rollback().await?;
    Ok(p.review)
}

/// POST /accounting/exceptions/{id}/resolve: assign the recovery to a customer receivable, or
/// write it off to financial_loss; reviewed, once.
pub async fn resolve(
    state: &AppState,
    access: &WorkspaceAccess,
    id: Uuid,
    cmd: Command<Action<ResolvePayload>>,
) -> Result<Answer, AppError> {
    const OP: &str = "accounting_exceptions_resolve";
    let ws = access.workspace_id;
    let by = access.user_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, by, &cmd).await? {
        return Ok(a);
    }
    let (i, fingerprint) = &cmd.input.payload;
    let p = prepare(&mut tx, ws, id, i).await?;
    if p.l.version != cmd.input.expected_version || &p.review.review_fingerprint != fingerprint {
        return Err(stale());
    }
    let item = p.l.recovery_item_id.ok_or(AppError::NotFound)?;
    let currency = p.l.currency_id.ok_or(AppError::NotFound)?;
    let residual = settlement::residual(&mut tx, ws, item).await?.residual;
    let digest: [u8; 32] = Sha256::digest(fingerprint.as_bytes()).into();
    let write_off = i.resolution == "write_off";
    let journal = SourceJournal {
        source_kind: "accounting_exception",
        source_id: id,
        effect_kind: if write_off { "written_off" } else { "assigned" },
        date: p.date,
        reason: if write_off {
            "Recovery written off"
        } else {
            "Recovery assigned"
        },
        debit: Side::Role(if write_off {
            "financial_loss"
        } else {
            "receivables"
        }),
        credit: Side::Role("financial_discrepancy"),
        relation_id: p.relation,
        bank_account_id: None,
        amount: residual,
        fingerprint: &digest,
        by,
    };
    let entry = post_source(&mut tx, ws, &journal).await?;
    let effect = entry_effect(&mut tx, ws, entry).await?;
    if !write_off {
        let account_id = settlement::role_account(&mut tx, ws, "receivables").await?;
        settlement::open_item(
            &mut tx,
            ws,
            &NewOpenItem {
                classification: "receivable",
                side: "debit",
                account_role: "receivables",
                account_id,
                source_kind: "accounting_exception",
                source_id: id,
                relation_id: p.relation,
                order_id: p.l.order_id,
                invoice_id: None,
                recognition_id: None,
                currency_id: currency,
                issue_date: p.date,
                due_date: None,
                origin: "posted",
                amount: residual,
                functional_amount: residual,
                effect_id: effect,
                by,
            },
        )
        .await?;
    }
    settlement::allocate(
        &mut tx,
        ws,
        &NewAllocation {
            kind: if write_off { "write_off" } else { "assignment" },
            source_item_id: None,
            target_item_id: item,
            date: p.date,
            currency_id: currency,
            amount: residual,
            effect_id: effect,
            by,
        },
    )
    .await?;
    sqlx::query(
        "UPDATE accounting_exceptions SET state = 'resolved', resolution = $3,
             resolution_effect_id = $4, relation_id = $5, reason = $6, evidence_ref = $7,
             version = version + 1, resolved_by = $8, resolved_at = clock_timestamp()
         WHERE workspace_id = $1 AND id = $2",
    )
    .bind(ws)
    .bind(id)
    .bind(i.resolution)
    .bind(effect)
    .bind(p.relation)
    .bind(&i.reason)
    .bind(&i.evidence_ref)
    .bind(by)
    .execute(&mut *tx)
    .await?;
    let answer = ok(&resource(&mut tx, ws, id).await?);
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[id]).await?;
    tx.commit().await?;
    Ok(answer)
}
