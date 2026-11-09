//! Stock rules: the product must track stock (physical type with `has_stock`), the warehouse
//! and bin must be live rows of the workspace, and every write goes through `repo::mutate`
//! or the set-level path under the row lock.

use std::collections::BTreeMap;

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde_json::Value;
use sqlx::PgConnection;

use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::json::human,
    idempotency::{self, Actor, Answer, Key},
    modules::{
        assistant::preconditions,
        lots::repo::{self as lots, LotRow},
        products::{
            dto::PHYSICAL, repo as products, repo::ProductRow, service as products_service,
        },
        stock::{
            dto::{Entry, Move, Mutate, NO_WAREHOUSE, SetRequirements, Transfer},
            repo::{self, Bin, Cause, Floor, ListFilter, Movement, MovementFilter, Mutated, Slot},
            resource,
        },
        warehouses::{
            repo::{self as warehouses, CompactRow},
            service as warehouse_service,
        },
    },
    money::round,
};

/// Laravel EnoughStockRule message.
const NOT_AVAILABLE: &str = "The mutation value is higher then the available stock";
/// The new level would leave the column's range.
const OUT_OF_RANGE: &str = "The stock level would be out of range.";

/// Laravel Product::shouldMutateStock.
pub fn tracks_stock(p: &ProductRow) -> bool {
    PHYSICAL.contains(&p.kind.as_str()) && p.has_stock
}

/// POST /products/{id}/stocks (Laravel MutateStock behind EnoughStockRule). Untracked
/// products and a zero change are no-ops.
pub async fn mutate(
    state: &AppState,
    workspace_id: Uuid,
    product_id: &str,
    m: Mutate,
    user_id: Uuid,
    key: Option<Key>,
) -> Result<Answer, AppError> {
    let product = product(state, workspace_id, product_id).await?;
    let warehouse_id = m.warehouse_id;
    let slot = slot(state, workspace_id, product.id, warehouse_id, m.bin_id).await?;
    let mut tx = state.db.begin().await?;
    if let Some(a) =
        idempotency::replay(&mut tx, workspace_id, Actor::User(user_id), key.as_ref()).await?
    {
        return Ok(a);
    }
    let ok = Answer::ok();
    // The column keeps 2 places: a change that rounds to 0 is a no-op, not an empty movement.
    if !tracks_stock(&product) || round(m.mutation_value, 2).is_zero() {
        idempotency::finish(&mut tx, workspace_id, key.as_ref(), &ok).await?;
        tx.commit().await?;
        return Ok(ok);
    }
    let cause = Cause {
        reason: m.reason.as_deref(),
        is_correction: m.is_correction,
        user_id: Some(user_id),
        related: None,
        meta: m.meta.as_ref(),
    };
    match repo::mutate(
        &mut tx,
        workspace_id,
        slot,
        m.mutation_value,
        Floor::Available,
        &cause,
    )
    .await?
    {
        Mutated::Short { .. } => Err(AppError::field("mutation_value", NOT_AVAILABLE)),
        Mutated::OutOfRange => Err(AppError::field("mutation_value", OUT_OF_RANGE)),
        Mutated::Done { before, after, .. } => {
            // Where the change went and the row's level around it, read in this transaction so
            // a till sale right after can't make the answer wrong. A replay sends the same.
            let warehouse = warehouses::compact(&mut *tx, workspace_id, warehouse_id)
                .await?
                .ok_or(AppError::NotFound)?;
            let done = Answer {
                status: 200,
                body: serde_json::json!({
                    "message": "ok",
                    "before": resource::qty(before),
                    "after": resource::qty(after),
                    "warehouse": { "id": warehouse.id, "name": warehouse.name },
                })
                .to_string()
                .into_bytes(),
            };
            idempotency::finish(&mut tx, workspace_id, key.as_ref(), &done).await?;
            tx.commit().await?;
            Ok(done)
        }
    }
}

/// The path's live product, else 404.
async fn product(state: &AppState, workspace_id: Uuid, id: &str) -> Result<ProductRow, AppError> {
    let id = Uuid::parse_str(id).map_err(|_| AppError::NotFound)?;
    products::find(&state.db, workspace_id, id)
        .await?
        .ok_or(AppError::NotFound)
}

