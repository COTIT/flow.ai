//! Order rules: pricing lines from the catalog through the pricing engine (integer minor
//! units), numbering, and the status machine with its stock effects. Every write locks the
//! order row first, then stock through `stock::reserve` (product order), so two orders never
//! deadlock. Wrong status = 409.

use std::collections::HashMap;

use rust_decimal::{Decimal, prelude::ToPrimitive};
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    idempotency::{self, Actor, Answer, Key},
    modules::{
        currencies, farm_supply, invoice_payments,
        invoices::{
            self,
            dto::{InvoiceInput, ItemFields, ItemRow, RowKind},
        },
        orders::{
            dto::{
                LineInput, LinesQuantity, OrderAction, OrderCreated, OrderInput, OrderLineResource,
                OrderPatch, OrderResource,
            },
            payments::{Paid, paid_totals},
            repo::{self, LineRow, NewLine, OrderRow},
        },
        pricing::engine,
        products, settings,
        stock::{
            repo::Cause,
            reserve::{self, Reserved, Want},
        },
        webhooks::events,
    },
    numbering::{self, Appendix, Series},
};

const NO_CURRENCY: &str = "No default currency has been found. Please create one";
const NO_WAREHOUSE: &str = "No main warehouse has been set. Choose a warehouse.";
const OTHER_CURRENCY: &str = "This product is priced in another currency.";
const NOT_DRAFT: &str = "Only a draft order can change this.";
const NOT_OPEN: &str = "Only an open order can change this.";
const NOT_PICKED: &str = "Pick every stock line in full before completing.";
const NOT_CANCELLABLE: &str = "This order can no longer be cancelled.";
const NOT_RETURNABLE: &str = "Only a completed order takes returns.";
const NOT_INVOICEABLE: &str = "Only an open or completed order can be invoiced.";
const INVOICED: &str = "This order already has an invoice.";
const NO_RELATION: &str = "Set a relation on the order before invoicing it.";
const NOTHING_TO_REFUND: &str = "Nothing paid on this order is left to refund.";
/// Cancelling a paid order needs its payment refunded first.
pub const REFUND_REQUIRED: &str = "refund_required";

fn conflict(msg: &'static str) -> AppError {
    AppError::Conflict(msg)
}

fn field(key: String, msg: impl Into<String>) -> AppError {
    AppError::field(&key, msg)
}

/// `d` (already rounded to `dp`) as minor units.
fn minor(d: Decimal, dp: u32) -> Option<i64> {
    let mut d = d;
    d.rescale(dp);
    i64::try_from(d.mantissa()).ok()
}

/// Lines priced in minor units, the order totals and the per-rate summary.
pub struct Priced {
    pub lines: Vec<NewLine>,
    pub subtotal: i64,
    pub vat: i64,
    pub total: i64,
    pub summary: Value,
}

