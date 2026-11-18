//! One-shot financial dispatch and authoritative reconciliation shared by orders/invoices.
use crate::{
    AppError, AppState,
    http::app_key::AppKey,
    idempotency::{self, Actor, Answer, Key},
    jobs::repo::Claimed,
    modules::{
        gateway::{
            accounts,
            repo::{self, Operation},
        },
        invoice_payments, invoices, orders,
    },
    money,
    payments::{
        Error,
        globadyme::{Chargeback, NewRefund, NewRequest, Transaction},
    },
};
use axum::http::HeaderMap;
use chrono::Utc;
use rust_decimal::Decimal;
use serde_json::{Value, json};
use sqlx::{Connection, PgConnection};
use uuid::Uuid;

const REFUND_REVIEW: &str = "External refund or chargeback observation requires reconciliation.";

fn expected_request_id(op: &Operation) -> String {
    op.provider_id
        .clone()
        .unwrap_or_else(|| op.request_id.to_string())
}

/// Safe browser/operator projection; credentials and invoice tokens never leave engine.
pub fn view(state: &AppState, op: &Operation) -> Value {
    let mismatch = op.provider == "globadyme"
        && !accounts::environment_allowed(state, op.environment.as_deref());
    json!({"payment_id":op.id,"kind":op.kind,"status":op.status,"amount":op.amount,"amount_minor":op.amount.to_string(),
        "currency":op.currency,"decimals":op.decimals,"checkout_url":if can_checkout(state, op) { op.checkout_url.as_deref() } else { None },
        "environment":op.environment,"merchant_id":op.merchant_id,"order_id":op.order_id,
        "invoice_id":op.invoice_id,"credit_note_id":op.credit_note_id,
        "captured_transaction_id":op.captured_transaction_id,"attention":op.attention || mismatch,
        "attention_reason":if mismatch { Some(accounts::REALM_MISMATCH) } else { op.attention_reason.as_deref() }})
}

fn can_checkout(state: &AppState, op: &Operation) -> bool {
    (op.provider != "globadyme" || accounts::environment_allowed(state, op.environment.as_deref()))
        && op.kind == "payment"
        && op.status == "open"
        && !op.attention
}

fn down(_: Error) -> AppError {
    AppError::Unavailable("Payment provider is unavailable; existing submissions remain reserved.")
}

/// Claim commits before financial POST. Only winner sends, including after process crashes.
pub async fn submit(
    state: &AppState,
    ws: Uuid,
    id: Uuid,
    redirect_url: &str,
) -> Result<(), AppError> {
    let op = repo::find(&state.db, ws, id)
        .await?
        .ok_or(AppError::NotFound)?;
    if op.provider != "globadyme" {
        return Ok(());
    }
    accounts::require_environment(state, op.environment.as_deref())?;
    if op.status != "creating" || op.claimed_at.is_some() {
        return Ok(());
    }
    let account = accounts::bound(
        state,
        ws,
        op.environment.as_deref().unwrap_or(""),
        op.merchant_id.as_deref(),
    )
    .await
    .map_err(|error| {
        if op.kind == "payment" {
            accounts::setup_error(error)
        } else {
            error
        }
    })?;
    let mut dispatch = state.db.acquire().await?;
    // Session fence closes on cancellation; never return advisory-locked session to pool.
    dispatch.close_on_drop();
    let fence = format!("globadyme:{ws}:{id}");
    let active: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock(hashtextextended($1,0))")
        .bind(&fence)
        .fetch_one(&mut *dispatch)
        .await?;
    if !active {
        return Ok(());
    }
    let result = dispatch_fenced(state, &mut dispatch, ws, id, op, account, redirect_url).await;
    // Close-on-drop frees fence only after backend exit; next caller would see stale "busy".
    if let Err(error) = sqlx::query("SELECT pg_advisory_unlock(hashtextextended($1,0))")
        .bind(&fence)
        .execute(&mut *dispatch)
        .await
    {
        tracing::warn!(%error, payment = %id, "dispatch fence unlock failed; connection close releases it");
    }
    result
}

/// Fenced part of `submit`: claim, provider call, result write. Caller holds and releases fence.
async fn dispatch_fenced(
    state: &AppState,
    dispatch: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
    op: Operation,
    account: accounts::Account,
    redirect_url: &str,
) -> Result<(), AppError> {
    let mut tx = dispatch.begin().await?;
    lock_owner(&mut tx, ws, &op).await?;
    let op = repo::find(&mut *tx, ws, id)
        .await?
        .ok_or(AppError::NotFound)?;
    accounts::require_environment(state, op.environment.as_deref())?;
    if op.status != "creating" || op.claimed_at.is_some() {
        return Ok(());
    }
    if op.kind == "payment" {
        accounts::bind(&mut tx, ws, &account)
            .await
            .map_err(accounts::setup_error)?;
    }
    let refund_transaction = if op.kind == "refund" {
        let parent = repo::find(&mut *tx, ws, op.parent_id.ok_or(AppError::NotFound)?)
            .await?
            .ok_or(AppError::NotFound)?;
        accounts::require_environment(state, parent.environment.as_deref())?;
        if parent.attention || parent.captured_transaction_id.is_none() {
            return Err(AppError::Conflict("Refund source requires reconciliation."));
        }
        parent.captured_transaction_id
    } else {
        None
    };
    if op.kind == "refund" {
        accounts::bind_history(&mut tx, ws, &account).await?;
    }
    if !repo::claim(&mut *tx, ws, id).await? {
        return Ok(());
    }
    tx.commit().await?;
    if op.kind == "refund" {
        let transaction = refund_transaction
            .as_deref()
            .ok_or(AppError::Conflict("Refund source is unavailable."))?;
        match account
            .client
            .refund(&NewRefund {
                transaction_id: transaction,
                merchant_id: &account.merchant_id,
                currency: &op.currency,
                amount: op.amount,
                decimals: op.decimals as u32,
                description: &format!("Refund {}", op.id),
            })
            .await
        {
            Ok(result) => {
                let mut tx = dispatch.begin().await?;
                lock_owner(&mut tx, ws, &op).await?;
                sqlx::query("UPDATE order_payments SET submission_result=$3 WHERE workspace_id=$1 AND id=$2")
                    .bind(ws).bind(id).bind(&result.raw).execute(&mut *tx).await?;
                confirm_refund_in(state, &mut tx, ws, id, false).await?;
                tx.commit().await?;
            }
            Err(e) => record_failure(state, dispatch, ws, id, e).await?,
        }
    } else {
        let request = op.request_id.to_string();
        let description = format!("Genesy payment {}", op.id);
        let result = account
            .client
            .request(&NewRequest {
                request_id: &request,
                merchant_id: &account.merchant_id,
                customer_id: &op.customer_id,
                amount: op.amount,
                decimals: op.decimals as u32,
                currency: &op.currency,
                country: &op.country,
                description: &description,
                account_holder_name: &op.payer_name,
                email_address: op.payer_email.as_deref().unwrap_or(""),
                redirect_url,
                link_validity_days: 1,
            })
            .await;
        match result {
            Ok(result) => {
                let saved = sqlx::query("UPDATE order_payments SET status='open',provider_id=$3,checkout_url=$4,link_id=$5,last_error=NULL,updated_at=now() WHERE workspace_id=$1 AND id=$2 AND status='unknown' AND environment=$6")
                    .bind(ws).bind(id).bind(result.request_id).bind(result.checkout_url).bind(result.link_id).bind(state.payments.globadyme_environment().map(|environment| environment.as_str())).execute(&mut *dispatch).await;
                if let Err(error) = saved {
                    if error.as_database_error().is_some_and(|error| {
                        error.constraint() == Some("order_payments_gateway_provider_key")
                    }) {
                        repo::attention(&mut *dispatch, ws, id, "Provider request identity is already bound to another operation; do not retry.").await?;
                    } else {
                        return Err(error.into());
                    }
                }
            }
            Err(e) => record_failure(state, dispatch, ws, id, e).await?,
        }
    }
    Ok(())
}

