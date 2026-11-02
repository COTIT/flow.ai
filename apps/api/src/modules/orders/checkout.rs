//! Web shop side of orders: checkout (hold stock, `awaiting_payment` for 30 minutes), the
//! shop's minimal view, the payment intent and the expiry job. Provider calls never run
//! inside a transaction or under a lock: the `creating` row is committed first and its id is
//! the provider's idempotency key, so a retry (here or by the sweep) gets the same payment.

use chrono::{Duration, Utc};
use serde_json::json;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::app_key::AppKey,
    idempotency::{self, Actor, Answer, Key},
    jobs::runner::Step,
    modules::orders::{
        dto::{CheckoutInput, PaymentInput, PaymentIntentResource, StorefrontOrder},
        repo::{self, OrderRow},
        service,
    },
    payments::{self, NewIntent},
};

/// How long an unpaid web order holds its stock.
pub const HOLD_MINUTES: i64 = 30;
/// Orders one expiry chunk cancels.
const EXPIRE_CHUNK: i64 = 100;
const NO_PROVIDER: &str = "no payment provider configured";
const PROVIDER_DOWN: &str = "payment provider unavailable, try again";
const NOT_PAYABLE: &str = "This order takes no payment.";

/// The shop's view of an order.
pub fn minimal(o: &OrderRow) -> StorefrontOrder {
    StorefrontOrder {
        id: o.id,
        number: o.number.clone(),
        status: o.status.clone(),
        currency: o.currency.clone(),
        decimals: o.decimals,
        subtotal: o.subtotal,
        vat: o.vat,
        total: o.total,
        expires_at: o.expires_at,
        paid_at: o.paid_at,
    }
}

async fn awaiting(db: impl sqlx::PgExecutor<'_>, workspace_id: Uuid) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM orders
           WHERE workspace_id = $1 AND status = 'awaiting_payment'"#,
        workspace_id,
    )
    .fetch_one(db)
    .await
}

/// New web order holding its stock; 201 `{data: minimal order}`.
pub async fn create(
    state: &AppState,
    key: &AppKey,
    input: CheckoutInput,
    idem: Option<Key>,
) -> Result<Answer, AppError> {
    let ws = key.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::replay(&mut tx, ws, Actor::AppKey(key.id), idem.as_ref()).await? {
        return Ok(a);
    }
    // shortcut: parallel checkouts may overshoot the cap by a few; it bounds abuse, not money.
    if awaiting(&mut *tx, ws).await? >= state.checkout_hold_max {
        return Err(AppError::TooManyRequests);
    }
    let (currency_id, dp) = service::currency(&mut tx, ws, None).await?;
    let warehouse_id = service::warehouse(&mut tx, ws, None).await?;
    let p = service::price(&mut tx, ws, currency_id, dp, &input.lines, true).await?;
    let number = service::number(&mut tx, ws).await?;
    let id = repo::insert(
        &mut tx,
        ws,
        &repo::NewOrder {
            number: &number,
            status: "awaiting_payment",
            source: "storefront",
            relation_id: None,
            currency_id,
            warehouse_id,
            customer: Some(&input.customer),
            reference: None,
            note: None,
            expires_at: Some(Utc::now() + Duration::minutes(HOLD_MINUTES)),
            created_by: None,
            app_key_id: Some(key.id),
        },
    )
    .await?;
    repo::write_lines(
        &mut tx,
        ws,
        id,
        &p.lines,
        (p.subtotal, p.vat, p.total),
        &p.summary,
    )
    .await?;
    let o = repo::lock(&mut tx, ws, id)
        .await?
        .ok_or(AppError::NotFound)?;
    let lines = repo::lines(&mut *tx, ws, &[id]).await?;
    service::hold(&mut tx, ws, &o, &lines).await?;
    service::created_event(&mut tx, ws, id).await?;
    let answer = Answer::created(&json!({ "data": minimal(&o) }));
    idempotency::finish(&mut tx, ws, idem.as_ref(), &answer).await?;
    tx.commit().await?;
    Ok(answer)
}

