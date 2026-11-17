//! Receipts and booking (masterplan §11/§7/§10). Receive accepts goods without stock and, in
//! an active book, locks each pool and records the pending-booking guard. Book adds stock and
//! value through the stock layer (`Valued::Receipt`): DR inventory / CR GRNI once per receipt
//! line, rate frozen on the effective receipt date, landed components capitalised once.
//! Lock order (book): command row → shared gate → order → receipt → receipt lines (id) →
//! order lines (id) → per line in (product, warehouse, id) order: warehouse SHARE → pool
//! UPDATE → bin → slot → period/accounts → effect → journal number → landed rows.

use chrono::NaiveDate;
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
            service::{created, not_configured, ok, stale, unsupported},
        },
        currency_rates::repo::in_force,
        purchasing::{
            dto::{
                AllocationResource, BookInput, BookJournal, BookPayload, BookReview,
                BookReviewCost, BookReviewLine, LandedCostResource, PageQuery, PoolPreview,
                PurchaseReceiptResource, RateSnapshot, ReceiptLineResource, ReceiptPayload,
            },
            repo::{self, Booked, NewLanded, NewReceiptLine, Rate, ReceiptLineRow, ReceiptRow},
            service::{amount, qty},
        },
        stock::{
            repo::{self as stock, Bin, Cause, Floor, Mutated, Slot},
            value::{self, Valued},
        },
    },
    money::minor,
    numbering::{self, Series},
};

fn reason(code: &'static str, message: &str) -> AppError {
    AppError::Reason {
        status: axum::http::StatusCode::UNPROCESSABLE_ENTITY,
        code,
        message: message.into(),
    }
}

fn backdated() -> AppError {
    reason(
        "backdated_valuation",
        "This stock was already valued on a later date. Late receipts book through the late-receipt review.",
    )
}

fn snapshot(r: &Rate) -> RateSnapshot {
    RateSnapshot {
        rate: r.0.normalize().to_string(),
        inverse: r.1,
        source: r.2.to_owned(),
        date: r.3,
    }
}

/// `from` → `to` rate in force on `on`: identity, direct, else inverse; never a guess.
async fn rate(
    conn: &mut PgConnection,
    ws: Uuid,
    from: &CurrencyRef,
    to: &CurrencyRef,
    on: NaiveDate,
    field: &str,
) -> Result<Rate, AppError> {
    if from.id == to.id {
        return Ok((Decimal::ONE, false, "identity", None));
    }
    if let Some(r) = in_force(&mut *conn, ws, from.id, to.id, on).await? {
        return Ok((r.rate, false, "workspace", Some(r.effective_on)));
    }
    if let Some(r) = in_force(&mut *conn, ws, to.id, from.id, on).await? {
        return Ok((r.rate, true, "workspace", Some(r.effective_on)));
    }
    Err(AppError::field(
        field,
        format!("No {} → {} rate is in force on {on}.", from.code, to.code),
    ))
}

fn state_of(l: &ReceiptLineRow) -> &'static str {
    match (l.booked_at.is_some(), l.late) {
        (true, _) => "booked",
        (false, true) => "late",
        (false, false) => "pending",
    }
}