async fn record_failure(
    state: &AppState,
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
    error: Error,
) -> Result<(), AppError> {
    // Explicit rejection is affirmative nonacceptance; transport/body ambiguity stays unknown.
    let refused = matches!(error, Error::Refused(_));
    sqlx::query("UPDATE order_payments SET status=CASE WHEN $3 THEN 'failed' ELSE status END,last_error=$4,attention=NOT $3,attention_reason=CASE WHEN $3 THEN NULL ELSE 'Submission outcome is unknown; do not retry.' END,updated_at=now() WHERE workspace_id=$1 AND id=$2 AND status='unknown' AND environment=$5")
        .bind(ws).bind(id).bind(refused).bind(if refused{"Provider rejected submission."}else{"Provider outcome is unknown."}).bind(state.payments.globadyme_environment().map(|environment| environment.as_str())).execute(conn).await?;
    Ok(())
}

/// Lock owner invoices in UUID order (or order), then source operation before child.
pub(crate) async fn lock_owner(
    conn: &mut PgConnection,
    ws: Uuid,
    op: &Operation,
) -> Result<(), AppError> {
    let linked_invoice = if let Some(order) = op.order_id {
        orders::repo::lock(conn, ws, order)
            .await?
            .ok_or(AppError::NotFound)?
            .invoice_id
    } else {
        None
    };
    let mut ids: Vec<Uuid> = [op.invoice_id, linked_invoice, op.credit_note_id]
        .into_iter()
        .flatten()
        .collect();
    ids.sort_unstable();
    ids.dedup();
    for id in ids {
        invoice_payments::repo::lock(conn, ws, id)
            .await?
            .ok_or(AppError::NotFound)?;
    }
    if let Some(parent) = op.parent_id {
        sqlx::query("SELECT id FROM order_payments WHERE workspace_id=$1 AND id=$2 FOR UPDATE")
            .bind(ws)
            .bind(parent)
            .execute(&mut *conn)
            .await?;
    }
    sqlx::query("SELECT id FROM order_payments WHERE workspace_id=$1 AND id=$2 FOR UPDATE")
        .bind(ws)
        .bind(op.id)
        .execute(conn)
        .await?;
    Ok(())
}

/// Confirm provider result under owner/source/refund locks, preserving unexpected real money.
pub async fn confirm_refund(state: &AppState, ws: Uuid, id: Uuid) -> Result<(), AppError> {
    let op = repo::find(&state.db, ws, id)
        .await?
        .ok_or(AppError::NotFound)?;
    accounts::require_environment(state, op.environment.as_deref())?;
    let mut tx = state.db.begin().await?;
    lock_owner(&mut tx, ws, &op).await?;
    confirm_refund_in(state, &mut tx, ws, id, false).await?;
    tx.commit().await?;
    Ok(())
}

/// Atomic audited confirmation; caller holds owner/source/refund locks in canonical order.
pub(crate) async fn confirm_refund_in(
    state: &AppState,
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
    manual: bool,
) -> Result<(), AppError> {
    let op = repo::find(&mut *conn, ws, id)
        .await?
        .ok_or(AppError::NotFound)?;
    accounts::require_environment(state, op.environment.as_deref())?;
    if let Some(parent) = op.parent_id {
        let parent = repo::find(&mut *conn, ws, parent)
            .await?
            .ok_or(AppError::NotFound)?;
        accounts::require_environment(state, parent.environment.as_deref())?;
    }
    if op.status == "refunded" && !manual {
        return Ok(());
    }
    if op.kind != "refund" || (manual && op.status != "unknown") {
        return Err(AppError::Conflict(
            "Refund no longer has an unknown submission.",
        ));
    }
    if op.credit_note_id.is_some() {
        invoices::gateway::apply_refund(
            state,
            conn,
            ws,
            &op,
            op.captured_transaction_id.as_deref().unwrap_or(""),
        )
        .await?;
    }
    if matches!(op.status.as_str(), "failed" | "canceled") {
        repo::attention(
            &mut *conn,
            ws,
            id,
            "Unexpected provider refund after nonacceptance resolution.",
        )
        .await?;
    }
    sqlx::query("UPDATE order_payments SET status='refunded',provider_id=$3,last_error=NULL,attention=CASE WHEN attention_reason='Submission outcome is unknown; do not retry.' THEN false ELSE attention END,attention_reason=CASE WHEN attention_reason='Submission outcome is unknown; do not retry.' THEN NULL ELSE attention_reason END,updated_at=now() WHERE workspace_id=$1 AND id=$2")
        .bind(ws).bind(id).bind(id.to_string()).execute(conn).await?;
    Ok(())
}

