//! Stock rules: the product must track stock (physical type with `has_stock`), the warehouse
//! and bin must be live rows of the workspace, and every write goes through `repo::mutate`
//! or the set-level path under the row lock.

use std::collections::BTreeMap;

use serde_json::Value;

use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::json::human,
    modules::{
        products::{
            dto::PHYSICAL, repo as products, repo::ProductRow, service as products_service,
        },
        stock::{
            dto::{Entry, Move, Mutate, NO_WAREHOUSE, SetRequirements},
            repo::{self, Cause, Floor, ListFilter, Movement, MovementFilter, Mutated, Slot},
            resource,
        },
        warehouses::{repo as warehouses, service as warehouse_service},
    },
    money::round,
};

/// Laravel EnoughStockRule message.
const NOT_AVAILABLE: &str = "The mutation value is higher then the available stock";

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
) -> Result<(), AppError> {
    let product = product(state, workspace_id, product_id).await?;
    let slot = slot(state, workspace_id, product.id, m.warehouse_id, m.bin_id).await?;
    if !tracks_stock(&product) || m.mutation_value.is_zero() {
        return Ok(());
    }
    let cause = Cause {
        reason: m.reason.as_deref(),
        is_correction: m.is_correction,
        user_id: Some(user_id),
        related: None,
        meta: m.meta.as_ref(),
    };
    let mut tx = state.db.begin().await?;
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
        Mutated::Done { .. } => Ok(tx.commit().await?),
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

/// Laravel StockLevelsSet reason: when only min/max changed, a "Min: a → b, Max: c → d."
/// prefix before the user's reason.
fn levels_reason(
    min: (Option<i32>, i32),
    max: (Option<i32>, i32),
    level_changed: bool,
    reason: Option<&str>,
) -> Option<String> {
    let shown = |v: Option<i32>| v.map_or_else(|| "Not set".to_owned(), number_format);
    let mut parts = Vec::new();
    if !level_changed {
        if min.0 != Some(min.1) {
            parts.push(format!("Min: {} → {}", shown(min.0), number_format(min.1)));
        }
        if max.0 != Some(max.1) {
            parts.push(format!("Max: {} → {}", shown(max.0), number_format(max.1)));
        }
    }
    match (parts.is_empty(), reason) {
        (true, r) => r.map(str::to_owned),
        // Laravel glued ". " and " " into a double space; one is meant.
        (false, Some(r)) => Some(format!("{}. {r}", parts.join(", "))),
        (false, None) => Some(format!("{}.", parts.join(", "))),
    }
}

/// Every entry's warehouse and bin must be live rows of the workspace (422 per entry).
async fn entry_slots(
    state: &AppState,
    workspace_id: Uuid,
    product_id: Uuid,
    entries: &[Entry],
) -> Result<Vec<Slot>, AppError> {
    let mut errors: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut slots = Vec::with_capacity(entries.len());
    for (i, e) in entries.iter().enumerate() {
        match slot(state, workspace_id, product_id, e.warehouse_id, e.bin_id).await {
            Ok(s) => slots.push(s),
            Err(AppError::Validation(found)) => {
                for (field, messages) in found {
                    let key = format!("warehouses.{i}.{field}");
                    let message = if field == "bin_id" {
                        format!("The selected {} is invalid.", human(&key))
                    } else {
                        format!("This {} does not exist", human(&key))
                    };
                    errors
                        .entry(key)
                        .or_default()
                        .extend(messages.iter().map(|_| message.clone()));
                }
            }
            Err(e) => return Err(e),
        }
    }
    if errors.is_empty() {
        Ok(slots)
    } else {
        Err(AppError::Validation(errors))
    }
}

/// POST /products/{id}/set-stock-levels (Laravel SetProductStockLevels + StockLevelsSet), all
/// entries in one transaction. Absent min/max keep their value.
pub async fn set_levels(
    state: &AppState,
    workspace_id: Uuid,
    product_id: &str,
    entries: Vec<Entry>,
    user_id: Uuid,
) -> Result<(), AppError> {
    let product = product(state, workspace_id, product_id).await?;
    let slots = entry_slots(state, workspace_id, product.id, &entries).await?;
    let mut tx = state.db.begin().await?;
    for (e, slot) in entries.iter().zip(slots) {
        let (row, created) = repo::lock_or_create(&mut tx, workspace_id, slot).await?;
        let existing = (!created).then_some(&row);
        let level = round(e.stock_lvl.unwrap_or_default(), 2);
        let min = e.minimal.unwrap_or(row.minimal_stock_level);
        let max = e.maximum.unwrap_or(row.maximum_stock_level);
        let before = (
            existing.map(|r| r.minimal_stock_level),
            existing.map(|r| r.maximum_stock_level),
        );
        let level_changed = level != row.stock_lvl;
        let req_changed = before.0 != Some(min) || before.1 != Some(max);
        repo::set_quantities(&mut tx, row.id, level, min, max).await?;
        if !level_changed && !req_changed {
            continue;
        }
        let reason = levels_reason(
            (before.0, min),
            (before.1, max),
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
    for (e, slot) in req.entries.iter().zip(slots) {
        let (row, created) = repo::lock_or_create(&mut tx, workspace_id, slot).await?;
        let existing = (!created).then_some(&row);
        let (min, max) = (e.minimal.unwrap_or_default(), e.maximum.unwrap_or_default());
        let before = (
            existing.map(|r| r.minimal_stock_level),
            existing.map(|r| r.maximum_stock_level),
        );
        repo::set_quantities(&mut tx, row.id, row.stock_lvl, min, max).await?;
        if before == (Some(min), Some(max)) {
            continue;
        }
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
    repo::soft_delete(&mut tx, row.id).await?;
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
    let from = slot(state, workspace_id, product.id, m.warehouse_id, m.bin_from)
        .await
        .map_err(|e| rename(e, "bin_id", "bin_id_from"))?;
    let to = slot(state, workspace_id, product.id, m.warehouse_id, m.bin_to)
        .await
        .map_err(|e| rename(e, "bin_id", "bin_id_to"))?;
    let warehouse = warehouses::find(&state.db, workspace_id, m.warehouse_id)
        .await?
        .ok_or_else(|| AppError::field("warehouse_id", NO_WAREHOUSE))?;
    let cause = Cause {
        reason: Some(&m.reason),
        user_id: Some(user_id),
        ..Cause::default()
    };
    let mut tx = state.db.begin().await?;
    // Lock both rows in one order (warehouse row first, then by bin id), so opposite moves
    // queue instead of deadlocking.
    let mut order = [from, to];
    order.sort_by_key(|s| s.bin.map(|b| b.bin_id));
    for s in order {
        repo::locked(&mut tx, workspace_id, s).await?;
    }
    if let Mutated::Short { level, .. } = repo::mutate(
        &mut tx,
        workspace_id,
        from,
        -m.quantity,
        Floor::Available,
        &cause,
    )
    .await?
    {
        return Err(AppError::BadRequest(format!(
            "Insufficient stock for product {} - {} in {} for {} Available stock: {}. Required: {}",
            product.reference_vendor.as_deref().unwrap_or_default(),
            product.name,
            warehouse.name,
            m.reason,
            level.normalize(),
            m.quantity.normalize(),
        )));
    }
    repo::mutate(&mut tx, workspace_id, to, m.quantity, Floor::None, &cause).await?;
    tx.commit().await?;
    Ok(())
}

/// The same 422 under another field name.
fn rename(e: AppError, from: &str, to: &str) -> AppError {
    match e {
        AppError::Validation(found) if found.contains_key(from) => {
            AppError::field(to, format!("The selected {} is invalid.", human(to)))
        }
        e => e,
    }
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
    let total = match rows.first() {
        Some(r) => r.total,
        None if f.offset > 0 => repo::movements_count(&state.db, workspace_id, p.id, f).await?,
        None => 0,
    };
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
                // shortcut: the product's reservations come with orders (final batch).
                "reserved": 0,
                "minimal_stock_level": r.minimal_stock_level,
                "maximum_stock_level": r.maximum_stock_level,
                "meta": r.meta,
                "relationships": {
                    "product": products.get(&r.product_id),
                    "warehouse": warehouses.get(&r.warehouse_id),
                },
            })
        })
        .collect();
    Ok((data, total))
}
