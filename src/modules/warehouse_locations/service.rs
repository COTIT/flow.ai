//! Warehouse location rules: every call first resolves the live warehouse of this workspace
//! (404 otherwise), so a location is only ever reached through its own warehouse.

use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::list::Page,
    modules::{
        warehouse_locations::{
            dto::LocationInput,
            repo::{self, LocationRow},
        },
        warehouses::repo::{self as warehouses, CompactRow},
    },
};

/// The path's warehouse, live and in this workspace, else 404.
pub async fn warehouse(
    state: &AppState,
    workspace_id: Uuid,
    warehouse_id: &str,
) -> Result<CompactRow, AppError> {
    warehouses::compact(&state.db, workspace_id, parse(warehouse_id)?)
        .await?
        .ok_or(AppError::NotFound)
}

/// One page plus total.
pub async fn list(
    state: &AppState,
    workspace_id: Uuid,
    warehouse_id: Uuid,
    pattern: Option<&str>,
    page: Page,
) -> Result<(Vec<LocationRow>, i64), AppError> {
    let (rows, mut total) = repo::list(
        &state.db,
        workspace_id,
        warehouse_id,
        pattern,
        page.size,
        page.offset(),
    )
    .await?;
    if rows.is_empty() && page.offset() > 0 {
        total = repo::count(&state.db, workspace_id, warehouse_id, pattern).await?;
    }
    Ok((rows, total))
}

/// One live location of this warehouse, else 404.
pub async fn get(
    state: &AppState,
    workspace_id: Uuid,
    warehouse_id: Uuid,
    id: &str,
) -> Result<LocationRow, AppError> {
    repo::find(&state.db, workspace_id, warehouse_id, parse(id)?)
        .await?
        .ok_or(AppError::NotFound)
}

/// Insert; returns the new id.
pub async fn create(
    state: &AppState,
    workspace_id: Uuid,
    warehouse_id: Uuid,
    input: LocationInput,
) -> Result<Uuid, AppError> {
    let meta = input.meta.flatten();
    Ok(repo::insert(
        &state.db,
        workspace_id,
        warehouse_id,
        &input.location,
        meta.as_ref(),
    )
    .await?)
}

/// Patch a live location, else 404.
pub async fn update(
    state: &AppState,
    workspace_id: Uuid,
    warehouse_id: Uuid,
    id: &str,
    input: LocationInput,
) -> Result<(), AppError> {
    let found = repo::update(
        &state.db,
        workspace_id,
        warehouse_id,
        parse(id)?,
        &input.location,
        input.meta.as_ref().map(Option::as_ref),
    )
    .await?;
    found.then_some(()).ok_or(AppError::NotFound)
}

/// Soft delete one, else 404.
// shortcut: storage locations (6F) must be soft-deleted with it once that table exists.
pub async fn delete(
    state: &AppState,
    workspace_id: Uuid,
    warehouse_id: Uuid,
    id: &str,
) -> Result<(), AppError> {
    let n = repo::soft_delete(&state.db, workspace_id, warehouse_id, &[parse(id)?]).await?;
    (n == 1).then_some(()).ok_or(AppError::NotFound)
}

/// Soft delete the listed ids of this warehouse; unknown ids are skipped (Laravel parity).
pub async fn delete_many(
    state: &AppState,
    workspace_id: Uuid,
    warehouse_id: Uuid,
    ids: &[Uuid],
) -> Result<(), AppError> {
    repo::soft_delete(&state.db, workspace_id, warehouse_id, ids).await?;
    Ok(())
}

/// Malformed id = unknown id = 404.
fn parse(id: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(id).map_err(|_| AppError::NotFound)
}
