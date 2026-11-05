//! Packing and transfer rules. Only a lot whose lineage reaches a supply batch is packed or
//! sent; a packed lot (or one received already packed) is never packed again. A transfer
//! leaves over a live link this workspace granted and arrives only at the link's other side,
//! which receives it once into a new lot of its own. Stock moves in the same transaction,
//! through the stock seam, and never below what a lot holds free.

use serde_json::json;
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::workspace::WorkspaceAccess,
    idempotency::{self, Actor, Answer, Key},
    modules::{
        farm_supply::service::lineage,
        lots::{
            repo::{self as lots, LotRow, NewEvent},
            service::{check_sendable, insert_lot},
        },
        products::repo as products,
        stock::{
            repo::{self as stock, Cause, Floor, Mutated, Slot},
            service::tracks_stock,
        },
        supply_chain::{
            dto::{NewDispatch, NewPacking, NewReceipt, PackedLotResource, SupplyTransferResource},
            repo::{self, Filter, TransferRow},
        },
        warehouses::repo as warehouses,
    },
};

const PACKED: &str = "A packed lot can't be packed again.";
const NOT_SUPPLY: &str = "Only a lot from a supply batch or a supply transfer can be packed.";
const NOT_SUPPLY_SEND: &str = "Only a lot from a supply batch can be sent over a supply link.";
const NOT_LIVE: &str = "The supply link is not live.";
const NOT_RECEIVER: &str = "Only the receiving workspace receives a transfer.";
const RECEIVED: &str = "This transfer was already received.";

fn parse(id: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(id).map_err(|_| AppError::NotFound)
}

/// Field errors for a product that isn't a live stock-tracked one here, or a foreign warehouse.
async fn check_product_warehouse(
    state: &AppState,
    ws: Uuid,
    product_id: Uuid,
    warehouse_id: Uuid,
) -> Result<(), AppError> {
    let mut errors = std::collections::BTreeMap::new();
    let product = products::find(&state.db, ws, product_id).await?;
    if !product.as_ref().is_some_and(tracks_stock) {
        errors.insert(
            "product_id".into(),
            vec!["The selected product id is invalid.".into()],
        );
    }
    if !warehouses::exists(&state.db, ws, warehouse_id).await? {
        errors.insert(
            "warehouse_id".into(),
            vec!["The selected warehouse id is invalid.".into()],
        );
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(AppError::Validation(errors))
    }
}

fn short(field: &str, m: Mutated) -> Result<(), AppError> {
    match m {
        Mutated::Done { .. } => Ok(()),
        Mutated::Short { level, reserved } => Err(AppError::field(
            field,
            format!(
                "Not enough in this lot here. Free: {}",
                (level - reserved).normalize()
            ),
        )),
        Mutated::OutOfRange => Err(AppError::field(
            field,
            "The stock level would be out of range.",
        )),
    }
}

fn new_lot(product_id: Uuid, code: String, origin: Option<String>) -> LotRow {
    LotRow {
        id: Uuid::nil(),
        product_id,
        code,
        public_code: String::new(),
        status: "released".into(),
        produced_on: None,
        best_before: None,
        expires_on: None,
        origin,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }
}