/// Live warehouse (and bin of that warehouse) of the workspace, else 422.
async fn slot(
    state: &AppState,
    workspace_id: Uuid,
    product_id: Uuid,
    warehouse_id: Uuid,
    bin_id: Option<Uuid>,
) -> Result<Slot, AppError> {
    if !warehouses::exists(&state.db, workspace_id, warehouse_id).await? {
        return Err(AppError::field("warehouse_id", NO_WAREHOUSE));
    }
    let bin = match bin_id {
        None => None,
        Some(b) => Some(
            repo::bin(&state.db, workspace_id, warehouse_id, b)
                .await?
                .ok_or_else(|| AppError::field("bin_id", "The selected bin id is invalid."))?,
        ),
    };
    Ok(Slot {
        product_id,
        warehouse_id,
        bin,
        lot_id: None,
    })
}

/// Laravel `number_format($x, 2)`: two decimals, comma thousands.
fn number_format(n: i32) -> String {
    let digits = n.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    let sign = if n < 0 { "-" } else { "" };
    format!("{sign}{out}.00")
}

/// Laravel StockLevelsSet reason: when only min/max changed (`Some(before, after)`), a
/// "Min: a → b, Max: c → d." prefix before the user's reason.
fn levels_reason(
    min: Option<(Option<i32>, i32)>,
    max: Option<(Option<i32>, i32)>,
    level_changed: bool,
    reason: Option<&str>,
) -> Option<String> {
    let shown = |v: Option<i32>| v.map_or_else(|| "Not set".to_owned(), number_format);
    let mut parts = Vec::new();
    if !level_changed {
        if let Some((before, after)) = min {
            parts.push(format!("Min: {} → {}", shown(before), number_format(after)));
        }
        if let Some((before, after)) = max {
            parts.push(format!("Max: {} → {}", shown(before), number_format(after)));
        }
    }
    let reason = match (parts.is_empty(), reason) {
        (true, r) => r.map(str::to_owned),
        // Laravel glued ". " and " " into a double space; one is meant.
        (false, Some(r)) => Some(format!("{}. {r}", parts.join(", "))),
        (false, None) => Some(format!("{}.", parts.join(", "))),
    };
    // The prefix plus a full 255-char reason would break the column's limit.
    reason.map(|r| r.chars().take(255).collect())
}

/// Every entry's warehouse and bin must be live rows of the workspace, the bin inside that
/// warehouse (422 per entry). One query for the warehouses, one for the bins.
async fn entry_slots(
    state: &AppState,
    workspace_id: Uuid,
    product_id: Uuid,
    entries: &[Entry],
) -> Result<Vec<Slot>, AppError> {
    let warehouse_ids: Vec<Uuid> = entries.iter().map(|e| e.warehouse_id).collect();
    let bin_ids: Vec<Uuid> = entries.iter().filter_map(|e| e.bin_id).collect();
    let live = repo::live_warehouses(&state.db, workspace_id, &warehouse_ids).await?;
    let bins = if bin_ids.is_empty() {
        Vec::new()
    } else {
        repo::live_bins(&state.db, workspace_id, &bin_ids).await?
    };
    let mut errors: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut slots = Vec::with_capacity(entries.len());
    for (i, e) in entries.iter().enumerate() {
        let key = |f: &str| format!("warehouses.{i}.{f}");
        if !live.contains(&e.warehouse_id) {
            let k = key("warehouse_id");
            errors.insert(
                k.clone(),
                vec![format!("This {} does not exist", human(&k))],
            );
            continue;
        }
        let bin = match e.bin_id {
            None => None,
            Some(b) => match bins
                .iter()
                .find(|x| x.bin_id == b && x.warehouse_id == e.warehouse_id)
            {
                Some(x) => Some(Bin {
                    shelf_id: x.shelf_id,
                    bin_id: x.bin_id,
                }),
                None => {
                    let k = key("bin_id");
                    errors.insert(
                        k.clone(),
                        vec![format!("The selected {} is invalid.", human(&k))],
                    );
                    continue;
                }
            },
        };
        slots.push(Slot {
            product_id,
            warehouse_id: e.warehouse_id,
            bin,
            lot_id: None,
        });
    }
    if errors.is_empty() {
        Ok(slots)
    } else {
        Err(AppError::Validation(errors))
    }
}

