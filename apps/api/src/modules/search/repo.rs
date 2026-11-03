//! Quick search SQL. Every statement filters `workspace_id`; `pattern` is an escaped
//! `%q%` (at least 3 characters of `q`), served by the trigram indexes of migration
//! `quick_search_trgm`.

use serde_json::Value;
use sqlx::PgExecutor;
use uuid::Uuid;

use crate::modules::search::dto::{QuickLot, QuickOrder, QuickProduct};

/// Live products by name, number, EAN-13 or barcode, newest first.
pub async fn products(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    pattern: &str,
    limit: i64,
) -> Result<Vec<QuickProduct>, sqlx::Error> {
    sqlx::query_as!(
        QuickProduct,
        r#"SELECT id, name, product_nr_formatted AS product_nr, ean13 FROM products
           WHERE workspace_id = $1 AND deleted_at IS NULL
             AND (name ILIKE $2 OR product_nr_formatted ILIKE $2 OR ean13 ILIKE $2
                  OR barcode ILIKE $2)
           ORDER BY created_at DESC, id DESC LIMIT $3"#,
        workspace_id,
        pattern,
        limit,
    )
    .fetch_all(db)
    .await
}

/// Orders by number or reference, newest first.
pub async fn orders(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    pattern: &str,
    limit: i64,
) -> Result<Vec<QuickOrder>, sqlx::Error> {
    sqlx::query_as!(
        QuickOrder,
        r#"SELECT o.id, o.number, o.reference, o.status, o.total,
                  upper(c.asset_type) AS "currency!", c.decimals
           FROM orders o JOIN currencies c ON c.workspace_id = o.workspace_id AND c.id = o.currency_id
           WHERE o.workspace_id = $1 AND (o.number ILIKE $2 OR o.reference ILIKE $2)
           ORDER BY o.created_at DESC, o.id DESC LIMIT $3"#,
        workspace_id,
        pattern,
        limit,
    )
    .fetch_all(db)
    .await
}

/// A numbered invoice, its number's series snapshot and customer name.
pub struct InvoiceHit {
    pub id: Uuid,
    pub invoice_number: i64,
    pub invoice_appendix: Value,
    pub status: String,
    pub customer_name: Option<String>,
}

/// The live invoice numbered `number` (one at most: numbers are unique per workspace).
pub async fn invoice(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    number: i64,
) -> Result<Option<InvoiceHit>, sqlx::Error> {
    sqlx::query_as!(
        InvoiceHit,
        r#"SELECT i.id, i.invoice_number AS "invoice_number!", i.invoice_appendix AS "invoice_appendix!",
                  i.status, coalesce(nullif(r.company_name, ''), nullif(r.alias, ''),
                      (SELECT nullif(concat_ws(' ', c.first_name, c.middle_name, c.last_name), '')
                       FROM contacts c
                       WHERE c.workspace_id = r.workspace_id AND c.relation_id = r.id
                         AND c.deleted_at IS NULL
                       ORDER BY c.created_at, c.id LIMIT 1)) AS customer_name
           FROM invoices i
           LEFT JOIN relations r ON r.workspace_id = i.workspace_id AND r.id = i.relation_id
           WHERE i.workspace_id = $1 AND i.invoice_number = $2 AND i.deleted_at IS NULL"#,
        workspace_id,
        number,
    )
    .fetch_optional(db)
    .await
}

/// Lots by code or public code, newest first.
pub async fn lots(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    pattern: &str,
    limit: i64,
) -> Result<Vec<QuickLot>, sqlx::Error> {
    sqlx::query_as!(
        QuickLot,
        r#"SELECT l.id, l.code, l.public_code, p.name AS product_name
           FROM lots l JOIN products p ON p.workspace_id = l.workspace_id AND p.id = l.product_id
           WHERE l.workspace_id = $1 AND (l.code ILIKE $2 OR l.public_code ILIKE $2)
           ORDER BY l.created_at DESC, l.id DESC LIMIT $3"#,
        workspace_id,
        pattern,
        limit,
    )
    .fetch_all(db)
    .await
}

/// A relation hit with its display name.
pub struct RelationHit {
    pub id: Uuid,
    pub name: Option<String>,
}

/// Live relations by company name, alias or a live contact's first or last name, newest first.
pub async fn relations(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    pattern: &str,
    limit: i64,
) -> Result<Vec<RelationHit>, sqlx::Error> {
    sqlx::query_as!(
        RelationHit,
        r#"SELECT r.id, coalesce(nullif(r.company_name, ''), nullif(r.alias, ''),
                      (SELECT nullif(concat_ws(' ', c.first_name, c.middle_name, c.last_name), '')
                       FROM contacts c
                       WHERE c.workspace_id = r.workspace_id AND c.relation_id = r.id
                         AND c.deleted_at IS NULL
                       ORDER BY c.created_at, c.id LIMIT 1)) AS name
           FROM relations r
           WHERE r.workspace_id = $1 AND r.deleted_at IS NULL
             AND (r.company_name ILIKE $2 OR r.alias ILIKE $2
                  OR r.id IN (SELECT c.relation_id FROM contacts c
                              WHERE c.workspace_id = $1 AND c.deleted_at IS NULL
                                AND (c.first_name ILIKE $2 OR c.last_name ILIKE $2)))
           ORDER BY r.created_at DESC, r.id DESC LIMIT $3"#,
        workspace_id,
        pattern,
        limit,
    )
    .fetch_all(db)
    .await
}
