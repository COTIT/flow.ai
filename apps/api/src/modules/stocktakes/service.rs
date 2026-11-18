//! Stocktake rules (Flow ERP M6, masterplan D10/§10). Start pauses the warehouse with a
//! committed `counting` row (no transaction stays open while people count); stock writers
//! refuse it (`stock::repo::record`, `reserve`). Post books counted − frozen through the stock
//! layer (stock, value, GL once) and releases the pause in the same transaction; cancel keeps
//! the count. Both rerun hold reallocation. Lock order: gate, stocktake, warehouse FOR UPDATE,
//! pools, stock rows (masterplan §6).

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
        json::Rules,
        list::{Page, Paginated},
        workspace::WorkspaceAccess,
    },
    idempotency::{self, Answer, Command},
    modules::{
        accounting::{
            dto::{Acted, Action, Create},
            gate,
            service::{created, ok, stale},
        },
        purchasing::{
            self,
            service::{amount, qty},
        },
        stock::{
            repo::{self as stock, Bin, Cause, Floor, Mutated, Slot},
            reserve,
            value::{self, Valued},
        },
        warehouses,
    },
};

use super::{
    dto::{
        CountPayload, CreatePayload, PageQuery, PostReview, PostReviewLine, StartPayload,
        StocktakeCancelPayload, StocktakeLine, StocktakePostPayload, StocktakeResource,
    },
    repo::{self, Head, Line},
};

fn unprocessable(message: &str) -> AppError {
    AppError::Unprocessable(message.into())
}

fn reason(code: &'static str, message: &str) -> AppError {
    AppError::Reason {
        status: axum::http::StatusCode::UNPROCESSABLE_ENTITY,
        code,
        message: message.into(),
    }
}

fn resource(h: Head, lines: Option<(Vec<Line>, i32)>) -> StocktakeResource {
    StocktakeResource {
        id: h.id,
        warehouse_id: h.warehouse_id,
        state: h.state,
        version: h.version,
        reason: h.reason,
        cancel_reason: h.cancel_reason,
        accounting_date: h.accounting_date,
        created_at: h.created_at,
        started_at: h.started_at,
        posted_at: h.posted_at,
        cancelled_at: h.cancelled_at,
        lines: lines.map(|(lines, dp)| {
            lines
                .into_iter()
                .map(|l| StocktakeLine {
                    stock_identity: l.stock_id,
                    product_id: l.product_id,
                    bin_id: l.bin_id,
                    lot_id: l.lot_id,
                    expected_quantity: qty(l.expected),
                    counted_quantity: l.counted.map(qty),
                    positive_adjustment_value: l.value.map(|v| amount(v, dp as i16)),
                })
                .collect()
        }),
    }
}

/// One stocktake with its lines.
pub async fn get(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
) -> Result<StocktakeResource, AppError> {
    let h = repo::head(conn, ws, id, false)
        .await?
        .ok_or(AppError::NotFound)?;
    let lines = repo::lines(&mut *conn, ws, id).await?;
    let dp = repo::functional_dp(&mut *conn, ws).await?;
    Ok(resource(h, Some((lines, dp))))
}

/// GET /stocktakes: headers, newest first.
pub async fn list(
    state: &AppState,
    ws: Uuid,
    q: &PageQuery,
) -> Result<Paginated<StocktakeResource>, AppError> {
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    let mut conn = state.db.acquire().await?;
    let (rows, total) = repo::list(&mut conn, ws, (page.size, page.offset())).await?;
    let data = rows.into_iter().map(|h| resource(h, None)).collect();
    Ok(Paginated::new(data, total, page))
}

/// POST /stocktakes: a draft for a live warehouse.
pub async fn create(
    state: &AppState,
    access: &WorkspaceAccess,
    cmd: Command<Create<CreatePayload>>,
) -> Result<Answer, AppError> {
    const OP: &str = "stocktakes_create";
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, access.user_id, &cmd).await? {
        return Ok(a);
    }
    let input = &cmd.input.payload;
    if stock::live_warehouses(&mut *tx, ws, &[input.warehouse_id])
        .await?
        .is_empty()
    {
        return Err(AppError::field(
            "payload.warehouse_id",
            "The selected warehouse is invalid.",
        ));
    }
    let id = repo::insert(
        &mut *tx,
        ws,
        input.warehouse_id,
        &input.reason,
        access.user_id,
    )
    .await?;
    let answer = created(&get(&mut tx, ws, id).await?);
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[id]).await?;
    tx.commit().await?;
    Ok(answer)
}