/// POST /products/{id}/set-stock-levels (Laravel SetProductStockLevels + StockLevelsSet), all
/// entries in one transaction. Absent min/max keep their value. The product must track stock
/// (422), and a level never drops below what orders hold on the row (422 per entry).
pub async fn set_levels(
    state: &AppState,
    workspace_id: Uuid,
    product_id: &str,
    entries: Vec<Entry>,
    user_id: Uuid,
) -> Result<(), AppError> {
    let product = product(state, workspace_id, product_id).await?;
    if !tracks_stock(&product) {
        return Err(AppError::field(
            "warehouses",
            "This product doesn't track stock.",
        ));
    }
    let slots = entry_slots(state, workspace_id, product.id, &entries).await?;
    let mut tx = state.db.begin().await?;
    let mut created = repo::lock_all(&mut tx, workspace_id, &slots).await?;
    if preconditions::expects("stock") {
        let facts = preconditions::stock_facts(&mut tx, workspace_id, product.id, &entries).await?;
        preconditions::check_locked(workspace_id, "stock", &facts)?;
    }
    let mut held_errors: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (i, (e, slot)) in entries.iter().zip(slots).enumerate() {
        let (row, _) = repo::lock_or_create(&mut tx, workspace_id, slot).await?;
        let existing = (!take(&mut created, &slot)).then_some(&row);
        let level = round(e.stock_lvl.unwrap_or_default(), 2);
        if level < row.reserved {
            held_errors.insert(
                format!("warehouses.{i}.stock_lvl"),
                vec![format!(
                    "The stock level can't be set below the {} held for orders.",
                    row.reserved.normalize()
                )],
            );
            continue;
        }
        let min = e.minimal.unwrap_or(row.minimal_stock_level);
        let max = e.maximum.unwrap_or(row.maximum_stock_level);
        let before = (
            existing.map(|r| r.minimal_stock_level),
            existing.map(|r| r.maximum_stock_level),
        );
        let level_changed = level != row.stock_lvl;
        // Only a sent min/max can change (Laravel compared absent ones as null).
        let min_changed = e.minimal.is_some() && before.0 != Some(min);
        let max_changed = e.maximum.is_some() && before.1 != Some(max);
        if !level_changed && !min_changed && !max_changed {
            continue;
        }
        repo::set_quantities(&mut tx, workspace_id, row.id, level, min, max).await?;
        let reason = levels_reason(
            min_changed.then_some((before.0, min)),
            max_changed.then_some((before.1, max)),
            level_changed,
            e.reason.as_deref(),
        );
        let cause = Cause {
            user_id: Some(user_id),
            ..Cause::default()
        };
        let movement = Movement {
            kind: if level_changed {
                "set_level"
            } else {
                "set_requirements"
            },
            change: level_changed.then(|| level - row.stock_lvl),
            before: row.stock_lvl,
            after: level,
            min: (before.0, Some(min)),
            max: (before.1, Some(max)),
            reason: reason.as_deref(),
            cause: &cause,
        };
        repo::record(&mut tx, workspace_id, &row, &movement).await?;
    }
    if !held_errors.is_empty() {
        // Dropping the transaction rolls back every entry: all or nothing.
        return Err(AppError::Validation(held_errors));
    }
    tx.commit().await?;
    Ok(())
}

