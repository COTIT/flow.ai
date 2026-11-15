//! Purchase order lifecycle: draft create/update, reviewed confirm (freezes supplier and
//! line meaning), close of open remainders, cancel, reads and history. No stock or GL here.
//! Lock order: command row → shared book gate → order FOR UPDATE → order lines FOR UPDATE (id)
//! → number counter last.

use std::collections::HashMap;

use rust_decimal::Decimal;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::{
        json::Rules,
        list::{Page, Paginated},
        workspace::WorkspaceAccess,
    },
    idempotency::{self, Answer, Command},
    modules::{
        accounting::{
            dto::{Action, Create, CurrencyRef},
            gate,
            service::{created, ok, stale},
        },
        purchasing::{
            dto::{
                CancelPayload, CloseLinesPayload, ConfirmLine, ConfirmPayload, ConfirmReview,
                OrderInput, OrderListQuery, OrderPayload, ProductRef, PurchaseOrderEvent,
                PurchaseOrderLineResource, PurchaseOrderResource, PurchaseOrderTotals, SupplierRef,
                UnitRef,
            },
            repo::{self, Header, LineRow, NewLine, OrderRow},
        },
        relations::dto::supplier_out,
        stock::repo::live_warehouses,
    },
    numbering::{self, Series},
};

/// Stock-tracked kinds a purchase line may order (D18 physical goods).
const STOCK_KINDS: [&str; 2] = ["physical", "physical_second_chance"];
/// Largest base quantity (`stock::repo::LEVEL_MAX`).
const QTY_MAX: Decimal = crate::modules::stock::repo::LEVEL_MAX;

/// Minor units as an exact decimal string at `dp` places.
pub fn amount(minor: i64, dp: i16) -> String {
    Decimal::new(minor, u32::try_from(dp).unwrap_or(0)).to_string()
}

/// A quantity as a plain decimal string (no trailing zeros).
pub fn qty(d: Decimal) -> String {
    d.normalize().to_string()
}

/// A line total past the money column (SQL extension overflow) as a field error.
fn too_large(e: sqlx::Error) -> AppError {
    match e.as_database_error().and_then(|d| d.code()) {
        Some(c) if c == "22003" => AppError::field("payload.lines", "A line total is too large."),
        _ => e.into(),
    }
}

fn not_draft() -> AppError {
    AppError::Unprocessable("Only draft orders can change.".into())
}

fn currency_ref(o: &OrderRow) -> CurrencyRef {
    CurrencyRef {
        id: o.currency_id,
        code: o.currency_code.clone(),
        decimals: o.currency_decimals,
    }
}

fn open_of(l: &LineRow) -> Decimal {
    l.base_quantity - l.received_quantity - l.closed_quantity
}

