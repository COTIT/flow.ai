//! Shipment lifecycle: draft create/update (planning capacity, no stock hold), cancel, and
//! reviewed post that takes exactly the shipped quantity off the order's hold. Lock order:
//! command row → order FOR UPDATE → shipment FOR UPDATE → shared book gate → stock rows
//! (`reserve::commit`) → recognition journal (active books) → journal number last.

use std::collections::HashMap;

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::{
        list::{Page, Paginated},
        workspace::WorkspaceAccess,
    },
    idempotency::{self, Answer, Command},
    modules::{
        accounting::{
            dto::{Action, Create},
            service::{created, ok, stale},
        },
        orders::repo::{self as orders, LineRow as OrderLine, OrderRow},
        purchasing::{
            dto::{CancelPayload, PageQuery},
            service::qty,
        },
        shipments::{
            dto::{
                ShipmentInput, ShipmentLineResource, ShipmentPayload, ShipmentPostPayload,
                ShipmentPostReview, ShipmentResource,
            },
            recognition,
            repo::{self, LineRow, ShipmentRow},
        },
        stock::{
            repo::Cause,
            reserve::{self, Want},
        },
    },
};

const NOT_OPEN: &str = "Only an open order can ship.";

fn not_draft() -> AppError {
    AppError::Unprocessable("Only draft shipments can change.".into())
}

fn line_resource(l: &LineRow) -> ShipmentLineResource {
    ShipmentLineResource {
        id: l.id,
        order_line_id: l.order_line_id,
        product_id: l.product_id,
        title: l.title.clone(),
        quantity: qty(l.quantity),
    }
}

fn resource(s: ShipmentRow, lines: &[LineRow], ws: Uuid, open: bool) -> ShipmentResource {
    let allowed_actions = match (s.state.as_str(), open) {
        ("draft", true) => vec!["update", "post", "cancel"],
        ("draft", false) => vec!["cancel"],
        _ => vec![],
    };
    ShipmentResource {
        id: s.id,
        workspace_id: ws,
        order_id: s.order_id,
        version: s.version,
        note: s.note,
        lines: lines.iter().map(line_resource).collect(),
        accounting_date: s.accounting_date,
        recognition_id: s.recognition_id,
        created_by: s.created_by,
        created_at: s.created_at,
        updated_at: s.updated_at,
        posted_by: s.posted_by,
        posted_at: s.posted_at,
        cancelled_at: s.cancelled_at,
        cancel_reason: s.cancel_reason,
        state: s.state,
        allowed_actions,
    }
}

/// Shipment rows of `order` as resources, lines in one query.
async fn resources(
    conn: &mut PgConnection,
    ws: Uuid,
    rows: Vec<ShipmentRow>,
    open: bool,
) -> Result<Vec<ShipmentResource>, AppError> {
    let ids: Vec<Uuid> = rows.iter().map(|s| s.id).collect();
    let mut by: HashMap<Uuid, Vec<LineRow>> = HashMap::new();
    for l in repo::lines(&mut *conn, ws, &ids).await? {
        by.entry(l.shipment_id).or_default().push(l);
    }
    Ok(rows
        .into_iter()
        .map(|s| {
            let lines = by.remove(&s.id).unwrap_or_default();
            resource(s, &lines, ws, open)
        })
        .collect())
}

/// Whether `order` (of the workspace) is open; 404 when unknown.
async fn order_open(conn: &mut PgConnection, ws: Uuid, order: Uuid) -> Result<bool, AppError> {
    let o = orders::find(&mut *conn, ws, order)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(o.status == "open")
}

/// One shipment of `order`; 404 outside the workspace or order.
pub async fn get(
    conn: &mut PgConnection,
    ws: Uuid,
    order: Uuid,
    id: Uuid,
) -> Result<ShipmentResource, AppError> {
    let open = order_open(conn, ws, order).await?;
    let (rows, _) = repo::shipments(&mut *conn, ws, order, Some(id), (1, 0)).await?;
    resources(conn, ws, rows, open)
        .await?
        .pop()
        .ok_or(AppError::NotFound)
}

/// GET /orders/{id}/shipments: newest first.
pub async fn list(
    state: &AppState,
    ws: Uuid,
    order: Uuid,
    q: &PageQuery,
) -> Result<Paginated<ShipmentResource>, AppError> {
    let mut conn = state.db.acquire().await?;
    let open = order_open(&mut conn, ws, order).await?;
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    let (rows, total) =
        repo::shipments(&mut conn, ws, order, None, (page.size, page.offset())).await?;
    let data = resources(&mut conn, ws, rows, open).await?;
    Ok(Paginated::new(data, total, page))
}

/// Lock the order (404 outside the workspace); it and its lines.
async fn lock_order(
    conn: &mut PgConnection,
    ws: Uuid,
    order: Uuid,
) -> Result<(OrderRow, Vec<OrderLine>), AppError> {
    let o = orders::lock(&mut *conn, ws, order)
        .await?
        .ok_or(AppError::NotFound)?;
    let lines = orders::lines(&mut *conn, ws, &[order]).await?;
    Ok((o, lines))
}