async fn resources(
    conn: &mut PgConnection,
    ws: Uuid,
    rows: Vec<ReceiptRow>,
    accounting: bool,
) -> Result<Vec<PurchaseReceiptResource>, AppError> {
    let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
    let lines = repo::receipt_lines(&mut *conn, ws, &ids, None).await?;
    let landed = repo::landed(&mut *conn, ws, &ids).await?;
    Ok(rows
        .into_iter()
        .map(|r| {
            // Setup bookings carry no values; 2 only formats nothing.
            let functional_currency = r
                .functional
                .clone()
                .map(|(id, code, decimals)| CurrencyRef { id, code, decimals });
            let fdp = functional_currency.as_ref().map_or(2, |c| c.decimals);
            let mine: Vec<&ReceiptLineRow> =
                lines.iter().filter(|l| l.receipt_id == r.id).collect();
            let pending = mine.iter().any(|l| state_of(l) == "pending");
            let booked = mine.iter().filter(|l| l.booked_at.is_some()).count();
            PurchaseReceiptResource {
                id: r.id,
                purchase_order_id: r.purchase_order_id,
                number: r.number,
                version: r.version,
                effective_receipt_date: r.effective_receipt_date,
                note: r.note,
                recorded_at: r.recorded_at,
                created_by: r.created_by,
                functional_currency,
                booking_status: match booked {
                    0 => "unbooked",
                    n if n == mine.len() => "booked",
                    _ => "partial",
                },
                lines: mine
                    .iter()
                    .map(|l| ReceiptLineResource {
                        id: l.id,
                        po_line_id: l.po_line_id,
                        product_id: l.product_id,
                        warehouse_id: l.warehouse_id,
                        bin_id: l.bin_id,
                        lot_id: l.lot_id,
                        base_quantity: qty(l.base_quantity),
                        state: state_of(l),
                        booked_at: l.booked_at,
                        accounting_date: l.accounting_date,
                        net_value: l.net_value.map(|v| amount(v, fdp)),
                        landed_value: l.landed_value.map(|v| amount(v, fdp)),
                        value: l.value.map(|v| amount(v, fdp)),
                        rate: match (l.rate, l.rate_inverse, l.rate_source.as_deref()) {
                            (Some(rate), Some(inverse), Some(source)) => Some(RateSnapshot {
                                rate: rate.normalize().to_string(),
                                inverse,
                                source: source.to_owned(),
                                date: l.rate_date,
                            }),
                            _ => None,
                        },
                        journal_entry_id: l.journal_entry_id.filter(|_| accounting),
                    })
                    .collect(),
                landed_costs: landed
                    .iter()
                    .filter(|c| c.receipt_id == r.id)
                    .map(|c| LandedCostResource {
                        id: c.id,
                        client_component_ref: c.client_component_ref,
                        amount: amount(c.amount, c.currency_decimals),
                        currency: CurrencyRef {
                            id: c.currency_id,
                            code: c.currency_code.clone(),
                            decimals: c.currency_decimals,
                        },
                        functional_amount: amount(c.functional_amount, fdp),
                        rate: RateSnapshot {
                            rate: c.rate.normalize().to_string(),
                            inverse: c.rate_inverse,
                            source: c.rate_source.clone(),
                            date: c.rate_date,
                        },
                        evidence_ref: c.evidence_ref.clone(),
                        allocations: c
                            .allocations
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(|a| {
                                Some(AllocationResource {
                                    receipt_line_id: a["receipt_line_id"].as_str()?.parse().ok()?,
                                    functional_amount: amount(
                                        a["functional_amount"].as_i64()?,
                                        fdp,
                                    ),
                                })
                            })
                            .collect(),
                    })
                    .collect(),
                allowed_actions: if pending { vec!["book"] } else { vec![] },
            }
        })
        .collect())
}

/// One receipt of an order; 404 outside it.
pub async fn get(
    conn: &mut PgConnection,
    ws: Uuid,
    order: Uuid,
    id: Uuid,
    accounting: bool,
) -> Result<PurchaseReceiptResource, AppError> {
    let (rows, _) = repo::receipts(&mut *conn, ws, order, Some(id), (1, 0)).await?;
    resources(conn, ws, rows, accounting)
        .await?
        .pop()
        .ok_or(AppError::NotFound)
}

/// GET /purchase-orders/{id}/receipts: newest first.
pub async fn list(
    state: &AppState,
    access: &WorkspaceAccess,
    order: Uuid,
    q: &PageQuery,
) -> Result<Paginated<PurchaseReceiptResource>, AppError> {
    let ws = access.workspace_id;
    let mut conn = state.db.acquire().await?;
    crate::modules::purchasing::service::get(&mut conn, ws, order).await?;
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    let (rows, total) =
        repo::receipts(&mut *conn, ws, order, None, (page.size, page.offset())).await?;
    let data = resources(&mut conn, ws, rows, access.allows("accounting.read")).await?;
    Ok(Paginated::new(data, total, page))
}

