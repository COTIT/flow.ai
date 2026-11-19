//! Sale recognition in active books (masterplan §12 RECOGNITION): amounts per order line in
//! SQL (cumulative share of the line by shipped quantity, so the shipment reaching the full
//! quantity takes the exact rest), one immutable recognition with its lines, and its source
//! journal DR unbilled_receivables / CR revenue / CR tax_payable. Till sales add the GL
//! receipt DR cash|provider_clearing / CR unbilled_receivables. Functional-currency orders
//! only; foreign ones are refused before any effect.

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
            repo as accounting,
            service::unsupported,
            settlement::{self, NewAllocation, NewOpenItem, Side},
        },
        orders::repo::OrderRow,
        purchasing::service::amount,
        shipments::dto::RecognitionPreview,
        stock::value,
    },
};

const FOREIGN: &str =
    "Sales in a currency other than the book's functional currency can't be recognised yet.";

/// One recognised order line, with the recognition totals (same on every row).
struct Amount {
    order_line_id: Uuid,
    product_id: Uuid,
    quantity: Decimal,
    vat_rate: Decimal,
    discount: Decimal,
    net: i64,
    tax: i64,
    total_net: i64,
    total_tax: i64,
    total_gross: i64,
}

/// Whether sales recognise in this book (shared gate taken); 422
/// `unsupported_active_erp_operation` for a currency other than the functional one.
pub async fn check(conn: &mut PgConnection, ws: Uuid, currency: Uuid) -> Result<bool, AppError> {
    if !value::active(&mut *conn, ws).await? {
        return Ok(false);
    }
    let functional = accounting::settings(&mut *conn, ws, false)
        .await?
        .functional_currency_id;
    if functional != Some(currency) {
        return Err(unsupported(FOREIGN));
    }
    Ok(true)
}

/// 422 on `field` unless `date` is the book's business day: recognition and its COGS are
/// one fulfilment and share one date; nothing is backdated or postdated.
pub async fn check_date(
    conn: &mut PgConnection,
    ws: Uuid,
    date: NaiveDate,
    field: &str,
) -> Result<(), AppError> {
    let today = crate::modules::shipments::repo::book_today(conn, ws).await?;
    if date != today {
        return Err(AppError::field(
            field,
            format!("The accounting date must be today in the book's timezone ({today})."),
        ));
    }
    Ok(())
}

/// Amounts of shipping `lines` (order line, quantity) now: the line's share up to the
/// already posted shipments plus this quantity, less the share up to the posted ones.
async fn amounts(
    conn: &mut PgConnection,
    ws: Uuid,
    lines: &[(Uuid, Decimal)],
) -> Result<Vec<Amount>, AppError> {
    let ids: Vec<Uuid> = lines.iter().map(|l| l.0).collect();
    let qty: Vec<Decimal> = lines.iter().map(|l| l.1).collect();
    Ok(sqlx::query!(
        r#"WITH l AS (
               SELECT ol.id, ol.product_id, u.q, ol.vat_rate, ol.discount_percentage,
                      ol.quantity AS whole, ol.line_excl, ol.line_vat,
                      coalesce((SELECT sum(sl.quantity) FROM shipment_lines sl
                                JOIN shipments s ON s.workspace_id = sl.workspace_id
                                                AND s.id = sl.shipment_id AND s.state = 'posted'
                                WHERE sl.workspace_id = $1 AND sl.order_line_id = ol.id), 0) AS before
               FROM UNNEST($2::uuid[], $3::numeric[]) AS u(id, q)
               JOIN order_lines ol ON ol.workspace_id = $1 AND ol.id = u.id
           ), m AS (
               SELECT l.*,
                      (money_round_div(line_excl * (before + q), whole)
                       - money_round_div(line_excl * before, whole))::bigint AS net,
                      -- Active-book lines: Σ each tax component's own share (credits reverse
                      -- the same components, M6-A); else the line tax's share.
                      coalesce((SELECT sum(money_round_div(t.tax * (before + q), whole)
                                           - money_round_div(t.tax * before, whole))
                                FROM order_line_taxes t
                                WHERE t.workspace_id = $1 AND t.order_line_id = l.id),
                               money_round_div(line_vat * (before + q), whole)
                               - money_round_div(line_vat * before, whole))::bigint AS tax
               FROM l
           )
           SELECT id AS "order_line_id!", product_id AS "product_id!", q AS "quantity!",
                  vat_rate AS "vat_rate!", discount_percentage AS "discount!",
                  net AS "net!", tax AS "tax!",
                  (sum(net) OVER ())::bigint AS "total_net!",
                  (sum(tax) OVER ())::bigint AS "total_tax!",
                  (sum(net + tax) OVER ())::bigint AS "total_gross!"
           FROM m ORDER BY id"#,
        ws,
        &ids,
        &qty
    )
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .map(|r| Amount {
        order_line_id: r.order_line_id,
        product_id: r.product_id,
        quantity: r.quantity,
        vat_rate: r.vat_rate,
        discount: r.discount,
        net: r.net,
        tax: r.tax,
        total_net: r.total_net,
        total_tax: r.total_tax,
        total_gross: r.total_gross,
    })
    .collect())
}