/// Draft capacity under the order lock: per order line, Σ draft + posted (other shipments)
/// + this one ≤ ordered − returned.
async fn check_capacity(
    conn: &mut PgConnection,
    ws: Uuid,
    order: &[OrderLine],
    input: &[(Uuid, Decimal)],
    except: Option<Uuid>,
) -> Result<(), AppError> {
    let Some(first) = order.first() else {
        return Err(AppError::NotFound);
    };
    let planned = repo::planned(&mut *conn, ws, first.order_id, except).await?;
    let mut r = crate::http::json::Rules::default();
    for (i, (id, q)) in input.iter().enumerate() {
        let Some(l) = order.iter().find(|l| l.id == *id) else {
            r.add(
                &format!("payload.lines.{i}.order_line_id"),
                "The selected order line is invalid.",
            );
            continue;
        };
        let used = planned
            .iter()
            .find(|p| p.0 == *id)
            .map_or(Decimal::ZERO, |p| p.1);
        let left = l.quantity - l.returned - used;
        r.check(
            &format!("payload.lines.{i}.quantity"),
            *q <= left,
            format!("Only {} is left to ship on this line.", qty(left)),
        );
    }
    r.into_result()
}

/// Insert a draft of `input` on the locked open order; its id.
pub async fn create_locked(
    conn: &mut PgConnection,
    ws: Uuid,
    (o, lines): (&OrderRow, &[OrderLine]),
    input: &ShipmentInput,
    by: Uuid,
) -> Result<Uuid, AppError> {
    if o.status != "open" {
        return Err(AppError::Conflict(NOT_OPEN));
    }
    check_capacity(&mut *conn, ws, lines, &input.lines, None).await?;
    let id = repo::insert(&mut *conn, ws, o.id, input.note.as_deref(), by).await?;
    repo::replace_lines(conn, ws, (o.id, id), &input.lines).await?;
    Ok(id)
}

/// POST /orders/{id}/shipments: a draft.
pub async fn create(
    state: &AppState,
    access: &WorkspaceAccess,
    order: Uuid,
    cmd: Command<Create<ShipmentPayload>>,
) -> Result<Answer, AppError> {
    const OP: &str = "shipments_create";
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, access.user_id, &cmd).await? {
        return Ok(a);
    }
    let (o, lines) = lock_order(&mut tx, ws, order).await?;
    let id = create_locked(
        &mut tx,
        ws,
        (&o, &lines),
        &cmd.input.payload,
        access.user_id,
    )
    .await?;
    let answer = created(&get(&mut tx, ws, order, id).await?);
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[id]).await?;
    tx.commit().await?;
    Ok(answer)
}

/// Lock order then shipment, check the caller's version; the order, its lines and the
/// shipment state.
async fn lock_both(
    conn: &mut PgConnection,
    ws: Uuid,
    (order, id): (Uuid, Uuid),
    expected: i64,
) -> Result<(OrderRow, Vec<OrderLine>, String), AppError> {
    let (o, lines) = lock_order(&mut *conn, ws, order).await?;
    let (state, version) = repo::lock(conn, ws, order, id)
        .await?
        .ok_or(AppError::NotFound)?;
    if version != expected {
        return Err(stale());
    }
    Ok((o, lines, state))
}

/// PATCH /orders/{id}/shipments/{shipment}: replace a draft's lines and note.
pub async fn update(
    state: &AppState,
    access: &WorkspaceAccess,
    (order, id): (Uuid, Uuid),
    cmd: Command<Action<ShipmentPayload>>,
) -> Result<Answer, AppError> {
    const OP: &str = "shipments_update";
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, access.user_id, &cmd).await? {
        return Ok(a);
    }
    let (o, lines, st) = lock_both(&mut tx, ws, (order, id), cmd.input.expected_version).await?;
    if st != "draft" {
        return Err(not_draft());
    }
    if o.status != "open" {
        return Err(AppError::Conflict(NOT_OPEN));
    }
    let input = &cmd.input.payload;
    check_capacity(&mut tx, ws, &lines, &input.lines, Some(id)).await?;
    repo::update_note(&mut *tx, ws, id, input.note.as_deref()).await?;
    repo::replace_lines(&mut tx, ws, (order, id), &input.lines).await?;
    let answer = ok(&get(&mut tx, ws, order, id).await?);
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[id]).await?;
    tx.commit().await?;
    Ok(answer)
}

/// POST .../cancel: a draft only; frees its planned capacity.
pub async fn cancel(
    state: &AppState,
    access: &WorkspaceAccess,
    (order, id): (Uuid, Uuid),
    cmd: Command<Action<CancelPayload>>,
) -> Result<Answer, AppError> {
    const OP: &str = "shipments_cancel";
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, access.user_id, &cmd).await? {
        return Ok(a);
    }
    let (_, _, st) = lock_both(&mut tx, ws, (order, id), cmd.input.expected_version).await?;
    if st != "draft" {
        return Err(not_draft());
    }
    repo::cancel(&mut *tx, ws, id, &cmd.input.payload, access.user_id).await?;
    let answer = ok(&get(&mut tx, ws, order, id).await?);
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[id]).await?;
    tx.commit().await?;
    Ok(answer)
}

