//! Price correction rules: at most `MAX_BANDS` live bands per workspace (every product render
//! loads them all); patches keep both ends set.

use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    modules::price_corrections::{
        dto::CorrectionInput,
        repo::{self, CorrectionRow},
    },
};

/// Live bands a workspace may hold. Laravel had no cap.
pub const MAX_BANDS: i64 = 1000;

/// Page of rows and the total.
pub async fn list(
    state: &AppState,
    workspace_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<(Vec<CorrectionRow>, i64), AppError> {
    let rows = repo::list(&state.db, workspace_id, limit, offset).await?;
    let total = match rows.first() {
        Some(r) => r.total.unwrap_or(0),
        None if offset > 0 => repo::count(&state.db, workspace_id).await?,
        None => 0,
    };
    Ok((rows, total))
}

/// One live band, else 404.
pub async fn get(
    state: &AppState,
    workspace_id: Uuid,
    id: &str,
) -> Result<CorrectionRow, AppError> {
    repo::find(&state.db, workspace_id, parse(id)?)
        .await?
        .ok_or(AppError::NotFound)
}

/// Create under the quota, creates serialised per workspace; returns the new id.
pub async fn create(
    state: &AppState,
    workspace_id: Uuid,
    c: CorrectionInput,
) -> Result<Uuid, AppError> {
    let mut tx = state.db.begin().await?;
    repo::lock_quota(&mut tx, workspace_id).await?;
    if repo::count(&mut *tx, workspace_id).await? >= MAX_BANDS {
        return Err(AppError::field(
            "price_corrections",
            format!("The workspace can't hold more than {MAX_BANDS} price corrections."),
        ));
    }
    let id = repo::insert(&mut *tx, workspace_id, &c).await?;
    tx.commit().await?;
    Ok(id)
}

/// Patch a live band, else 404.
pub async fn update(
    state: &AppState,
    workspace_id: Uuid,
    id: &str,
    c: CorrectionInput,
) -> Result<(), AppError> {
    found(repo::update(&state.db, workspace_id, parse(id)?, &c).await?)
}

/// Soft delete a live band, else 404.
pub async fn delete(state: &AppState, workspace_id: Uuid, id: &str) -> Result<(), AppError> {
    found(repo::soft_delete(&state.db, workspace_id, parse(id)?).await?)
}

/// Malformed id = unknown id = 404.
fn parse(id: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(id).map_err(|_| AppError::NotFound)
}

fn found(changed: bool) -> Result<(), AppError> {
    changed.then_some(()).ok_or(AppError::NotFound)
}