/// POST /products/{id}/set-stock-requirements (Laravel SetProductStockRequirements +
/// StockRequirementsSet): min/max of the warehouse-level rows, and optionally the product's
/// default warehouse.
pub async fn set_requirements(
    state: &AppState,
    workspace_id: Uuid,
    product_id: &str,
    req: SetRequirements,
    user_id: Uuid,
) -> Result<(), AppError> {
    let product = product(state, workspace_id, product_id).await?;
    let mut slots = entry_slots(state, workspace_id, product.id, &req.entries).await?;
    // Laravel picked the warehouse's "first" row; the warehouse-level row is the defined one.
    for s in &mut slots {
        s.bin = None;
    }
    if let Some(w) = req.default_warehouse_id
        && !warehouses::exists(&state.db, workspace_id, w).await?
    {
        return Err(AppError::field(
            "default_warehouse_id",
            "The selected default warehouse id is invalid.",
        ));
    }
    let mut tx = state.db.begin().await?;
    let mut created = repo::lock_all(&mut tx, workspace_id, &slots).await?;
    for (e, slot) in req.entries.iter().zip(slots) {
        let (row, _) = repo::lock_or_create(&mut tx, workspace_id, slot).await?;
        let existing = (!take(&mut created, &slot)).then_some(&row);
        let (min, max) = (e.minimal.unwrap_or_default(), e.maximum.unwrap_or_default());
        let before = (
            existing.map(|r| r.minimal_stock_level),
            existing.map(|r| r.maximum_stock_level),
        );
        if before == (Some(min), Some(max)) {
            continue;
        }
        repo::set_quantities(&mut tx, workspace_id, row.id, row.stock_lvl, min, max).await?;
        let cause = Cause {
            user_id: Some(user_id),
            ..Cause::default()
        };
        let movement = Movement {
            kind: "set_requirements",
            change: None,
            before: row.stock_lvl,
            after: row.stock_lvl,
            min: (before.0, Some(min)),
            max: (before.1, Some(max)),
            reason: None,
            cause: &cause,
        };
        repo::record(&mut tx, workspace_id, &row, &movement).await?;
    }
    if let Some(w) = req.default_warehouse_id
        && product.default_warehouse_id != Some(w)
    {
        products::set_default_warehouse(&mut tx, workspace_id, product.id, w).await?;
    }
    tx.commit().await?;
    Ok(())
}

/// DELETE /products/{id}/stocks (Laravel DeleteProductStock): soft-deletes the slot's row,
/// only at level 0. The ledger stays.
pub async fn delete(
    state: &AppState,
    workspace_id: Uuid,
    product_id: &str,
    warehouse_id: Uuid,
    bin_id: Option<Uuid>,
) -> Result<(), AppError> {
    let product = product(state, workspace_id, product_id).await?;
    let slot = slot(state, workspace_id, product.id, warehouse_id, bin_id).await?;
    let mut tx = state.db.begin().await?;
    let row = repo::locked(&mut tx, workspace_id, slot)
        .await?
        .ok_or_else(|| AppError::BadRequest("Stock not found".into()))?;
    if !row.stock_lvl.is_zero() {
        return Err(AppError::BadRequest(
            "Only stock at level 0 can be deleted".into(),
        ));
    }
    repo::soft_delete(&mut tx, workspace_id, row.id).await?;
    tx.commit().await?;
    Ok(())
}

/// POST /products/{id}/move-stock-between-bins (Laravel MoveStockBetweenBins): a decrease on
/// the source and an increase on the target in one transaction. A missing bin means the
/// warehouse-level row.
pub async fn move_between_bins(
    state: &AppState,
    workspace_id: Uuid,
    product_id: &str,
    m: Move,
    user_id: Uuid,
) -> Result<(), AppError> {
    let product = product(state, workspace_id, product_id).await?;
    if !warehouses::exists(&state.db, workspace_id, m.warehouse_id).await? {
        return Err(AppError::field("warehouse_id", NO_WAREHOUSE));
    }
    let ids: Vec<Uuid> = [m.bin_from, m.bin_to].into_iter().flatten().collect();
    let bins = if ids.is_empty() {
        Vec::new()
    } else {
        repo::live_bins(&state.db, workspace_id, &ids).await?
    };
    // Laravel MoveStockBetweenBinsRequest wording.
    let bin = |id: Option<Uuid>, which: &str, field: &str| match id {
        None => Ok(None),
        Some(b) => match bins.iter().find(|x| x.bin_id == b) {
            None => Err(AppError::field(
                field,
                format!("The selected {which} bin does not exist."),
            )),
            Some(x) if x.warehouse_id != m.warehouse_id => Err(AppError::field(
                field,
                format!("The selected {which} bin does not belong to the selected warehouse."),
            )),
            Some(x) => Ok(Some(Bin {
                shelf_id: x.shelf_id,
                bin_id: x.bin_id,
            })),
        },
    };
    let slot = |bin| Slot {
        product_id: product.id,
        warehouse_id: m.warehouse_id,
        bin,
        lot_id: None,
    };
    let from = slot(bin(m.bin_from, "source", "bin_id_from")?);
    let to = slot(bin(m.bin_to, "target", "bin_id_to")?);
    if m.bin_from == m.bin_to {
        return Err(AppError::field(
            "bin_id_to",
            "Source and target bins must be different.",
        ));
    }
    let cause = Cause {
        reason: Some(&m.reason),
        user_id: Some(user_id),
        ..Cause::default()
    };
    let mut tx = state.db.begin().await?;
    // Both rows in the shared lock order, so opposite moves and set-levels queue.
    repo::lock_all(&mut tx, workspace_id, &[from, to]).await?;
    // Laravel checked this in the request, before any lock; here it holds under the lock.
    if let Mutated::Short { level, reserved } = repo::mutate(
        &mut tx,
        workspace_id,
        from,
        -m.quantity,
        Floor::Available,
        &cause,
    )
    .await?
    {
        return Err(insufficient(level, reserved, m.quantity));
    }
    if let Mutated::OutOfRange =
        repo::mutate(&mut tx, workspace_id, to, m.quantity, Floor::None, &cause).await?
    {
        return Err(AppError::field("quantity", OUT_OF_RANGE));
    }
    tx.commit().await?;
    Ok(())
}