/// What shipping `lines` of `o` recognises; None in a setup book.
pub async fn preview(
    conn: &mut PgConnection,
    ws: Uuid,
    o: &OrderRow,
    lines: &[(Uuid, Decimal)],
) -> Result<Option<RecognitionPreview>, AppError> {
    if !check(&mut *conn, ws, o.currency_id).await? {
        return Ok(None);
    }
    let a = amounts(conn, ws, lines).await?;
    let (net, tax, gross) = a
        .first()
        .map_or((0, 0, 0), |t| (t.total_net, t.total_tax, t.total_gross));
    Ok(Some(RecognitionPreview {
        net: amount(net, o.decimals),
        tax: amount(tax, o.decimals),
        gross: amount(gross, o.decimals),
    }))
}

/// Where a recognition comes from.
struct Source {
    kind: &'static str,
    id: Uuid,
    order: Uuid,
    shipment: Option<Uuid>,
    relation: Option<Uuid>,
    currency: Uuid,
}

/// A written recognition.
struct Recorded {
    /// Its unbilled receivable open item.
    item: Uuid,
    gross: i64,
}

/// The account a mapped `role` posts to (422 `accounting_not_configured` unmapped).
pub async fn role_account(conn: &mut PgConnection, ws: Uuid, role: &str) -> Result<Uuid, AppError> {
    accounting::mappings(&mut *conn, ws)
        .await?
        .into_iter()
        .find(|m| m.role == role)
        .map(|m| m.account_id)
        .ok_or_else(crate::modules::accounting::service::not_configured)
}

/// A receivable open item to open in the functional currency.
pub struct Receivable<'a> {
    /// `receivables` (billed) or `unbilled_receivables`.
    pub role: &'a str,
    pub source: (&'a str, Uuid),
    pub relation: Option<Uuid>,
    pub order: Option<Uuid>,
    pub invoice: Option<Uuid>,
    pub recognition: Option<Uuid>,
    pub currency: Uuid,
    pub date: NaiveDate,
    pub amount: i64,
    pub effect: Uuid,
    pub by: Uuid,
}

/// Open receivable `r` (debit side); its id.
pub async fn open_receivable(
    conn: &mut PgConnection,
    ws: Uuid,
    r: &Receivable<'_>,
) -> Result<Uuid, AppError> {
    let account_id = role_account(&mut *conn, ws, r.role).await?;
    settlement::open_item(
        conn,
        ws,
        &NewOpenItem {
            classification: "receivable",
            side: "debit",
            account_role: r.role,
            account_id,
            source_kind: r.source.0,
            source_id: r.source.1,
            relation_id: r.relation,
            order_id: r.order,
            invoice_id: r.invoice,
            recognition_id: r.recognition,
            currency_id: r.currency,
            issue_date: r.date,
            due_date: None,
            origin: "posted",
            amount: r.amount,
            functional_amount: r.amount,
            effect_id: r.effect,
            by: r.by,
        },
    )
    .await
}

