//! Village rules: every farm member reads them (names are not personal data); ops and admin
//! write. A village is born with a planned capacity in the same transaction (RUAHA: the energy
//! view drops a village without one). Capacity is shown to managers and to the village's own
//! members. A create with a known id replays instead of inserting twice.

use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::{farm::FarmAccess, workspace::NOT_ALLOWED},
    modules::farm_villages::{
        dto::NewVillageInput,
        repo::{self, CapacityRow, NewCapacity, NewVillage, VillagePatch, VillageRow},
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

/// The row as this caller may see it: capacity only for managers and the village's members.
fn visible(access: &FarmAccess, row: VillageRow) -> VillageRow {
    if access.manages() || access.member_villages().contains(&row.id) {
        row
    } else {
        row.without_capacity()
    }
}

/// One page of the workspace's villages.
pub async fn list(
    state: &AppState,
    access: &FarmAccess,
    limit: i64,
    offset: i64,
) -> Result<(Vec<VillageRow>, i64), AppError> {
    let (rows, total) = repo::list(&state.db, access.workspace_id, limit, offset).await?;
    Ok((
        rows.into_iter().map(|r| visible(access, r)).collect(),
        total,
    ))
}

/// Create a village and its first capacity. False = replayed (the id already existed in this
/// workspace).
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
    let id = repo::insert(&mut *tx, ws, &new).await.map_err(|e| {
        match e.as_database_error().and_then(|d| d.constraint()) {
            // Same id in another workspace: not ours to replay.
            Some("farm_villages_pkey") => AppError::Conflict("This id is already in use."),
            _ => AppError::unique(e, "farm_villages_code_key", "code", CODE_TAKEN),
        }
    })?;
    repo::replace_capacity(&mut tx, ws, id, access.user_id, &input.capacity).await?;
    let row = repo::find(&mut *tx, ws, id)
        .await?
        .ok_or(AppError::NotFound)?;
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
    let ws = access.workspace_id;
    let mut tx = access.begin(&state.db).await?;
    repo::update(&mut *tx, ws, id, &patch)
        .await
        .map_err(|e| AppError::unique(e, "farm_villages_code_key", "code", CODE_TAKEN))?
        .ok_or(AppError::NotFound)?;
    let row = repo::find(&mut *tx, ws, id)
        .await?
        .ok_or(AppError::NotFound)?;
    tx.commit().await?;
    Ok(row)
}

/// Record a new current capacity for a village; the previous one is kept as history.
pub async fn add_capacity(
    state: &AppState,
    access: &FarmAccess,
    village: &str,
    capacity: NewCapacity,
) -> Result<CapacityRow, AppError> {
    require_manager(access)?;
    let id = Uuid::parse_str(village).map_err(|_| AppError::NotFound)?;
    let ws = access.workspace_id;
    let mut tx = access.begin(&state.db).await?;
    if !repo::lock(&mut *tx, ws, id).await? {
        return Err(AppError::NotFound);
    }
    let row = repo::replace_capacity(&mut tx, ws, id, access.user_id, &capacity).await?;
    tx.commit().await?;
    Ok(row)
}