/// POST /purchase-orders/{id}/receipts: accept goods against open quantities.
pub async fn receive(
    state: &AppState,
    access: &WorkspaceAccess,
    order: Uuid,
    cmd: Command<Create<ReceiptPayload>>,
) -> Result<Answer, AppError> {
    const OP: &str = "purchase_receipts_create";
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, access.user_id, &cmd).await? {
        return Ok(a);
    }
    let active = value::active(&mut tx, ws).await?;
    let (order_state, _, _) = repo::lock_order(&mut tx, ws, order)
        .await?
        .ok_or(AppError::NotFound)?;
    if order_state != "confirmed" {
        return Err(AppError::Unprocessable(
            "Only confirmed orders can receive goods.".into(),
        ));
    }
    let input = &cmd.input.payload;
    let po_lines = repo::lines(&mut tx, ws, &[order], true).await?;
    let mut r = Rules::default();
    if input.date > repo::today(&mut *tx, ws).await? {
        r.add(
            "payload.effective_receipt_date",
            "The receipt date can't be in the future.",
        );
    }
    let mut lines = Vec::with_capacity(input.lines.len());
    for (i, l) in input.lines.iter().enumerate() {
        let f = |name: &str| format!("payload.lines.{i}.{name}");
        let po = po_lines
            .iter()
            .find(|p| p.id == l.po_line)
            .ok_or(AppError::NotFound)?;
        let open = po.base_quantity - po.received_quantity - po.closed_quantity;
        r.check(
            &f("base_quantity"),
            l.quantity <= open,
            format!("Only {} is still open on this line.", qty(open)),
        );
        if stock::live_warehouses(&mut *tx, ws, &[l.warehouse])
            .await?
            .is_empty()
        {
            r.add(&f("warehouse_id"), "The selected warehouse is invalid.");
        } else if let Some(b) = l.bin
            && stock::bin(&mut *tx, ws, l.warehouse, b).await?.is_none()
        {
            r.add(&f("bin_id"), "The selected bin is not in this warehouse.");
        }
        if let Some(lot) = l.lot
            && !repo::lot_of(&mut *tx, ws, lot, po.product_id).await?
        {
            r.add(
                &f("lot_id"),
                "The selected lot is not a lot of this product.",
            );
        }
        lines.push(NewReceiptLine {
            po_line: l.po_line,
            product: po.product_id,
            warehouse: l.warehouse,
            bin: l.bin,
            lot: l.lot,
            quantity: l.quantity,
            late: false,
        });
    }
    r.into_result()?;
    if active {
        // Pools in (product, warehouse) order, each after its warehouse (stock-layer order).
        let mut pools: Vec<(Uuid, Uuid)> = lines.iter().map(|l| (l.product, l.warehouse)).collect();
        pools.sort_unstable();
        pools.dedup();
        for (product, warehouse) in pools {
            repo::share_warehouse(&mut tx, ws, warehouse)
                .await?
                .ok_or(AppError::NotFound)?;
            let last = value::pool(&mut tx, ws, product, warehouse).await?;
            let late = last.is_some_and(|d| input.date < d);
            for l in lines
                .iter_mut()
                .filter(|l| l.product == product && l.warehouse == warehouse)
            {
                l.late = late;
            }
        }
    }
    let number = numbering::next(&mut tx, ws, Series::PurchaseReceipt, 1).await?;
    let (id, recorded_at) = repo::insert_receipt(
        &mut tx,
        ws,
        order,
        number,
        input.date,
        input.note.as_deref(),
        access.user_id,
    )
    .await?;
    repo::insert_receipt_lines(&mut tx, ws, id, (input.date, recorded_at), &lines).await?;
    repo::touch(&mut tx, ws, order).await?;
    let answer = created(&get(&mut tx, ws, order, id, access.allows("accounting.read")).await?);
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[id]).await?;
    tx.commit().await?;
    Ok(answer)
}

