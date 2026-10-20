//! Bin rules: reached only through a live shelf, row, section and warehouse; code unique per
//! row (Laravel's DB key).

use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    modules::{
        warehouse_bins::{
            dto::{CODE_TAKEN, Fields},
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

/// All live bins of a live warehouse, else 404.
pub async fn of_warehouse(
    state: &AppState,
    workspace_id: Uuid,
    warehouse_id: &str,
) -> Result<Vec<HierarchyRow>, AppError> {
    let id = warehouse(state, workspace_id, warehouse_id).await?;
    Ok(repo::of_warehouse(&state.db, workspace_id, id).await?)
}

/// Insert; returns the new id.
pub async fn create(state: &AppState, sh: &ShelfRow, f: Fields) -> Result<Uuid, AppError> {
    let code = f.bin_code.as_deref().unwrap_or_default();
    if repo::code_taken(&state.db, sh.row_id, code, None).await? {
        return Err(AppError::field("bin_code", CODE_TAKEN));
    }
    repo::insert(&state.db, sh, &f).await.map_err(code_race)
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
    if let Some(code) = f.bin_code.as_deref()
        && repo::code_taken(&state.db, sh.row_id, code, Some(id)).await?
    {
        return Err(AppError::field("bin_code", CODE_TAKEN));
    }
    repo::update(&state.db, workspace_id, sh.id, id, &f)
        .await
        .map_err(code_race)?
        .then_some(())
        .ok_or(AppError::NotFound)
}

/// Soft delete, else 404.
pub async fn delete(
    state: &AppState,
    workspace_id: Uuid,
    sh: &ShelfRow,
    id: &str,
) -> Result<(), AppError> {
    repo::soft_delete(&state.db, workspace_id, sh.id, parse(id)?)
        .await?
        .then_some(())
        .ok_or(AppError::NotFound)
}

/// The unique index caught a race the pre-check missed: same 422.
fn code_race(e: sqlx::Error) -> AppError {
    match e.as_database_error().and_then(|d| d.constraint()) {
        Some("warehouse_section_row_bins_code_unique") => AppError::field("bin_code", CODE_TAKEN),
        _ => AppError::Database(e),
    }
}
