//! SQL for invoice payments. Writes run in the service's transaction, under the row lock of
//! every invoice they touch; rows are inserted or voided, never changed otherwise (the table's
//! trigger refuses the rest).

use chrono::{DateTime, NaiveDate, Utc};
use serde_json::Value;
use sqlx::{PgConnection, PgExecutor};
use uuid::Uuid;

/// What a payment write reads from its invoice, row locked.
pub struct Target {
    pub id: Uuid,
    pub status: String,
    pub is_credit: bool,
    pub relation_id: Uuid,
    pub currency_id: Uuid,
    /// Minor units at `dp`.
    pub total: i64,
    pub paid_total: i64,
    /// Decimals the totals were priced at.
    pub dp: Option<i32>,
    pub invoice_number: Option<i64>,
    pub invoice_appendix: Option<Value>,
}

/// The live invoice `id`, locked until commit (`FOR NO KEY UPDATE`, as every invoice write).
pub async fn lock(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<Option<Target>, sqlx::Error> {
    sqlx::query_as!(
        Target,
        r#"SELECT id, status, is_credit, relation_id, currency_id, total, paid_total,
                  (summary ->> 'dp')::int AS dp, invoice_number, invoice_appendix
           FROM invoices
           WHERE workspace_id = $1 AND id = $2 AND deleted_at IS NULL FOR NO KEY UPDATE"#,
        workspace_id,
        id,
    )
    .fetch_optional(conn)
    .await
}

/// A payment to insert.
pub struct NewPayment<'a> {
    pub id: Uuid,
    pub invoice_id: Uuid,
    pub user_id: Option<Uuid>,
    pub date: NaiveDate,
    pub amount: i64,
    pub description: Option<&'a str>,
    pub payment_method: &'a str,
    pub provider: Option<&'a str>,
    pub provider_id: Option<&'a str>,
    pub meta: Option<&'a Value>,
    pub settle_twin_id: Option<Uuid>,
}