/// Gate, then the stocktake FOR UPDATE at the caller's version.
async fn lock(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
    expected: i64,
) -> Result<Head, AppError> {
    gate::shared(&mut *conn, &[ws]).await?;
    let h = repo::head(conn, ws, id, true)
        .await?
        .ok_or(AppError::NotFound)?;
    if h.version != expected {
        return Err(stale());
    }
    Ok(h)
}

/// The warehouse FOR UPDATE: open stock writers (FOR SHARE) finish first, new ones wait.
async fn lock_warehouse(
    conn: &mut PgConnection,
    ws: Uuid,
    warehouse: Uuid,
) -> Result<(), AppError> {
    // A start may wait on a writer; give up instead of starving the pool.
    sqlx::query!("SET LOCAL lock_timeout = '5s'")
        .execute(&mut *conn)
        .await?;
    if !warehouses::repo::lock_for_delete(conn, ws, warehouse).await? {
        return Err(AppError::NotFound);
    }
    Ok(())
}

/// POST /stocktakes/{id}/start: pause the warehouse and freeze its stock rows.
pub async fn start(
    state: &AppState,
    access: &WorkspaceAccess,
    id: Uuid,
    cmd: Command<Action<StartPayload>>,
) -> Result<Answer, AppError> {
    const OP: &str = "stocktakes_start";
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, access.user_id, &cmd).await? {
        return Ok(a);
    }
    let h = lock(&mut tx, ws, id, cmd.input.expected_version).await?;
    if h.state != "draft" {
        return Err(unprocessable("Only a draft stocktake can start."));
    }
    lock_warehouse(&mut tx, ws, h.warehouse_id).await?;
    if stock::paused(&mut *tx, ws, h.warehouse_id).await? {
        return Err(stock::paused_error());
    }
    if purchasing::repo::pending(&mut *tx, ws, h.warehouse_id, None).await? {
        return Err(reason(
            "receipt_booking_required",
            "Goods received here are waiting to be booked. Book them before counting.",
        ));
    }
    repo::snapshot(&mut *tx, ws, id, h.warehouse_id).await?;
    repo::set_state(&mut *tx, ws, id, "counting", access.user_id, (None, None)).await?;
    let answer = ok(&get(&mut tx, ws, id).await?);
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[id]).await?;
    tx.commit().await?;
    Ok(answer)
}

/// PUT /stocktakes/{id}/counts: store counted quantities (and found-stock values).
pub async fn count(
    state: &AppState,
    access: &WorkspaceAccess,
    id: Uuid,
    cmd: Command<Action<CountPayload>>,
) -> Result<Answer, AppError> {
    const OP: &str = "stocktakes_count";
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, access.user_id, &cmd).await? {
        return Ok(a);
    }
    let h = lock(&mut tx, ws, id, cmd.input.expected_version).await?;
    if h.state != "counting" {
        return Err(unprocessable(
            "Only a stocktake being counted takes counts.",
        ));
    }
    let lines = &cmd.input.payload;
    let ids: Vec<Uuid> = lines.iter().map(|l| l.stock_id).collect();
    let counted: Vec<Decimal> = lines.iter().map(|l| l.counted).collect();
    // Setup books never read the value: it is not stored.
    let active = value::active(&mut tx, ws).await?;
    let sent: Vec<Option<Decimal>> = lines.iter().map(|l| l.value.filter(|_| active)).collect();
    let dp = repo::functional_dp(&mut *tx, ws).await?;
    let checked = repo::check_counts(&mut *tx, ws, id, &ids, &sent, dp).await?;
    let mut r = Rules::default();
    for (i, ((known, minor), v)) in checked.iter().zip(&sent).enumerate() {
        r.check(
            &format!("payload.lines.{i}.stock_identity"),
            *known,
            "This stock row isn't part of the count.",
        );
        r.check(
            &format!("payload.lines.{i}.positive_adjustment_value"),
            v.is_none() || minor.is_some(),
            format!("The value can have at most {dp} decimal places."),
        );
    }
    r.into_result()?;
    let minors: Vec<Option<i64>> = checked.into_iter().map(|c| c.1).collect();
    repo::set_counts(&mut *tx, ws, id, &ids, &counted, &minors, access.user_id).await?;
    repo::bump(&mut *tx, ws, id).await?;
    let answer = ok(&get(&mut tx, ws, id).await?);
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[id]).await?;
    tx.commit().await?;
    Ok(answer)
}

