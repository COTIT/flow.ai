//! Provider payments of orders after the intent: the webhook inbox (a webhook only names an
//! id; the status is re-fetched from the provider and checked against our row), the drain
//! that applies it under the order lock, the sweep that recovers creates a dead request left
//! behind (same idempotency key) and re-fetches stale ones, and refunds. Provider calls never
//! run inside a transaction.

use serde_json::json;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    idempotency::{self, Actor, Answer, Key},
    jobs::{
        kinds::{Ctx, PAYMENT_INBOX},
        repo::{self as jobs, Claimed, NewJob},
        runner::Step,
    },
    modules::orders::{checkout, dto::OrderAction, repo as orders, service},
    payments::{self, NewRefund, Provider, Status},
};

/// Tries per inbox row before it is given up.
const GIVE_UP: i16 = 10;
/// Inbox rows one drain run applies.
const DRAIN_CHUNK: usize = 50;
const NOTHING_PAID: &str = "Nothing has been paid on this order.";

/// What an order's payments allow: `net` paid (paid − refunded) gates cancel, `refundable`
/// (the first paid payment − its refunds not failed) gates a refund.
#[derive(Clone, Copy, Default)]
pub struct Paid {
    pub net: i64,
    pub refundable: i64,
}

/// [`Paid`] per order of `ids`, in one statement; an order without payments is absent.
pub async fn paid_totals(
    db: impl sqlx::PgExecutor<'_>,
    workspace_id: Uuid,
    ids: &[Uuid],
) -> Result<std::collections::HashMap<Uuid, Paid>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT p.order_id AS "order_id!",
                  (coalesce(sum(p.amount) FILTER (WHERE p.kind = 'payment' AND p.status = 'paid'), 0)
                   - coalesce(sum(p.amount) FILTER (WHERE p.kind = 'refund'
                                                     AND p.status = 'refunded'), 0))::int8
                    AS "net!",
                  coalesce((SELECT f.amount - (SELECT coalesce(sum(r.amount), 0)
                                               FROM order_payments r
                                               WHERE r.workspace_id = f.workspace_id
                                                 AND r.parent_id = f.id AND r.kind = 'refund'
                                                 AND r.status NOT IN ('failed', 'canceled'))
                            FROM order_payments f
                            WHERE f.workspace_id = $1 AND f.order_id = p.order_id
                              AND f.kind = 'payment' AND f.status = 'paid'
                            ORDER BY f.created_at LIMIT 1), 0)::int8 AS "refundable!"
           FROM order_payments p
           WHERE p.workspace_id = $1 AND p.order_id = ANY($2)
           GROUP BY p.order_id"#,
        workspace_id,
        ids,
    )
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| {
            let paid = Paid {
                net: r.net,
                refundable: r.refundable,
            };
            (r.order_id, paid)
        })
        .collect())
}

/// A payment or refund row.
struct PaymentRow {
    id: Uuid,
    order_id: Uuid,
    kind: String,
    parent_id: Option<Uuid>,
    amount: i64,
    currency: String,
}

/// Why an inbox row wasn't applied.
enum Skip {
    /// Never apply it (reason stored).
    Reject(&'static str),
    /// The provider is down: try later.
    Retry,
}

/// Queue a re-fetch of `external_id` (one pending row per payment) and the workspace's drain.
async fn queue(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    provider: &str,
    external_id: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO payment_webhook_inbox (provider, workspace_id, external_id)
         VALUES ($1, $2, $3)
         ON CONFLICT (provider, workspace_id, external_id) WHERE status = 'pending'
         DO UPDATE SET received_at = now()",
        provider,
        workspace_id,
        external_id,
    )
    .execute(&mut *conn)
    .await?;
    let job = NewJob {
        workspace_id: Some(workspace_id),
        concurrency_key: Some(format!("{PAYMENT_INBOX}:{workspace_id}")),
        ..NewJob::new(PAYMENT_INBOX)
    };
    jobs::enqueue_coalesced(conn, &job).await.map(|_| ())
}