/// POST /lots/{lot}/packings: the child lot, its stock and the parent's `packed` event.
pub async fn pack(
    state: &AppState,
    access: &WorkspaceAccess,
    id: &str,
    p: NewPacking,
    key: Option<Key>,
) -> Result<Answer, AppError> {
    let ws = access.workspace_id;
    let parent = lots::find(&state.db, ws, parse(id)?)
        .await?
        .ok_or(AppError::NotFound)?;
    let line = lineage(&mut *state.db.acquire().await?, parent.id).await?;
    match &line {
        None => return Err(AppError::Unprocessable(NOT_SUPPLY.into())),
        Some(l) if l.pack.is_some() => return Err(AppError::Unprocessable(PACKED.into())),
        Some(_) => {}
    }
    check_product_warehouse(state, ws, p.product_id, p.warehouse_id).await?;
    let mut tx = state.db.begin().await?;
    if let Some(a) =
        idempotency::replay(&mut tx, ws, Actor::User(access.user_id), key.as_ref()).await?
    {
        return Ok(a);
    }
    let parent = lots::locked(&mut tx, ws, parent.id)
        .await?
        .ok_or(AppError::NotFound)?;
    check_sendable(&parent)?;
    let mut child = new_lot(p.product_id, p.code, parent.origin.clone());
    child.best_before = p.best_before;
    child.expires_on = p.expires_on;
    let meta = json!({ "packed_from": parent.id });
    let child = insert_lot(&mut tx, ws, access.user_id, child, Some(meta)).await?;
    repo::set_pack(&mut tx, ws, child.id, parent.id, p.raw_kg, p.units).await?;
    let from = Slot {
        product_id: parent.product_id,
        warehouse_id: p.warehouse_id,
        bin: None,
        lot_id: Some(parent.id),
    };
    let to = Slot {
        product_id: child.product_id,
        warehouse_id: p.warehouse_id,
        bin: None,
        lot_id: Some(child.id),
    };
    stock::lock_all(&mut tx, ws, &[from, to]).await?;
    let cause = Cause {
        reason: Some("packing"),
        user_id: Some(access.user_id),
        related: Some(("lot_packing", child.id)),
        ..Cause::default()
    };
    short(
        "raw_kg",
        stock::mutate(&mut tx, ws, from, -p.raw_kg, Floor::Available, &cause).await?,
    )?;
    short(
        "units",
        stock::mutate(&mut tx, ws, to, p.units, Floor::None, &cause).await?,
    )?;
    let event = NewEvent {
        lot_id: parent.id,
        kind: "packed",
        from_status: None,
        to_status: None,
        user_id: access.user_id,
        meta: Some(
            json!({ "child_lot_id": child.id, "raw_kg": format!("{:.2}", p.raw_kg), "units": format!("{:.2}", p.units) }),
        ),
    };
    lots::add_event(&mut tx, ws, &event).await?;
    let packed = PackedLotResource {
        id: child.id,
        code: child.code,
        public_code: child.public_code,
        product_id: child.product_id,
        parent_lot_id: parent.id,
        pack_raw_kg: format!("{:.2}", p.raw_kg),
        pack_units: format!("{:.2}", p.units),
        best_before: child.best_before,
        expires_on: child.expires_on,
    };
    let answer = Answer::created(&json!({ "data": packed }));
    idempotency::finish(&mut tx, ws, key.as_ref(), &answer).await?;
    tx.commit().await?;
    Ok(answer)
}

async fn one(state: &AppState, ws: Uuid, id: Uuid) -> Result<TransferRow, AppError> {
    let (mut rows, _) = repo::list(&state.db, ws, Filter::id(id), 1, 0).await?;
    rows.pop().ok_or(AppError::NotFound)
}