/// SQL-computed money of one booked line (functional minor units; `doc` in PO currency).
struct LineValue {
    net: i64,
    landed: i64,
    doc: i64,
    value: i64,
    rate: Rate,
}

/// One line a booking adds.
struct PlannedLine {
    row: ReceiptLineRow,
    value: Option<LineValue>,
}

/// One component a booking capitalises.
struct PlannedCost {
    component: Uuid,
    amount: i64,
    currency: CurrencyRef,
    functional: i64,
    rate: Rate,
    evidence_ref: String,
    allocations: Vec<(Uuid, i64)>,
}

struct Plan {
    review: BookReview,
    lines: Vec<PlannedLine>,
    costs: Vec<PlannedCost>,
    receipt_version: i64,
}

/// Booking facts under locks, shared by review and book.
async fn prepare(
    conn: &mut PgConnection,
    ws: Uuid,
    (order, receipt): (Uuid, Uuid),
    input: &BookInput,
    accounting: bool,
) -> Result<Plan, AppError> {
    let active = value::active(&mut *conn, ws).await?;
    let (_, order_version, currency_id) = repo::lock_order(&mut *conn, ws, order)
        .await?
        .ok_or(AppError::NotFound)?;
    let (receipt_version, date) = repo::lock_receipt(&mut *conn, ws, order, receipt)
        .await?
        .ok_or(AppError::NotFound)?;
    let mut rows = repo::receipt_lines(&mut *conn, ws, &[receipt], Some(&input.lines)).await?;
    if rows.len() != input.lines.len() {
        return Err(AppError::NotFound);
    }
    for l in &rows {
        if l.booked_at.is_some() {
            return Err(stale());
        }
        if l.late {
            return Err(backdated());
        }
    }
    // Order lines FOR UPDATE: their booked quantity/value feed the cumulative share.
    repo::lines(&mut *conn, ws, &[order], true).await?;
    // Stock-layer order: pools by (product, warehouse), then line id.
    rows.sort_by_key(|l| (l.product_id, l.warehouse_id, l.id));
    let fp_lines: Vec<Value> = rows
        .iter()
        .map(|l| {
            json!([
                l.id,
                l.base_quantity.normalize().to_string(),
                l.warehouse_id,
                l.bin_id,
                l.lot_id
            ])
        })
        .collect();
    if !active {
        if !input.landed.is_empty() {
            return Err(unsupported(
                "Landed costs need an active accounting book: there is no inventory value to add them to.",
            ));
        }
        let fingerprint: [u8; 32] = Sha256::digest(
            json!({ "action": "book", "receipt": receipt, "version": receipt_version,
                    "order_version": order_version, "date": date, "active": false,
                    "lines": fp_lines })
            .to_string(),
        )
        .into();
        let review = BookReview {
            receipt_id: receipt,
            receipt_version,
            purchase_order_version: order_version,
            accounting_date: date,
            functional_currency: None,
            lines: rows
                .iter()
                .map(|l| BookReviewLine {
                    receipt_line_id: l.id,
                    base_quantity: qty(l.base_quantity),
                    net_value: None,
                    landed_value: None,
                    value: None,
                    rate: None,
                })
                .collect(),
            landed_costs: vec![],
            total: None,
            journal: None,
            pool: None,
            review_fingerprint: hex::encode(fingerprint),
        };
        let lines = rows
            .into_iter()
            .map(|row| PlannedLine { row, value: None })
            .collect();
        return Ok(Plan {
            review,
            lines,
            costs: vec![],
            receipt_version,
        });
    }
    let settings = crate::modules::accounting::repo::settings(&mut *conn, ws, false).await?;
    let functional = match settings.functional_currency_id {
        Some(id) => crate::modules::accounting::repo::currency(&mut *conn, ws, id).await?,
        None => None,
    }
    .ok_or_else(not_configured)?;
    let po_currency = crate::modules::accounting::repo::currency(&mut *conn, ws, currency_id)
        .await?
        .ok_or_else(not_configured)?;
    let po_rate = rate(conn, ws, &po_currency, &functional, date, "currency_id").await?;
    let dp = |c: &CurrencyRef| i32::from(c.decimals);
    // Chronology and eligible order, read-only here; the stock layer rechecks under locks.
    let selected: Vec<Uuid> = rows.iter().map(|l| l.id).collect();
    for l in &rows {
        if repo::pending_before_outside(&mut *conn, ws, l.id, &selected).await? {
            return Err(reason(
                "receipt_booking_required",
                "An earlier receipt of this stock is waiting. Book it first.",
            ));
        }
        if repo::pool_state(&mut *conn, ws, l.product_id, l.warehouse_id)
            .await?
            .and_then(|p| p.2)
            .is_some_and(|last| date < last)
        {
            return Err(backdated());
        }
    }
    // Money in SQL: cumulative order-line share and its conversion per line.
    let nets = repo::nets(
        &mut *conn,
        ws,
        &selected,
        &po_rate,
        (dp(&po_currency), dp(&functional)),
    )
    .await?;
    let net_of = |id: Uuid| {
        nets.iter()
            .find(|n| n.0 == id)
            .map_or((0, 0), |n| (n.1, n.2))
    };
    let refs: Vec<Uuid> = input.landed.iter().map(|c| c.component).collect();
    let used = repo::used_components(&mut *conn, ws, order, &refs).await?;
    let mut r = Rules::default();
    let mut costs = Vec::with_capacity(input.landed.len());
    for (i, c) in input.landed.iter().enumerate() {
        let f = |name: &str| format!("payload.landed_costs.{i}.{name}");
        if used.contains(&c.component) {
            r.add(
                &f("client_component_ref"),
                "This landed cost was already capitalised on this order.",
            );
            continue;
        }
        let Some(currency) =
            crate::modules::accounting::repo::currency(&mut *conn, ws, c.currency).await?
        else {
            r.add(&f("currency_id"), "The selected currency is invalid.");
            continue;
        };
        let places = u32::try_from(currency.decimals).unwrap_or(2);
        let Ok(amount_minor) = minor(c.amount, places) else {
            r.add(
                &f("amount"),
                format!("The amount must have at most {places} decimal places."),
            );
            continue;
        };
        let cost_rate = rate(conn, ws, &currency, &functional, date, &f("currency_id")).await?;
        let pair = (dp(&currency), dp(&functional));
        let functional_amount = repo::convert(&mut *conn, amount_minor, &cost_rate, pair).await?;
        let allocations = match &c.explicit {
            Some(explicit) => {
                let parts: Option<Vec<(Uuid, i64)>> = explicit
                    .iter()
                    .map(|(line, a)| {
                        selected
                            .contains(line)
                            .then(|| minor(*a, places).ok().map(|m| (*line, m)))
                            .flatten()
                    })
                    .collect();
                let shares = match parts {
                    Some(parts) => {
                        repo::explicit_shares(
                            &mut *conn,
                            &parts,
                            (amount_minor, functional_amount),
                            &cost_rate,
                            pair,
                        )
                        .await?
                    }
                    None => None,
                };
                if shares.is_none() {
                    r.add(
                        &f("explicit_line_allocations"),
                        "Allocations must name booked lines and add up to the amount.",
                    );
                }
                shares
            }
            None => {
                let bases: Vec<i64> = selected.iter().map(|id| net_of(*id).1).collect();
                let shares =
                    repo::allocate(&mut *conn, functional_amount, &selected, &bases).await?;
                if shares.is_empty() {
                    r.add(
                        &f("explicit_line_allocations"),
                        "These lines have no purchase value to spread the cost over. Allocate it explicitly.",
                    );
                    None
                } else {
                    Some(shares)
                }
            }
        };
        let Some(allocations) = allocations else {
            continue;
        };
        costs.push(PlannedCost {
            component: c.component,
            amount: amount_minor,
            currency,
            functional: functional_amount,
            rate: cost_rate,
            evidence_ref: c.evidence_ref.clone(),
            allocations,
        });
    }
    r.into_result()?;
    let fdp = functional.decimals;
    // Per-line landed sums, line values and the booking total, in SQL.
    let shares: Vec<(Uuid, i64)> = costs
        .iter()
        .flat_map(|c| c.allocations.iter().copied())
        .collect();
    let net_list: Vec<i64> = selected.iter().map(|id| net_of(*id).1).collect();
    let (sums, total) = repo::booking_totals(&mut *conn, &selected, &net_list, &shares).await?;
    let lines: Vec<PlannedLine> = rows
        .into_iter()
        .map(|row| {
            let (doc, net) = net_of(row.id);
            let (landed, value) = sums
                .iter()
                .find(|s| s.0 == row.id)
                .map_or((0, net), |s| (s.1, s.2));
            PlannedLine {
                row,
                value: Some(LineValue {
                    net,
                    landed,
                    doc,
                    value,
                    rate: po_rate,
                }),
            }
        })
        .collect();
    let pool = if accounting {
        let values: Vec<i64> = selected
            .iter()
            .map(|id| sums.iter().find(|s| s.0 == *id).map_or(0, |s| s.2))
            .collect();
        Some(
            repo::pool_preview(&mut *conn, ws, &selected, &values)
                .await?
                .into_iter()
                .map(|p| PoolPreview {
                    product_id: p.product_id,
                    warehouse_id: p.warehouse_id,
                    quantity_before: qty(p.quantity_before),
                    value_before: amount(p.value_before, fdp),
                    quantity_after: qty(p.quantity_after),
                    value_after: amount(p.value_after, fdp),
                })
                .collect(),
        )
    } else {
        None
    };
    let rate_json = |r: &Rate| json!([r.0.normalize().to_string(), r.1, r.2, r.3]);
    let fingerprint: [u8; 32] = Sha256::digest(
        json!({
            "action": "book", "receipt": receipt, "version": receipt_version,
            "order_version": order_version, "date": date, "active": true,
            "functional": functional.id, "lines": fp_lines,
            "values": lines.iter().map(|l| l.value.as_ref().map(|v| json!([v.net, v.landed, v.doc, v.value, rate_json(&v.rate)]))).collect::<Vec<_>>(),
            "costs": costs.iter().map(|c| json!([c.component, c.amount, c.currency.id, c.functional,
                rate_json(&c.rate), c.evidence_ref, c.allocations])).collect::<Vec<_>>(),
        })
        .to_string(),
    )
    .into();
    let review = BookReview {
        receipt_id: receipt,
        receipt_version,
        purchase_order_version: order_version,
        accounting_date: date,
        lines: lines
            .iter()
            .map(|l| {
                let v = l.value.as_ref();
                BookReviewLine {
                    receipt_line_id: l.row.id,
                    base_quantity: qty(l.row.base_quantity),
                    net_value: v.map(|v| amount(v.net, fdp)),
                    landed_value: v.map(|v| amount(v.landed, fdp)),
                    value: v.map(|v| amount(v.value, fdp)),
                    rate: v.map(|v| snapshot(&v.rate)),
                }
            })
            .collect(),
        landed_costs: costs
            .iter()
            .map(|c| BookReviewCost {
                client_component_ref: c.component,
                amount: amount(c.amount, c.currency.decimals),
                currency: c.currency.clone(),
                functional_amount: amount(c.functional, fdp),
                rate: snapshot(&c.rate),
                allocations: c
                    .allocations
                    .iter()
                    .map(|a| AllocationResource {
                        receipt_line_id: a.0,
                        functional_amount: amount(a.1, fdp),
                    })
                    .collect(),
            })
            .collect(),
        functional_currency: Some(functional),
        total: Some(amount(total, fdp)),
        journal: Some(BookJournal {
            debit_role: "inventory",
            credit_role: "grni",
        }),
        pool,
        review_fingerprint: hex::encode(fingerprint),
    };
    Ok(Plan {
        review,
        lines,
        costs,
        receipt_version,
    })
}