fn resource(o: OrderRow, lines: Vec<LineRow>, ws: Uuid) -> PurchaseOrderResource {
    let dp = o.currency_decimals;
    let any_open = lines.iter().any(|l| open_of(l) > Decimal::ZERO);
    let any_received = lines.iter().any(|l| l.received_quantity > Decimal::ZERO);
    let receipt_status = match (any_received, any_open) {
        (false, _) => "not_received",
        (true, true) => "partial",
        (true, false) => "received",
    };
    let allowed_actions = match o.state.as_str() {
        "draft" => vec!["update", "confirm", "cancel"],
        "confirmed" => {
            let mut a = Vec::new();
            if any_open {
                a.extend(["receive", "close"]);
            }
            if !any_received {
                a.push("cancel");
            }
            a
        }
        _ => vec![],
    };
    PurchaseOrderResource {
        id: o.id,
        workspace_id: ws,
        number: o.number,
        version: o.version,
        receipt_status,
        supplier: SupplierRef {
            id: o.supplier_id,
            name: o.supplier_name.clone(),
        },
        supplier_snapshot: o.supplier_snapshot.clone(),
        currency: currency_ref(&o),
        purchase_date: o.purchase_date,
        expected_date: o.expected_date,
        reference: o.reference,
        note: o.note,
        default_warehouse_id: o.default_warehouse_id,
        totals: PurchaseOrderTotals {
            ordered: amount(o.ordered_total, dp),
            received: amount(o.received_total, dp),
            open: amount(o.open_total, dp),
        },
        lines: lines
            .into_iter()
            .map(|l| PurchaseOrderLineResource {
                id: l.id,
                line_no: l.line_no,
                open_quantity: qty(open_of(&l)),
                product: ProductRef {
                    id: l.product_id,
                    name: l.product_name,
                    unit: l.product_unit,
                },
                purchase_unit: l
                    .purchase_unit_id
                    .zip(l.unit_code)
                    .map(|(id, code)| UnitRef {
                        id,
                        code,
                        ratio: qty(l.ratio),
                    }),
                input_quantity: qty(l.input_quantity),
                ratio: qty(l.ratio),
                base_quantity: qty(l.base_quantity),
                unit_price: qty(l.unit_price),
                extension: amount(l.extension, dp),
                warehouse_id: l.warehouse_id,
                received_quantity: qty(l.received_quantity),
                booked_quantity: qty(l.booked_quantity),
                closed_quantity: qty(l.closed_quantity),
            })
            .collect(),
        state: o.state,
        created_by: o.created_by,
        created_at: o.created_at,
        updated_at: o.updated_at,
        confirmed_by: o.confirmed_by,
        confirmed_at: o.confirmed_at,
        cancelled_at: o.cancelled_at,
        cancel_reason: o.cancel_reason,
        allowed_actions,
    }
}

/// One order; 404 outside the workspace.
pub async fn get(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
) -> Result<PurchaseOrderResource, AppError> {
    let (mut rows, _) = repo::orders(&mut *conn, ws, Some(id), None, None, (1, 0)).await?;
    let o = rows.pop().ok_or(AppError::NotFound)?;
    let lines = repo::lines(conn, ws, &[id], false).await?;
    Ok(resource(o, lines, ws))
}

/// GET /purchase-orders: newest first; lines in one query for the page.
pub async fn list(
    state: &AppState,
    ws: Uuid,
    q: &OrderListQuery,
) -> Result<Paginated<PurchaseOrderResource>, AppError> {
    let mut r = Rules::default();
    if let Some(s) = &q.state {
        r.check(
            "state",
            ["draft", "confirmed", "cancelled"].contains(&s.as_str()),
            "The state must be draft, confirmed or cancelled.",
        );
    }
    r.into_result()?;
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    let mut conn = state.db.acquire().await?;
    let (rows, total) = repo::orders(
        &mut *conn,
        ws,
        None,
        q.state.as_deref(),
        q.supplier_id,
        (page.size, page.offset()),
    )
    .await?;
    let ids: Vec<Uuid> = rows.iter().map(|o| o.id).collect();
    let mut by_order: HashMap<Uuid, Vec<LineRow>> = HashMap::new();
    for l in repo::lines(&mut conn, ws, &ids, false).await? {
        by_order.entry(l.purchase_order_id).or_default().push(l);
    }
    let data = rows
        .into_iter()
        .map(|o| {
            let lines = by_order.remove(&o.id).unwrap_or_default();
            resource(o, lines, ws)
        })
        .collect();
    Ok(Paginated::new(data, total, page))
}

