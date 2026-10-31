//! Village rules: every farm member reads them (names are not personal data); ops and admin
//! write. A create with a known id replays instead of inserting twice.

use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::{farm::FarmAccess, workspace::NOT_ALLOWED},
    modules::farm_villages::{
        dto::NewVillageInput,
        repo::{self, NewVillage, VillagePatch, VillageRow},
    },
};

const CODE_TAKEN: &str = "The code has already been taken.";

fn require_manager(access: &FarmAccess) -> Result<(), AppError> {
    if access.manages() {
        Ok(())
    } else {
        Err(AppError::Forbidden(NOT_ALLOWED))
    }
}

/// One page of the workspace's villages.
pub async fn list(
    state: &AppState,
    access: &FarmAccess,
    limit: i64,
    offset: i64,
) -> Result<(Vec<VillageRow>, i64), AppError> {
    Ok(repo::list(&state.db, access.workspace_id, limit, offset).await?)
}

/// Create a village. False = replayed (the id already existed in this workspace).
pub async fn create(
    state: &AppState,
    access: &FarmAccess,
    input: NewVillageInput,
) -> Result<(VillageRow, bool), AppError> {
    require_manager(access)?;
    let ws = access.workspace_id;
    if let Some(id) = input.id
        && let Some(row) = repo::find(&state.db, ws, id).await?
    {
        return Ok((row, false));
    }
    let mut tx = access.begin(&state.db).await?;
    let new = NewVillage {
        id: input.id,
        code: &input.code,
        name: &input.name,
        latitude: input.latitude,
        longitude: input.longitude,
        primary_language: &input.primary_language,
    };
    let row = repo::insert(&mut *tx, ws, &new).await.map_err(|e| {
        match e.as_database_error().and_then(|d| d.constraint()) {
            // Same id in another workspace: not ours to replay.
            Some("farm_villages_pkey") => AppError::Conflict("This id is already in use."),
            _ => AppError::unique(e, "farm_villages_code_key", "code", CODE_TAKEN),
        }
    })?;
    tx.commit().await?;
    Ok((row, true))
}

/// Change a village of this workspace.
pub async fn update(
    state: &AppState,
    access: &FarmAccess,
    id: &str,
    patch: VillagePatch,
) -> Result<VillageRow, AppError> {
    require_manager(access)?;
    let id = Uuid::parse_str(id).map_err(|_| AppError::NotFound)?;
    let mut tx = access.begin(&state.db).await?;
    let row = repo::update(&mut *tx, access.workspace_id, id, &patch)
        .await
        .map_err(|e| AppError::unique(e, "farm_villages_code_key", "code", CODE_TAKEN))?
        .ok_or(AppError::NotFound)?;
    tx.commit().await?;
    Ok(row)
}