/// `POST /payments/webhooks/{provider}/{workspace}`: queue a re-fetch when the body names a
/// payment or refund of that workspace at the configured provider; anything else is ignored,
/// so the answer never tells which workspaces or ids exist.
pub async fn receive(
    state: &AppState,
    provider: &str,
    workspace: &str,
    body: &[u8],
) -> Result<(), AppError> {
    let Some(p) = state.payments.as_ref().filter(|p| p.name() == provider) else {
        return Ok(());
    };
    let (Ok(ws), Some(id)) = (Uuid::parse_str(workspace), p.webhook_id(body)) else {
        return Ok(());
    };
    let mut tx = state.db.begin().await?;
    let known = sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM order_payments
                          WHERE workspace_id = $1 AND provider = $2 AND provider_id = $3) AS "e!""#,
        ws,
        provider,
        id,
    )
    .fetch_one(&mut *tx)
    .await?;
    if known {
        queue(&mut tx, ws, provider, &id).await?;
        tx.commit().await?;
    }
    Ok(())
}

/// Job `payment_inbox` (one per workspace): re-fetch and apply pending inbox rows, oldest
/// first. A row closes only if no new webhook for it arrived during the fetch.
pub async fn drain(ctx: &Ctx, job: &Claimed) -> Result<Step, AppError> {
    let (Some(p), Some(ws)) = (ctx.payments.as_ref(), job.workspace_id) else {
        return Ok(Step::Done);
    };
    for _ in 0..DRAIN_CHUNK {
        let mut tx = ctx.db.begin().await?;
        let row = sqlx::query!(
            "SELECT id, external_id, received_at, attempts FROM payment_webhook_inbox
             WHERE workspace_id = $1 AND provider = $2 AND status = 'pending'
             ORDER BY received_at, id LIMIT 1 FOR UPDATE SKIP LOCKED",
            ws,
            p.name(),
        )
        .fetch_optional(&mut *tx)
        .await?;
        let Some(row) = row else {
            return Ok(Step::Done);
        };
        sqlx::query!(
            "UPDATE payment_webhook_inbox SET attempts = attempts + 1 WHERE id = $1",
            row.id
        )
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        let (status, reason) = match settle(&ctx.db, p, ws, &row.external_id).await? {
            Ok(()) => ("processed", None),
            Err(Skip::Reject(reason)) => ("rejected", Some(reason)),
            Err(Skip::Retry) if row.attempts + 1 >= GIVE_UP => ("rejected", Some("gave up")),
            // The sweep re-drives pending rows once the provider is back.
            Err(Skip::Retry) => return Ok(Step::Done),
        };
        sqlx::query!(
            "UPDATE payment_webhook_inbox SET status = $2, reason = $3, processed_at = now()
             WHERE id = $1 AND status = 'pending' AND (received_at = $4 OR $2 = 'rejected')",
            row.id,
            status,
            reason,
            row.received_at,
        )
        .execute(&ctx.db)
        .await?;
    }
    Ok(Step::More)
}

async fn payment_row(
    db: &PgPool,
    workspace_id: Uuid,
    provider: &str,
    external_id: &str,
) -> Result<Option<PaymentRow>, sqlx::Error> {
    sqlx::query_as!(
        PaymentRow,
        "SELECT id, order_id, kind, parent_id, amount, currency FROM order_payments
         WHERE workspace_id = $1 AND provider = $2 AND provider_id = $3",
        workspace_id,
        provider,
        external_id,
    )
    .fetch_optional(db)
    .await
}

/// Re-fetch one payment or refund and apply what the provider says.
async fn settle(
    db: &PgPool,
    p: &Provider,
    workspace_id: Uuid,
    external_id: &str,
) -> Result<Result<(), Skip>, AppError> {
    let Some(row) = payment_row(db, workspace_id, p.name(), external_id).await? else {
        return Ok(Err(Skip::Reject("unknown")));
    };
    let fetched = if row.kind == "payment" {
        p.fetch(workspace_id, external_id).await
    } else {
        let parent: Option<String> = sqlx::query_scalar!(
            "SELECT provider_id FROM order_payments WHERE workspace_id = $1 AND id = $2",
            workspace_id,
            row.parent_id,
        )
        .fetch_one(db)
        .await?;
        let parent = parent.unwrap_or_default();
        p.fetch_refund(workspace_id, &parent, external_id).await
    };
    let intent = match fetched {
        Ok(i) => i,
        Err(payments::Error::Unavailable(_)) => return Ok(Err(Skip::Retry)),
        Err(_) => return Ok(Err(Skip::Reject("not found at the provider"))),
    };
    let ours = row.kind == "refund"
        || (intent.metadata["workspace_id"] == json!(workspace_id)
            && intent.metadata["order_id"] == json!(row.order_id)
            && intent.metadata["payment_id"] == json!(row.id));
    if !ours || intent.amount != row.amount || intent.currency != row.currency {
        return Ok(Err(Skip::Reject("mismatch")));
    }
    apply(db, workspace_id, &row, intent.status).await?;
    Ok(Ok(()))
}