/// Check references and turn input lines into stored lines (ratio frozen, base quantity
/// exact, extension in order-currency minor units).
async fn prepare(
    conn: &mut PgConnection,
    ws: Uuid,
    input: &OrderInput,
) -> Result<Vec<NewLine>, AppError> {
    let mut r = Rules::default();
    if !repo::supplier_exists(&mut *conn, ws, input.supplier).await? {
        r.add("payload.supplier_id", "The selected supplier is invalid.");
    }
    let currency =
        crate::modules::accounting::repo::currency(&mut *conn, ws, input.currency).await?;
    if currency.is_none() {
        r.add("payload.currency_id", "The selected currency is invalid.");
    }
    let wanted: Vec<Uuid> = input
        .lines
        .iter()
        .filter_map(|l| l.warehouse)
        .chain(input.default_warehouse)
        .collect();
    let live = live_warehouses(&mut *conn, ws, &wanted).await?;
    if input.default_warehouse.is_some_and(|w| !live.contains(&w)) {
        r.add(
            "payload.default_warehouse_id",
            "The selected warehouse is invalid.",
        );
    }
    let product_ids: Vec<Uuid> = input.lines.iter().map(|l| l.product).collect();
    let products = repo::products(&mut *conn, ws, &product_ids).await?;
    let unit_ids: Vec<Uuid> = input.lines.iter().filter_map(|l| l.unit).collect();
    let units = repo::units(&mut *conn, ws, &unit_ids).await?;
    let mut out = Vec::with_capacity(input.lines.len());
    for (i, l) in input.lines.iter().enumerate() {
        let f = |name: &str| format!("payload.lines.{i}.{name}");
        match products.iter().find(|p| p.0 == l.product) {
            None => r.add(&f("product_id"), "The selected product is invalid."),
            Some((_, has_stock, kind)) if !has_stock || !STOCK_KINDS.contains(&kind.as_str()) => r
                .add(
                    &f("product_id"),
                    "Only stock-tracked goods can be ordered here.",
                ),
            Some(_) => {}
        }
        if l.warehouse.is_some_and(|w| !live.contains(&w)) {
            r.add(&f("warehouse_id"), "The selected warehouse is invalid.");
        }
        let ratio = match l.unit {
            None => Some(Decimal::ONE),
            Some(u) => match units.iter().find(|x| x.0 == u) {
                Some((_, product, ratio, true)) if *product == l.product => Some(*ratio),
                Some((_, _, _, false)) => {
                    r.add(&f("purchase_unit_id"), "This purchase unit is inactive.");
                    None
                }
                _ => {
                    r.add(
                        &f("purchase_unit_id"),
                        "The selected purchase unit is invalid.",
                    );
                    None
                }
            },
        };
        let Some(ratio) = ratio else { continue };
        let base = l.quantity.checked_mul(ratio).map(|b| b.normalize());
        let Some(base) = base.filter(|b| b.scale() <= 2 && *b <= QTY_MAX) else {
            r.add(
                &f("quantity"),
                "This quantity in base units needs more than 2 decimal places or is too large.",
            );
            continue;
        };
        out.push(NewLine {
            product: l.product,
            unit: l.unit,
            input_quantity: l.quantity,
            ratio,
            base_quantity: base,
            unit_price: l.unit_price,
            warehouse: l.warehouse,
        });
    }
    r.into_result()?;
    Ok(out)
}

fn header(input: &OrderInput) -> Header<'_> {
    Header {
        supplier: input.supplier,
        currency: input.currency,
        purchase_date: input.purchase_date,
        expected_date: input.expected_date,
        reference: input.reference.as_deref(),
        note: input.note.as_deref(),
        default_warehouse: input.default_warehouse,
    }
}

/// POST /purchase-orders: a numbered draft.
pub async fn create(
    state: &AppState,
    access: &WorkspaceAccess,
    cmd: Command<Create<OrderPayload>>,
) -> Result<Answer, AppError> {
    const OP: &str = "purchase_orders_create";
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, access.user_id, &cmd).await? {
        return Ok(a);
    }
    gate::shared(&mut tx, &[ws]).await?;
    let input = &cmd.input.payload;
    let lines = prepare(&mut tx, ws, input).await?;
    let number = numbering::next(&mut tx, ws, Series::PurchaseOrder, 1).await?;
    let id = repo::insert_order(&mut tx, ws, number, &header(input), access.user_id).await?;
    repo::replace_lines(&mut tx, ws, id, &lines)
        .await
        .map_err(too_large)?;
    let answer = created(&get(&mut tx, ws, id).await?);
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[id]).await?;
    tx.commit().await?;
    Ok(answer)
}