/// Authoritative GET only. Empty/failed attempts never prove request nonacceptance.
pub async fn refresh(state: &AppState, ws: Uuid, id: Uuid) -> Result<(), AppError> {
    let op = repo::find(&state.db, ws, id)
        .await?
        .ok_or(AppError::NotFound)?;
    if op.provider != "globadyme" {
        return Ok(());
    }
    accounts::require_environment(state, op.environment.as_deref())?;
    let source = if op.kind == "refund" {
        let parent = repo::find(&state.db, ws, op.parent_id.ok_or(AppError::NotFound)?)
            .await?
            .ok_or(AppError::NotFound)?;
        accounts::require_environment(state, parent.environment.as_deref())?;
        if parent.kind != "payment"
            || parent.provider != op.provider
            || parent.environment != op.environment
            || parent.merchant_id != op.merchant_id
            || parent.account_revision != op.account_revision
            || parent.currency != op.currency
            || parent.decimals != op.decimals
            || parent.customer_id != op.customer_id
            || parent.country != op.country
            || parent.order_id != op.order_id
            || parent.invoice_id != op.invoice_id
            || parent.captured_transaction_id != op.captured_transaction_id
        {
            return Err(AppError::Conflict("Refund source binding has changed."));
        }
        parent
    } else {
        op
    };
    refresh_collection(state, &source).await?;
    Ok(())
}

async fn refresh_collection(
    state: &AppState,
    op: &Operation,
) -> Result<Vec<Transaction>, AppError> {
    let ws = op.workspace_id;
    let id = op.id;
    let account = accounts::bound(
        state,
        ws,
        op.environment.as_deref().unwrap_or(""),
        op.merchant_id.as_deref(),
    )
    .await?;
    let request = expected_request_id(op);
    let transactions = account.client.fetch(&request).await.map_err(down)?;
    let mut tx = state.db.begin().await?;
    lock_owner(&mut tx, ws, op).await?;
    let current = repo::find(&mut *tx, ws, id)
        .await?
        .ok_or(AppError::NotFound)?;
    accounts::require_environment(state, current.environment.as_deref())?;
    if expected_request_id(&current) != request {
        return Err(AppError::Conflict(
            "Provider request identity changed; refresh before reconciling.",
        ));
    }
    let mut conflict = false;
    let mut external = false;
    let mut chargeback_observed = false;
    let confirmed_refunds:i64=sqlx::query_scalar("SELECT coalesce(sum(amount),0)::bigint FROM order_payments WHERE workspace_id=$1 AND parent_id=$2 AND kind='refund' AND status='refunded'")
        .bind(ws).bind(id).fetch_one(&mut *tx).await?;
    for t in &transactions {
        let valid = t.request_id == request
            && t.customer_id.as_deref() == Some(op.customer_id.as_str())
            && t.currency == op.currency
            && t.country == op.country
            && money::minor(t.amount, op.decimals as u32).ok() == Some(op.amount);
        conflict |= !valid;
        chargeback_observed |= t.status == "CHARGEBACKED";
        external |= match t.status.as_str() {
            "CHARGEBACKED" => false,
            "REFUNDED" => confirmed_refunds != op.amount,
            // Aggregate status carries no refunded amount; every later observation needs review.
            "PARTIALLY_REFUNDED" => true,
            _ => false,
        };
        conflict |= !observe(&mut tx, op, t).await?;
    }
    let successful:Vec<String>=sqlx::query_scalar("SELECT transaction_id FROM payment_transactions WHERE workspace_id=$1 AND payment_id=$2 AND (status='PROCESSED' OR transaction_id=$3) AND request_id=$4 AND customer_id=$5 AND amount=$6 AND currency=$7 ORDER BY transaction_id")
        .bind(ws).bind(id).bind(&current.captured_transaction_id).bind(&request).bind(&op.customer_id).bind(Decimal::new(op.amount,op.decimals as u32)).bind(&op.currency).fetch_all(&mut *tx).await?;
    if conflict || successful.len() > 1 {
        repo::attention(
            &mut *tx,
            ws,
            id,
            if conflict {
                "Provider identity or money conflicts with bound request."
            } else {
                "Multiple successful charges require reconciliation."
            },
        )
        .await?;
    } else if successful.len() == 1 && current.captured_transaction_id.is_none() {
        // Unknown-submission attention alone can clear when one exact authoritative capture exists.
        let unsafe_attention = current.attention
            && current.attention_reason.as_deref()
                != Some("Submission outcome is unknown; do not retry.");
        if !unsafe_attention {
            sqlx::query("UPDATE order_payments SET status='paid',captured_transaction_id=$3,attention=false,attention_reason=NULL,last_error=NULL,updated_at=now() WHERE workspace_id=$1 AND id=$2")
                .bind(ws).bind(id).bind(&successful[0]).execute(&mut *tx).await?;
            let accepted = repo::find(&mut *tx, ws, id)
                .await?
                .ok_or(AppError::NotFound)?;
            if accepted.invoice_id.is_some() {
                invoices::gateway::apply_collection(state, &mut tx, ws, &accepted, &successful[0])
                    .await?;
            } else {
                orders::payments::apply_gateway(state, &mut tx, ws, &accepted).await?;
            }
        }
    }
    let refund_dispatch_active: bool = if external {
        // Never wait for refund fence while holding owner/source locks; dispatch needs them to commit.
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM order_payments WHERE workspace_id=$1 AND parent_id=$2 AND kind='refund' AND status IN ('creating','open','unknown') AND NOT pg_try_advisory_xact_lock(hashtextextended('globadyme:'||workspace_id::text||':'||id::text,0)))")
            .bind(ws).bind(id).fetch_one(&mut *tx).await?
    } else {
        false
    };
    if (chargeback_observed || (external && !refund_dispatch_active))
        && !conflict
        && successful.len() <= 1
        && (!current.attention
            || matches!(
                current.attention_reason.as_deref(),
                Some(REFUND_REVIEW) | Some("Submission outcome is unknown; do not retry.")
            ))
    {
        repo::attention(&mut *tx, ws, id, REFUND_REVIEW).await?;
    }
    sqlx::query("UPDATE order_payments SET last_checked_at=now() WHERE workspace_id=$1 AND id=$2")
        .bind(ws)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(transactions)
}

