//! Later invoices of recognised sales (masterplan §12 LATER INVOICE): a draft invoice copies
//! complete recognitions (quantity, order-line price, tax, discount) and reserves each with one
//! billing link; issue in an active book reclassifies what is still unpaid DR receivables / CR
//! unbilled_receivables (billed item opened first, then a reclassification allocation), or
//! recognises a standalone invoice DR receivables / CR revenue / CR tax_payable. A fully
//! settled recognition bills as a document only. Credit notes are refused in active books.

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
            journals::{SourceHeader, SourceLine, post_source_lines},
            service::unsupported,
            settlement::{self, NewAllocation},
        },
        invoices::{
            self,
            dto::{InvoiceInput, ItemFields, ItemRow, RowKind},
        },
        shipments::recognition::{self, Receivable, open_receivable},
    },
};

const NOTHING: &str = "There is no recognised, unbilled sale to invoice.";
const BILLED: &str = "This sale is already on an invoice.";
const OTHER_PARTY: &str = "The invoice must go to the sale's customer, in its currency.";
const CREDIT: &str = "Credit notes can't be issued in an active book yet.";
const LINKED: &str = "Lines of an invoice billing recognised sales can't change.";

/// A recognition a draft may bill, under a row lock.
struct Billable {
    relation_id: Option<Uuid>,
    currency_id: Uuid,
    linked: bool,
}

/// Lock `ids` (recognitions of `order`, id order); 404 for one outside the order.
async fn lock_recognitions(
    conn: &mut PgConnection,
    ws: Uuid,
    order: Uuid,
    ids: &[Uuid],
) -> Result<Vec<Billable>, AppError> {
    let rows = sqlx::query_as!(
        Billable,
        r#"SELECT r.relation_id, r.currency_id,
                  EXISTS (SELECT 1 FROM invoice_recognition_links l
                          WHERE l.workspace_id = r.workspace_id AND l.recognition_id = r.id)
                      AS "linked!"
           FROM sales_recognitions r
           WHERE r.workspace_id = $1 AND r.order_id = $2 AND r.id = ANY($3)
           ORDER BY r.id FOR UPDATE"#,
        ws,
        order,
        ids
    )
    .fetch_all(&mut *conn)
    .await?;
    if rows.len() != ids.len() {
        return Err(AppError::NotFound);
    }
    Ok(rows)
}

/// Recognitions of `order` no invoice holds yet.
pub async fn unbilled(
    conn: &mut PgConnection,
    ws: Uuid,
    order: Uuid,
) -> Result<Vec<Uuid>, AppError> {
    Ok(sqlx::query_scalar!(
        "SELECT r.id FROM sales_recognitions r
         WHERE r.workspace_id = $1 AND r.order_id = $2
           AND NOT EXISTS (SELECT 1 FROM invoice_recognition_links l
                           WHERE l.workspace_id = r.workspace_id AND l.recognition_id = r.id)
         ORDER BY r.id",
        ws,
        order
    )
    .fetch_all(conn)
    .await?)
}

