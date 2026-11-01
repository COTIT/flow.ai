//! Tower rules: staff read the villages they are staff of; farmers 403, other villages 404.

use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::{farm::FarmAccess, workspace::NOT_ALLOWED},
    modules::farm_tower::repo::{self, ProductionRow, QualityRow, QueueRow},
};

/// The village id for a staff read of one village: farmers 403, villages the caller is not
/// staff of (or unknown ids) 404. Shared by every per-village Tower read.
pub async fn staff_village(
    state: &AppState,
    access: &FarmAccess,
    village: &str,
) -> Result<Uuid, AppError> {
    if !access.is_staff() {
        return Err(AppError::Forbidden(NOT_ALLOWED));
    }
    let village = Uuid::parse_str(village).map_err(|_| AppError::NotFound)?;
    if !access.staff_in(village)
        || !repo::village_exists(&state.db, access.workspace_id, village).await?
    {
        return Err(AppError::NotFound);
    }
    Ok(village)
}

pub async fn production(
    state: &AppState,
    access: &FarmAccess,
    village: &str,
) -> Result<Vec<ProductionRow>, AppError> {
    let village = staff_village(state, access, village).await?;
    Ok(repo::production(&state.db, access.workspace_id, village).await?)
}

pub async fn data_quality(
    state: &AppState,
    access: &FarmAccess,
    village: &str,
) -> Result<(Uuid, QualityRow), AppError> {
    let village = staff_village(state, access, village).await?;
    Ok((
        village,
        repo::data_quality(&state.db, access.workspace_id, village).await?,
    ))
}

/// Outstanding records the caller's verify lists hold, per kind. Any farm role.
pub async fn verify_queue(state: &AppState, access: &FarmAccess) -> Result<QueueRow, AppError> {
    let scope = access.staff_scope();
    Ok(repo::verify_queue(
        &state.db,
        access.workspace_id,
        scope.all,
        &scope.villages,
        access.person_id,
    )
    .await?)
}