/// Lock the order and check the caller's version; (state, version).
async fn lock(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
    expected: i64,
) -> Result<String, AppError> {
    gate::shared(&mut *conn, &[ws]).await?;
    let (state, version, _) = repo::lock_order(conn, ws, id)
        .await?
        .ok_or(AppError::NotFound)?;
    if version != expected {
        return Err(stale());
    }
    Ok(state)
}

/// PATCH /purchase-orders/{id}: replace a draft's fields and lines.
pub async fn update(
    state: &AppState,
    access: &WorkspaceAccess,
    id: Uuid,
    cmd: Command<Action<OrderPayload>>,
) -> Result<Answer, AppError> {
    const OP: &str = "purchase_orders_update";
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, access.user_id, &cmd).await? {
        return Ok(a);
    }
    if lock(&mut tx, ws, id, cmd.input.expected_version).await? != "draft" {
        return Err(not_draft());
    }
    let input = &cmd.input.payload;
    let lines = prepare(&mut tx, ws, input).await?;
    repo::update_order(&mut tx, ws, id, &header(input)).await?;
    repo::replace_lines(&mut tx, ws, id, &lines)
        .await
        .map_err(too_large)?;
    let answer = ok(&get(&mut tx, ws, id).await?);
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[id]).await?;
    tx.commit().await?;
    Ok(answer)
}

/// What confirm freezes, from current facts.
async fn prepare_confirm(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
) -> Result<ConfirmReview, AppError> {
    let o = get(&mut *conn, ws, id).await?;
    if o.state != "draft" {
        return Err(not_draft());
    }
    let (company, alias, addresses, data) = repo::supplier_facts(&mut *conn, ws, o.supplier.id)
        .await?
        .ok_or(AppError::NotFound)?;
    let snapshot = json!({
        "id": o.supplier.id, "company_name": company, "alias": alias, "addresses": addresses,
        "supplier_data": data.as_ref().map(supplier_out),
    });
    let lines: Vec<ConfirmLine> = o
        .lines
        .iter()
        .map(|l| ConfirmLine {
            line_id: l.id,
            product_id: l.product.id,
            purchase_unit_id: l.purchase_unit.as_ref().map(|u| u.id),
            ratio: l.ratio.clone(),
            base_quantity: l.base_quantity.clone(),
            unit_price: l.unit_price.clone(),
            extension: l.extension.clone(),
        })
        .collect();
    let fingerprint: [u8; 32] = Sha256::digest(
        json!({
            "action": "confirm", "order": id, "version": o.version, "supplier": snapshot,
            "currency": o.currency.id,
            "lines": lines.iter().map(|l| json!([l.line_id, l.product_id, l.purchase_unit_id,
                l.ratio, l.base_quantity, l.unit_price, l.extension])).collect::<Vec<Value>>(),
        })
        .to_string(),
    )
    .into();
    Ok(ConfirmReview {
        purchase_order_id: id,
        version: o.version,
        supplier_snapshot: snapshot,
        currency: o.currency,
        lines,
        totals: o.totals,
        review_fingerprint: hex::encode(fingerprint),
    })
}

/// POST /purchase-orders/{id}/confirm/review: no writes.
pub async fn confirm_review(
    state: &AppState,
    ws: Uuid,
    id: Uuid,
) -> Result<ConfirmReview, AppError> {
    let mut tx = state.db.begin().await?;
    let review = prepare_confirm(&mut tx, ws, id).await?;
    tx.rollback().await?;
    Ok(review)
}

/// POST /purchase-orders/{id}/confirm: freeze the reviewed order.
pub async fn confirm(
    state: &AppState,
    access: &WorkspaceAccess,
    id: Uuid,
    cmd: Command<Action<ConfirmPayload>>,
) -> Result<Answer, AppError> {
    const OP: &str = "purchase_orders_confirm";
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, access.user_id, &cmd).await? {
        return Ok(a);
    }
    if lock(&mut tx, ws, id, cmd.input.expected_version).await? != "draft" {
        return Err(not_draft());
    }
    let review = prepare_confirm(&mut tx, ws, id).await?;
    if review.review_fingerprint != cmd.input.payload {
        return Err(stale());
    }
    repo::confirm(&mut tx, ws, id, &review.supplier_snapshot, access.user_id).await?;
    let answer = ok(&get(&mut tx, ws, id).await?);
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[id]).await?;
    tx.commit().await?;
    Ok(answer)
}

