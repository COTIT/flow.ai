//! SQL for invoices and their item lines. Writes run inside the service's transaction; item
//! lines are written in one statement per kind (UNNEST), whatever the line count.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde_json::Value;
use sqlx::{PgConnection, PgExecutor};
use uuid::Uuid;

/// Invoice header as stored.
pub struct InvoiceRow {
    pub id: Uuid,
    pub relation_id: Uuid,
    pub currency_id: Uuid,
    pub status: String,
    pub is_credit: bool,
    pub payment_method: String,
    pub reference: Option<String>,
    pub note: Option<String>,
    pub billing_address: Option<Value>,
    pub addressed_contact: Option<Value>,
    pub customer_ref: Option<Value>,
    pub meta: Option<Value>,
    pub due_date: Option<DateTime<Utc>>,
    pub send_date: Option<DateTime<Utc>>,
    pub summary: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// One stored item line.
pub struct ItemRow {
    pub id: Uuid,
    pub product_id: Option<Uuid>,
    pub product_variant_id: Option<Uuid>,
    pub mutate_from_warehouse_id: Option<Uuid>,
    pub title: String,
    pub description: Option<String>,
    pub quantity: Decimal,
    pub price: Decimal,
    pub vat: Decimal,
    pub discount_percentage: Decimal,
    pub purchase_price: Decimal,
    pub position: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Header fields of a new invoice.
pub struct NewInvoice<'a> {
    pub relation_id: Uuid,
    pub currency_id: Uuid,
    pub is_credit: bool,
    pub payment_method: &'a str,
    pub reference: Option<&'a str>,
    pub note: Option<&'a str>,
    pub billing_address: Option<&'a Value>,
    pub addressed_contact: Option<&'a Value>,
    pub meta: Option<&'a Value>,
    pub created_by: Uuid,
}

/// Header fields an update may change; `set_*` false keeps the stored value.
pub struct HeaderPatch<'a> {
    pub relation_id: Uuid,
    pub set_reference: bool,
    pub reference: Option<&'a str>,
    pub set_note: bool,
    pub note: Option<&'a str>,
    pub set_due_date: bool,
    pub due_date: Option<DateTime<Utc>>,
    pub set_billing_address: bool,
    pub billing_address: Option<&'a Value>,
    pub set_addressed_contact: bool,
    pub addressed_contact: Option<&'a Value>,
    pub set_customer_ref: bool,
    pub customer_ref: Option<&'a Value>,
    pub set_meta: bool,
    pub meta: Option<&'a Value>,
}

/// New lines, one array per column, same length (UNNEST order = position order).
#[derive(Default)]
pub struct NewLines {
    pub product_id: Vec<Option<Uuid>>,
    pub product_variant_id: Vec<Option<Uuid>>,
    pub warehouse_id: Vec<Option<Uuid>>,
    pub title: Vec<String>,
    pub description: Vec<Option<String>>,
    pub quantity: Vec<Decimal>,
    pub price: Vec<Decimal>,
    pub vat: Vec<Decimal>,
    pub discount: Vec<Decimal>,
    pub purchase_price: Vec<Decimal>,
    pub position: Vec<i32>,
}

/// Changed lines; a None keeps that column (Laravel updated only the sent keys).
#[derive(Default)]
pub struct LineChanges {
    pub id: Vec<Uuid>,
    pub product_variant_id: Vec<Option<Uuid>>,
    pub warehouse_id: Vec<Option<Uuid>>,
    pub title: Vec<String>,
    pub description: Vec<Option<String>>,
    pub quantity: Vec<Decimal>,
    pub price: Vec<Option<Decimal>>,
    pub vat: Vec<Decimal>,
    pub discount: Vec<Option<Decimal>>,
    pub purchase_price: Vec<Decimal>,
    pub position: Vec<Option<i32>>,
}

/// A live product as an item line takes it.
pub struct ProductDefaults {
    pub id: Uuid,
    pub default_warehouse_id: Option<Uuid>,
    pub default_variant_id: Option<Uuid>,
}

/// The live relation `id`, key-share locked until commit (a concurrent delete waits and then
/// sees this invoice); its stored addresses.
pub async fn lock_relation(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<Option<Value>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT addresses FROM relations
         WHERE workspace_id = $1 AND id = $2 AND deleted_at IS NULL FOR KEY SHARE",
        workspace_id,
        id,
    )
    .fetch_optional(conn)
    .await
}