/// 422 on `quantity` when the source can't give `requested` (Laravel bin-move wording).
fn insufficient(level: Decimal, reserved: Decimal, requested: Decimal) -> AppError {
    AppError::field(
        "quantity",
        format!(
            "Insufficient stock available. Available: {}, Reserved: {}, Requested: {}",
            level.normalize(),
            reserved.normalize(),
            requested.normalize()
        ),
    )
}

/// Conflict code when a lot's stock may not move: recalled, on hold (quarantine) or expired.
pub fn lot_unmovable(lot: &LotRow, today: NaiveDate) -> Option<&'static str> {
    match lot.status.as_str() {
        "recalled" => Some("lot_recalled"),
        "quarantine" => Some("lot_quarantined"),
        _ if lot.expires_on.is_some_and(|e| e < today) => Some("lot_expired"),
        _ => None,
    }
}

/// Plain sentence for a [`lot_unmovable`] code, for the assistant's refusal before a review.
fn unmovable_words(code: &str) -> &'static str {
    match code {
        "lot_recalled" => "This batch is recalled. Its stock can't be moved.",
        "lot_quarantined" => "This batch is on hold (quarantine). Its stock can't be moved.",
        _ => "This batch is past its expiry date. Its stock can't be moved.",
    }
}

/// What a transfer touches, read and checked before any write.
pub struct TransferPlan {
    product: ProductRow,
    from: CompactRow,
    to: CompactRow,
    lot: Option<LotRow>,
}

impl TransferPlan {
    fn slot(&self, warehouse_id: Uuid) -> Slot {
        Slot {
            product_id: self.product.id,
            warehouse_id,
            bin: None,
            lot_id: self.lot.as_ref().map(|l| l.id),
        }
    }

    /// Identity the assistant review binds; levels stay out (the floor rechecks them).
    pub fn locked(&self) -> Value {
        serde_json::json!({
            "product": { "id": self.product.id, "name": self.product.name },
            "lot": self.lot.as_ref().map(|l| serde_json::json!({ "id": l.id, "code": l.code, "status": l.status })),
            "from": { "id": self.from.id, "name": self.from.name },
            "to": { "id": self.to.id, "name": self.to.name },
        })
    }
}