/// A draft invoice to `relation` billing recognitions `ids` of `order` (order locked by the
/// caller): items copy the recognised quantities at the order lines' price, tax and
/// discount; one draft link per recognition. Its id.
pub async fn draft(
    conn: &mut PgConnection,
    ws: Uuid,
    by: Uuid,
    (order, currency, reference): (Uuid, Uuid, &str),
    (relation, contact): (Uuid, Option<Uuid>),
    ids: &[Uuid],
) -> Result<Uuid, AppError> {
    if ids.is_empty() {
        return Err(AppError::Unprocessable(NOTHING.into()));
    }
    let rows = lock_recognitions(&mut *conn, ws, order, ids).await?;
    if rows.iter().any(|r| r.linked) {
        return Err(AppError::Conflict(BILLED));
    }
    if rows
        .iter()
        .any(|r| r.relation_id != Some(relation) || r.currency_id != currency)
    {
        return Err(AppError::field("payload.relation_id", OTHER_PARTY));
    }
    let lines = sqlx::query!(
        r#"SELECT ol.title, rl.quantity, ol.unit_price, c.decimals, rl.vat_rate,
                  rl.discount_percentage, ol.product_id, ol.product_variant_id
           FROM sales_recognition_lines rl
           JOIN order_lines ol ON ol.workspace_id = rl.workspace_id AND ol.id = rl.order_line_id
           JOIN orders o ON o.workspace_id = ol.workspace_id AND o.id = ol.order_id
           JOIN currencies c ON c.workspace_id = o.workspace_id AND c.id = o.currency_id
           WHERE rl.workspace_id = $1 AND rl.recognition_id = ANY($2)
           ORDER BY rl.recognition_id, ol.position, ol.id"#,
        ws,
        ids
    )
    .fetch_all(&mut *conn)
    .await?;
    let items = lines
        .into_iter()
        .enumerate()
        .map(|(n, l)| ItemRow {
            index: n,
            kind: RowKind::New(ItemFields {
                title: l.title,
                description: None,
                quantity: l.quantity,
                price: Some(Decimal::new(
                    l.unit_price,
                    u32::try_from(l.decimals).unwrap_or(2),
                )),
                vat: l.vat_rate,
                discount: Some(l.discount_percentage),
                purchase_price: None,
                position: i32::try_from(n + 1).ok(),
                product_id: Some(Some(l.product_id)),
                product_variant_id: Some(l.product_variant_id),
                mutate_from_warehouse_id: Some(None),
            }),
        })
        .collect();
    let input = InvoiceInput {
        relation_id: Some(relation),
        currency_id: Some(Some(currency)),
        addressed_contact_id: contact.map(|c| Some(Some(c))),
        reference: Some(Some(reference.to_owned())),
        note: None,
        payment_method: None,
        is_credit: false,
        due_date: None,
        billing_address: None,
        customer_ref: None,
        meta: Some(Some(json!({ "order_id": order, "recognition_ids": ids }))),
        items,
    };
    let invoice = invoices::service::create_in(conn, ws, by, input).await?;
    sqlx::query!(
        "INSERT INTO invoice_recognition_links (workspace_id, invoice_id, recognition_id)
         SELECT $1, $2, u FROM UNNEST($3::uuid[]) AS u",
        ws,
        invoice,
        ids
    )
    .execute(&mut *conn)
    .await
    .map_err(|e| match e.as_database_error().and_then(|d| d.code()) {
        Some(c) if c == "23505" => AppError::Conflict(BILLED),
        _ => e.into(),
    })?;
    Ok(invoice)
}

/// Whether draft `invoice` bills recognitions (its lines and party are frozen).
pub async fn linked(conn: &mut PgConnection, ws: Uuid, invoice: Uuid) -> Result<bool, AppError> {
    Ok(sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM invoice_recognition_links
                          WHERE workspace_id = $1 AND invoice_id = $2) AS "x!""#,
        ws,
        invoice
    )
    .fetch_one(conn)
    .await?)
}

/// 422 when a linked draft would change lines, party or currency (repricing).
pub async fn refuse_repricing(
    conn: &mut PgConnection,
    ws: Uuid,
    invoice: Uuid,
    changes: bool,
) -> Result<(), AppError> {
    if changes && linked(conn, ws, invoice).await? {
        return Err(AppError::Unprocessable(LINKED.into()));
    }
    Ok(())
}