/// Live contact `id` of `relation_id` as Laravel AddressedContact (snapshot fields).
pub async fn addressed_contact(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    relation_id: Uuid,
    id: Uuid,
) -> Result<Option<Value>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT jsonb_build_object(
                'uuid', id, 'initials', initials, 'prefix', prefix, 'first_name', first_name,
                'middle_name', middle_name, 'last_name', last_name, 'job_title', job_title,
                'email', email, 'default_for', default_for, 'phones', phones) AS "c!"
           FROM contacts
           WHERE workspace_id = $1 AND relation_id = $2 AND id = $3 AND deleted_at IS NULL"#,
        workspace_id,
        relation_id,
        id,
    )
    .fetch_optional(db)
    .await
}

/// Live products among `ids`, key-share locked until commit, with the defaults a new line
/// takes (default warehouse, default variant).
pub async fn lock_products(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    ids: &[Uuid],
) -> Result<Vec<ProductDefaults>, sqlx::Error> {
    sqlx::query_as!(
        ProductDefaults,
        r#"SELECT p.id, p.default_warehouse_id,
                  (SELECT v.id FROM product_variants v
                   WHERE v.workspace_id = p.workspace_id AND v.product_id = p.id
                     AND v.is_default AND v.deleted_at IS NULL) AS default_variant_id
           FROM products p
           WHERE p.workspace_id = $1 AND p.id = ANY($2) AND p.deleted_at IS NULL
           ORDER BY p.id
           FOR KEY SHARE OF p"#,
        workspace_id,
        ids,
    )
    .fetch_all(conn)
    .await
}

/// Live variants among `ids` (key-share locked).
pub async fn lock_variants(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    ids: &[Uuid],
) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT id FROM product_variants
         WHERE workspace_id = $1 AND id = ANY($2) AND deleted_at IS NULL
         ORDER BY id FOR KEY SHARE",
        workspace_id,
        ids,
    )
    .fetch_all(conn)
    .await
}

/// Live warehouses among `ids` (key-share locked).
pub async fn lock_warehouses(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    ids: &[Uuid],
) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT id FROM warehouses
         WHERE workspace_id = $1 AND id = ANY($2) AND deleted_at IS NULL
         ORDER BY id FOR KEY SHARE",
        workspace_id,
        ids,
    )
    .fetch_all(conn)
    .await
}

/// New invoice header; returns its id.
pub async fn insert(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    n: &NewInvoice<'_>,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO invoices (workspace_id, relation_id, currency_id, is_credit, payment_method,
                               reference, note, billing_address, addressed_contact, meta,
                               created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
         RETURNING id",
        workspace_id,
        n.relation_id,
        n.currency_id,
        n.is_credit,
        n.payment_method,
        n.reference,
        n.note,
        n.billing_address,
        n.addressed_contact,
        n.meta,
        n.created_by,
    )
    .fetch_one(conn)
    .await
}