/// Product, both warehouses and the lot of a transfer, all of this workspace: 404 product,
/// 422 per field. `lock` takes the lot row `FOR UPDATE` so a recall can't slip in.
async fn transfer_plan(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    product: ProductRow,
    t: &Transfer,
    lock: bool,
) -> Result<TransferPlan, AppError> {
    if !tracks_stock(&product) {
        return Err(AppError::field(
            "product",
            "This product doesn't track stock.",
        ));
    }
    let from = warehouses::compact(&mut *conn, workspace_id, t.from).await?;
    let to = warehouses::compact(&mut *conn, workspace_id, t.to).await?;
    let lot = match t.lot_id {
        None => None,
        Some(id) if lock => lots::locked(&mut *conn, workspace_id, id).await?,
        Some(id) => lots::find(&mut *conn, workspace_id, id).await?,
    };
    let mut errors: BTreeMap<String, Vec<String>> = BTreeMap::new();
    if from.is_none() {
        errors.insert("warehouse_id_from".into(), vec![NO_WAREHOUSE.into()]);
    }
    if to.is_none() {
        errors.insert("warehouse_id_to".into(), vec![NO_WAREHOUSE.into()]);
    }
    let lot_ok = lot.as_ref().is_none_or(|l| l.product_id == product.id);
    if t.lot_id.is_some() && (lot.is_none() || !lot_ok) {
        errors.insert(
            "lot_id".into(),
            vec!["The selected lot id is invalid.".into()],
        );
    }
    match (from, to) {
        (Some(from), Some(to)) if errors.is_empty() => Ok(TransferPlan {
            product,
            from,
            to,
            lot,
        }),
        _ => Err(AppError::Validation(errors)),
    }
}

