//! Bound gateway persistence; financial submissions are claimed before HTTP dispatch.
use crate::{AppError, modules::gateway::accounts::Account};
use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgExecutor};
use uuid::Uuid;

/// Durable collection/refund operation and immutable account binding.
#[derive(Clone, sqlx::FromRow)]
pub struct Operation {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub order_id: Option<Uuid>,
    pub invoice_id: Option<Uuid>,
    pub credit_note_id: Option<Uuid>,
    pub parent_id: Option<Uuid>,
    pub kind: String,
    pub provider: String,
    pub environment: Option<String>,
    pub merchant_id: Option<String>,
    pub account_revision: Option<i64>,
    pub request_id: Uuid,
    pub status: String,
    pub amount: i64,
    pub currency: String,
    pub decimals: i32,
    pub customer_id: String,
    pub country: String,
    pub payer_name: String,
    pub payer_email: Option<String>,
    pub checkout_url: Option<String>,
    pub link_id: Option<String>,
    pub captured_transaction_id: Option<String>,
    pub attention: bool,
    pub attention_reason: Option<String>,
    pub claimed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Scoped operation lookup.
pub async fn find(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    id: Uuid,
) -> Result<Option<Operation>, sqlx::Error> {
    sqlx::query_as("SELECT * FROM order_payments WHERE workspace_id=$1 AND id=$2")
        .bind(ws)
        .bind(id)
        .fetch_optional(db)
        .await
}

/// Insert collection after caller locks its owner and binds account.
#[allow(clippy::too_many_arguments)]
pub async fn insert_collection(
    conn: &mut PgConnection,
    ws: Uuid,
    order: Option<Uuid>,
    invoice: Option<Uuid>,
    amount: i64,
    currency: &str,
    decimals: i32,
    account: &Account,
    country: &str,
    payer: &str,
    email: Option<&str>,
) -> Result<Uuid, AppError> {
    if decimals != 2 || amount <= 0 {
        return Err(AppError::field(
            "amount",
            "Globadyme requires a positive two-decimal currency amount.",
        ));
    }
    if crate::modules::relations::countries::find(country).is_none() {
        return Err(AppError::field("country", "Select a valid ISO country."));
    }
    if payer.trim().is_empty() || payer.chars().count() > 255 {
        return Err(AppError::field(
            "payer_name",
            "Enter the payer's full name.",
        ));
    }
    let id = Uuid::now_v7();
    sqlx::query("INSERT INTO order_payments(id,workspace_id,order_id,invoice_id,kind,provider,environment,merchant_id,account_revision,request_id,amount,currency,decimals,customer_id,country,payer_name,payer_email) VALUES($1,$2,$3,$4,'payment','globadyme',$5,$6,$7,$1,$8,$9,$10,$11,$12,$13,$14)")
        .bind(id).bind(ws).bind(order).bind(invoice).bind(&account.environment).bind(&account.merchant_id)
        .bind(account.revision).bind(amount).bind(currency).bind(decimals).bind(id.to_string())
        .bind(country).bind(payer.trim()).bind(email).execute(conn).await?;
    Ok(id)
}

/// Durable exclusive claim. Crash after commit remains unknown; no replay.
pub async fn claim(db: impl PgExecutor<'_>, ws: Uuid, id: Uuid) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query("UPDATE order_payments SET claimed_at=now(),status='unknown',attempts=attempts+1,updated_at=now() WHERE workspace_id=$1 AND id=$2 AND provider='globadyme' AND status='creating' AND claimed_at IS NULL")
        .bind(ws).bind(id).execute(db).await?.rows_affected() == 1)
}

/// Preserve unresolved financial evidence without changing accepted receipts.
pub async fn attention(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    id: Uuid,
    reason: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE order_payments SET attention=true,attention_reason=$3,updated_at=now() WHERE workspace_id=$1 AND id=$2")
        .bind(ws).bind(id).bind(reason).execute(db).await?;
    Ok(())
}