/// Apply a re-fetched status under the order lock, then the payment row's. A paid or
/// refunded row never moves again.
async fn apply(
    db: &PgPool,
    workspace_id: Uuid,
    row: &PaymentRow,
    status: Status,
) -> Result<(), AppError> {
    let mut tx = db.begin().await?;
    let Some(o) = orders::lock(&mut tx, workspace_id, row.order_id).await? else {
        return Ok(());
    };
    let current: String = sqlx::query_scalar!(
        "SELECT status FROM order_payments WHERE id = $1 FOR UPDATE",
        row.id
    )
    .fetch_one(&mut *tx)
    .await?;
    if matches!(current.as_str(), "paid" | "refunded") || status == Status::Open {
        return Ok(());
    }
    let to = match (row.kind.as_str(), status) {
        ("payment", Status::Refunded) | ("refund", Status::Paid | Status::Expired) => return Ok(()),
        (_, s) => s.as_str(),
    };
    sqlx::query!(
        "UPDATE order_payments SET status = $2, updated_at = now() WHERE id = $1",
        row.id,
        to,
    )
    .execute(&mut *tx)
    .await?;
    if row.kind == "payment" && status == Status::Paid {
        paid(&mut tx, workspace_id, &o, row).await?;
    }
    tx.commit().await?;
    Ok(())
}

/// The order got its money: awaiting → open; an expired one reopens if its stock is still
/// there, else stays cancelled and the payment is refunded (once, by the sweep).
async fn paid(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    o: &orders::OrderRow,
    row: &PaymentRow,
) -> Result<(), AppError> {
    sqlx::query!(
        "UPDATE orders SET paid_at = coalesce(paid_at, now()), updated_at = now()
         WHERE workspace_id = $1 AND id = $2",
        workspace_id,
        o.id,
    )
    .execute(&mut *conn)
    .await?;
    let reopen = match o.status.as_str() {
        "awaiting_payment" => true,
        "cancelled" => {
            let lines = orders::lines(&mut *conn, workspace_id, &[o.id]).await?;
            match service::hold(conn, workspace_id, o, &lines).await {
                Ok(()) => true,
                Err(AppError::Validation(_)) => {
                    auto_refund(conn, workspace_id, o.id, row).await?;
                    false
                }
                Err(e) => return Err(e),
            }
        }
        _ => false,
    };
    if reopen {
        orders::set_status(conn, workspace_id, o.id, "open").await?;
    }
    Ok(())
}

/// A full refund of `row` the system owes (late payment, no stock); once per payment.
async fn auto_refund(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    order_id: Uuid,
    row: &PaymentRow,
) -> Result<(), sqlx::Error> {
    let provider: String =
        sqlx::query_scalar!("SELECT provider FROM order_payments WHERE id = $1", row.id)
            .fetch_one(&mut *conn)
            .await?;
    sqlx::query!(
        "INSERT INTO order_payments (workspace_id, order_id, kind, provider, parent_id, auto,
                                     amount, currency)
         VALUES ($1, $2, 'refund', $3, $4, true, $5, $6)
         ON CONFLICT (parent_id) WHERE kind = 'refund' AND auto DO NOTHING",
        workspace_id,
        order_id,
        provider,
        row.id,
        row.amount,
        row.currency,
    )
    .execute(conn)
    .await
    .map(|_| ())
}

/// A row waiting for its create call.
struct Creating {
    id: Uuid,
    workspace_id: Uuid,
    order_id: Uuid,
    kind: String,
    parent_id: Option<Uuid>,
    amount: i64,
    currency: String,
    redirect_url: Option<String>,
    webhook_url: Option<String>,
    expired: bool,
}

