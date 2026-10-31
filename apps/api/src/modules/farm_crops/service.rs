//! Crop rules: every farm member reads them; ops and admin write. Never deleted: retire a crop
//! with `is_active = false`.

use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::{farm::FarmAccess, workspace::NOT_ALLOWED},
    modules::farm_crops::repo::{self, CropPatch, CropRow},
};

const CODE_TAKEN: &str = "The code has already been taken.";

fn require_manager(access: &FarmAccess) -> Result<(), AppError> {
    if access.manages() {
        Ok(())
    } else {
        Err(AppError::Forbidden(NOT_ALLOWED))
    }
}

/// One page of the workspace's crops.
pub async fn list(
    state: &AppState,
    access: &FarmAccess,
    limit: i64,
    offset: i64,
) -> Result<(Vec<CropRow>, i64), AppError> {
    Ok(repo::list(&state.db, access.workspace_id, limit, offset).await?)
}

/// Add a crop.
pub async fn create(
    state: &AppState,
    access: &FarmAccess,
    input: CropPatch,
) -> Result<CropRow, AppError> {
    require_manager(access)?;
    let mut tx = access.begin(&state.db).await?;
    let row = repo::insert(&mut *tx, access.workspace_id, &input)
        .await
        .map_err(|e| AppError::unique(e, "farm_crops_code_key", "code", CODE_TAKEN))?;
    tx.commit().await?;
    Ok(row)
}

/// Change a crop of this workspace.
pub async fn update(
    state: &AppState,
    access: &FarmAccess,
    id: &str,
    patch: CropPatch,
) -> Result<CropRow, AppError> {
    require_manager(access)?;
    let id = Uuid::parse_str(id).map_err(|_| AppError::NotFound)?;
    let mut tx = access.begin(&state.db).await?;
    let row = repo::update(&mut *tx, access.workspace_id, id, &patch)
        .await
        .map_err(|e| AppError::unique(e, "farm_crops_code_key", "code", CODE_TAKEN))?
        .ok_or(AppError::NotFound)?;
    tx.commit().await?;
    Ok(row)
}
