//! Bin rules: reached only through a live shelf, row, section and warehouse; code unique per
//! row (Laravel's DB key).

use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    modules::{
        warehouse_bins::{
            dto::{CODE_TAKEN, Fields, NewBin},
            repo::{self, BinRow, HierarchyRow},
        },
        warehouse_sections::service::{parse, warehouse},
        warehouse_shelves::{
            repo::ShelfRow,
            service::{self as shelves, RowPath},
        },
    },
};

/// `{warehouse}/{section}/{row}/{shelf}` of the request path.
pub type ShelfPath<'a> = (RowPath<'a>, &'a str);

/// The path's shelf, else 404.
pub async fn shelf(
    state: &AppState,
    workspace_id: Uuid,
    path: ShelfPath<'_>,
) -> Result<ShelfRow, AppError> {
    shelves::find(state, workspace_id, path.0, path.1).await
}

/// The path's bin, else 404.
pub async fn find(
    state: &AppState,
    workspace_id: Uuid,
    path: ShelfPath<'_>,
    id: &str,
) -> Result<BinRow, AppError> {
    let sh = shelf(state, workspace_id, path).await?;
    repo::find(&state.db, workspace_id, sh.id, parse(id)?)
        .await?
        .ok_or(AppError::NotFound)
}

/// One page plus total.
pub async fn list(
    state: &AppState,
    workspace_id: Uuid,
    shelf_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<(Vec<BinRow>, i64), AppError> {
    let (rows, mut total) = repo::list(&state.db, workspace_id, shelf_id, limit, offset).await?;
    if rows.is_empty() && offset > 0 {
        total = repo::count(&state.db, workspace_id, shelf_id).await?;
    }
    Ok((rows, total))
}

/// Live bins of a live warehouse (else 404), plus whether the cap cut the list.
pub async fn of_warehouse(
    state: &AppState,
    workspace_id: Uuid,
    warehouse_id: &str,
) -> Result<(Vec<HierarchyRow>, bool), AppError> {
    let id = warehouse(state, workspace_id, warehouse_id).await?;
    let mut rows =
        repo::of_warehouse(&state.db, workspace_id, id, repo::WAREHOUSE_BINS_MAX + 1).await?;
    let truncated = rows.len() as i64 > repo::WAREHOUSE_BINS_MAX;
    rows.truncate(repo::WAREHOUSE_BINS_MAX as usize);
    Ok((rows, truncated))
}

/// Insert; returns the new id.
pub async fn create(state: &AppState, sh: &ShelfRow, n: NewBin) -> Result<Uuid, AppError> {
    if !quota_allows(state, sh.workspace_id, 1).await? {
        return Err(quota_error("bin_code"));
    }
    repo::insert(&state.db, sh, &n).await.map_err(code_clash)
}

/// Patch; 404 when the bin is gone.
pub async fn update(
    state: &AppState,
    workspace_id: Uuid,
    sh: &ShelfRow,
    id: &str,
    f: Fields,
) -> Result<(), AppError> {
    let id = parse(id)?;
    // No code pre-check: an unknown id must stay a 404; the index catches a clash.
    repo::update(&state.db, workspace_id, sh.id, id, &f)
        .await
        .map_err(code_clash)?
        .then_some(())
        .ok_or(AppError::NotFound)
}

/// Soft delete; 409 `bin_holds_stock` while stock lies in it, else 404.
pub async fn delete(
    state: &AppState,
    workspace_id: Uuid,
    sh: &ShelfRow,
    id: &str,
) -> Result<(), AppError> {
    let id = parse(id)?;
    let mut tx = state.db.begin().await?;
    // Waits for open stock writes in the bin; the check below then sees them.
    if !repo::lock_for_delete(&mut tx, workspace_id, sh.id, id).await? {
        return Err(AppError::NotFound);
    }
    if !repo::soft_delete(&mut *tx, workspace_id, sh.id, id).await? {
        return Err(AppError::Conflict("bin_holds_stock"));
    }
    tx.commit().await?;
    Ok(())
}

/// True when the workspace can take `adding` more bins.
pub async fn quota_allows(
    state: &AppState,
    workspace_id: Uuid,
    adding: i64,
) -> Result<bool, AppError> {
    let existing = repo::count_in_workspace(&state.db, workspace_id).await?;
    Ok(repo::fits_quota(existing, adding))
}

/// The quota 422, on `field`.
pub fn quota_error(field: &str) -> AppError {
    AppError::field(
        field,
        format!(
            "The workspace can't hold more than {} bins.",
            repo::WORKSPACE_BINS_MAX
        ),
    )
}

/// The live-code unique index fired: same 422 as the pre-check.
fn code_clash(e: sqlx::Error) -> AppError {
    AppError::unique(
        e,
        "warehouse_section_row_bins_code_unique",
        "bin_code",
        CODE_TAKEN,
    )
}
