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
    pub invoice_number: Option<i64>,
    pub invoice_appendix: Option<Value>,
    pub previous_invoice_id: Option<Uuid>,
    pub pdf_file_id: Option<Uuid>,
    /// Sealed payment token and its key id.
    pub payment_token: Option<Vec<u8>>,
    pub payment_token_key_id: Option<String>,
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
    pub credit_amount: Decimal,
    pub mutated_from_stock: Decimal,
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
    /// A credit note's source invoice.
    pub previous_invoice_id: Option<Uuid>,
    pub customer_ref: Option<&'a Value>,
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
    /// A credit line's source line.
    pub parent_item_id: Vec<Option<Uuid>>,
}

/// Changed lines; a None keeps that column, a false `set_*` keeps the paired one (Laravel
/// updated only the sent keys).
#[derive(Default)]
pub struct LineChanges {
    pub id: Vec<Uuid>,
    pub product_variant_id: Vec<Option<Uuid>>,
    pub set_warehouse: Vec<bool>,
    pub warehouse_id: Vec<Option<Uuid>>,
    pub title: Vec<String>,
    pub set_description: Vec<bool>,
    pub description: Vec<Option<String>>,
    pub quantity: Vec<Decimal>,
    pub price: Vec<Option<Decimal>>,
    pub vat: Vec<Decimal>,
    pub discount: Vec<Option<Decimal>>,
    pub purchase_price: Vec<Decimal>,
    pub position: Vec<Option<i32>>,
}

/// A live variant and its product.
pub struct VariantRow {
    pub id: Uuid,
    pub product_id: Uuid,
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
    if ids.is_empty() {
        return Ok(Vec::new());
    }
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

/// Live variants among `ids`. No lock: variant and warehouse deletes have no "in use" guard.
pub async fn variants(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    ids: &[Uuid],
) -> Result<Vec<VariantRow>, sqlx::Error> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    sqlx::query_as!(
        VariantRow,
        "SELECT id, product_id FROM product_variants
         WHERE workspace_id = $1 AND id = ANY($2) AND deleted_at IS NULL",
        workspace_id,
        ids,
    )
    .fetch_all(conn)
    .await
}

/// Live warehouses among `ids`.
pub async fn warehouses(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    ids: &[Uuid],
) -> Result<Vec<Uuid>, sqlx::Error> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    sqlx::query_scalar!(
        "SELECT id FROM warehouses
         WHERE workspace_id = $1 AND id = ANY($2) AND deleted_at IS NULL",
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
                               created_by, previous_invoice_id, customer_ref)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
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
        n.previous_invoice_id,
        n.customer_ref,
    )
    .fetch_one(conn)
    .await
}

/// What a write needs from the locked invoice.
pub struct Locked {
    pub status: String,
    pub relation_id: Uuid,
    pub currency_id: Uuid,
    pub is_credit: bool,
    pub invoice_number: Option<i64>,
    pub due_date: Option<DateTime<Utc>>,
    /// Minor units.
    pub total: i64,
    /// Decimals the stored totals were priced at.
    pub dp: Option<i32>,
    pub has_token: bool,
}