fn source_refund_review(op: &Operation) -> bool {
    op.provider == "globadyme"
        && op.kind == "payment"
        && op.status == "paid"
        && op.captured_transaction_id.is_some()
        && op.attention
        && op.attention_reason.as_deref() == Some(REFUND_REVIEW)
}

/// Clear only reviewed refund warning; provider totals remain explicit operator evidence.
pub(crate) async fn reconcile_refund_source(
    state: &AppState,
    ws: Uuid,
    user: Uuid,
    id: Uuid,
    reason: &str,
    evidence: &str,
) -> Result<(), AppError> {
    let op = repo::find(&state.db, ws, id)
        .await?
        .ok_or(AppError::NotFound)?;
    accounts::require_environment(state, op.environment.as_deref())?;
    if !source_refund_review(&op) {
        return Err(AppError::Conflict(
            "Only a paid source with a refund observation can be reconciled.",
        ));
    }
    let transactions = refresh_collection(state, &op).await?;
    refund_chargebacks_clear(state, &op).await?;
    let mut tx = state.db.begin().await?;
    lock_owner(&mut tx, ws, &op).await?;
    let current = repo::find(&mut *tx, ws, id)
        .await?
        .ok_or(AppError::NotFound)?;
    accounts::require_environment(state, current.environment.as_deref())?;
    if !source_refund_review(&current) {
        return Err(AppError::Conflict(
            "Payment evidence changed; review before resolving.",
        ));
    }
    let captured = current
        .captured_transaction_id
        .as_deref()
        .ok_or(AppError::NotFound)?;
    let source = transactions
        .iter()
        .find(|t| t.id == captured)
        .ok_or(AppError::Conflict(
            "Fresh provider evidence for captured payment is required.",
        ))?;
    let request = expected_request_id(&current);
    if transactions.iter().any(|t| {
        t.request_id != request
            || t.customer_id.as_deref() != Some(current.customer_id.as_str())
            || t.currency != current.currency
            || t.country != current.country
            || money::minor(t.amount, current.decimals as u32).ok() != Some(current.amount)
            || (t.id != captured
                && matches!(
                    t.status.as_str(),
                    "PROCESSED" | "PARTIALLY_REFUNDED" | "REFUNDED" | "CHARGEBACKED"
                ))
    }) {
        return Err(AppError::Conflict(
            "Provider evidence conflicts with captured payment.",
        ));
    }
    let matches: bool = sqlx::query_scalar("SELECT (CASE $3::text WHEN 'REFUNDED' THEN total=$4 WHEN 'PARTIALLY_REFUNDED' THEN total>0 AND total<$4 ELSE false END) AND NOT unresolved FROM (SELECT coalesce(sum(r.amount) FILTER (WHERE r.status='refunded'),0)::bigint AS total,coalesce(bool_or(r.status NOT IN ('refunded','failed','canceled') OR r.attention OR r.provider<>p.provider OR r.environment IS DISTINCT FROM p.environment OR r.merchant_id IS DISTINCT FROM p.merchant_id OR r.account_revision IS DISTINCT FROM p.account_revision OR r.currency<>p.currency OR r.decimals<>p.decimals OR r.customer_id<>p.customer_id OR r.country<>p.country OR r.order_id IS DISTINCT FROM p.order_id OR r.invoice_id IS DISTINCT FROM p.invoice_id OR r.captured_transaction_id IS DISTINCT FROM p.captured_transaction_id),false) AS unresolved FROM order_payments p LEFT JOIN order_payments r ON r.workspace_id=p.workspace_id AND r.parent_id=p.id AND r.kind='refund' WHERE p.workspace_id=$1 AND p.id=$2) refunds")
        .bind(ws).bind(id).bind(&source.status).bind(current.amount).fetch_one(&mut *tx).await?;
    let unsafe_history: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM payment_chargebacks WHERE workspace_id=$1 AND payment_id=$2) OR EXISTS(SELECT 1 FROM payment_observations WHERE workspace_id=$1 AND payment_id=$2 AND (evidence->>'status'='CHARGEBACKED' OR (transaction_id<>$3 AND evidence->>'status' IN ('PROCESSED','PARTIALLY_REFUNDED','REFUNDED')) OR evidence->>'requestId' IS DISTINCT FROM $4 OR evidence->>'customerId' IS DISTINCT FROM $5 OR evidence->>'currency' IS DISTINCT FROM $6 OR evidence->>'country' IS DISTINCT FROM $7 OR (evidence->>'amount')::numeric<>$8))")
        .bind(ws).bind(id).bind(captured).bind(&request).bind(&current.customer_id).bind(&current.currency).bind(&current.country).bind(Decimal::new(current.amount,current.decimals as u32)).fetch_one(&mut *tx).await?;
    if !matches || unsafe_history {
        return Err(AppError::Conflict(
            "Refund evidence, reservations or chargebacks still require reconciliation.",
        ));
    }
    sqlx::query("INSERT INTO payment_audit(workspace_id,payment_id,environment,user_id,action,reason,evidence) VALUES($1,$2,$3,$4,'refund_reconciled',$5,$6)")
        .bind(ws).bind(id).bind(&current.environment).bind(user).bind(reason).bind(evidence).execute(&mut *tx).await?;
    sqlx::query("UPDATE order_payments SET attention=false,attention_reason=NULL,updated_at=now() WHERE workspace_id=$1 AND id=$2")
        .bind(ws).bind(id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}