/// Post the recognition journal, write the recognition, open its unbilled receivable and
/// link the source; None when there is nothing to recognise (nothing written).
async fn record(
    conn: &mut PgConnection,
    ws: Uuid,
    src: &Source,
    a: &[Amount],
    (date, by): (NaiveDate, Uuid),
) -> Result<Option<Recorded>, AppError> {
    let Some(t) = a.first() else { return Ok(None) };
    // shortcut: free goods recognise nothing (a journal needs a non-zero total).
    if t.total_gross == 0 {
        return Ok(None);
    }
    let fingerprint: [u8; 32] = Sha256::digest(
        json!({ "source": [src.kind, src.id], "net": t.total_net, "tax": t.total_tax }).to_string(),
    )
    .into();
    let header = SourceHeader {
        source_kind: src.kind,
        source_id: src.id,
        effect_kind: "fulfilled",
        date,
        reason: "sale",
        fingerprint: &fingerprint,
        by,
    };
    let lines: Vec<SourceLine> = [
        ("unbilled_receivables", t.total_gross, 0, src.relation),
        ("revenue", 0, t.total_net, None),
        ("tax_payable", 0, t.total_tax, None),
    ]
    .into_iter()
    .filter(|l| l.1 > 0 || l.2 > 0)
    .map(|(role, debit, credit, relation_id)| SourceLine {
        side: Side::Role(role),
        debit,
        credit,
        relation_id,
        bank_account_id: None,
    })
    .collect();
    let (entry, effect) = post_source_lines(&mut *conn, ws, &header, &lines).await?;
    let ids: Vec<Uuid> = a.iter().map(|l| l.order_line_id).collect();
    let products: Vec<Uuid> = a.iter().map(|l| l.product_id).collect();
    let qty: Vec<Decimal> = a.iter().map(|l| l.quantity).collect();
    let net: Vec<i64> = a.iter().map(|l| l.net).collect();
    let tax: Vec<i64> = a.iter().map(|l| l.tax).collect();
    let vat: Vec<Decimal> = a.iter().map(|l| l.vat_rate).collect();
    let discount: Vec<Decimal> = a.iter().map(|l| l.discount).collect();
    let recognition = sqlx::query_scalar!(
        "WITH r AS (
             INSERT INTO sales_recognitions (workspace_id, source_kind, source_id, order_id,
                 shipment_id, relation_id, currency_id, accounting_date, net, tax,
                 functional_net, functional_tax, journal_entry_id, effect_id, created_by)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $9, $10, $11, $12, $13)
             RETURNING id)
         , l AS (
             INSERT INTO sales_recognition_lines (workspace_id, recognition_id, order_line_id,
                 product_id, quantity, net, tax, vat_rate, discount_percentage)
             SELECT $1, r.id, u.l, u.p, u.q, u.n, u.t, u.v, u.d
             FROM r, UNNEST($14::uuid[], $15::uuid[], $16::numeric[], $17::int8[], $18::int8[],
                            $19::numeric[], $20::numeric[]) AS u(l, p, q, n, t, v, d))
         SELECT id AS \"id!\" FROM r",
        ws,
        src.kind,
        src.id,
        src.order,
        src.shipment,
        src.relation,
        src.currency,
        date,
        t.total_net,
        t.total_tax,
        entry,
        effect,
        by,
        &ids,
        &products,
        &qty,
        &net,
        &tax,
        &vat,
        &discount
    )
    .fetch_one(&mut *conn)
    .await?;
    let item = open_receivable(
        &mut *conn,
        ws,
        &Receivable {
            role: "unbilled_receivables",
            source: (src.kind, src.id),
            relation: src.relation,
            order: Some(src.order),
            invoice: None,
            recognition: Some(recognition),
            currency: src.currency,
            date,
            amount: t.total_gross,
            effect,
            by,
        },
    )
    .await?;
    settlement::link_source(
        conn,
        ws,
        src.kind,
        src.id,
        Some(effect),
        "posted",
        &fingerprint,
    )
    .await?;
    Ok(Some(Recorded {
        item,
        gross: t.total_gross,
    }))
}