/// The live invoice `id`, row locked until commit: one writer per invoice, so item writes and
/// totals can't interleave.
pub async fn lock(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<Option<Locked>, sqlx::Error> {
    sqlx::query_as!(
        Locked,
        r#"SELECT status, relation_id, currency_id, is_credit, invoice_number, due_date, total,
                  (summary ->> 'dp')::int AS dp, payment_token_hash IS NOT NULL AS "has_token!"
           FROM invoices
           WHERE workspace_id = $1 AND id = $2 AND deleted_at IS NULL FOR NO KEY UPDATE"#,
        workspace_id,
        id,
    )
    .fetch_optional(conn)
    .await
}

/// The live invoice `id`, share locked until commit: its lines can't change while a copy reads
/// them, so the products on them stay guarded.
pub async fn find_for_share(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<Option<InvoiceRow>, sqlx::Error> {
    sqlx::query_as!(
        InvoiceRow,
        "SELECT id, relation_id, currency_id, status, is_credit, payment_method, reference, note,
                billing_address, addressed_contact, customer_ref, meta, due_date, send_date,
                summary, created_at, updated_at, invoice_number, invoice_appendix,
                previous_invoice_id, pdf_file_id, payment_token, payment_token_key_id
         FROM invoices WHERE workspace_id = $1 AND id = $2 AND deleted_at IS NULL FOR SHARE",
        workspace_id,
        id,
    )
    .fetch_optional(conn)
    .await
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

/// `(id, product_id)` of the invoice's lines.
pub async fn item_products(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    invoice_id: Uuid,
) -> Result<Vec<(Uuid, Option<Uuid>)>, sqlx::Error> {
    let rows = sqlx::query!(
        "SELECT id, product_id FROM items WHERE workspace_id = $1 AND invoice_id = $2",
        workspace_id,
        invoice_id,
    )
    .fetch_all(conn)
    .await?;
    Ok(rows.into_iter().map(|r| (r.id, r.product_id)).collect())
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
                            discount_percentage, purchase_price, position, parent_item_id)
         SELECT $1, $2, u.product_id, u.variant_id, u.warehouse_id, u.title, u.description,
                u.quantity, u.price, u.vat, u.discount, u.purchase_price, u.position, u.parent
         FROM unnest($3::uuid[], $4::uuid[], $5::uuid[], $6::text[], $7::text[],
                     $8::numeric[], $9::numeric[], $10::numeric[], $11::numeric[],
                     $12::numeric[], $13::int4[], $14::uuid[])
              AS u(product_id, variant_id, warehouse_id, title, description, quantity, price,
                   vat, discount, purchase_price, position, parent)",
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
        &l.parent_item_id as &[Option<Uuid>],
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Apply `c` to lines of `invoice_id`, one statement; a line equal to what is sent is left
/// alone (no new row version). The product never changes (Laravel).
pub async fn update_lines(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    invoice_id: Uuid,
    c: &LineChanges,
) -> Result<(), sqlx::Error> {
    if c.id.is_empty() {
        return Ok(());
    }
    sqlx::query!(
        "UPDATE items i SET
            product_variant_id = n.variant_id,
            mutate_from_warehouse_id = n.warehouse_id,
            title = n.title,
            description = n.description,
            quantity = n.quantity,
            price = n.price,
            vat = n.vat,
            discount_percentage = n.discount,
            purchase_price = n.purchase_price,
            position = n.position,
            updated_at = now()
         FROM (SELECT u.id,
                      COALESCE(u.variant_id, c.product_variant_id) AS variant_id,
                      CASE WHEN u.set_warehouse THEN u.warehouse_id
                           ELSE c.mutate_from_warehouse_id END AS warehouse_id,
                      u.title,
                      CASE WHEN u.set_description THEN u.description
                           ELSE c.description END AS description,
                      u.quantity, COALESCE(u.price, c.price) AS price, u.vat,
                      COALESCE(u.discount, c.discount_percentage) AS discount,
                      u.purchase_price, COALESCE(u.position, c.position) AS position
               FROM unnest($3::uuid[], $4::uuid[], $5::bool[], $6::uuid[], $7::text[],
                           $8::bool[], $9::text[], $10::numeric[], $11::numeric[],
                           $12::numeric[], $13::numeric[], $14::numeric[], $15::int4[])
                    AS u(id, variant_id, set_warehouse, warehouse_id, title, set_description,
                         description, quantity, price, vat, discount, purchase_price, position)
               JOIN items c ON c.id = u.id) AS n
         WHERE i.id = n.id AND i.workspace_id = $1 AND i.invoice_id = $2
           AND (i.product_variant_id, i.mutate_from_warehouse_id, i.title, i.description,
                i.quantity, i.price, i.vat, i.discount_percentage, i.purchase_price, i.position)
               IS DISTINCT FROM
               (n.variant_id, n.warehouse_id, n.title, n.description, n.quantity, n.price,
                n.vat, n.discount, n.purchase_price, n.position)",
        workspace_id,
        invoice_id,
        &c.id,
        &c.product_variant_id as &[Option<Uuid>],
        &c.set_warehouse,
        &c.warehouse_id as &[Option<Uuid>],
        &c.title,
        &c.set_description,
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
    workspace_id: Uuid,
    invoice_id: Uuid,
    ids: &[Uuid],
) -> Result<(), sqlx::Error> {
    if ids.is_empty() {
        return Ok(());
    }
    sqlx::query!(
        "DELETE FROM items WHERE workspace_id = $1 AND invoice_id = $2 AND id = ANY($3)",
        workspace_id,
        invoice_id,
        ids,
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// The invoice's lines in display order.
pub async fn lines(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    invoice_id: Uuid,
) -> Result<Vec<ItemRow>, sqlx::Error> {
    sqlx::query_as!(
        ItemRow,
        "SELECT id, product_id, product_variant_id, mutate_from_warehouse_id, title, description,
                quantity, price, vat, discount_percentage, purchase_price, position, created_at,
                updated_at, credit_amount, mutated_from_stock
         FROM items WHERE workspace_id = $1 AND invoice_id = $2 ORDER BY position, id",
        workspace_id,
        invoice_id,
    )
    .fetch_all(db)
    .await
}

/// Store the totals computed from the lines.
pub async fn set_totals(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    id: Uuid,
    subtotal: i64,
    total: i64,
    summary: &Value,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE invoices SET subtotal = $3, total = $4, summary = $5
         WHERE workspace_id = $1 AND id = $2",
        workspace_id,
        id,
        subtotal,
        total,
        summary,
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// What a PDF prints a currency with.
pub struct CurrencyPrint {
    pub symbol_prefix: Option<String>,
    pub symbol_suffix: Option<String>,
    pub decimals: i16,
}

/// Print data of currency `id` (a deleted currency still prints its invoices).
pub async fn currency_print(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<CurrencyPrint, sqlx::Error> {
    sqlx::query_as!(
        CurrencyPrint,
        "SELECT symbol_prefix, symbol_suffix, decimals FROM currencies
         WHERE workspace_id = $1 AND id = $2",
        workspace_id,
        id,
    )
    .fetch_one(db)
    .await
}

/// The workspace's newest live invoice (a template preview's sample).
pub async fn newest(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT id FROM invoices WHERE workspace_id = $1 AND deleted_at IS NULL
         ORDER BY created_at DESC, id DESC LIMIT 1",
        workspace_id,
    )
    .fetch_optional(db)
    .await
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
                summary, created_at, updated_at, invoice_number, invoice_appendix,
                previous_invoice_id, pdf_file_id, payment_token, payment_token_key_id
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

/// One page of live invoices, newest first, plus the total count when `count` (a short first
/// page counts itself).
pub async fn list(
    db: &sqlx::PgPool,
    workspace_id: Uuid,
    f: &ListFilter<'_>,
    limit: i64,
    offset: i64,
    count: bool,
) -> Result<(Vec<InvoiceRow>, i64), sqlx::Error> {
    // Laravel on_account: the in_status set, or on-account invoices still open.
    let rows = sqlx::query_as!(
        InvoiceRow,
        r#"SELECT i.id, i.relation_id, i.currency_id, i.status, i.is_credit, i.payment_method,
                  i.reference, i.note, i.billing_address, i.addressed_contact, i.customer_ref,
                  i.meta, i.due_date, i.send_date, i.summary, i.created_at, i.updated_at,
                  i.invoice_number, i.invoice_appendix, i.previous_invoice_id, i.pdf_file_id,
                  i.payment_token, i.payment_token_key_id
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
    let shown = i64::try_from(rows.len()).unwrap_or(i64::MAX);
    if !count || (offset == 0 && shown < limit) {
        return Ok((rows, shown));
    }
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

/// What a send stores on the invoice; a None keeps the stored value.
pub struct Sent<'a> {
    pub number: i64,
    pub appendix: &'a Value,
    pub status: &'a str,
    pub due_date: Option<DateTime<Utc>>,
    pub billing_address: &'a Value,
    pub addressed_contact: &'a Value,
    /// New payment token: sha256, sealed copy, key id.
    pub token: Option<(&'a [u8], &'a [u8], &'a str)>,
}

/// Number the invoice and record the send (`send_date` once).
pub async fn mark_sent(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    id: Uuid,
    s: &Sent<'_>,
) -> Result<(), sqlx::Error> {
    let (hash, sealed, key_id) = s
        .token
        .map_or((None, None, None), |(h, t, k)| (Some(h), Some(t), Some(k)));
    sqlx::query!(
        "UPDATE invoices SET
            invoice_number = $3, invoice_appendix = $4, status = $5,
            send_date = COALESCE(send_date, clock_timestamp()), due_date = COALESCE(due_date, $6),
            billing_address = $7, addressed_contact = $8,
            payment_token_hash = COALESCE($9, payment_token_hash),
            payment_token = COALESCE($10, payment_token),
            payment_token_key_id = COALESCE($11, payment_token_key_id),
            updated_at = now()
         WHERE workspace_id = $1 AND id = $2",
        workspace_id,
        id,
        s.number,
        s.appendix,
        s.status,
        s.due_date,
        s.billing_address,
        s.addressed_contact,
        hash,
        sealed,
        key_id,
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Resend: the new billing address and addressed contact, and a payment token when the
/// invoice was sent without one.
pub async fn set_recipient(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    id: Uuid,
    billing_address: &Value,
    addressed_contact: &Value,
    token: Option<(&[u8], &[u8], &str)>,
) -> Result<(), sqlx::Error> {
    let (hash, sealed, key_id) =
        token.map_or((None, None, None), |(h, t, k)| (Some(h), Some(t), Some(k)));
    sqlx::query!(
        "UPDATE invoices SET billing_address = $3, addressed_contact = $4,
            payment_token_hash = COALESCE($5, payment_token_hash),
            payment_token = COALESCE($6, payment_token),
            payment_token_key_id = COALESCE($7, payment_token_key_id),
            updated_at = now()
         WHERE workspace_id = $1 AND id = $2",
        workspace_id,
        id,
        billing_address,
        addressed_contact,
        hash,
        sealed,
        key_id,
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// The PDF last sent.
pub async fn set_pdf(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
    file_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE invoices SET pdf_file_id = $3 WHERE workspace_id = $1 AND id = $2",
        workspace_id,
        id,
        file_id,
    )
    .execute(db)
    .await?;
    Ok(())
}

/// A line whose product keeps stock and whose quantity hasn't fully left stock yet, with the
/// warehouse it moves in: the line's, else the product's default, else the workspace's main
/// warehouse (live ones only); None when there is none.
pub struct StockLine {
    pub id: Uuid,
    pub quantity: Decimal,
    pub mutated_from_stock: Decimal,
    pub product_id: Uuid,
    pub product_name: String,
    pub reference_vendor: Option<String>,
    pub warehouse_id: Option<Uuid>,
    pub warehouse_name: Option<String>,
    pub is_store: Option<bool>,
}

/// Laravel SendInvoice::mutateStock's lines (Product::shouldMutateStock: physical types with
/// `has_stock`), in display order.
pub async fn stock_lines(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    invoice_id: Uuid,
) -> Result<Vec<StockLine>, sqlx::Error> {
    sqlx::query_as!(
        StockLine,
        r#"SELECT i.id, i.quantity, i.mutated_from_stock, p.id AS product_id,
                  p.name AS product_name, p.reference_vendor,
                  w.id AS "warehouse_id?", w.name AS "warehouse_name?", w.is_store AS "is_store?"
           FROM items i
           JOIN products p ON p.workspace_id = i.workspace_id AND p.id = i.product_id
           LEFT JOIN LATERAL (
               SELECT w.id, w.name, COALESCE(w.is_store, false) AS is_store
               FROM warehouses w
               WHERE w.workspace_id = i.workspace_id AND w.deleted_at IS NULL
                 AND (w.id = COALESCE(i.mutate_from_warehouse_id, p.default_warehouse_id)
                      OR w.default_warehouse)
               ORDER BY (w.id = COALESCE(i.mutate_from_warehouse_id, p.default_warehouse_id))
                        IS TRUE DESC
               LIMIT 1
           ) w ON true
           WHERE i.workspace_id = $1 AND i.invoice_id = $2
             AND p.type IN ('physical', 'physical_second_chance') AND p.has_stock
             AND i.quantity <> i.mutated_from_stock
           ORDER BY i.position, i.id"#,
        workspace_id,
        invoice_id,
    )
    .fetch_all(conn)
    .await
}

/// Lines `ids` fully left stock.
pub async fn mark_mutated(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    invoice_id: Uuid,
    ids: &[Uuid],
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE items SET mutated_from_stock = quantity, updated_at = now()
         WHERE workspace_id = $1 AND invoice_id = $2 AND id = ANY($3)",
        workspace_id,
        invoice_id,
        ids,
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// What a send or credit reads from the relation.
pub struct RelationTerms {
    pub kind: String,
    pub terms_and_periods: Option<Value>,
    pub allow_invoicing_on_account: bool,
}

/// The relation's type, payment terms and on-account flag (deleted relations included).
pub async fn relation_terms(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<Option<RelationTerms>, sqlx::Error> {
    sqlx::query_as!(
        RelationTerms,
        r#"SELECT type AS kind, terms_and_periods, allow_invoicing_on_account
           FROM relations WHERE workspace_id = $1 AND id = $2"#,
        workspace_id,
        id,
    )
    .fetch_optional(db)
    .await
}

/// The live, sent invoice holding payment token `hash`: (workspace, id).
pub async fn by_token(
    db: impl PgExecutor<'_>,
    hash: &[u8],
) -> Result<Option<(Uuid, Uuid)>, sqlx::Error> {
    let row = sqlx::query!(
        "SELECT workspace_id, id FROM invoices
         WHERE payment_token_hash = $1 AND deleted_at IS NULL AND invoice_number IS NOT NULL",
        hash,
    )
    .fetch_optional(db)
    .await?;
    Ok(row.map(|r| (r.workspace_id, r.id)))
}

/// Add `quantities` to the credited amount of lines `ids` (one statement).
pub async fn add_credited(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    invoice_id: Uuid,
    ids: &[Uuid],
    quantities: &[Decimal],
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE items i SET credit_amount = i.credit_amount + u.quantity, updated_at = now()
         FROM unnest($3::uuid[], $4::numeric[]) AS u(id, quantity)
         WHERE i.workspace_id = $1 AND i.invoice_id = $2 AND i.id = u.id",
        workspace_id,
        invoice_id,
        ids,
        quantities,
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// True when a numbered invoice of the workspace is in currency `currency_id`.
pub async fn sent_in_currency(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    currency_id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM invoices
                          WHERE workspace_id = $1 AND currency_id = $2
                            AND invoice_number IS NOT NULL) AS "e!""#,
        workspace_id,
        currency_id,
    )
    .fetch_one(db)
    .await
}