/// POST /purchase-orders/{id}/close-lines: close open remainders; received goods stay
/// bookable.
pub async fn close_lines(
    state: &AppState,
    access: &WorkspaceAccess,
    id: Uuid,
    cmd: Command<Action<CloseLinesPayload>>,
) -> Result<Answer, AppError> {
    const OP: &str = "purchase_orders_close_lines";
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, access.user_id, &cmd).await? {
        return Ok(a);
    }
    if lock(&mut tx, ws, id, cmd.input.expected_version).await? != "confirmed" {
        return Err(AppError::Unprocessable(
            "Only confirmed orders have remainders to close.".into(),
        ));
    }
    let input = &cmd.input.payload;
    let lines = repo::lines(&mut tx, ws, &[id], true).await?;
    let mut r = Rules::default();
    for (i, (line, q)) in input.lines.iter().enumerate() {
        let Some(l) = lines.iter().find(|l| l.id == *line) else {
            return Err(AppError::NotFound);
        };
        let open = open_of(l);
        r.check(
            &format!("payload.lines.{i}.quantity"),
            *q <= open,
            format!("Only {} is still open on this line.", qty(open)),
        );
    }
    r.into_result()?;
    for (line, q) in &input.lines {
        repo::close_line(&mut tx, ws, id, *line, *q, &input.reason, access.user_id).await?;
    }
    repo::touch(&mut tx, ws, id).await?;
    let answer = ok(&get(&mut tx, ws, id).await?);
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[id]).await?;
    tx.commit().await?;
    Ok(answer)
}

/// POST /purchase-orders/{id}/cancel: only while nothing was received; closes every open
/// remainder of a confirmed order.
pub async fn cancel(
    state: &AppState,
    access: &WorkspaceAccess,
    id: Uuid,
    cmd: Command<Action<CancelPayload>>,
) -> Result<Answer, AppError> {
    const OP: &str = "purchase_orders_cancel";
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, access.user_id, &cmd).await? {
        return Ok(a);
    }
    let order_state = lock(&mut tx, ws, id, cmd.input.expected_version).await?;
    if order_state == "cancelled" {
        return Err(AppError::Unprocessable(
            "This order is already cancelled.".into(),
        ));
    }
    let reason = &cmd.input.payload;
    let lines = repo::lines(&mut tx, ws, &[id], true).await?;
    if lines.iter().any(|l| l.received_quantity > Decimal::ZERO) {
        return Err(AppError::Unprocessable(
            "Goods were received on this order. Close the open remainder instead.".into(),
        ));
    }
    if order_state == "confirmed" {
        for l in lines.iter().filter(|l| open_of(l) > Decimal::ZERO) {
            repo::close_line(&mut tx, ws, id, l.id, open_of(l), reason, access.user_id).await?;
        }
    }
    repo::cancel(&mut tx, ws, id, reason, access.user_id).await?;
    let answer = ok(&get(&mut tx, ws, id).await?);
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[id]).await?;
    tx.commit().await?;
    Ok(answer)
}

/// GET /purchase-orders/{id}/events: history from the order's rows, newest first.
pub async fn events(
    state: &AppState,
    ws: Uuid,
    id: Uuid,
) -> Result<Vec<PurchaseOrderEvent>, AppError> {
    let mut conn = state.db.acquire().await?;
    get(&mut conn, ws, id).await?;
    Ok(repo::events(&mut *conn, ws, id)
        .await?
        .into_iter()
        .map(|e| PurchaseOrderEvent {
            at: e.at,
            kind: e.kind,
            actor: e.actor,
            ref_id: e.ref_id,
            detail: e.detail,
        })
        .collect())
}