async fn observe(
    conn: &mut PgConnection,
    op: &Operation,
    t: &Transaction,
) -> Result<bool, AppError> {
    let existing:Option<Value>=sqlx::query_scalar("SELECT evidence FROM payment_transactions WHERE workspace_id=$1 AND environment=$2 AND transaction_id=$3")
        .bind(op.workspace_id).bind(&op.environment).bind(&t.id).fetch_optional(&mut *conn).await?;
    if existing.as_ref() != Some(&t.raw) {
        sqlx::query("INSERT INTO payment_observations(workspace_id,payment_id,environment,transaction_id,evidence) VALUES($1,$2,$3,$4,$5)")
            .bind(op.workspace_id).bind(op.id).bind(&op.environment).bind(&t.id).bind(&t.raw).execute(&mut *conn).await?;
    }
    let result=sqlx::query("INSERT INTO payment_transactions(workspace_id,environment,transaction_id,payment_id,request_id,customer_id,amount,currency,status,settled,settlement_id,settlement_date,evidence) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13) ON CONFLICT(workspace_id,environment,transaction_id) DO UPDATE SET status=excluded.status,settled=payment_transactions.settled OR excluded.settled,settlement_id=coalesce(payment_transactions.settlement_id,excluded.settlement_id),settlement_date=coalesce(payment_transactions.settlement_date,excluded.settlement_date),evidence=excluded.evidence,updated_at=now() WHERE payment_transactions.payment_id=excluded.payment_id AND payment_transactions.request_id=excluded.request_id AND payment_transactions.customer_id IS NOT DISTINCT FROM excluded.customer_id AND payment_transactions.amount=excluded.amount AND payment_transactions.currency=excluded.currency")
        .bind(op.workspace_id).bind(&op.environment).bind(&t.id).bind(op.id).bind(&t.request_id).bind(&t.customer_id)
        .bind(t.amount).bind(&t.currency).bind(&t.status).bind(t.settled).bind(&t.settlement_id).bind(&t.settlement_date).bind(&t.raw).execute(&mut *conn).await?;
    Ok(result.rows_affected() == 1)
}

/// PNG fetched only from fixed provider endpoint with bound server-side credentials.
pub async fn qr(state: &AppState, ws: Uuid, id: Uuid) -> Result<Vec<u8>, AppError> {
    let op = repo::find(&state.db, ws, id)
        .await?
        .ok_or(AppError::NotFound)?;
    if !can_checkout(state, &op) {
        return Err(AppError::NotFound);
    }
    let account = accounts::bound(
        state,
        ws,
        op.environment.as_deref().unwrap_or(""),
        op.merchant_id.as_deref(),
    )
    .await?;
    account
        .client
        .qr(op.link_id.as_deref().ok_or(AppError::NotFound)?)
        .await
        .map_err(down)
}

/// Hosted shop collection on same bound engine as invoice payments.
pub async fn order_intent(
    state: &AppState,
    key: &AppKey,
    id: &str,
    input: orders::dto::PaymentInput,
) -> Result<orders::dto::PaymentIntentResource, AppError> {
    let ws = key.workspace_id;
    let id = Uuid::parse_str(id).map_err(|_| AppError::NotFound)?;
    let selection = accounts::select(state, ws).await;
    let mut tx = state.db.begin().await?;
    let order = orders::repo::lock(&mut tx, ws, id)
        .await?
        .filter(|o| o.source == "storefront")
        .ok_or(AppError::NotFound)?;
    if order.status != "awaiting_payment"
        || order.expires_at.is_none_or(|e| e <= Utc::now())
        || order.total <= 0
    {
        return Err(AppError::Conflict("This order is not payable."));
    }
    let live:Option<Uuid>=sqlx::query_scalar("SELECT id FROM order_payments WHERE workspace_id=$1 AND order_id=$2 AND kind='payment' AND status IN ('creating','open','unknown')")
        .bind(ws).bind(id).fetch_optional(&mut *tx).await?;
    let payment = if let Some(live) = live {
        let existing = repo::find(&mut *tx, ws, live)
            .await?
            .ok_or(AppError::NotFound)?;
        if existing.provider != "globadyme" {
            return Err(AppError::Conflict(
                "Payment configuration changed; resume existing checkout.",
            ));
        }
        accounts::require_environment(state, existing.environment.as_deref())?;
        live
    } else {
        let account = selection?.ok_or(AppError::Unavailable("No payment provider configured."))?;
        accounts::bind(&mut tx, ws, &account).await?;
        let customer = order.customer.as_ref().unwrap_or(&Value::Null);
        let country = customer["shipping_address"]["country"]
            .as_str()
            .or_else(|| customer["billing_address"]["country"].as_str())
            .unwrap_or("")
            .to_ascii_uppercase();
        let payment = repo::insert_collection(
            &mut tx,
            ws,
            Some(id),
            None,
            order.total,
            &order.currency,
            i32::from(order.decimals),
            &account,
            &country,
            customer["name"].as_str().unwrap_or(""),
            customer["email"].as_str(),
        )
        .await?;
        sqlx::query("UPDATE order_payments SET redirect_url=$3 WHERE workspace_id=$1 AND id=$2")
            .bind(ws)
            .bind(payment)
            .bind(&input.redirect_url)
            .execute(&mut *tx)
            .await?;
        payment
    };
    tx.commit().await?;
    submit(state, ws, payment, &input.redirect_url).await?;
    let op = repo::find(&state.db, ws, payment)
        .await?
        .ok_or(AppError::NotFound)?;
    accounts::require_environment(state, op.environment.as_deref())?;
    let checkout_url = if can_checkout(state, &op) {
        accounts::bound(
            state,
            ws,
            op.environment.as_deref().unwrap_or(""),
            op.merchant_id.as_deref(),
        )
        .await
        .map_err(accounts::setup_error)?;
        op.checkout_url
    } else {
        None
    };
    Ok(orders::dto::PaymentIntentResource {
        payment_id: op.id,
        status: op.status,
        checkout_url,
    })
}