/// One refund create call; the row id is the idempotency key.
async fn refund_at_provider(
    db: &PgPool,
    p: &Provider,
    workspace_id: Uuid,
    refund_id: Uuid,
    parent_id: Option<Uuid>,
    amount: i64,
    currency: &str,
) -> Result<(), AppError> {
    let parent = sqlx::query!(
        r#"SELECT p.provider_id, c.decimals AS "decimals!"
           FROM order_payments p
           JOIN orders o ON o.workspace_id = p.workspace_id AND o.id = p.order_id
           JOIN currencies c ON c.workspace_id = o.workspace_id AND c.id = o.currency_id
           WHERE p.workspace_id = $1 AND p.id = $2"#,
        workspace_id,
        parent_id,
    )
    .fetch_one(db)
    .await?;
    let key = refund_id.to_string();
    let created = p
        .refund(&NewRefund {
            workspace_id,
            payment_id: parent.provider_id.as_deref().unwrap_or_default(),
            amount,
            currency,
            decimals: u32::try_from(parent.decimals).unwrap_or(2),
            idempotency_key: &key,
        })
        .await;
    let mut conn = db.acquire().await?;
    match created {
        Ok(i) => {
            let status = match i.status {
                Status::Refunded | Status::Failed | Status::Canceled => i.status.as_str(),
                _ => "open",
            };
            sqlx::query!(
                "UPDATE order_payments SET provider_id = $3, status = $4, attempts = attempts + 1,
                        last_error = NULL, updated_at = now()
                 WHERE workspace_id = $1 AND id = $2 AND status = 'creating'",
                workspace_id,
                refund_id,
                i.id,
                status,
            )
            .execute(&mut *conn)
            .await?;
        }
        Err(e) => {
            let refused = !matches!(e, payments::Error::Unavailable(_));
            let msg: String = e.to_string().chars().take(255).collect();
            sqlx::query!(
                "UPDATE order_payments SET attempts = attempts + 1, last_error = $3,
                        status = CASE WHEN $4 THEN 'failed' ELSE status END, updated_at = now()
                 WHERE workspace_id = $1 AND id = $2 AND status = 'creating'",
                workspace_id,
                refund_id,
                msg,
                refused,
            )
            .execute(&mut *conn)
            .await?;
        }
    }
    Ok(())
}

/// Job `payment_sweep` (every minute): retry creates a dead request left behind (system
/// refunds right away, others after a minute; given up after an hour), queue a re-fetch of
/// payments and refunds left open 15 minutes, and re-drive pending inboxes.
pub async fn sweep(ctx: &Ctx) -> Result<Step, AppError> {
    let Some(p) = ctx.payments.as_ref() else {
        return Ok(Step::Done);
    };
    let due = sqlx::query_as!(
        Creating,
        r#"SELECT id, workspace_id, order_id, kind, parent_id, amount, currency, redirect_url,
                  webhook_url, created_at < now() - interval '1 hour' AS "expired!"
           FROM order_payments
           WHERE provider = $1 AND status = 'creating'
             AND ((kind = 'refund' AND attempts = 0) OR updated_at < now() - interval '1 minute')
           ORDER BY updated_at LIMIT 100"#,
        p.name(),
    )
    .fetch_all(&ctx.db)
    .await?;
    for r in due {
        if r.expired {
            sqlx::query!(
                "UPDATE order_payments SET status = 'failed', last_error = 'gave up',
                        updated_at = now()
                 WHERE id = $1 AND status = 'creating'",
                r.id
            )
            .execute(&ctx.db)
            .await?;
            continue;
        }
        if r.kind == "refund" {
            refund_at_provider(
                &ctx.db,
                p,
                r.workspace_id,
                r.id,
                r.parent_id,
                r.amount,
                &r.currency,
            )
            .await?;
            continue;
        }
        let Some(o) = orders::find(&ctx.db, r.workspace_id, r.order_id).await? else {
            continue;
        };
        let created = checkout::create_at_provider(
            p,
            r.workspace_id,
            &o,
            r.id,
            r.redirect_url.as_deref().unwrap_or_default(),
            r.webhook_url.as_deref().unwrap_or_default(),
        )
        .await;
        let mut conn = ctx.db.acquire().await?;
        // Unavailable is recorded on the row; the next sweep tries again.
        let _ = checkout::record_create(&mut conn, r.workspace_id, r.id, created).await;
    }
    let stale = sqlx::query!(
        r#"UPDATE order_payments SET updated_at = now()
           WHERE id IN (SELECT id FROM order_payments
                        WHERE provider = $1 AND status = 'open' AND provider_id IS NOT NULL
                          AND updated_at < now() - interval '15 minutes'
                          AND created_at > now() - interval '2 days'
                        ORDER BY updated_at LIMIT 500)
           RETURNING workspace_id, provider_id AS "provider_id!""#,
        p.name(),
    )
    .fetch_all(&ctx.db)
    .await?;
    let mut tx = ctx.db.begin().await?;
    for s in stale {
        queue(&mut tx, s.workspace_id, p.name(), &s.provider_id).await?;
    }
    let pending: Vec<(Uuid, String, String)> = sqlx::query_as(
        "SELECT DISTINCT ON (workspace_id) workspace_id, provider, external_id
         FROM payment_webhook_inbox WHERE status = 'pending' AND provider = $1 LIMIT 100",
    )
    .bind(p.name())
    .fetch_all(&mut *tx)
    .await?;
    for (ws, provider, id) in pending {
        queue(&mut tx, ws, &provider, &id).await?;
    }
    tx.commit().await?;
    Ok(Step::Done)
}

