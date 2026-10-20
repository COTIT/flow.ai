//! Hourly rate rules: the VAT group must be a live group of the same workspace.

use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    modules::{
        hourly_rates::{
            dto::{BAD_VAT_GROUP, HourlyRatePatch, NewHourlyRate},
            repo::{self, HourlyRateRow},
        },
        vat_groups::repo as vat_groups,
    },
};

/// One list page plus the total matching count.
pub async fn list(
    state: &AppState,
    workspace_id: Uuid,
    pattern: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<HourlyRateRow>, i64), AppError> {
    let (rows, mut total) =
        repo::select(&state.db, workspace_id, None, pattern, limit, offset).await?;
    if rows.is_empty() && offset > 0 {
        total = repo::count(&state.db, workspace_id, pattern).await?;
    }
    Ok((rows, total))
}

pub async fn get(
    state: &AppState,
    workspace_id: Uuid,
    id: &str,
) -> Result<HourlyRateRow, AppError> {
    let (mut rows, _) = repo::select(&state.db, workspace_id, Some(parse(id)?), None, 1, 0).await?;
    rows.pop().ok_or(AppError::NotFound)
}

pub async fn create(
    state: &AppState,
    workspace_id: Uuid,
    input: NewHourlyRate,
) -> Result<Uuid, AppError> {
    live_vat_group(state, workspace_id, input.vat_group_id).await?;
    Ok(repo::insert(&state.db, workspace_id, &input).await?)
}

pub async fn update(
    state: &AppState,
    workspace_id: Uuid,
    id: &str,
    patch: HourlyRatePatch,
) -> Result<(), AppError> {
    let id = parse(id)?;
    if let Some(Some(vat)) = patch.vat_group_id {
        live_vat_group(state, workspace_id, vat).await?;
    }
    found(repo::update(&state.db, workspace_id, id, &patch).await?)
}

/// Soft delete, no guard (Laravel parity): hour registrations copied the price already.
pub async fn delete(state: &AppState, workspace_id: Uuid, id: &str) -> Result<(), AppError> {
    found(repo::soft_delete(&state.db, workspace_id, parse(id)?).await?)
}

/// Laravel ValidateId: the uuid must be a live VAT group of this workspace.
async fn live_vat_group(state: &AppState, workspace_id: Uuid, id: Uuid) -> Result<(), AppError> {
    vat_groups::find(&state.db, workspace_id, id)
        .await?
        .map(drop)
        .ok_or_else(|| AppError::field("vat_group_id", BAD_VAT_GROUP))
}

/// Malformed id = unknown id = 404.
fn parse(id: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(id).map_err(|_| AppError::NotFound)
}

fn found(changed: bool) -> Result<(), AppError> {
    changed.then_some(()).ok_or(AppError::NotFound)
}