/// A web order of the key's workspace (office orders are not the shop's).
async fn web_order(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    id: &str,
    lock: bool,
) -> Result<OrderRow, AppError> {
    let id = Uuid::parse_str(id).map_err(|_| AppError::NotFound)?;
    let o = if lock {
        repo::lock(conn, workspace_id, id).await?
    } else {
        repo::find(conn, workspace_id, id).await?
    };
    o.filter(|o| o.source == "storefront")
        .ok_or(AppError::NotFound)
}

/// The shop's view of one of its orders.
pub async fn get(state: &AppState, key: &AppKey, id: &str) -> Result<StorefrontOrder, AppError> {
    let mut conn = state.db.acquire().await?;
    Ok(minimal(
        &web_order(&mut conn, key.workspace_id, id, false).await?,
    ))
}

/// A live payment row of an order.
struct LivePayment {
    id: Uuid,
    status: String,
    checkout_url: Option<String>,
}

async fn intent_of(conn: &mut PgConnection, id: Uuid) -> Result<PaymentIntentResource, AppError> {
    let row = sqlx::query!(
        "SELECT id, status, checkout_url FROM order_payments WHERE id = $1",
        id,
    )
    .fetch_one(conn)
    .await?;
    Ok(PaymentIntentResource {
        payment_id: row.id,
        status: row.status,
        checkout_url: row.checkout_url,
    })
}

/// The order's payment intent: its live one, or a new one through the provider. 503 without
/// a provider or while it is down (the `creating` row waits for a retry with the same key).
pub async fn intent(
    state: &AppState,
    key: &AppKey,
    id: &str,
    input: PaymentInput,
) -> Result<PaymentIntentResource, AppError> {
    let ws = key.workspace_id;
    let provider = state
        .payments
        .as_ref()
        .ok_or(AppError::Unavailable(NO_PROVIDER))?;
    let mut tx = state.db.begin().await?;
    let o = web_order(&mut tx, ws, id, true).await?;
    let payable = o.status == "awaiting_payment" && o.expires_at.is_some_and(|e| e > Utc::now());
    if !payable || o.total <= 0 {
        return Err(AppError::Conflict(NOT_PAYABLE));
    }
    let live = sqlx::query_as!(
        LivePayment,
        "SELECT id, status, checkout_url FROM order_payments
         WHERE workspace_id = $1 AND order_id = $2 AND kind = 'payment'
           AND status IN ('creating', 'open')
         FOR UPDATE",
        ws,
        o.id,
    )
    .fetch_optional(&mut *tx)
    .await?;
    let payment_id = match live {
        Some(p) if p.status == "open" && p.checkout_url.is_some() => {
            tx.commit().await?;
            return Ok(PaymentIntentResource {
                payment_id: p.id,
                status: p.status,
                checkout_url: p.checkout_url,
            });
        }
        Some(p) => p.id,
        None => {
            sqlx::query_scalar!(
                "INSERT INTO order_payments (workspace_id, order_id, kind, provider, amount,
                                             currency, redirect_url, webhook_url)
                 VALUES ($1, $2, 'payment', $3, $4, $5, $6, $7) RETURNING id",
                ws,
                o.id,
                provider.name(),
                o.total,
                o.currency,
                input.redirect_url,
                webhook_url(state, provider, ws),
            )
            .fetch_one(&mut *tx)
            .await?
        }
    };
    tx.commit().await?;
    let webhook = webhook_url(state, provider, ws);
    let created =
        create_at_provider(provider, ws, &o, payment_id, &input.redirect_url, &webhook).await;
    let mut conn = state.db.acquire().await?;
    record_create(&mut conn, ws, payment_id, created).await?;
    intent_of(&mut conn, payment_id).await
}