/// A cancelled draft gives its recognitions back.
pub async fn release(conn: &mut PgConnection, ws: Uuid, invoice: Uuid) -> Result<(), AppError> {
    sqlx::query!(
        "DELETE FROM invoice_recognition_links
         WHERE workspace_id = $1 AND invoice_id = $2 AND state = 'draft'",
        ws,
        invoice
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// One linked recognition at issue: its unbilled item and what is still unpaid.
struct Unpaid {
    recognition_id: Uuid,
    item: Uuid,
    residual: i64,
    relation_id: Option<Uuid>,
}

/// What issuing `invoice` does in an active book, read without writing.
pub struct IssuePlan {
    /// Linked recognitions with their unpaid residual (0 = document only).
    pub reclassify: Vec<(Uuid, i64)>,
    /// Standalone recognition (net, tax) when nothing is linked.
    pub standalone: Option<(i64, i64)>,
}

async fn unpaid(conn: &mut PgConnection, ws: Uuid, invoice: Uuid) -> Result<Vec<Unpaid>, AppError> {
    Ok(sqlx::query_as!(
        Unpaid,
        r#"SELECT l.recognition_id, i.id AS item, i.residual, i.relation_id
           FROM invoice_recognition_links l
           JOIN accounting_open_items i ON i.workspace_id = l.workspace_id
                AND i.recognition_id = l.recognition_id AND i.account_role = 'unbilled_receivables'
           WHERE l.workspace_id = $1 AND l.invoice_id = $2 AND l.state = 'draft'
           ORDER BY l.recognition_id
           FOR UPDATE OF l"#,
        ws,
        invoice
    )
    .fetch_all(conn)
    .await?)
}

/// (net, tax) of a standalone invoice from its stored totals (tax = total − subtotal, SQL).
async fn standalone(
    conn: &mut PgConnection,
    ws: Uuid,
    invoice: Uuid,
) -> Result<(i64, i64), AppError> {
    let r = sqlx::query!(
        r#"SELECT subtotal AS "net!", (total - subtotal) AS "tax!" FROM invoices
           WHERE workspace_id = $1 AND id = $2"#,
        ws,
        invoice
    )
    .fetch_one(conn)
    .await?;
    Ok((r.net, r.tax))
}

/// The issue plan of draft `invoice` (locked by the caller); 422 for a credit note or a
/// currency other than the functional one.
pub async fn plan(
    conn: &mut PgConnection,
    ws: Uuid,
    invoice: Uuid,
    (currency, is_credit): (Uuid, bool),
) -> Result<IssuePlan, AppError> {
    if is_credit {
        return Err(unsupported(CREDIT));
    }
    recognition::check(&mut *conn, ws, currency).await?;
    let linked = unpaid(&mut *conn, ws, invoice).await?;
    if !linked.is_empty() {
        return Ok(IssuePlan {
            reclassify: linked
                .iter()
                .map(|u| (u.recognition_id, u.residual))
                .collect(),
            standalone: None,
        });
    }
    Ok(IssuePlan {
        reclassify: Vec::new(),
        standalone: Some(standalone(conn, ws, invoice).await?),
    })
}

/// Issue's economics in an active book, in the issue transaction before numbering. Effect
/// `invoice/<id>/issued` (none for a document-only issue).
pub async fn issue(
    conn: &mut PgConnection,
    ws: Uuid,
    by: Uuid,
    (invoice, relation, currency, is_credit): (Uuid, Uuid, Uuid, bool),
    date: NaiveDate,
) -> Result<(), AppError> {
    if is_credit {
        return Err(unsupported(CREDIT));
    }
    recognition::check(&mut *conn, ws, currency).await?;
    let linked = unpaid(&mut *conn, ws, invoice).await?;
    if linked.is_empty() {
        return recognise_standalone(conn, ws, by, (invoice, relation, currency), date).await;
    }
    let open: Vec<&Unpaid> = linked.iter().filter(|u| u.residual > 0).collect();
    if !open.is_empty() {
        let fingerprint: [u8; 32] = Sha256::digest(
            json!({ "issue": invoice,
                    "bill": open.iter().map(|u| json!([u.recognition_id, u.residual])).collect::<Vec<_>>() })
                .to_string(),
        )
        .into();
        let header = SourceHeader {
            source_kind: "invoice",
            source_id: invoice,
            effect_kind: "issued",
            date,
            reason: "invoice issued",
            fingerprint: &fingerprint,
            by,
        };
        let lines: Vec<SourceLine> = open
            .iter()
            .flat_map(|u| {
                [
                    SourceLine {
                        role: "receivables",
                        debit: u.residual,
                        credit: 0,
                        relation_id: u.relation_id,
                        account: None,
                    },
                    SourceLine {
                        role: "unbilled_receivables",
                        debit: 0,
                        credit: u.residual,
                        relation_id: u.relation_id,
                        account: None,
                    },
                ]
            })
            .collect();
        let (_, effect) = post_source_lines(&mut *conn, ws, &header, &lines).await?;
        for u in &open {
            // Billed item first, for exactly the residual; then the reclassification.
            let billed = open_receivable(
                &mut *conn,
                ws,
                &Receivable {
                    role: "receivables",
                    source: ("invoice_billing", u.recognition_id),
                    relation: u.relation_id,
                    order: None,
                    invoice: Some(invoice),
                    recognition: Some(u.recognition_id),
                    currency,
                    date,
                    amount: u.residual,
                    effect,
                    by,
                },
            )
            .await?;
            settlement::allocate(
                &mut *conn,
                ws,
                &NewAllocation {
                    kind: "reclassification",
                    source_item_id: Some(u.item),
                    target_item_id: billed,
                    date,
                    currency_id: currency,
                    amount: u.residual,
                    effect_id: effect,
                    by,
                },
            )
            .await?;
        }
        settlement::link_source(
            conn,
            ws,
            "invoice",
            invoice,
            Some(effect),
            "posted",
            &fingerprint,
        )
        .await?;
    }
    sqlx::query!(
        "UPDATE invoice_recognition_links SET state = 'issued', issued_at = now()
         WHERE workspace_id = $1 AND invoice_id = $2 AND state = 'draft'",
        ws,
        invoice
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// A standalone invoice (no recognised order sale) recognises on issue: DR receivables / CR
/// revenue / CR tax_payable, a billed receivable item, source `invoice`.
async fn recognise_standalone(
    conn: &mut PgConnection,
    ws: Uuid,
    by: Uuid,
    (invoice, relation, currency): (Uuid, Uuid, Uuid),
    date: NaiveDate,
) -> Result<(), AppError> {
    let (net, tax) = standalone(&mut *conn, ws, invoice).await?;
    let gross = sqlx::query_scalar!(r#"SELECT ($1::int8 + $2::int8) AS "g!""#, net, tax)
        .fetch_one(&mut *conn)
        .await?;
    // shortcut: a zero or negative total recognises nothing (credits come later).
    if gross <= 0 || net < 0 || tax < 0 {
        return Ok(());
    }
    let fingerprint: [u8; 32] = Sha256::digest(
        json!({ "source": ["invoice", invoice], "net": net, "tax": tax }).to_string(),
    )
    .into();
    let header = SourceHeader {
        source_kind: "invoice",
        source_id: invoice,
        effect_kind: "issued",
        date,
        reason: "sale",
        fingerprint: &fingerprint,
        by,
    };
    let lines: Vec<SourceLine> = [
        ("receivables", gross, 0, Some(relation)),
        ("revenue", 0, net, None),
        ("tax_payable", 0, tax, None),
    ]
    .into_iter()
    .filter(|l| l.1 > 0 || l.2 > 0)
    .map(|(role, debit, credit, relation_id)| SourceLine {
        role,
        debit,
        credit,
        relation_id,
        account: None,
    })
    .collect();
    let (entry, effect) = post_source_lines(&mut *conn, ws, &header, &lines).await?;
    // shortcut: no recognition lines for invoice items yet (item-level net/tax isn't stored).
    let recognition = sqlx::query_scalar!(
        "INSERT INTO sales_recognitions (workspace_id, source_kind, source_id, invoice_id,
             relation_id, currency_id, accounting_date, net, tax, functional_net, functional_tax,
             journal_entry_id, effect_id, created_by)
         VALUES ($1, 'invoice', $2, $2, $3, $4, $5, $6, $7, $6, $7, $8, $9, $10) RETURNING id",
        ws,
        invoice,
        relation,
        currency,
        date,
        net,
        tax,
        entry,
        effect,
        by
    )
    .fetch_one(&mut *conn)
    .await?;
    open_receivable(
        &mut *conn,
        ws,
        &Receivable {
            role: "receivables",
            source: ("invoice", invoice),
            relation: Some(relation),
            order: None,
            invoice: Some(invoice),
            recognition: Some(recognition),
            currency,
            date,
            amount: gross,
            effect,
            by,
        },
    )
    .await?;
    settlement::link_source(
        conn,
        ws,
        "invoice",
        invoice,
        Some(effect),
        "posted",
        &fingerprint,
    )
    .await?;
    Ok(())
}