/// POST .../book/review: no writes.
pub async fn book_review(
    state: &AppState,
    access: &WorkspaceAccess,
    (order, receipt): (Uuid, Uuid),
    input: &BookInput,
) -> Result<BookReview, AppError> {
    let mut tx = state.db.begin().await?;
    let plan = prepare(
        &mut tx,
        access.workspace_id,
        (order, receipt),
        input,
        access.allows("accounting.read"),
    )
    .await?;
    tx.rollback().await?;
    Ok(plan.review)
}

/// POST .../book: add the selected lines' stock and value once.
pub async fn book(
    state: &AppState,
    access: &WorkspaceAccess,
    (order, receipt): (Uuid, Uuid),
    cmd: Command<Action<BookPayload>>,
) -> Result<Answer, AppError> {
    const OP: &str = "purchase_receipts_book";
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, access.user_id, &cmd).await? {
        return Ok(a);
    }
    let (input, fingerprint) = &cmd.input.payload;
    let plan = prepare(&mut tx, ws, (order, receipt), input, false).await?;
    if plan.receipt_version != cmd.input.expected_version
        || &plan.review.review_fingerprint != fingerprint
    {
        return Err(stale());
    }
    let date = plan.review.accounting_date;
    for l in &plan.lines {
        let row = &l.row;
        let slot = Slot {
            product_id: row.product_id,
            warehouse_id: row.warehouse_id,
            bin: row
                .bin_id
                .zip(row.shelf_id)
                .map(|(bin_id, shelf_id)| Bin { shelf_id, bin_id }),
            lot_id: row.lot_id,
        };
        let cause = Cause {
            reason: Some("purchase receipt"),
            user_id: Some(access.user_id),
            related: Some(("purchase_receipt_line", row.id)),
            value: Valued::Receipt {
                line: row.id,
                value: l.value.as_ref().map_or(0, |v| v.value),
                date,
            },
            ..Cause::default()
        };
        match stock::mutate(&mut tx, ws, slot, row.base_quantity, Floor::None, &cause).await? {
            Mutated::Done { .. } => {}
            _ => {
                return Err(AppError::Unprocessable(
                    "This stock level would leave the supported range.".into(),
                ));
            }
        }
        let booked = Booked {
            line: row.id,
            po_line: row.po_line_id,
            quantity: row.base_quantity,
            date,
            value: l.value.as_ref().map(|v| (v.net, v.landed, v.doc, v.rate)),
        };
        repo::mark_booked(&mut tx, ws, &booked, access.user_id).await?;
    }
    for c in &plan.costs {
        let landed = NewLanded {
            component: c.component,
            amount: c.amount,
            currency: c.currency.id,
            functional: c.functional,
            rate: c.rate,
            evidence_ref: &c.evidence_ref,
            allocations: &c.allocations,
        };
        repo::insert_landed(&mut tx, ws, (order, receipt), &landed, access.user_id).await?;
    }
    repo::touch_receipt(&mut tx, ws, receipt).await?;
    repo::touch(&mut tx, ws, order).await?;
    let answer = ok(&get(
        &mut tx,
        ws,
        order,
        receipt,
        access.allows("accounting.read"),
    )
    .await?);
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[receipt]).await?;
    tx.commit().await?;
    Ok(answer)
}