/// Recognise a posting shipment of `o` (checked by [`check`]); effect
/// `shipment/<id>/fulfilled`.
pub async fn shipment(
    conn: &mut PgConnection,
    ws: Uuid,
    o: &OrderRow,
    (id, lines): (Uuid, &[(Uuid, Decimal)]),
    at: (NaiveDate, Uuid),
) -> Result<(), AppError> {
    let a = amounts(&mut *conn, ws, lines).await?;
    let src = Source {
        kind: "shipment",
        id,
        order: o.id,
        shipment: Some(id),
        relation: o.relation_id,
        currency: o.currency_id,
    };
    record(conn, ws, &src, &a, at).await?;
    Ok(())
}

/// Recognise an anonymous till sale (order `order`, every line in full) and post its GL
/// receipt of `due` into `money_role`: effects `retail_sale/<order>/fulfilled` and
/// `retail_sale/<order>/received`. No cash ledger row (the sale row is the drawer's).
pub async fn retail_sale(
    conn: &mut PgConnection,
    ws: Uuid,
    (order, currency): (Uuid, Uuid),
    (due, money_role): (i64, &'static str),
    lines: &[(Uuid, Decimal)],
) -> Result<(), AppError> {
    if !check(&mut *conn, ws, currency).await? {
        return Ok(());
    }
    let date = crate::modules::shipments::repo::book_today(&mut *conn, ws).await?;
    // shortcut: nil = system actor (till), as stock valuation; actor_kind comes with M4.
    let by = Uuid::nil();
    let a = amounts(&mut *conn, ws, lines).await?;
    let src = Source {
        kind: "retail_sale",
        id: order,
        order,
        shipment: None,
        relation: None,
        currency,
    };
    let recorded = record(&mut *conn, ws, &src, &a, (date, by)).await?;
    let gross = recorded.as_ref().map_or(0, |r| r.gross);
    if due == 0 && gross == 0 {
        return Ok(());
    }
    // Cash rounding (due ≠ gross) goes to the rounding role.
    let r = sqlx::query!(
        r#"SELECT greatest($2::int8 - $1::int8, 0)::int8 AS "rounding_debit!",
                  greatest($1::int8 - $2::int8, 0)::int8 AS "rounding_credit!""#,
        due,
        gross
    )
    .fetch_one(&mut *conn)
    .await?;
    let fingerprint: [u8; 32] =
        Sha256::digest(json!({ "receipt": order, "due": due, "role": money_role }).to_string())
            .into();
    let header = SourceHeader {
        source_kind: "retail_sale",
        source_id: order,
        effect_kind: "received",
        date,
        reason: "sale receipt",
        fingerprint: &fingerprint,
        by,
    };
    let lines: Vec<SourceLine> = [
        (money_role, due, 0),
        ("unbilled_receivables", 0, gross),
        ("rounding", r.rounding_debit, r.rounding_credit),
    ]
    .into_iter()
    .filter(|l| l.1 > 0 || l.2 > 0)
    .map(|(role, debit, credit)| SourceLine {
        side: Side::Role(role),
        debit,
        credit,
        relation_id: None,
        bank_account_id: None,
    })
    .collect();
    let (_, effect) = post_source_lines(&mut *conn, ws, &header, &lines).await?;
    let Some(rec) = recorded else { return Ok(()) };
    // The receipt settles the retail item: a customer credit for the gross (the open-item
    // class for money received from a party) applied at once, so both close to 0.
    // shortcut: customer_credit sits on role receivables while the GL credit hits unbilled;
    // both residuals are 0 at commit, so per-role subledgers still match the GL.
    let account_id = role_account(&mut *conn, ws, "receivables").await?;
    let credit = settlement::open_item(
        &mut *conn,
        ws,
        &NewOpenItem {
            classification: "customer_credit",
            side: "credit",
            account_role: "receivables",
            account_id,
            source_kind: "retail_receipt",
            source_id: order,
            relation_id: None,
            order_id: Some(order),
            invoice_id: None,
            recognition_id: None,
            currency_id: currency,
            issue_date: date,
            due_date: None,
            origin: "posted",
            amount: gross,
            functional_amount: gross,
            effect_id: effect,
            by,
        },
    )
    .await?;
    settlement::allocate(
        conn,
        ws,
        &NewAllocation {
            kind: "application",
            source_item_id: Some(credit),
            target_item_id: rec.item,
            date,
            currency_id: currency,
            amount: gross,
            effect_id: effect,
            by,
        },
    )
    .await?;
    Ok(())
}