/// Order refund branch; legacy providers retain original admission and retry behavior.
pub async fn order_refund(
    state: &AppState,
    ws: Uuid,
    user: Uuid,
    id: &str,
    amount: Decimal,
    key: Option<&Key>,
) -> Result<Option<Answer>, AppError> {
    let order = Uuid::parse_str(id).map_err(|_| AppError::NotFound)?;
    let sources: Vec<Operation> = sqlx::query_as("SELECT * FROM order_payments WHERE workspace_id=$1 AND order_id=$2 AND provider='globadyme' AND status='paid'")
        .bind(ws).bind(order).fetch_all(&state.db).await?;
    if sources.is_empty() {
        return Ok(None);
    }
    for source in &sources {
        accounts::require_environment(state, source.environment.as_deref())?;
    }
    if crate::modules::assistant::preconditions::action_id().is_some() {
        return Err(AppError::Forbidden(
            "Gateway refunds require the explicit payment interface.",
        ));
    }
    let mut tx = state.db.begin().await?;
    let replay = idempotency::replay(&mut tx, ws, Actor::User(user), key).await?;
    let locked = orders::repo::lock(&mut tx, ws, order)
        .await?
        .ok_or(AppError::NotFound)?;
    let paid:Vec<Operation>=sqlx::query_as("SELECT * FROM order_payments WHERE workspace_id=$1 AND order_id=$2 AND kind='payment' AND status='paid' FOR UPDATE")
        .bind(ws).bind(order).fetch_all(&mut *tx).await?;
    for source in paid.iter().filter(|op| op.provider == "globadyme") {
        accounts::require_environment(state, source.environment.as_deref())?;
    }
    if let Some(answer) = replay {
        tx.commit().await?;
        return Ok(Some(answer));
    }
    if locked.invoice_id.is_some() {
        return Err(AppError::Conflict(
            "Refund an invoiced order through its linked credit note.",
        ));
    }
    if paid.len() != 1
        || paid[0].provider != "globadyme"
        || paid[0].attention
        || paid[0].captured_transaction_id.is_none()
    {
        return Err(AppError::Conflict(
            "Payment requires reconciliation before refunding.",
        ));
    }
    let source = &paid[0];
    let amount = money::minor(amount, source.decimals as u32)?;
    let reserved:i64=sqlx::query_scalar("SELECT coalesce(sum(amount),0)::bigint FROM order_payments WHERE workspace_id=$1 AND parent_id=$2 AND kind='refund' AND status NOT IN ('failed','canceled')")
        .bind(ws).bind(source.id).fetch_one(&mut *tx).await?;
    if amount <= 0 || amount > source.amount.checked_sub(reserved).unwrap_or(0) {
        return Err(AppError::field(
            "amount",
            "Amount exceeds remaining refundable payment.",
        ));
    }
    let refund = Uuid::now_v7();
    sqlx::query("INSERT INTO order_payments(id,workspace_id,order_id,kind,provider,parent_id,amount,currency,environment,merchant_id,account_revision,request_id,decimals,customer_id,country,payer_name,payer_email,captured_transaction_id,created_by) SELECT $1,workspace_id,order_id,'refund',provider,id,$4,currency,environment,merchant_id,account_revision,$1,decimals,customer_id,country,payer_name,payer_email,captured_transaction_id,$5 FROM order_payments WHERE workspace_id=$2 AND id=$3")
        .bind(refund).bind(ws).bind(source.id).bind(amount).bind(user).execute(&mut *tx).await?;
    let answer =
        Answer::created(&json!({"data":{"refund_id":refund,"amount":amount,"status":"creating"}}));
    idempotency::finish(&mut tx, ws, key, &answer).await?;
    tx.commit().await?;
    submit(state, ws, refund, "").await?;
    Ok(Some(answer))
}

/// Scoped callbacks verify raw signature before parsing and queue only existing local requests.
pub async fn callback(
    state: &AppState,
    ws: Uuid,
    env: &str,
    headers: &HeaderMap,
    body: &[u8],
) -> Result<(), AppError> {
    if !accounts::environment_allowed(state, Some(env)) {
        return Ok(());
    }
    let Some(signature) = headers.get("signature").and_then(|v| v.to_str().ok()) else {
        return Ok(());
    };
    let Some(current) = accounts::secret(state, ws, env, "callback_secret").await? else {
        return Ok(());
    };
    let previous = accounts::secret(state, ws, env, "previous_callback_secret").await?;
    let Some(request) = crate::payments::globadyme::verify_callback(
        body,
        signature,
        current.expose(),
        previous.as_ref().map(|s| s.expose()),
    ) else {
        return Ok(());
    };
    let payments:Vec<Uuid>=sqlx::query_scalar("SELECT id FROM order_payments WHERE workspace_id=$1 AND environment=$2 AND provider='globadyme' AND kind='payment' AND (provider_id=$3 OR (provider_id IS NULL AND request_id=$4)) LIMIT 2")
        .bind(ws).bind(env).bind(&request).bind(Uuid::parse_str(&request).ok()).fetch_all(&state.db).await?;
    let [payment] = payments.as_slice() else {
        return Ok(());
    };
    let mut tx = state.db.begin().await?;
    sqlx::query("INSERT INTO payment_webhook_inbox(provider,workspace_id,external_id,environment,payment_id) VALUES('globadyme',$1,$2,$3,$4) ON CONFLICT(provider,workspace_id,environment,external_id) WHERE status='pending' AND provider='globadyme' DO UPDATE SET received_at=now()")
        .bind(ws).bind(&request).bind(env).bind(payment).execute(&mut *tx).await?;
    let job = crate::jobs::repo::NewJob {
        workspace_id: Some(ws),
        concurrency_key: Some(format!("payment_inbox:{ws}")),
        ..crate::jobs::repo::NewJob::new(crate::jobs::kinds::PAYMENT_INBOX)
    };
    crate::jobs::repo::enqueue_coalesced(&mut tx, &job).await?;
    tx.commit().await?;
    Ok(())
}

/// Drain bounded callback chunk; transient provider failures remain pending for sweep.
pub async fn drain(state: &AppState, job: &Claimed) -> Result<crate::jobs::runner::Step, AppError> {
    let Some(ws) = job.workspace_id else {
        return Ok(crate::jobs::runner::Step::Done);
    };
    quarantine_inbox(state, Some(ws)).await?;
    let environment = state
        .payments
        .globadyme_environment()
        .map(|environment| environment.as_str());
    let rows:Vec<(Uuid,Uuid,chrono::DateTime<Utc>)>=sqlx::query_as("SELECT i.id,i.payment_id,i.received_at FROM payment_webhook_inbox i JOIN order_payments p ON p.workspace_id=i.workspace_id AND p.id=i.payment_id WHERE i.workspace_id=$1 AND i.provider='globadyme' AND i.status='pending' AND i.environment=$2 AND p.environment=$2 AND p.provider='globadyme' AND (i.attempts=0 OR i.received_at<now()-interval '1 minute') ORDER BY i.received_at,i.id LIMIT 50")
        .bind(ws).bind(environment).fetch_all(&state.db).await?;
    for (inbox, payment, received) in rows {
        if refresh(state, ws, payment).await.is_ok() {
            sqlx::query("UPDATE payment_webhook_inbox SET status='processed',processed_at=now() WHERE id=$1 AND workspace_id=$2 AND status='pending' AND received_at=$3")
            .bind(inbox).bind(ws).bind(received).execute(&state.db).await?;
        } else {
            sqlx::query("UPDATE payment_webhook_inbox SET attempts=least(attempts+1,32767),received_at=now() WHERE workspace_id=$1 AND id=$2 AND status='pending' AND received_at=$3")
                .bind(ws).bind(inbox).bind(received).execute(&state.db).await?;
        }
    }
    let more:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM payment_webhook_inbox i JOIN order_payments p ON p.workspace_id=i.workspace_id AND p.id=i.payment_id WHERE i.workspace_id=$1 AND i.provider='globadyme' AND i.status='pending' AND i.environment=$2 AND p.environment=$2 AND p.provider='globadyme' AND (i.attempts=0 OR i.received_at<now()-interval '1 minute'))")
        .bind(ws).bind(environment).fetch_one(&state.db).await?;
    Ok(if more {
        crate::jobs::runner::Step::More
    } else {
        crate::jobs::runner::Step::Done
    })
}

