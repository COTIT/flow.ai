//! Stock rules: the product must track stock (physical type with `has_stock`), the warehouse
//! and bin must be live rows of the workspace, and every write goes through `repo::mutate`
//! or the set-level path under the row lock.

use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    modules::{
        products::{dto::PHYSICAL, repo as products, repo::ProductRow},
        stock::{
            dto::{Mutate, NO_WAREHOUSE},
            repo::{self, Cause, Floor, Mutated, Slot},
        },
        warehouses::repo as warehouses,
    },
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