/// `(status, is_credit, currency_id)` of the live invoice `id`, row locked until commit: one
/// writer per invoice, so item writes and totals can't interleave.
pub async fn lock(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<Option<(String, bool, Uuid)>, sqlx::Error> {
    let row = sqlx::query!(
        "SELECT status, is_credit, currency_id FROM invoices
         WHERE workspace_id = $1 AND id = $2 AND deleted_at IS NULL FOR NO KEY UPDATE",
        workspace_id,
        id,
    )
    .fetch_optional(conn)
    .await?;
    Ok(row.map(|r| (r.status, r.is_credit, r.currency_id)))
}

/// Apply `p` to the header.
pub async fn update_header(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    id: Uuid,
    p: &HeaderPatch<'_>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE invoices SET
            relation_id = $3,
            reference = CASE WHEN $4 THEN $5 ELSE reference END,
            note = CASE WHEN $6 THEN $7 ELSE note END,
            due_date = CASE WHEN $8 THEN $9 ELSE due_date END,
            billing_address = CASE WHEN $10 THEN $11 ELSE billing_address END,
            addressed_contact = CASE WHEN $12 THEN $13 ELSE addressed_contact END,
            customer_ref = CASE WHEN $14 THEN $15 ELSE customer_ref END,
            meta = CASE WHEN $16 THEN $17 ELSE meta END,
            updated_at = now()
         WHERE workspace_id = $1 AND id = $2",
        workspace_id,
        id,
        p.relation_id,
        p.set_reference,
        p.reference,
        p.set_note,
        p.note,
        p.set_due_date,
        p.due_date,
        p.set_billing_address,
        p.billing_address,
        p.set_addressed_contact,
        p.addressed_contact,
        p.set_customer_ref,
        p.customer_ref,
        p.set_meta,
        p.meta,
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Set the status (cancel).
pub async fn set_status(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    id: Uuid,
    status: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE invoices SET status = $3, updated_at = now() WHERE workspace_id = $1 AND id = $2",
        workspace_id,
        id,
        status,
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Ids of the invoice's lines.
pub async fn item_ids(conn: &mut PgConnection, invoice_id: Uuid) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar!("SELECT id FROM items WHERE invoice_id = $1", invoice_id)
        .fetch_all(conn)
        .await
}

/// Insert `l` as lines of `invoice_id`, one statement.
pub async fn insert_lines(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    invoice_id: Uuid,
    l: &NewLines,
) -> Result<(), sqlx::Error> {
    if l.title.is_empty() {
        return Ok(());
    }
    sqlx::query!(
        "INSERT INTO items (workspace_id, invoice_id, product_id, product_variant_id,
                            mutate_from_warehouse_id, title, description, quantity, price, vat,
                            discount_percentage, purchase_price, position)
         SELECT $1, $2, u.product_id, u.variant_id, u.warehouse_id, u.title, u.description,
                u.quantity, u.price, u.vat, u.discount, u.purchase_price, u.position
         FROM unnest($3::uuid[], $4::uuid[], $5::uuid[], $6::text[], $7::text[],
                     $8::numeric[], $9::numeric[], $10::numeric[], $11::numeric[],
                     $12::numeric[], $13::int4[])
              AS u(product_id, variant_id, warehouse_id, title, description, quantity, price,
                   vat, discount, purchase_price, position)",
        workspace_id,
        invoice_id,
        &l.product_id as &[Option<Uuid>],
        &l.product_variant_id as &[Option<Uuid>],
        &l.warehouse_id as &[Option<Uuid>],
        &l.title,
        &l.description as &[Option<String>],
        &l.quantity,
        &l.price,
        &l.vat,
        &l.discount,
        &l.purchase_price,
        &l.position,
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Apply `c` to lines of `invoice_id`, one statement. The product never changes (Laravel).
pub async fn update_lines(
    conn: &mut PgConnection,
    invoice_id: Uuid,
    c: &LineChanges,
) -> Result<(), sqlx::Error> {
    if c.id.is_empty() {
        return Ok(());
    }
    sqlx::query!(
        "UPDATE items i SET
            product_variant_id = COALESCE(u.variant_id, i.product_variant_id),
            mutate_from_warehouse_id = COALESCE(u.warehouse_id, i.mutate_from_warehouse_id),
            title = u.title,
            description = u.description,
            quantity = u.quantity,
            price = COALESCE(u.price, i.price),
            vat = u.vat,
            discount_percentage = COALESCE(u.discount, i.discount_percentage),
            purchase_price = u.purchase_price,
            position = COALESCE(u.position, i.position),
            updated_at = now()
         FROM unnest($2::uuid[], $3::uuid[], $4::uuid[], $5::text[], $6::text[],
                     $7::numeric[], $8::numeric[], $9::numeric[], $10::numeric[],
                     $11::numeric[], $12::int4[])
              AS u(id, variant_id, warehouse_id, title, description, quantity, price, vat,
                   discount, purchase_price, position)
         WHERE i.id = u.id AND i.invoice_id = $1",
        invoice_id,
        &c.id,
        &c.product_variant_id as &[Option<Uuid>],
        &c.warehouse_id as &[Option<Uuid>],
        &c.title,
        &c.description as &[Option<String>],
        &c.quantity,
        &c.price as &[Option<Decimal>],
        &c.vat,
        &c.discount as &[Option<Decimal>],
        &c.purchase_price,
        &c.position as &[Option<i32>],
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Remove lines `ids` of `invoice_id`.
pub async fn delete_lines(
    conn: &mut PgConnection,
    invoice_id: Uuid,
    ids: &[Uuid],
) -> Result<(), sqlx::Error> {
    if ids.is_empty() {
        return Ok(());
    }
    sqlx::query!(
        "DELETE FROM items WHERE invoice_id = $1 AND id = ANY($2)",
        invoice_id,
        ids,
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// The invoice's lines in display order.
pub async fn lines(db: impl PgExecutor<'_>, invoice_id: Uuid) -> Result<Vec<ItemRow>, sqlx::Error> {
    sqlx::query_as!(
        ItemRow,
        "SELECT id, product_id, product_variant_id, mutate_from_warehouse_id, title, description,
                quantity, price, vat, discount_percentage, purchase_price, position, created_at,
                updated_at
         FROM items WHERE invoice_id = $1 ORDER BY position, id",
        invoice_id,
    )
    .fetch_all(db)
    .await
}

/// Store the totals computed from the lines.
pub async fn set_totals(
    conn: &mut PgConnection,
    id: Uuid,
    subtotal: i64,
    total: i64,
    summary: &Value,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE invoices SET subtotal = $2, total = $3, summary = $4 WHERE id = $1",
        id,
        subtotal,
        total,
        summary,
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Decimals of currency `id` (a deleted currency still prices its invoices).
pub async fn currency_decimals(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<i16, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT decimals FROM currencies WHERE workspace_id = $1 AND id = $2",
        workspace_id,
        id,
    )
    .fetch_one(db)
    .await
}

/// The live invoice `id` of this workspace.
pub async fn find(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<Option<InvoiceRow>, sqlx::Error> {
    sqlx::query_as!(
        InvoiceRow,
        "SELECT id, relation_id, currency_id, status, is_credit, payment_method, reference, note,
                billing_address, addressed_contact, customer_ref, meta, due_date, send_date,
                summary, created_at, updated_at
         FROM invoices WHERE workspace_id = $1 AND id = $2 AND deleted_at IS NULL",
        workspace_id,
        id,
    )
    .fetch_optional(db)
    .await
}

/// List filters (Laravel ListInvoice SQL path).
pub struct ListFilter<'a> {
    pub relation_id: Option<Uuid>,
    pub status: Option<&'a str>,
    pub in_status: Option<&'a [String]>,
    pub is_credit: Option<bool>,
    pub on_account: bool,
    /// Escaped ILIKE pattern on reference and the relation's name or alias.
    pub search: Option<&'a str>,
}

/// One page of live invoices, newest first, plus the total count.
pub async fn list(
    db: &sqlx::PgPool,
    workspace_id: Uuid,
    f: &ListFilter<'_>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<InvoiceRow>, i64), sqlx::Error> {
    // Laravel on_account: the in_status set, or on-account invoices still open.
    let rows = sqlx::query_as!(
        InvoiceRow,
        r#"SELECT i.id, i.relation_id, i.currency_id, i.status, i.is_credit, i.payment_method,
                  i.reference, i.note, i.billing_address, i.addressed_contact, i.customer_ref,
                  i.meta, i.due_date, i.send_date, i.summary, i.created_at, i.updated_at
           FROM invoices i
           WHERE i.workspace_id = $1 AND i.deleted_at IS NULL
             AND ($2::uuid IS NULL OR i.relation_id = $2)
             AND ($3::text IS NULL OR i.status = $3)
             AND ($5::bool IS NULL OR i.is_credit = $5)
             AND (CASE WHEN $6
                       THEN ($4::text[] IS NOT NULL AND i.status = ANY($4))
                            OR (i.meta -> 'on_account' = 'true'
                                AND i.status NOT IN ('paid', 'credited', 'cancelled'))
                       ELSE $4::text[] IS NULL OR i.status = ANY($4) END)
             AND ($7::text IS NULL OR i.reference ILIKE $7 OR EXISTS (
                     SELECT 1 FROM relations r
                     WHERE r.workspace_id = i.workspace_id AND r.id = i.relation_id
                       AND (r.company_name ILIKE $7 OR r.alias ILIKE $7)))
           ORDER BY i.created_at DESC, i.id DESC
           LIMIT $8 OFFSET $9"#,
        workspace_id,
        f.relation_id,
        f.status,
        f.in_status,
        f.is_credit,
        f.on_account,
        f.search,
        limit,
        offset,
    )
    .fetch_all(db)
    .await?;
    let total = sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM invoices i
           WHERE i.workspace_id = $1 AND i.deleted_at IS NULL
             AND ($2::uuid IS NULL OR i.relation_id = $2)
             AND ($3::text IS NULL OR i.status = $3)
             AND ($5::bool IS NULL OR i.is_credit = $5)
             AND (CASE WHEN $6
                       THEN ($4::text[] IS NOT NULL AND i.status = ANY($4))
                            OR (i.meta -> 'on_account' = 'true'
                                AND i.status NOT IN ('paid', 'credited', 'cancelled'))
                       ELSE $4::text[] IS NULL OR i.status = ANY($4) END)
             AND ($7::text IS NULL OR i.reference ILIKE $7 OR EXISTS (
                     SELECT 1 FROM relations r
                     WHERE r.workspace_id = i.workspace_id AND r.id = i.relation_id
                       AND (r.company_name ILIKE $7 OR r.alias ILIKE $7)))"#,
        workspace_id,
        f.relation_id,
        f.status,
        f.in_status,
        f.is_credit,
        f.on_account,
        f.search,
    )
    .fetch_one(db)
    .await?;
    Ok((rows, total))
}

/// True when a line uses `product_id` (the product delete guard).
pub async fn product_in_use(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    product_id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM items WHERE workspace_id = $1 AND product_id = $2) AS "e!""#,
        workspace_id,
        product_id,
    )
    .fetch_one(db)
    .await
}

/// True when the relation has a live invoice, cancelled ones included (the relation delete
/// guard; Laravel counted invoices, orders, estimates and work orders).
pub async fn relation_has_documents(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    relation_id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM invoices
                          WHERE workspace_id = $1 AND relation_id = $2 AND deleted_at IS NULL) AS "e!""#,
        workspace_id,
        relation_id,
    )
    .fetch_one(db)
    .await
}