/// Imported callbacks remain auditable, but can never become proof of a financial receipt.
async fn quarantine_inbox(state: &AppState, workspace: Option<Uuid>) -> Result<(), AppError> {
    let environment = state
        .payments
        .globadyme_environment()
        .map(|environment| environment.as_str());
    sqlx::query("WITH stale AS (SELECT i.id FROM payment_webhook_inbox i LEFT JOIN order_payments p ON p.workspace_id=i.workspace_id AND p.id=i.payment_id WHERE i.provider='globadyme' AND i.status='pending' AND ($1::uuid IS NULL OR i.workspace_id=$1) AND ($2::text IS NULL OR i.environment IS DISTINCT FROM $2 OR p.environment IS DISTINCT FROM $2 OR p.provider IS DISTINCT FROM 'globadyme') ORDER BY i.received_at,i.id LIMIT 100 FOR UPDATE OF i SKIP LOCKED) UPDATE payment_webhook_inbox i SET status='rejected',reason=$3,processed_at=now() FROM stale WHERE i.id=stale.id")
        .bind(workspace).bind(environment).bind(accounts::REALM_MISMATCH).execute(&state.db).await?;
    Ok(())
}

/// Recover unsent claims, pending/unknown requests and paid observations without age cutoff.
pub async fn sweep(state: &AppState) -> Result<(), AppError> {
    quarantine_inbox(state, None).await?;
    let environment = state
        .payments
        .globadyme_environment()
        .map(|environment| environment.as_str());
    let due:Vec<Operation>=sqlx::query_as("SELECT * FROM order_payments WHERE provider='globadyme' AND environment=$1 AND (last_checked_at IS NULL OR last_checked_at<now()-interval '15 minutes') ORDER BY last_checked_at NULLS FIRST,id LIMIT 100")
        .bind(environment).fetch_all(&state.db).await?;
    for op in due {
        if op.status == "creating" && op.claimed_at.is_none() {
            let redirect = if let Some(invoice) = op.invoice_id.filter(|_| op.kind == "payment") {
                invoices::gateway::redirect(state, op.workspace_id, invoice)
                    .await
                    .ok()
            } else {
                sqlx::query_scalar::<_, Option<String>>(
                    "SELECT redirect_url FROM order_payments WHERE workspace_id=$1 AND id=$2",
                )
                .bind(op.workspace_id)
                .bind(op.id)
                .fetch_one(&state.db)
                .await?
            };
            if op.kind == "refund" || redirect.is_some() {
                let _ = submit(
                    state,
                    op.workspace_id,
                    op.id,
                    redirect.as_deref().unwrap_or(""),
                )
                .await;
            }
        }
        let _ = refresh(state, op.workspace_id, op.id).await;
        sqlx::query(
            "UPDATE order_payments SET last_checked_at=now() WHERE workspace_id=$1 AND id=$2 AND environment=$3",
        )
        .bind(op.workspace_id)
        .bind(op.id)
        .bind(environment)
        .execute(&state.db)
        .await?;
    }
    let pending:Vec<Uuid>=sqlx::query_scalar("SELECT DISTINCT i.workspace_id FROM payment_webhook_inbox i JOIN order_payments p ON p.workspace_id=i.workspace_id AND p.id=i.payment_id WHERE i.provider='globadyme' AND i.status='pending' AND i.environment=$1 AND p.environment=$1 AND p.provider='globadyme' ORDER BY i.workspace_id LIMIT 100")
        .bind(environment).fetch_all(&state.db).await?;
    for ws in pending {
        let job = crate::jobs::repo::NewJob {
            workspace_id: Some(ws),
            concurrency_key: Some(format!("payment_inbox:{ws}")),
            ..crate::jobs::repo::NewJob::new(crate::jobs::kinds::PAYMENT_INBOX)
        };
        let mut conn = state.db.acquire().await?;
        crate::jobs::repo::enqueue_coalesced(&mut conn, &job).await?;
    }
    chargebacks(state).await?;
    Ok(())
}

/// Fair account/page cursor prevents old accounts and later pages from starving.
pub async fn chargebacks(state: &AppState) -> Result<(), AppError> {
    let environment = state
        .payments
        .globadyme_environment()
        .map(|environment| environment.as_str());
    let accounts:Vec<(Uuid,String,String)>=sqlx::query_as("SELECT workspace_id,environment,merchant_id FROM payment_accounts WHERE used_at IS NOT NULL AND environment=$1 ORDER BY chargeback_checked_at NULLS FIRST,workspace_id,environment LIMIT 10")
        .bind(environment).fetch_all(&state.db).await?;
    for (ws, env, merchant) in accounts {
        scan_chargebacks(state, ws, &env, &merchant).await?;
    }
    Ok(())
}

/// Explicit refresh stays inside requested account's workspace/environment boundary.
pub async fn chargebacks_account(state: &AppState, op: &Operation) -> Result<(), AppError> {
    accounts::require_environment(state, op.environment.as_deref())?;
    if let (Some(env), Some(merchant)) = (&op.environment, &op.merchant_id) {
        scan_chargebacks(state, op.workspace_id, env, merchant).await?;
    }
    Ok(())
}