/// POST /lots/{lot}/transfers: stock out of the lot, a dispatched transfer to the link's
/// other side. A retry under the same Idempotency-Key replays the first answer.
pub async fn dispatch(
    state: &AppState,
    access: &WorkspaceAccess,
    id: &str,
    d: NewDispatch,
    key: Option<Key>,
) -> Result<Answer, AppError> {
    let ws = access.workspace_id;
    // First, so a retry answers as before even if the link was revoked since.
    let mut tx = state.db.begin().await?;
    if let Some(answer) =
        idempotency::replay(&mut tx, ws, Actor::User(access.user_id), key.as_ref()).await?
    {
        return Ok(answer);
    }
    let lot = lots::find(&state.db, ws, parse(id)?)
        .await?
        .ok_or(AppError::NotFound)?;
    let link = repo::link(&state.db, d.link_id, ws)
        .await?
        .filter(|l| l.from_workspace_id == ws)
        .ok_or_else(|| AppError::field("link_id", "The selected link id is invalid."))?;
    if !link.live {
        return Err(AppError::Forbidden(NOT_LIVE));
    }
    if lineage(&mut *state.db.acquire().await?, lot.id)
        .await?
        .is_none()
    {
        return Err(AppError::Unprocessable(NOT_SUPPLY_SEND.into()));
    }
    if !warehouses::exists(&state.db, ws, d.warehouse_id).await? {
        return Err(AppError::field(
            "warehouse_id",
            "The selected warehouse id is invalid.",
        ));
    }
    let lot = lots::locked(&mut tx, ws, lot.id)
        .await?
        .ok_or(AppError::NotFound)?;
    check_sendable(&lot)?;
    let tid = repo::insert_transfer(
        &mut tx,
        d.link_id,
        ws,
        lot.id,
        d.warehouse_id,
        d.quantity,
        link.to_workspace_id,
        access.user_id,
    )
    .await?;
    let slot = Slot {
        product_id: lot.product_id,
        warehouse_id: d.warehouse_id,
        bin: None,
        lot_id: Some(lot.id),
    };
    stock::lock_all(&mut tx, ws, &[slot]).await?;
    let cause = Cause {
        reason: Some("supply transfer"),
        user_id: Some(access.user_id),
        related: Some(("supply_transfer", tid)),
        ..Cause::default()
    };
    short(
        "quantity",
        stock::mutate(&mut tx, ws, slot, -d.quantity, Floor::Available, &cause).await?,
    )?;
    let event = NewEvent {
        lot_id: lot.id,
        kind: "dispatched",
        from_status: None,
        to_status: None,
        user_id: access.user_id,
        meta: Some(json!({ "transfer_id": tid, "quantity": format!("{:.2}", d.quantity) })),
    };
    lots::add_event(&mut tx, ws, &event).await?;
    let row = repo::rows(&mut *tx, ws, Filter::id(tid), 1, 0)
        .await?
        .pop()
        .ok_or(AppError::NotFound)?;
    let answer = Answer::created(&json!({ "data": SupplyTransferResource::new(row, ws) }));
    idempotency::finish(&mut tx, ws, key.as_ref(), &answer).await?;
    tx.commit().await?;
    Ok(answer)
}

/// One page of the workspace's transfers (either side), newest first.
pub async fn list(
    state: &AppState,
    access: &WorkspaceAccess,
    f: Filter<'_>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<TransferRow>, i64), AppError> {
    Ok(repo::list(&state.db, access.workspace_id, f, limit, offset).await?)
}

/// POST /supply-transfers/{transfer}/receive: a new lot here with the received stock.
pub async fn receive(
    state: &AppState,
    access: &WorkspaceAccess,
    id: &str,
    r: NewReceipt,
) -> Result<TransferRow, AppError> {
    let ws = access.workspace_id;
    let t = one(state, ws, parse(id)?).await?;
    if t.to_workspace_id != ws {
        return Err(AppError::Forbidden(NOT_RECEIVER));
    }
    let live = repo::link(&state.db, t.link_id, ws)
        .await?
        .is_some_and(|l| l.live);
    if !live {
        return Err(AppError::Forbidden(NOT_LIVE));
    }
    check_product_warehouse(state, ws, r.product_id, r.warehouse_id).await?;
    let mut tx = state.db.begin().await?;
    let (sent, received, _) = repo::lock_incoming(&mut tx, t.id, ws)
        .await?
        .ok_or(AppError::NotFound)?;
    if received {
        return Err(AppError::Conflict(RECEIVED));
    }
    if r.quantity > sent {
        return Err(AppError::field(
            "quantity",
            format!(
                "The quantity must not be greater than the {} dispatched.",
                sent.normalize()
            ),
        ));
    }
    let meta = json!({ "supply_transfer_id": t.id });
    let lot = insert_lot(
        &mut tx,
        ws,
        access.user_id,
        new_lot(r.product_id, r.code, None),
        Some(meta),
    )
    .await?;
    repo::mark_received(&mut tx, t.id, lot.id, r.quantity, access.user_id).await?;
    let slot = Slot {
        product_id: lot.product_id,
        warehouse_id: r.warehouse_id,
        bin: None,
        lot_id: Some(lot.id),
    };
    stock::lock_all(&mut tx, ws, &[slot]).await?;
    let cause = Cause {
        reason: Some("supply transfer received"),
        user_id: Some(access.user_id),
        related: Some(("supply_transfer", t.id)),
        ..Cause::default()
    };
    short(
        "quantity",
        stock::mutate(&mut tx, ws, slot, r.quantity, Floor::None, &cause).await?,
    )?;
    tx.commit().await?;
    one(state, ws, t.id).await
}
