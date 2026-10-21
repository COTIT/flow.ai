//! Discount rules: the group must be a live group of the workspace; the window must run
//! forward (Laravel `after:valid_from`, checked on the merged row on update).

use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    modules::{
        discount_groups,
        discounts::{
            dto::{DiscountInput, GROUP_INVALID, TO_AFTER_FROM},
            repo::{self, DiscountRow},
        },
    },
};

/// Page of rows and the total; `group` filters by a live group (unknown group = 404).
pub async fn list(
    state: &AppState,
    workspace_id: Uuid,
    group: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<DiscountRow>, i64), AppError> {
    let group = match group.map(str::trim).filter(|g| !g.is_empty()) {
        None => None,
        Some(g) => Some(
            live_group(state, workspace_id, g)
                .await?
                .ok_or(AppError::NotFound)?,
        ),
    };
    let rows = repo::list(&state.db, workspace_id, group, limit, offset).await?;
    let total = match rows.first() {
        Some(r) => r.total.unwrap_or(0),
        None if offset > 0 => repo::count(&state.db, workspace_id, group).await?,
        None => 0,
    };
    Ok((rows, total))
}

/// One live discount, else 404.
pub async fn get(state: &AppState, workspace_id: Uuid, id: &str) -> Result<DiscountRow, AppError> {
    repo::find(&state.db, workspace_id, parse(id)?)
        .await?
        .ok_or(AppError::NotFound)
}

/// Create inside a live group; returns the new id.
pub async fn create(
    state: &AppState,
    workspace_id: Uuid,
    d: DiscountInput,
) -> Result<Uuid, AppError> {
    let group = match d.discount_group_id {
        Some(id) => discount_groups::repo::find(&state.db, workspace_id, id)
            .await?
            .map(|g| g.id),
        None => None,
    }
    .ok_or_else(|| AppError::field("discount_group_id", GROUP_INVALID))?;
    window(d.valid_from.flatten(), d.valid_to.flatten())?;
    Ok(repo::insert(&state.db, workspace_id, group, &d).await?)
}

/// Patch a live discount (window checked on the merged row), else 404.
pub async fn update(
    state: &AppState,
    workspace_id: Uuid,
    id: &str,
    d: DiscountInput,
) -> Result<(), AppError> {
    let current = get(state, workspace_id, id).await?;
    let from = d.valid_from.unwrap_or(current.valid_from);
    let to = d.valid_to.unwrap_or(current.valid_to);
    window(from, to)?;
    found(repo::update(&state.db, workspace_id, current.id, &d).await?)
}

/// Soft delete a live discount, else 404.
pub async fn delete(state: &AppState, workspace_id: Uuid, id: &str) -> Result<(), AppError> {
    found(repo::soft_delete(&state.db, workspace_id, parse(id)?).await?)
}

/// Laravel `after:valid_from`: the end lies after the start.
fn window(from: Option<chrono::NaiveDate>, to: Option<chrono::NaiveDate>) -> Result<(), AppError> {
    match (from, to) {
        (Some(f), Some(t)) if t <= f => Err(AppError::field("valid_to", TO_AFTER_FROM)),
        _ => Ok(()),
    }
}

async fn live_group(
    state: &AppState,
    workspace_id: Uuid,
    id: &str,
) -> Result<Option<Uuid>, AppError> {
    let Ok(id) = Uuid::parse_str(id) else {
        return Ok(None);
    };
    Ok(discount_groups::repo::find(&state.db, workspace_id, id)
        .await?
        .map(|g| g.id))
}

/// Malformed id = unknown id = 404.
fn parse(id: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(id).map_err(|_| AppError::NotFound)
}

fn found(changed: bool) -> Result<(), AppError> {
    changed.then_some(()).ok_or(AppError::NotFound)
}