async fn refund_chargebacks_clear(state: &AppState, op: &Operation) -> Result<(), AppError> {
    let account = accounts::bound(
        state,
        op.workspace_id,
        op.environment.as_deref().unwrap_or(""),
        op.merchant_id.as_deref(),
    )
    .await?;
    let captured = op
        .captured_transaction_id
        .as_deref()
        .ok_or(AppError::NotFound)?;
    let mut pagination = None;
    let mut ids = std::collections::HashSet::new();
    for page in 0..10 {
        let found = account.client.chargebacks(page, 50).await.map_err(down)?;
        let metadata = (found.total_pages, found.total_elements, found.page_size);
        let count = found.content.len() as u64;
        let mut identity_conflict = false;
        let mut disputed = false;
        let mut duplicate = false;
        // Retain observed source evidence before rejecting incomplete or conflicting pages.
        for charge in found.content {
            duplicate |= !ids.insert(charge.chargeback_id.clone());
            if charge.merchant_id != account.merchant_id {
                identity_conflict = true;
                if charge.transaction_id == captured {
                    repo::attention(
                        &state.db,
                        op.workspace_id,
                        op.id,
                        "Provider identity or money conflicts with bound request.",
                    )
                    .await?;
                }
                continue;
            }
            if charge.transaction_id == captured {
                if charge.currency != op.currency {
                    repo::attention(
                        &state.db,
                        op.workspace_id,
                        op.id,
                        "Provider identity or money conflicts with bound request.",
                    )
                    .await?;
                    identity_conflict = true;
                    continue;
                }
                observe_chargeback(state, op.workspace_id, &account, charge).await?;
                disputed = true;
            }
        }
        if identity_conflict {
            return Err(AppError::Conflict(
                "Chargeback identity conflicts with bound account or payment.",
            ));
        }
        let pages = found.total_elements.div_ceil(50).max(1);
        let valid_pages =
            found.total_pages == pages || (found.total_elements == 0 && found.total_pages == 0);
        if found.current_page != u64::from(page)
            || found.page_size != 50
            || !valid_pages
            || found.total_pages > 10
            || found.last != (u64::from(page) + 1 == pages)
            || count
                != found
                    .total_elements
                    .saturating_sub(u64::from(page) * 50)
                    .min(50)
            || pagination.is_some_and(|previous| previous != metadata)
            || duplicate
        {
            return Err(AppError::Unavailable(
                "Complete consistent chargeback evidence is required before refund reconciliation.",
            ));
        }
        pagination = Some(metadata);
        if disputed {
            return Err(AppError::Conflict("Chargeback requires reconciliation."));
        }
        if found.last && ids.len() as u64 == found.total_elements {
            return Ok(());
        }
    }
    Err(AppError::Unavailable(
        "Complete chargeback evidence exceeds the reconciliation page limit.",
    ))
}

async fn observe_chargeback(
    state: &AppState,
    ws: Uuid,
    account: &accounts::Account,
    charge: Chargeback,
) -> Result<(), AppError> {
    let env = account.environment.as_str();
    let merchant = account.merchant_id.as_str();
    if charge.merchant_id != merchant {
        return Ok(());
    }
    let payment:Option<Uuid>=sqlx::query_scalar("SELECT t.payment_id FROM payment_transactions t JOIN order_payments p ON p.workspace_id=t.workspace_id AND p.id=t.payment_id WHERE t.workspace_id=$1 AND t.environment=$2 AND t.transaction_id=$3 AND t.currency=$4 AND p.environment=$2 AND p.provider='globadyme'")
        .bind(ws).bind(env).bind(&charge.transaction_id).bind(&charge.currency).fetch_optional(&state.db).await?;
    let Some(payment) = payment else {
        return Ok(());
    };
    let old:Option<Value>=sqlx::query_scalar("SELECT evidence FROM payment_chargebacks WHERE workspace_id=$1 AND environment=$2 AND chargeback_id=$3")
        .bind(ws).bind(env).bind(&charge.chargeback_id).fetch_optional(&state.db).await?;
    let mut evidence = charge.raw;
    if old
        .as_ref()
        .is_none_or(|old| old["status"] != evidence["status"])
    {
        if let Ok(details) = account.client.chargeback(&charge.chargeback_id).await {
            evidence["provider_details"] = details.raw;
        }
    } else if let Some(details) = old.as_ref().and_then(|v| v.get("provider_details")) {
        evidence["provider_details"] = details.clone();
    }
    sqlx::query("INSERT INTO payment_chargebacks(workspace_id,environment,chargeback_id,transaction_id,payment_id,status,amount,currency,evidence) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT(workspace_id,environment,chargeback_id) DO UPDATE SET status=excluded.status,evidence=excluded.evidence,updated_at=now() WHERE payment_chargebacks.transaction_id=excluded.transaction_id AND payment_chargebacks.payment_id=excluded.payment_id AND payment_chargebacks.amount=excluded.amount AND payment_chargebacks.currency=excluded.currency")
        .bind(ws).bind(env).bind(&charge.chargeback_id).bind(&charge.transaction_id).bind(payment).bind(&charge.status).bind(charge.amount).bind(&charge.currency).bind(evidence).execute(&state.db).await?;
    repo::attention(
        &state.db,
        ws,
        payment,
        "Chargeback requires reconciliation.",
    )
    .await?;
    Ok(())
}

async fn scan_chargebacks(
    state: &AppState,
    ws: Uuid,
    env: &str,
    merchant: &str,
) -> Result<(), AppError> {
    accounts::require_environment(state, Some(env))?;
    let page:Option<i32>=sqlx::query_scalar("UPDATE payment_accounts SET chargeback_checked_at=now() WHERE workspace_id=$1 AND environment=$2 AND merchant_id=$3 RETURNING chargeback_page")
        .bind(ws).bind(env).bind(merchant).fetch_optional(&state.db).await?;
    let Some(page) = page else {
        return Ok(());
    };
    let Ok(account) = accounts::bound(state, ws, env, Some(merchant)).await else {
        return Ok(());
    };
    let Ok(found) = account.client.chargebacks(page as u32, 50).await else {
        return Ok(());
    };
    for charge in found.content {
        observe_chargeback(state, ws, &account, charge).await?;
    }
    let next = if found.last {
        0
    } else {
        page.checked_add(1).ok_or(AppError::Conflict(
            "Provider chargeback page limit exceeded.",
        ))?
    };
    sqlx::query("UPDATE payment_accounts SET chargeback_page=$4 WHERE workspace_id=$1 AND environment=$2 AND merchant_id=$3 AND chargeback_page=$5")
        .bind(ws).bind(env).bind(merchant).bind(next).bind(page).execute(&state.db).await?;
    Ok(())
}