/// Assistant review of a transfer: identity plus both levels as of proposal time. A transfer
/// the native route would refuse (short stock, unmovable lot) is refused here in plain words.
pub async fn transfer_facts(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    product_id: Uuid,
    t: &Transfer,
) -> Result<Value, AppError> {
    let product = products::find(&mut *conn, workspace_id, product_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let plan = transfer_plan(conn, workspace_id, product, t, false).await?;
    if let Some(code) = plan.lot.as_ref().and_then(|l| lot_unmovable(l, today())) {
        return Err(AppError::field("lot_id", unmovable_words(code)));
    }
    let (from_lvl, reserved) =
        repo::level(&mut *conn, workspace_id, plan.slot(plan.from.id)).await?;
    if from_lvl - reserved < t.quantity {
        return Err(insufficient(from_lvl, reserved, t.quantity));
    }
    let (to_lvl, _) = repo::level(&mut *conn, workspace_id, plan.slot(plan.to.id)).await?;
    let shown = |d: Decimal| d.normalize().to_string();
    Ok(serde_json::json!({
        "kind": "stock_transfer",
        "locked": plan.locked(),
        "changes": [
            { "field": "stock_from", "label": plan.from.name, "before": shown(from_lvl), "after": shown(from_lvl - t.quantity) },
            { "field": "stock_to", "label": plan.to.name, "before": shown(to_lvl), "after": shown(to_lvl + t.quantity) },
        ],
    }))
}

fn today() -> NaiveDate {
    chrono::Utc::now().date_naive()
}

/// POST /products/{id}/transfer-stock: take from one warehouse and add to another of the same
/// workspace in one transaction, lot kept. The source never goes below what it has free
/// (level − reserved), whatever the warehouse type. Both movements share one transfer id.
pub async fn transfer(
    state: &AppState,
    workspace_id: Uuid,
    product_id: &str,
    t: Transfer,
    user_id: Uuid,
) -> Result<Value, AppError> {
    let product = product(state, workspace_id, product_id).await?;
    let mut tx = state.db.begin().await?;
    let plan = transfer_plan(&mut tx, workspace_id, product, &t, true).await?;
    if let Some(code) = plan.lot.as_ref().and_then(|l| lot_unmovable(l, today())) {
        return Err(AppError::Conflict(code));
    }
    if preconditions::expects("stock_transfer") {
        preconditions::check_locked(workspace_id, "stock_transfer", &plan.locked())?;
    }
    let cause = Cause {
        reason: Some(&t.reason),
        user_id: Some(user_id),
        related: Some(("stock_transfer", Uuid::now_v7())),
        ..Cause::default()
    };
    let (from, to) = (plan.slot(plan.from.id), plan.slot(plan.to.id));
    repo::lock_all(&mut tx, workspace_id, &[from, to]).await?;
    let out = match repo::mutate(
        &mut tx,
        workspace_id,
        from,
        -t.quantity,
        Floor::Available,
        &cause,
    )
    .await?
    {
        Mutated::Short { level, reserved } => {
            return Err(insufficient(level, reserved, t.quantity));
        }
        Mutated::OutOfRange => return Err(AppError::field("quantity", OUT_OF_RANGE)),
        Mutated::Done { before, after, .. } => (before, after),
    };
    let into =
        match repo::mutate(&mut tx, workspace_id, to, t.quantity, Floor::None, &cause).await? {
            Mutated::Done { before, after, .. } => (before, after),
            _ => return Err(AppError::field("quantity", OUT_OF_RANGE)),
        };
    tx.commit().await?;
    let side = |w: &CompactRow, (before, after): (Decimal, Decimal)| {
        serde_json::json!({
            "warehouse": { "id": w.id, "name": w.name },
            "before": resource::qty(before),
            "after": resource::qty(after),
        })
    };
    Ok(serde_json::json!({
        "from": side(&plan.from, out),
        "to": side(&plan.to, into),
        "lot": plan.lot.as_ref().map(|l| serde_json::json!({ "id": l.id, "code": l.code })),
    }))
}

/// True the first time `slot` is taken out of `created` (rows this request made).
fn take(created: &mut Vec<repo::SlotKey>, slot: &Slot) -> bool {
    let key = repo::key(slot);
    created
        .iter()
        .position(|k| *k == key)
        .map(|i| created.swap_remove(i))
        .is_some()
}

/// GET /products/{id}/stock-with-bins.
pub async fn with_bins(
    state: &AppState,
    workspace_id: Uuid,
    product_id: &str,
) -> Result<Vec<Value>, AppError> {
    let p = product(state, workspace_id, product_id).await?;
    let rows = repo::with_bins(&state.db, workspace_id, p.id).await?;
    Ok(rows.iter().map(resource::with_bins).collect())
}

/// GET /products/{id}/configured-bins.
pub async fn configured_bins(
    state: &AppState,
    workspace_id: Uuid,
    product_id: &str,
) -> Result<Vec<Value>, AppError> {
    let p = product(state, workspace_id, product_id).await?;
    let bins = repo::configured_bins(&state.db, workspace_id, p.id).await?;
    Ok(resource::configured(&bins))
}

/// GET /products/{id}/stock-movements: one page, newest first, plus the total.
pub async fn movements(
    state: &AppState,
    workspace_id: Uuid,
    product_id: &str,
    f: &MovementFilter,
) -> Result<(Vec<Value>, i64), AppError> {
    let p = product(state, workspace_id, product_id).await?;
    let rows = repo::movements(&state.db, workspace_id, p.id, f).await?;
    let total = repo::movements_count(&state.db, workspace_id, p.id, f).await?;
    Ok((rows.iter().map(resource::movement).collect(), total))
}

/// GET /products/stocks and /products/stocks/negative: one page of StockResource rows with
/// the product (list shape) and the full warehouse, plus the total.
pub async fn list(
    state: &AppState,
    workspace_id: Uuid,
    f: &ListFilter<'_>,
) -> Result<(Vec<Value>, i64), AppError> {
    let rows = repo::list(&state.db, workspace_id, f).await?;
    let total = match rows.first() {
        Some(r) => r.total,
        None if f.offset > 0 => repo::count(&state.db, workspace_id, f).await?,
        None => 0,
    };
    let distinct = |ids: Vec<Uuid>| {
        let mut ids = ids;
        ids.sort_unstable();
        ids.dedup();
        ids
    };
    let product_ids = distinct(rows.iter().map(|r| r.product_id).collect());
    let warehouse_ids = distinct(rows.iter().map(|r| r.warehouse_id).collect());
    let products = products_service::render_many(state, workspace_id, &product_ids).await?;
    let warehouses = warehouse_service::resources(state, workspace_id, &warehouse_ids).await?;
    let data = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.id,
                "stock_lvl": resource::qty(r.stock_lvl),
                "reserved": resource::qty(r.reserved),
                "available": resource::qty(r.available),
                "minimal_stock_level": r.minimal_stock_level,
                "maximum_stock_level": r.maximum_stock_level,
                "meta": r.meta,
                "lot": resource::lot(r.lot_id, r.lot_code.as_deref(), r.lot_expires_on),
                "relationships": {
                    "product": products.get(&r.product_id),
                    "warehouse": warehouses.get(&r.warehouse_id),
                },
            })
        })
        .collect();
    Ok((data, total))
}