/// Rerun hold reallocation off lots picking skips, for this warehouse only.
async fn reallocate(conn: &mut PgConnection, ws: Uuid, warehouse: Uuid) -> Result<(), AppError> {
    for (lot, product) in repo::invalid_held_lots(&mut *conn, ws, warehouse).await? {
        reserve::move_off_lot(conn, ws, lot, product, Some(warehouse)).await?;
    }
    Ok(())
}

/// Book the count in the caller's transaction (the review rolls it back): pause released, one
/// movement per changed line, reallocation. Returns the changed lines with their value.
async fn book(
    conn: &mut PgConnection,
    ws: Uuid,
    h: &Head,
    date: NaiveDate,
    by: Uuid,
) -> Result<Vec<PostReviewLine>, AppError> {
    if h.state != "counting" {
        return Err(unprocessable(
            "Only a stocktake being counted can be posted.",
        ));
    }
    lock_warehouse(conn, ws, h.warehouse_id).await?;
    let lines = repo::lines(&mut *conn, ws, h.id).await?;
    let mut r = Rules::default();
    for (i, l) in lines.iter().enumerate() {
        r.check(
            &format!("lines.{i}.counted_quantity"),
            l.counted.is_some(),
            "Count every line before posting. A missing count is not zero.",
        );
    }
    r.into_result()?;
    let changed: Vec<(&Line, Decimal)> = lines
        .iter()
        .filter_map(|l| l.counted.map(|c| (l, c - l.expected)))
        .filter(|(_, d)| !d.is_zero())
        .collect();
    for (l, _) in &changed {
        if l.counted.is_some_and(|c| c < l.reserved) {
            return Err(unprocessable(
                "A count is below stock held for orders. Release those holds or recount.",
            ));
        }
    }
    // Pause off first: this transaction's own movements pass; others wait on FOR UPDATE.
    repo::set_state(&mut *conn, ws, h.id, "posted", by, (Some(date), None)).await?;
    let active = value::active(conn, ws).await?;
    let mut explicit: HashMap<Uuid, Option<i64>> = HashMap::new();
    if active {
        let products: Vec<Uuid> = changed.iter().map(|(l, _)| l.product_id).collect();
        let pools: HashMap<Uuid, Decimal> =
            repo::pool_quantities(&mut *conn, ws, h.warehouse_id, &products)
                .await?
                .into_iter()
                .collect();
        for (l, d) in &changed {
            if *d < Decimal::ZERO {
                continue;
            }
            let empty = pools.get(&l.product_id).is_none_or(|q| q.is_zero());
            match (empty, l.value) {
                (true, None) => return Err(value::value_required()),
                (false, Some(_)) => {
                    return Err(reason(
                        "stocktake_value_not_allowed",
                        "This product already has a cost here. Found stock takes the average cost; remove the value.",
                    ));
                }
                (_, v) => {
                    explicit.insert(l.stock_id, v);
                }
            }
        }
    }
    let slot = |l: &Line| Slot {
        product_id: l.product_id,
        warehouse_id: h.warehouse_id,
        bin: l
            .bin_id
            .zip(l.shelf_id)
            .map(|(bin_id, shelf_id)| Bin { shelf_id, bin_id }),
        lot_id: l.lot_id,
    };
    let slots: Vec<Slot> = changed.iter().map(|(l, _)| slot(l)).collect();
    stock::lock_all(conn, ws, &slots).await?;
    for (l, d) in &changed {
        let cause = Cause {
            reason: Some("stocktake"),
            user_id: Some(by),
            related: Some(("stocktake", h.id)),
            value: Valued::Count {
                value: explicit.get(&l.stock_id).copied().flatten(),
                date,
            },
            ..Cause::default()
        };
        match stock::mutate(conn, ws, slot(l), *d, Floor::Available, &cause).await? {
            Mutated::Done { .. } => {}
            Mutated::Short { .. } => {
                return Err(unprocessable(
                    "A count is below stock held for orders. Release those holds or recount.",
                ));
            }
            Mutated::OutOfRange => {
                return Err(unprocessable(
                    "This stock level would leave the supported range.",
                ));
            }
        }
    }
    let values: HashMap<Uuid, i64> = repo::values(&mut *conn, ws, h.id)
        .await?
        .into_iter()
        .collect();
    let dp = repo::functional_dp(&mut *conn, ws).await? as i16;
    reallocate(conn, ws, h.warehouse_id).await?;
    Ok(changed
        .iter()
        .map(|(l, d)| PostReviewLine {
            stock_identity: l.stock_id,
            delta: qty(*d),
            value: values.get(&l.stock_id).map(|v| amount(*v, dp)),
        })
        .collect())
}