/// What posting `id` does now, with its fingerprint (order locked by the caller).
async fn prepare(
    conn: &mut PgConnection,
    ws: Uuid,
    o: &OrderRow,
    id: Uuid,
    date: NaiveDate,
) -> Result<ShipmentPostReview, AppError> {
    let s = get(&mut *conn, ws, o.id, id).await?;
    if s.state != "draft" {
        return Err(not_draft());
    }
    if o.status != "open" {
        return Err(AppError::Conflict(NOT_OPEN));
    }
    let lines: Vec<(Uuid, Decimal)> = repo::lines(&mut *conn, ws, &[id])
        .await?
        .iter()
        .map(|l| (l.order_line_id, l.quantity))
        .collect();
    let preview = recognition::preview(&mut *conn, ws, o, &lines).await?;
    let fingerprint: [u8; 32] = Sha256::digest(
        json!({
            "action": "post", "shipment": id, "version": s.version, "date": date,
            "lines": s.lines.iter().map(|l| json!([l.order_line_id, l.quantity])).collect::<Vec<_>>(),
            "recognition": preview.as_ref().map(|p| json!([p.net, p.tax])),
        })
        .to_string(),
    )
    .into();
    Ok(ShipmentPostReview {
        shipment_id: id,
        version: s.version,
        accounting_date: date,
        lines: s.lines,
        recognition: preview,
        review_fingerprint: hex::encode(fingerprint),
    })
}

/// POST .../post/review: no writes.
pub async fn post_review(
    state: &AppState,
    ws: Uuid,
    (order, id): (Uuid, Uuid),
    date: NaiveDate,
) -> Result<ShipmentPostReview, AppError> {
    let mut tx = state.db.begin().await?;
    let (o, _) = lock_order(&mut tx, ws, order).await?;
    let review = prepare(&mut tx, ws, &o, id, date).await?;
    tx.rollback().await?;
    Ok(review)
}

/// Post the draft `id` of the locked open order: take exactly its quantities off the hold
/// (stock lines), add them to the lines' delivered quantity, recognise the sale in an active
/// book, stamp it posted.
pub async fn post_locked(
    conn: &mut PgConnection,
    ws: Uuid,
    (o, order_lines): (&OrderRow, &[OrderLine]),
    id: Uuid,
    (date, by): (NaiveDate, Uuid),
) -> Result<(), AppError> {
    let lines = repo::lines(&mut *conn, ws, &[id]).await?;
    // Refusals (foreign currency) before any stock or GL effect.
    let active = recognition::check(&mut *conn, ws, o.currency_id).await?;
    let shipped: Vec<(Uuid, Decimal)> = lines
        .iter()
        .map(|l| (l.order_line_id, l.quantity))
        .collect();
    let mut wants = Vec::new();
    let mut rows = Vec::new();
    for l in &lines {
        let Some(ol) = order_lines.iter().find(|x| x.id == l.order_line_id) else {
            return Err(AppError::Internal(
                "shipment line without its order line".into(),
            ));
        };
        if ol.tracked {
            wants.push(Want {
                product_id: ol.product_id,
                quantity: l.quantity,
            });
        }
        rows.push((ol.id, ol.picked, ol.deducted + l.quantity, ol.returned));
    }
    let reason = format!("order {} shipment", o.number);
    let cause = Cause {
        reason: Some(&reason),
        user_id: Some(by),
        related: Some(("order", o.id)),
        ..Cause::default()
    };
    reserve::commit(&mut *conn, ws, o.warehouse_id, &wants, &cause).await?;
    orders::set_line_quantities(&mut *conn, ws, &rows).await?;
    if active {
        recognition::shipment(&mut *conn, ws, o, (id, &shipped), (date, by)).await?;
    }
    repo::mark_posted(conn, ws, id, date, by).await?;
    Ok(())
}

/// POST .../post: post the reviewed draft once.
pub async fn post(
    state: &AppState,
    access: &WorkspaceAccess,
    (order, id): (Uuid, Uuid),
    cmd: Command<Action<ShipmentPostPayload>>,
) -> Result<Answer, AppError> {
    const OP: &str = "shipments_post";
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, access.user_id, &cmd).await? {
        return Ok(a);
    }
    let (o, lines, st) = lock_both(&mut tx, ws, (order, id), cmd.input.expected_version).await?;
    if st != "draft" {
        return Err(not_draft());
    }
    let (date, fingerprint) = &cmd.input.payload;
    let review = prepare(&mut tx, ws, &o, id, *date).await?;
    if &review.review_fingerprint != fingerprint {
        return Err(stale());
    }
    post_locked(&mut tx, ws, (&o, &lines), id, (*date, access.user_id)).await?;
    let answer = ok(&get(&mut tx, ws, order, id).await?);
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[id]).await?;
    tx.commit().await?;
    Ok(answer)
}