/// Where the provider posts status changes of this workspace's payments.
fn webhook_url(state: &AppState, provider: &payments::Provider, workspace_id: Uuid) -> String {
    format!(
        "{}/v1/payments/webhooks/{}/{workspace_id}",
        state.api_public_url,
        provider.name()
    )
}

/// One create call for a `creating` row; the row id is the idempotency key.
pub async fn create_at_provider(
    provider: &payments::Provider,
    workspace_id: Uuid,
    o: &OrderRow,
    payment_id: Uuid,
    redirect_url: &str,
    webhook: &str,
) -> Result<payments::Intent, payments::Error> {
    let key = payment_id.to_string();
    let description = format!("Order {}", o.number);
    provider
        .create(&NewIntent {
            workspace_id,
            amount: o.total,
            currency: &o.currency,
            decimals: u32::try_from(o.decimals).unwrap_or(2),
            description: &description,
            redirect_url,
            webhook_url: webhook,
            metadata: json!({ "workspace_id": workspace_id, "order_id": o.id, "payment_id": payment_id }),
            idempotency_key: &key,
            expires_at: o.expires_at,
        })
        .await
}

/// Store a create call's outcome on its `creating` row; Err when the shop must retry.
pub async fn record_create(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    payment_id: Uuid,
    created: Result<payments::Intent, payments::Error>,
) -> Result<(), AppError> {
    match created {
        Ok(i) => {
            sqlx::query!(
                "UPDATE order_payments SET provider_id = $3, checkout_url = $4, status = 'open',
                        attempts = attempts + 1, last_error = NULL, updated_at = now()
                 WHERE workspace_id = $1 AND id = $2 AND status = 'creating'",
                workspace_id,
                payment_id,
                i.id,
                i.checkout_url,
            )
            .execute(conn)
            .await?;
            Ok(())
        }
        Err(e) => {
            let refused = matches!(e, payments::Error::Refused(_) | payments::Error::NotFound);
            let msg: String = e.to_string().chars().take(255).collect();
            sqlx::query!(
                "UPDATE order_payments SET attempts = attempts + 1, last_error = $3,
                        status = CASE WHEN $4 THEN 'failed' ELSE status END, updated_at = now()
                 WHERE workspace_id = $1 AND id = $2 AND status = 'creating'",
                workspace_id,
                payment_id,
                msg,
                refused,
            )
            .execute(conn)
            .await?;
            Err(AppError::Unavailable(PROVIDER_DOWN))
        }
    }
}

/// Job `order_expire`: cancel up to a chunk of web orders past `expires_at`, giving their stock
/// back; More while a full chunk was found. A late payment is settled by the webhook drain.
pub async fn expire(db: &PgPool) -> Result<Step, AppError> {
    let due: Vec<(Uuid, Uuid)> = sqlx::query_as(
        "SELECT workspace_id, id FROM orders
         WHERE status = 'awaiting_payment' AND expires_at < now()
         ORDER BY expires_at LIMIT $1",
    )
    .bind(EXPIRE_CHUNK)
    .fetch_all(db)
    .await?;
    for &(ws, id) in &due {
        let mut tx = db.begin().await?;
        let Some(o) = repo::lock(&mut tx, ws, id).await? else {
            continue;
        };
        // Re-checked under the lock: paid or cancelled meanwhile.
        if o.status != "awaiting_payment" || o.expires_at.is_none_or(|e| e >= Utc::now()) {
            continue;
        }
        let lines = repo::lines(&mut *tx, ws, &[id]).await?;
        service::unhold(&mut tx, ws, &o, &lines).await?;
        repo::set_status(&mut tx, ws, id, "cancelled").await?;
        tx.commit().await?;
    }
    Ok(if i64::try_from(due.len()).unwrap_or(0) == EXPIRE_CHUNK {
        Step::More
    } else {
        Step::Done
    })
}