fn fingerprint(h: &Head, date: NaiveDate, lines: &[PostReviewLine]) -> String {
    let lines: Vec<_> = lines
        .iter()
        .map(|l| json!([l.stock_identity, l.delta, l.value]))
        .collect();
    hex::encode(Sha256::digest(
        json!({ "id": h.id, "version": h.version, "date": date, "lines": lines }).to_string(),
    ))
}

/// POST /stocktakes/{id}/post/review: the real post, rolled back (exact values, no writes).
pub async fn post_review(
    state: &AppState,
    access: &WorkspaceAccess,
    id: Uuid,
    date: NaiveDate,
) -> Result<PostReview, AppError> {
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    gate::shared(&mut tx, &[ws]).await?;
    let h = repo::head(&mut tx, ws, id, true)
        .await?
        .ok_or(AppError::NotFound)?;
    let lines = book(&mut tx, ws, &h, date, access.user_id).await?;
    tx.rollback().await?;
    Ok(PostReview {
        accounting_date: date,
        review_fingerprint: fingerprint(&h, date, &lines),
        lines,
    })
}

/// POST /stocktakes/{id}/post: book the reviewed count once and release the pause.
pub async fn post(
    state: &AppState,
    access: &WorkspaceAccess,
    id: Uuid,
    cmd: Command<Action<StocktakePostPayload>>,
) -> Result<Answer, AppError> {
    const OP: &str = "stocktakes_post";
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, access.user_id, &cmd).await? {
        return Ok(a);
    }
    let Acted {
        expected_version,
        payload: (date, sent),
        ..
    } = &cmd.input;
    let h = lock(&mut tx, ws, id, *expected_version).await?;
    let lines = book(&mut tx, ws, &h, *date, access.user_id).await?;
    if &fingerprint(&h, *date, &lines) != sent {
        return Err(stale());
    }
    let answer = ok(&get(&mut tx, ws, id).await?);
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[id]).await?;
    tx.commit().await?;
    Ok(answer)
}

/// POST /stocktakes/{id}/cancel: keep the count, move nothing, release the pause.
pub async fn cancel(
    state: &AppState,
    access: &WorkspaceAccess,
    id: Uuid,
    cmd: Command<Action<StocktakeCancelPayload>>,
) -> Result<Answer, AppError> {
    const OP: &str = "stocktakes_cancel";
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, access.user_id, &cmd).await? {
        return Ok(a);
    }
    let h = lock(&mut tx, ws, id, cmd.input.expected_version).await?;
    let counting = match h.state.as_str() {
        "draft" => false,
        "counting" => true,
        _ => {
            return Err(unprocessable(
                "Only a draft or counting stocktake can be cancelled.",
            ));
        }
    };
    if counting {
        lock_warehouse(&mut tx, ws, h.warehouse_id).await?;
    }
    let why = cmd.input.payload.as_str();
    repo::set_state(
        &mut *tx,
        ws,
        id,
        "cancelled",
        access.user_id,
        (None, Some(why)),
    )
    .await?;
    if counting {
        reallocate(&mut tx, ws, h.warehouse_id).await?;
    }
    let answer = ok(&get(&mut tx, ws, id).await?);
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[id]).await?;
    tx.commit().await?;
    Ok(answer)
}