/// Paid minus refunded of an order, in minor units, and the paid payment (provider, id).
pub async fn net_paid(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    order_id: Uuid,
) -> Result<(i64, Option<(String, Option<String>)>), sqlx::Error> {
    let r = sqlx::query!(
        r#"SELECT
             coalesce(sum(amount) FILTER (WHERE kind = 'payment' AND status = 'paid'), 0)::int8
               AS "paid!",
             coalesce(sum(amount) FILTER (WHERE kind = 'refund' AND status = 'refunded'), 0)::int8
               AS "refunded!",
             (array_agg(provider) FILTER (WHERE kind = 'payment' AND status = 'paid'))[1]
               AS provider,
             (array_agg(provider_id) FILTER (WHERE kind = 'payment' AND status = 'paid'))[1]
               AS provider_id
           FROM order_payments WHERE workspace_id = $1 AND order_id = $2"#,
        workspace_id,
        order_id,
    )
    .fetch_one(conn)
    .await?;
    Ok((r.paid - r.refunded, r.provider.map(|p| (p, r.provider_id))))
}

/// POST /orders/{order}/refunds: refund part of what was paid (`amount` minor units, at most
/// paid − refunds not failed). The refund row commits first; the provider call follows and a
/// failure is left to the sweep. 201 `{data:{refund_id, amount, status}}`.
pub async fn refund(
    state: &AppState,
    workspace_id: Uuid,
    user_id: Uuid,
    id: &str,
    amount: i64,
    key: Option<Key>,
) -> Result<Answer, AppError> {
    let id = Uuid::parse_str(id).map_err(|_| AppError::NotFound)?;
    let mut tx = state.db.begin().await?;
    if let Some(a) =
        idempotency::replay(&mut tx, workspace_id, Actor::User(user_id), key.as_ref()).await?
    {
        return Ok(a);
    }
    let o = orders::lock(&mut tx, workspace_id, id)
        .await?
        .ok_or(AppError::NotFound)?;
    let paid = paid_totals(&mut *tx, workspace_id, &[o.id]).await?;
    let paid = paid.get(&o.id).copied().unwrap_or_default();
    service::check(OrderAction::Refund, &o, &[], paid)?;
    let pay = sqlx::query!(
        r#"SELECT p.id, p.provider, p.amount, p.currency,
                  (SELECT coalesce(sum(r.amount), 0)::int8 FROM order_payments r
                   WHERE r.workspace_id = p.workspace_id AND r.parent_id = p.id
                     AND r.kind = 'refund' AND r.status NOT IN ('failed', 'canceled')) AS "out!"
           FROM order_payments p
           WHERE p.workspace_id = $1 AND p.order_id = $2 AND p.kind = 'payment'
             AND p.status = 'paid'
           ORDER BY p.created_at LIMIT 1"#,
        workspace_id,
        o.id,
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(AppError::Conflict(NOTHING_PAID))?;
    let left = pay.amount - pay.out;
    if amount > left {
        return Err(AppError::field(
            "amount",
            format!("At most {left} can be refunded."),
        ));
    }
    let refund_id: Uuid = sqlx::query_scalar!(
        "INSERT INTO order_payments (workspace_id, order_id, kind, provider, parent_id, amount,
                                     currency, created_by)
         VALUES ($1, $2, 'refund', $3, $4, $5, $6, $7) RETURNING id",
        workspace_id,
        o.id,
        pay.provider,
        pay.id,
        amount,
        pay.currency,
        user_id,
    )
    .fetch_one(&mut *tx)
    .await?;
    let answer = Answer::created(&json!({
        "data": { "refund_id": refund_id, "amount": amount, "status": "creating" }
    }));
    idempotency::finish(&mut tx, workspace_id, key.as_ref(), &answer).await?;
    tx.commit().await?;
    if let Some(p) = state.payments.as_ref().filter(|p| p.name() == pay.provider) {
        refund_at_provider(
            &state.db,
            p,
            workspace_id,
            refund_id,
            Some(pay.id),
            amount,
            &pay.currency,
        )
        .await?;
    }
    Ok(answer)
}