/// Price `lines` in the order's currency: catalog price (the variant's when given) unless the
/// office sent one, catalog VAT, then the pricing engine. `web` refuses unsellable products.
pub async fn price(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    currency_id: Uuid,
    dp: u32,
    lines: &[LineInput],
    web: bool,
) -> Result<Priced, AppError> {
    let ids: Vec<Uuid> = lines.iter().map(|l| l.product_id).collect();
    let found: HashMap<Uuid, repo::CatalogProduct> = repo::products(&mut *conn, workspace_id, &ids)
        .await?
        .into_iter()
        .map(|p| (p.id, p))
        .collect();
    let vids: Vec<Uuid> = lines.iter().filter_map(|l| l.product_variant_id).collect();
    let variants: HashMap<Uuid, repo::CatalogVariant> =
        repo::variants(&mut *conn, workspace_id, &vids)
            .await?
            .into_iter()
            .map(|v| (v.id, v))
            .collect();
    let scale = Decimal::from(10_i64.pow(dp));
    let mut errors = std::collections::BTreeMap::<String, Vec<String>>::new();
    let mut add = |k: String, m: &str| errors.entry(k).or_default().push(m.to_owned());
    let mut priced = Vec::new();
    let mut engine_lines = Vec::new();
    for (n, l) in lines.iter().enumerate() {
        let Some(p) = found.get(&l.product_id).filter(|p| !web || p.sellable) else {
            add(
                format!("lines.{n}.product_id"),
                "The selected product is invalid.",
            );
            continue;
        };
        if p.currency_id != currency_id {
            add(format!("lines.{n}.product_id"), OTHER_CURRENCY);
            continue;
        }
        let variant = match l.product_variant_id {
            Some(v) => match variants.get(&v).filter(|v| v.product_id == p.id) {
                Some(v) => Some(v),
                None => {
                    add(
                        format!("lines.{n}.product_variant_id"),
                        "The selected variant is invalid.",
                    );
                    continue;
                }
            },
            None => None,
        };
        let catalog = variant.map_or(p.price, |v| v.price);
        let unit = match l.unit_price {
            Some(u) => Some(u),
            None => (catalog * scale).round().to_i64(),
        };
        let Some(unit) = unit else {
            add(
                format!("lines.{n}.unit_price"),
                "The price is out of range.",
            );
            continue;
        };
        let title =
            l.title
                .clone()
                .unwrap_or_else(|| match variant.and_then(|v| v.sku.as_deref()) {
                    Some(sku) => format!("{} ({sku})", p.name),
                    None => p.name.clone(),
                });
        engine_lines.push(engine::Line {
            quantity: l.quantity,
            price: Decimal::new(unit, dp),
            vat: p.vat,
            discount: l.discount,
            purchase_price: None,
            set_child: false,
        });
        priced.push(NewLine {
            product_id: p.id,
            product_variant_id: variant.map(|v| v.id),
            title,
            quantity: l.quantity,
            unit_price: unit,
            vat_rate: p.vat,
            discount: l.discount,
            line_excl: 0,
            line_vat: 0,
            tracked: p.has_stock,
        });
    }
    if !errors.is_empty() {
        return Err(AppError::Validation(errors));
    }
    let out_of_range = |n: usize| {
        field(
            format!("lines.{n}"),
            format!("The lines.{n} is out of range."),
        )
    };
    let s = engine::summarise(&engine_lines, dp).map_err(out_of_range)?;
    for (n, (line, a)) in priced.iter_mut().zip(&s.lines).enumerate() {
        line.line_excl = minor(a.excl, dp).ok_or_else(|| out_of_range(n))?;
        line.line_vat = minor(a.line_vat, dp).ok_or_else(|| out_of_range(n))?;
    }
    let last = priced.len().saturating_sub(1);
    let m = |d: Decimal| minor(d, dp).ok_or_else(|| out_of_range(last));
    let vats = s
        .vats
        .iter()
        .map(|v| {
            Ok(json!({ "rate": v.rate.normalize().to_string(), "base": m(v.base)?, "vat": m(v.vat)? }))
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    Ok(Priced {
        lines: priced,
        subtotal: m(s.excl)?,
        vat: m(s.vat)?,
        total: m(s.total)?,
        summary: json!({ "vats": vats }),
    })
}

/// Currency id and decimals: the sent one or the workspace's main currency.
pub async fn currency(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    sent: Option<Uuid>,
) -> Result<(Uuid, u32), AppError> {
    let id = match sent {
        Some(id) => id,
        None => products::repo::main_currency(&mut *conn, workspace_id)
            .await?
            .ok_or_else(|| AppError::field("currency_id", NO_CURRENCY))?,
    };
    let row = currencies::repo::find(&mut *conn, workspace_id, id)
        .await?
        .ok_or_else(|| AppError::field("currency_id", "The selected currency id is invalid."))?;
    let dp =
        u32::try_from(row.decimals).map_err(|_| AppError::Internal("negative decimals".into()))?;
    Ok((id, dp))
}

/// The sent warehouse (live) or the main one.
pub async fn warehouse(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    sent: Option<Uuid>,
) -> Result<Uuid, AppError> {
    match sent {
        Some(id) if repo::warehouse_exists(&mut *conn, workspace_id, id).await? => Ok(id),
        Some(_) => Err(AppError::field(
            "warehouse_id",
            "The selected warehouse id is invalid.",
        )),
        None => repo::default_warehouse(&mut *conn, workspace_id)
            .await?
            .ok_or_else(|| AppError::field("warehouse_id", NO_WAREHOUSE)),
    }
}

async fn relation(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    id: Option<Uuid>,
) -> Result<(), AppError> {
    if let Some(id) = id
        && !crate::modules::pricing::repo::relation_exists(&mut *conn, workspace_id, id).await?
    {
        return Err(AppError::field(
            "relation_id",
            "The selected relation id is invalid.",
        ));
    }
    Ok(())
}

/// Next order number (`order` settings numbering). Takes the counter lock: call late.
pub async fn number(conn: &mut PgConnection, workspace_id: Uuid) -> Result<String, AppError> {
    let section = settings::service::section_or_default(&mut *conn, workspace_id, "order").await?;
    let n = numbering::next(
        conn,
        workspace_id,
        Series::Order,
        numbering::start_number(&section),
    )
    .await?;
    Ok(Appendix::from_section(&section).format(n))
}

/// `order.created`: ids, number, status and totals; never customer details.
pub async fn created_event(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    order_id: Uuid,
) -> Result<(), AppError> {
    let o = repo::find(&mut *conn, workspace_id, order_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let payload = json!({
        "order_id": o.id, "number": o.number, "status": o.status, "source": o.source,
        "currency": o.currency, "subtotal": o.subtotal, "vat": o.vat, "total": o.total,
    });
    events::emit(conn, workspace_id, events::ORDER_CREATED, payload).await?;
    Ok(())
}

fn cause<'a>(o: &'a OrderRow, reason: &'a str) -> Cause<'a> {
    Cause {
        reason: Some(reason),
        related: Some(("order", o.id)),
        ..Cause::default()
    }
}

/// Stock lines as wants (full quantities), with each want's line index.
fn wants(lines: &[LineRow], qty: impl Fn(&LineRow) -> Decimal) -> (Vec<Want>, Vec<usize>) {
    lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.tracked && qty(l) > Decimal::ZERO)
        .map(|(n, l)| {
            (
                Want {
                    product_id: l.product_id,
                    quantity: qty(l),
                },
                n,
            )
        })
        .unzip()
}

/// Hold the order's stock lines; 422 on the first short line.
pub async fn hold(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    o: &OrderRow,
    lines: &[LineRow],
) -> Result<(), AppError> {
    let (wants, index) = wants(lines, |l| l.quantity);
    match reserve::reserve(
        conn,
        workspace_id,
        o.warehouse_id,
        &wants,
        &cause(o, "order"),
    )
    .await?
    {
        Reserved::Done => Ok(()),
        Reserved::Short {
            index: i,
            available,
        } => Err(field(
            format!("lines.{}.quantity", index[i]),
            format!("Not enough stock: {} available.", available.normalize()),
        )),
    }
}

/// Give back the order's hold.
pub async fn unhold(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    o: &OrderRow,
    lines: &[LineRow],
) -> Result<(), AppError> {
    let (wants, _) = wants(lines, |l| l.quantity);
    reserve::release(
        conn,
        workspace_id,
        o.warehouse_id,
        &wants,
        &cause(o, "order"),
    )
    .await?;
    Ok(())
}

/// Why the API refuses `action` on `o` now, else Ok. Every action route runs it under the
/// order lock and `allowed_actions` lists the ones it passes, so the two can't disagree.
pub fn check(
    action: OrderAction,
    o: &OrderRow,
    lines: &[LineRow],
    paid: Paid,
) -> Result<(), AppError> {
    let need = |ok: bool, msg: &'static str| if ok { Ok(()) } else { Err(conflict(msg)) };
    let status = o.status.as_str();
    match action {
        OrderAction::Confirm => need(status == "draft", NOT_DRAFT),
        OrderAction::Pick => need(status == "open", NOT_OPEN),
        OrderAction::Complete => {
            need(status == "open", NOT_OPEN)?;
            let picked = lines.iter().all(|l| !l.tracked || l.picked == l.quantity);
            need(picked, NOT_PICKED)
        }
        OrderAction::Cancel => {
            let open = matches!(status, "draft" | "awaiting_payment" | "open");
            need(open, NOT_CANCELLABLE)?;
            need(o.paid_at.is_none() || paid.net <= 0, REFUND_REQUIRED)
        }
        OrderAction::Return => need(
            matches!(status, "completed" | "partially_returned"),
            NOT_RETURNABLE,
        ),
        OrderAction::Refund => need(paid.refundable > 0, NOTHING_TO_REFUND),
        OrderAction::Invoice => {
            let done = matches!(status, "completed" | "partially_returned" | "returned");
            need(status == "open" || done, NOT_INVOICEABLE)?;
            need(o.invoice_id.is_none(), INVOICED)?;
            match o.relation_id {
                Some(_) => Ok(()),
                None => Err(AppError::field("relation_id", NO_RELATION)),
            }
        }
    }
}

/// [`Paid`] of one order, read in `conn` (fresh under the order lock).
async fn paid_of(conn: &mut PgConnection, workspace_id: Uuid, id: Uuid) -> Result<Paid, AppError> {
    let mut m = paid_totals(conn, workspace_id, &[id]).await?;
    Ok(m.remove(&id).unwrap_or_default())
}

/// The order with its lines as a resource.
pub fn resource(o: OrderRow, lines: Vec<LineRow>, paid: Paid) -> OrderResource {
    let q = |d: Decimal| format!("{d:.2}");
    let allowed_actions = OrderAction::ALL
        .into_iter()
        .filter(|a| check(*a, &o, &lines, paid).is_ok())
        .collect();
    OrderResource {
        allowed_actions,
        id: o.id,
        number: o.number,
        status: o.status,
        source: o.source,
        relation_id: o.relation_id,
        currency_id: o.currency_id,
        currency: o.currency,
        decimals: o.decimals,
        warehouse_id: o.warehouse_id,
        customer: o.customer,
        reference: o.reference,
        note: o.note,
        subtotal: o.subtotal,
        vat: o.vat,
        total: o.total,
        summary: o.summary,
        expires_at: o.expires_at,
        paid_at: o.paid_at,
        completed_at: o.completed_at,
        cancelled_at: o.cancelled_at,
        invoice_id: o.invoice_id,
        lines: lines
            .into_iter()
            .map(|l| OrderLineResource {
                id: l.id,
                product_id: l.product_id,
                product_variant_id: l.product_variant_id,
                title: l.title,
                quantity: q(l.quantity),
                unit_price: l.unit_price,
                vat_rate: l.vat_rate.normalize().to_string(),
                discount_percentage: l.discount_percentage.normalize().to_string(),
                line_excl: l.line_excl,
                line_vat: l.line_vat,
                tracked: l.tracked,
                picked: q(l.picked),
                deducted: q(l.deducted),
                returned: q(l.returned),
            })
            .collect(),
        created_at: o.created_at,
        updated_at: o.updated_at,
    }
}

/// The order as `{data: ...}`, read in `conn`.
pub async fn data(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<Value, AppError> {
    let o = repo::find(&mut *conn, workspace_id, id)
        .await?
        .ok_or(AppError::NotFound)?;
    let lines = repo::lines(&mut *conn, workspace_id, &[id]).await?;
    let paid = paid_of(conn, workspace_id, id).await?;
    serde_json::to_value(json!({ "data": resource(o, lines, paid) }))
        .map_err(|e| AppError::Internal(e.to_string()))
}

fn ok_json(body: &Value) -> Answer {
    Answer {
        status: 200,
        body: body.to_string().into_bytes(),
    }
}

fn parse(id: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(id).map_err(|_| AppError::NotFound)
}

/// New office draft; 201 `{data: {order_id, number}}`.
pub async fn create(
    state: &AppState,
    workspace_id: Uuid,
    user_id: Uuid,
    input: OrderInput,
    key: Option<Key>,
) -> Result<Answer, AppError> {
    let mut tx = state.db.begin().await?;
    if let Some(a) =
        idempotency::replay(&mut tx, workspace_id, Actor::User(user_id), key.as_ref()).await?
    {
        return Ok(a);
    }
    let (currency_id, dp) = currency(&mut tx, workspace_id, input.currency_id).await?;
    let warehouse_id = warehouse(&mut tx, workspace_id, input.warehouse_id).await?;
    relation(&mut tx, workspace_id, input.relation_id).await?;
    let p = price(&mut tx, workspace_id, currency_id, dp, &input.lines, false).await?;
    let number = number(&mut tx, workspace_id).await?;
    let id = repo::insert(
        &mut tx,
        workspace_id,
        &repo::NewOrder {
            number: &number,
            status: "draft",
            source: "office",
            relation_id: input.relation_id,
            currency_id,
            warehouse_id,
            customer: None,
            reference: input.reference.as_deref(),
            note: input.note.as_deref(),
            expires_at: None,
            created_by: Some(user_id),
            app_key_id: None,
        },
    )
    .await?;
    repo::write_lines(
        &mut tx,
        workspace_id,
        id,
        &p.lines,
        (p.subtotal, p.vat, p.total),
        &p.summary,
    )
    .await?;
    created_event(&mut tx, workspace_id, id).await?;
    let answer = Answer::created(&json!({ "data": OrderCreated { order_id: id, number } }));
    idempotency::finish(&mut tx, workspace_id, key.as_ref(), &answer).await?;
    tx.commit().await?;
    Ok(answer)
}

/// One order with lines.
pub async fn get(
    state: &AppState,
    workspace_id: Uuid,
    id: &str,
) -> Result<OrderResource, AppError> {
    let id = parse(id)?;
    let o = repo::find(&state.db, workspace_id, id)
        .await?
        .ok_or(AppError::NotFound)?;
    let lines = repo::lines(&state.db, workspace_id, &[id]).await?;
    let paid = paid_totals(&state.db, workspace_id, &[id]).await?;
    Ok(resource(
        o,
        lines,
        paid.get(&id).copied().unwrap_or_default(),
    ))
}

/// A page of orders with their lines; total matches.
pub async fn list(
    state: &AppState,
    workspace_id: Uuid,
    status: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<OrderResource>, i64), AppError> {
    let (rows, mut total) = repo::list(&state.db, workspace_id, status, limit, offset).await?;
    if rows.is_empty() && offset > 0 {
        total = repo::count(&state.db, workspace_id, status).await?;
    }
    let ids: Vec<Uuid> = rows.iter().map(|o| o.id).collect();
    let mut by_order: HashMap<Uuid, Vec<LineRow>> = HashMap::new();
    for l in repo::lines(&state.db, workspace_id, &ids).await? {
        by_order.entry(l.order_id).or_default().push(l);
    }
    let paid = paid_totals(&state.db, workspace_id, &ids).await?;
    let data = rows
        .into_iter()
        .map(|o| {
            let lines = by_order.remove(&o.id).unwrap_or_default();
            let p = paid.get(&o.id).copied().unwrap_or_default();
            resource(o, lines, p)
        })
        .collect();
    Ok((data, total))
}

/// A state change under the order lock: replay, lock, `step`, answer the order.
async fn transition<F>(
    state: &AppState,
    workspace_id: Uuid,
    user_id: Uuid,
    id: &str,
    key: Option<Key>,
    step: F,
) -> Result<Answer, AppError>
where
    F: AsyncFnOnce(&mut PgConnection, OrderRow, Vec<LineRow>) -> Result<(), AppError>,
{
    let id = parse(id)?;
    let mut tx = state.db.begin().await?;
    if let Some(a) =
        idempotency::replay(&mut tx, workspace_id, Actor::User(user_id), key.as_ref()).await?
    {
        return Ok(a);
    }
    let o = repo::lock(&mut tx, workspace_id, id)
        .await?
        .ok_or(AppError::NotFound)?;
    let lines = repo::lines(&mut *tx, workspace_id, &[id]).await?;
    step(&mut tx, o, lines).await?;
    let answer = ok_json(&data(&mut tx, workspace_id, id).await?);
    idempotency::finish(&mut tx, workspace_id, key.as_ref(), &answer).await?;
    tx.commit().await?;
    Ok(answer)
}

/// PATCH: header any time before an invoice exists; lines and warehouse on drafts only.
pub async fn update(
    state: &AppState,
    workspace_id: Uuid,
    user_id: Uuid,
    id: &str,
    patch: OrderPatch,
    key: Option<Key>,
) -> Result<Answer, AppError> {
    transition(state, workspace_id, user_id, id, key, async |conn, o, _| {
        let draft = o.status == "draft";
        if !draft && (patch.lines.is_some() || patch.warehouse_id.is_some()) {
            return Err(conflict(NOT_DRAFT));
        }
        let relation_id = patch.relation_id.unwrap_or(o.relation_id);
        relation(conn, workspace_id, relation_id).await?;
        let warehouse_id = match patch.warehouse_id {
            Some(w) => warehouse(conn, workspace_id, Some(w)).await?,
            None => o.warehouse_id,
        };
        let reference = patch.reference.unwrap_or(o.reference.clone());
        let note = patch.note.unwrap_or(o.note.clone());
        repo::update_header(
            conn,
            workspace_id,
            o.id,
            relation_id,
            warehouse_id,
            reference.as_deref(),
            note.as_deref(),
        )
        .await?;
        if let Some(lines) = patch.lines {
            let dp = u32::try_from(o.decimals).unwrap_or(2);
            let p = price(conn, workspace_id, o.currency_id, dp, &lines, false).await?;
            repo::write_lines(
                conn,
                workspace_id,
                o.id,
                &p.lines,
                (p.subtotal, p.vat, p.total),
                &p.summary,
            )
            .await?;
        }
        Ok(())
    })
    .await
}

/// draft → open: reserve every stock line.
pub async fn confirm(
    state: &AppState,
    workspace_id: Uuid,
    user_id: Uuid,
    id: &str,
    key: Option<Key>,
) -> Result<Answer, AppError> {
    transition(
        state,
        workspace_id,
        user_id,
        id,
        key,
        async |conn, o, lines| {
            check(OrderAction::Confirm, &o, &lines, Paid::default())?;
            hold(conn, workspace_id, &o, &lines).await?;
            repo::set_status(conn, workspace_id, o.id, "open").await?;
            Ok(())
        },
    )
    .await
}

/// Record picked totals on an open order's lines.
pub async fn pick(
    state: &AppState,
    workspace_id: Uuid,
    user_id: Uuid,
    id: &str,
    input: LinesQuantity,
    key: Option<Key>,
) -> Result<Answer, AppError> {
    transition(
        state,
        workspace_id,
        user_id,
        id,
        key,
        async |conn, o, lines| {
            check(OrderAction::Pick, &o, &lines, Paid::default())?;
            let mut rows = Vec::new();
            for (n, (line_id, q)) in input.lines.iter().enumerate() {
                let Some(l) = lines.iter().find(|l| l.id == *line_id) else {
                    return Err(field(
                        format!("lines.{n}.line_id"),
                        "The selected line is invalid.",
                    ));
                };
                if *q > l.quantity {
                    return Err(field(
                        format!("lines.{n}.quantity"),
                        "The quantity must not be greater than the line's quantity.",
                    ));
                }
                rows.push((l.id, *q, l.deducted, l.returned));
            }
            repo::set_line_quantities(conn, workspace_id, &rows)
                .await
                .map_err(Into::into)
        },
    )
    .await
}

/// open → completed: take every stock line's hold off the shelf.
pub async fn complete(
    state: &AppState,
    workspace_id: Uuid,
    user_id: Uuid,
    id: &str,
    key: Option<Key>,
) -> Result<Answer, AppError> {
    transition(
        state,
        workspace_id,
        user_id,
        id,
        key,
        async |conn, o, lines| {
            check(OrderAction::Complete, &o, &lines, Paid::default())?;
            let (wants, _) = wants(&lines, |l| l.quantity);
            let reason = format!("order {}", o.number);
            reserve::commit(
                conn,
                workspace_id,
                o.warehouse_id,
                &wants,
                &cause(&o, &reason),
            )
            .await?;
            let rows: Vec<_> = lines
                .iter()
                .map(|l| (l.id, l.picked, l.quantity, l.returned))
                .collect();
            repo::set_line_quantities(conn, workspace_id, &rows).await?;
            farm_supply::sale::on_sale(conn, workspace_id, o.id, &o.number).await?;
            repo::set_status(conn, workspace_id, o.id, "completed").await?;
            Ok(())
        },
    )
    .await
}

/// draft, awaiting_payment or open → cancelled, giving the hold back. A paid order needs a
/// refund first (409 `refund_required`).
pub async fn cancel(
    state: &AppState,
    workspace_id: Uuid,
    user_id: Uuid,
    id: &str,
    key: Option<Key>,
) -> Result<Answer, AppError> {
    transition(
        state,
        workspace_id,
        user_id,
        id,
        key,
        async |conn, o, lines| cancel_locked(conn, workspace_id, &o, &lines).await,
    )
    .await
}

/// Cancel an order the caller holds locked.
pub async fn cancel_locked(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    o: &OrderRow,
    lines: &[LineRow],
) -> Result<(), AppError> {
    let paid = paid_of(conn, workspace_id, o.id).await?;
    check(OrderAction::Cancel, o, lines, paid)?;
    if o.status != "draft" {
        unhold(conn, workspace_id, o, lines).await?;
    }
    repo::set_status(conn, workspace_id, o.id, "cancelled").await?;
    Ok(())
}

/// Delete a draft.
pub async fn delete(
    state: &AppState,
    workspace_id: Uuid,
    user_id: Uuid,
    id: &str,
    key: Option<Key>,
) -> Result<Answer, AppError> {
    let id = parse(id)?;
    let mut tx = state.db.begin().await?;
    if let Some(a) =
        idempotency::replay(&mut tx, workspace_id, Actor::User(user_id), key.as_ref()).await?
    {
        return Ok(a);
    }
    let o = repo::lock(&mut tx, workspace_id, id)
        .await?
        .ok_or(AppError::NotFound)?;
    if o.status != "draft" {
        return Err(conflict(NOT_DRAFT));
    }
    repo::delete(&mut tx, workspace_id, id).await?;
    let answer = Answer::ok();
    idempotency::finish(&mut tx, workspace_id, key.as_ref(), &answer).await?;
    tx.commit().await?;
    Ok(answer)
}

/// Take back returned quantities of a completed order (`lines[].quantity` = returned now),
/// optionally putting them back on stock; partially_returned until every line is back.
pub async fn returns(
    state: &AppState,
    workspace_id: Uuid,
    user_id: Uuid,
    id: &str,
    input: LinesQuantity,
    key: Option<Key>,
) -> Result<Answer, AppError> {
    transition(
        state,
        workspace_id,
        user_id,
        id,
        key,
        async |conn, o, mut lines| {
            check(OrderAction::Return, &o, &lines, Paid::default())?;
            let returned = |lines: &[LineRow]| {
                let mut m = std::collections::BTreeMap::new();
                for l in lines.iter().filter(|l| l.tracked && !l.returned.is_zero()) {
                    *m.entry(l.product_id).or_insert(Decimal::ZERO) += l.returned;
                }
                m
            };
            let before = returned(&lines);
            let mut back = Vec::new();
            for (n, (line_id, q)) in input.lines.iter().enumerate() {
                let Some(l) = lines.iter_mut().find(|l| l.id == *line_id) else {
                    return Err(field(
                        format!("lines.{n}.line_id"),
                        "The selected line is invalid.",
                    ));
                };
                if q.is_zero() || l.returned + *q > l.deducted {
                    return Err(field(
                        format!("lines.{n}.quantity"),
                        format!(
                            "Return more than 0 and at most {} of this line.",
                            (l.deducted - l.returned).normalize()
                        ),
                    ));
                }
                l.returned += *q;
                if l.tracked {
                    back.push(Want {
                        product_id: l.product_id,
                        quantity: *q,
                    });
                }
            }
            if input.restock && !back.is_empty() {
                let reason = format!("return order {}", o.number);
                // Back into the lots they left (the farmers' debits use the same spread).
                let after = returned(&lines);
                let plan =
                    farm_supply::sale::restock_plan(conn, workspace_id, o.id, &before, &after)
                        .await?;
                reserve::restock_lots(
                    conn,
                    workspace_id,
                    o.warehouse_id,
                    &plan,
                    &cause(&o, &reason),
                )
                .await?;
            }
            let rows: Vec<_> = lines
                .iter()
                .map(|l| (l.id, l.picked, l.deducted, l.returned))
                .collect();
            repo::set_line_quantities(conn, workspace_id, &rows).await?;
            farm_supply::sale::on_return(conn, workspace_id, o.id, &o.number, &returned(&lines))
                .await?;
            let all = lines.iter().all(|l| l.returned == l.quantity);
            let status = if all {
                "returned"
            } else {
                "partially_returned"
            };
            repo::set_status(conn, workspace_id, o.id, status).await?;
            Ok(())
        },
    )
    .await
}

/// Book what the customer paid online (paid − refunded) on the order's new invoice, so its
/// balance is right. A draft keeps its status until it is sent.
async fn book_paid(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    user_id: Uuid,
    o: &OrderRow,
    invoice_id: Uuid,
) -> Result<(), AppError> {
    let (net, paid) = super::payments::net_paid(conn, workspace_id, o.id).await?;
    let Some((provider, provider_id)) = paid.filter(|_| net > 0) else {
        return Ok(());
    };
    let description = format!("Paid online for order {}", o.number);
    let row = invoice_payments::repo::NewPayment {
        id: Uuid::now_v7(),
        invoice_id,
        user_id,
        date: o.paid_at.unwrap_or_else(chrono::Utc::now).date_naive(),
        amount: net,
        description: Some(&description),
        payment_method: "provider",
        provider: Some(&provider),
        provider_id: provider_id.as_deref(),
        meta: None,
        settle_twin_id: None,
    };
    invoice_payments::repo::insert(conn, workspace_id, &[row]).await?;
    invoice_payments::repo::set_balance(conn, workspace_id, invoice_id, net, None).await?;
    Ok(())
}

/// Make a draft invoice of the order's lines (once). The invoice moves no stock: the order
/// already did.
pub async fn invoice(
    state: &AppState,
    workspace_id: Uuid,
    user_id: Uuid,
    id: &str,
    key: Option<Key>,
) -> Result<Answer, AppError> {
    transition(
        state,
        workspace_id,
        user_id,
        id,
        key,
        async |conn, o, lines| {
            check(OrderAction::Invoice, &o, &lines, Paid::default())?;
            let Some(relation_id) = o.relation_id else {
                return Err(AppError::field("relation_id", NO_RELATION));
            };
            let dp = u32::try_from(o.decimals).unwrap_or(2);
            let items = lines
                .iter()
                .enumerate()
                .map(|(n, l)| ItemRow {
                    index: n,
                    kind: RowKind::New(ItemFields {
                        title: l.title.clone(),
                        description: None,
                        quantity: l.quantity,
                        price: Some(Decimal::new(l.unit_price, dp)),
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
                relation_id: Some(relation_id),
                currency_id: Some(Some(o.currency_id)),
                addressed_contact_id: None,
                reference: Some(Some(
                    o.reference.clone().unwrap_or_else(|| o.number.clone()),
                )),
                note: o.note.clone().map(Some),
                payment_method: None,
                is_credit: false,
                due_date: None,
                billing_address: None,
                customer_ref: None,
                meta: Some(Some(json!({ "order_id": o.id, "order_number": o.number }))),
                items,
            };
            let invoice_id =
                invoices::service::create_in(conn, workspace_id, user_id, input).await?;
            repo::set_invoice(conn, workspace_id, o.id, invoice_id).await?;
            book_paid(conn, workspace_id, user_id, &o, invoice_id).await?;
            Ok(())
        },
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minor_units_are_exact() {
        assert_eq!(minor(Decimal::new(1234, 2), 2), Some(1234));
        assert_eq!(minor(Decimal::new(5, 0), 3), Some(5000));
        assert_eq!(minor(Decimal::ZERO, 0), Some(0));
    }
}