/// Insert `rows` as paid payments in one statement (a settle's two sides point at each other,
/// so they go in together).
pub async fn insert(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    rows: &[NewPayment<'_>],
) -> Result<(), sqlx::Error> {
    let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
    let invoices: Vec<Uuid> = rows.iter().map(|r| r.invoice_id).collect();
    let users: Vec<Option<Uuid>> = rows.iter().map(|r| r.user_id).collect();
    let dates: Vec<NaiveDate> = rows.iter().map(|r| r.date).collect();
    let amounts: Vec<i64> = rows.iter().map(|r| r.amount).collect();
    let descriptions: Vec<Option<&str>> = rows.iter().map(|r| r.description).collect();
    let methods: Vec<&str> = rows.iter().map(|r| r.payment_method).collect();
    let providers: Vec<Option<&str>> = rows.iter().map(|r| r.provider).collect();
    let provider_ids: Vec<Option<&str>> = rows.iter().map(|r| r.provider_id).collect();
    let metas: Vec<Option<&Value>> = rows.iter().map(|r| r.meta).collect();
    let twins: Vec<Option<Uuid>> = rows.iter().map(|r| r.settle_twin_id).collect();
    sqlx::query!(
        "INSERT INTO invoice_payments (id, workspace_id, invoice_id, user_id, date, amount,
                                       description, payment_method, provider, provider_id,
                                       status, is_paid, meta, settle_twin_id)
         SELECT u.id, $1, u.invoice_id, u.user_id, u.date, u.amount, u.description, u.method,
                u.provider, u.provider_id, 'paid', true, u.meta, u.twin
         FROM unnest($2::uuid[], $3::uuid[], $4::uuid[], $5::date[], $6::int8[], $7::text[],
                     $8::text[], $9::text[], $10::text[], $11::jsonb[], $12::uuid[])
              AS u(id, invoice_id, user_id, date, amount, description, method, provider,
                   provider_id, meta, twin)",
        workspace_id,
        &ids,
        &invoices,
        &users as &[Option<Uuid>],
        &dates,
        &amounts,
        &descriptions as &[Option<&str>],
        &methods as &[&str],
        &providers as &[Option<&str>],
        &provider_ids as &[Option<&str>],
        &metas as &[Option<&Value>],
        &twins as &[Option<Uuid>],
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Sum of the invoice's paid, unvoided payments and whether cash is among them.
pub async fn sums(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    invoice_id: Uuid,
) -> Result<(i64, bool), sqlx::Error> {
    let row = sqlx::query!(
        r#"SELECT COALESCE(sum(amount), 0)::bigint AS "paid!",
                  COALESCE(bool_or(payment_method = 'cash'), false) AS "cash!"
           FROM invoice_payments
           WHERE workspace_id = $1 AND invoice_id = $2 AND is_paid AND voided_at IS NULL"#,
        workspace_id,
        invoice_id,
    )
    .fetch_one(conn)
    .await?;
    Ok((row.paid, row.cash))
}

/// How many live (unvoided) payments the invoice has.
pub async fn count_live(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    invoice_id: Uuid,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM invoice_payments
           WHERE workspace_id = $1 AND invoice_id = $2 AND voided_at IS NULL"#,
        workspace_id,
        invoice_id,
    )
    .fetch_one(conn)
    .await
}

/// Store the recomputed paid total and, when Some, the new status.
pub async fn set_balance(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    id: Uuid,
    paid_total: i64,
    status: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE invoices SET paid_total = $3, status = COALESCE($4, status), updated_at = now()
         WHERE workspace_id = $1 AND id = $2",
        workspace_id,
        id,
        paid_total,
        status,
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// The unvoided payment `id` of invoice `invoice_id`: the invoice of its settle twin, if any.
/// None when unknown, voided or another invoice's.
pub async fn find_live(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    invoice_id: Uuid,
    id: Uuid,
) -> Result<Option<Option<Uuid>>, sqlx::Error> {
    let row = sqlx::query!(
        r#"SELECT t.invoice_id AS "twin_invoice?"
           FROM invoice_payments p
           LEFT JOIN invoice_payments t
             ON t.workspace_id = p.workspace_id AND t.id = p.settle_twin_id
           WHERE p.workspace_id = $1 AND p.invoice_id = $2 AND p.id = $3
             AND p.voided_at IS NULL"#,
        workspace_id,
        invoice_id,
        id,
    )
    .fetch_optional(db)
    .await?;
    Ok(row.map(|r| r.twin_invoice))
}

/// Void payment `id` of invoice `invoice_id` and its settle twin; how many rows (0 when it was
/// voided meanwhile).
pub async fn void(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    invoice_id: Uuid,
    id: Uuid,
    user_id: Uuid,
) -> Result<u64, sqlx::Error> {
    Ok(sqlx::query!(
        "UPDATE invoice_payments SET voided_at = now(), voided_by = $4, updated_at = now()
         WHERE workspace_id = $1 AND voided_at IS NULL
           AND ((invoice_id = $2 AND id = $3) OR settle_twin_id = $3)",
        workspace_id,
        invoice_id,
        id,
        user_id,
    )
    .execute(conn)
    .await?
    .rows_affected())
}

/// A stored payment.
pub struct PaymentRow {
    pub id: Uuid,
    pub date: NaiveDate,
    pub amount: i64,
    pub description: Option<String>,
    pub payment_method: String,
    pub provider: Option<String>,
    pub provider_id: Option<String>,
    pub status: Option<String>,
    pub is_paid: bool,
    pub meta: Option<Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// The invoice's unvoided payments, by date then entry.
pub async fn list(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    invoice_id: Uuid,
) -> Result<Vec<PaymentRow>, sqlx::Error> {
    sqlx::query_as!(
        PaymentRow,
        "SELECT id, date, amount, description, payment_method, provider, provider_id, status,
                is_paid, meta, created_at, updated_at
         FROM invoice_payments
         WHERE workspace_id = $1 AND invoice_id = $2 AND voided_at IS NULL
         ORDER BY date, created_at, id",
        workspace_id,
        invoice_id,
    )
    .fetch_all(db)
    .await
}
