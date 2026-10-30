//! Custom status rules: name unique per (workspace, domain) across archived rows too,
//! case-insensitive; the domain is fixed after create.

use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    modules::{
        custom_statuses::{
            dto::{CustomStatusPatch, NewCustomStatus},
            repo::{self, CustomStatusRow, ListFilter},
        },
        custom_types::dto::NOT_UNIQUE,
    },
};

/// One list page plus the total matching count.
pub async fn list(
    state: &AppState,
    workspace_id: Uuid,
    f: &ListFilter<'_>,
) -> Result<(Vec<CustomStatusRow>, i64), AppError> {
    let (rows, mut total) = repo::list(&state.db, workspace_id, f).await?;
    if rows.is_empty() && f.offset > 0 {
        total = repo::count(&state.db, workspace_id, f).await?;
    }
    Ok((rows, total))
}

/// Archived statuses are still readable by id (Laravel GetCustomStatus).
pub async fn get(
    state: &AppState,
    workspace_id: Uuid,
    id: &str,
) -> Result<CustomStatusRow, AppError> {
    repo::find(&state.db, workspace_id, parse(id)?)
        .await?
        .ok_or(AppError::NotFound)
}

/// Insert after the module's checks; returns the new id.
pub async fn create(
    state: &AppState,
    workspace_id: Uuid,
    input: NewCustomStatus,
) -> Result<Uuid, AppError> {
    unique(state, workspace_id, &input.domain, &input.name, None).await?;
    let ids = repo::insert_many(
        &state.db,
        workspace_id,
        &input.domain,
        &[input.name.as_str()],
        input.meta.as_ref(),
    )
    .await
    .map_err(taken)?;
    ids.into_iter()
        .next()
        .ok_or_else(|| AppError::Internal("insert returned no id".into()))
}

/// Saving the unchanged name is fine (Laravel's rule didn't skip the row itself).
pub async fn update(
    state: &AppState,
    workspace_id: Uuid,
    id: &str,
    patch: CustomStatusPatch,
) -> Result<(), AppError> {
    let row = get(state, workspace_id, id).await?;
    if let Some(name) = &patch.name {
        unique(state, workspace_id, &row.domain, name, Some(row.id)).await?;
    }
    let changed = repo::update(
        &state.db,
        workspace_id,
        row.id,
        patch.name.as_deref(),
        patch.meta.as_ref().map(Option::as_ref),
    )
    .await
    .map_err(taken)?;
    changed.then_some(()).ok_or(AppError::NotFound)
}

/// Soft delete a live row of this workspace, else 404.
pub async fn delete(state: &AppState, workspace_id: Uuid, id: &str) -> Result<(), AppError> {
    repo::soft_delete(&state.db, workspace_id, parse(id)?)
        .await?
        .then_some(())
        .ok_or(AppError::NotFound)
}

/// Friendly pre-check; the unique index still catches a parallel insert (`taken`).
async fn unique(
    state: &AppState,
    workspace_id: Uuid,
    domain: &str,
    name: &str,
    except: Option<Uuid>,
) -> Result<(), AppError> {
    if repo::name_taken(&state.db, workspace_id, domain, name, except).await? {
        return Err(AppError::field("name", NOT_UNIQUE));
    }
    Ok(())
}

fn taken(err: sqlx::Error) -> AppError {
    match err.as_database_error().and_then(|e| e.constraint()) {
        Some("custom_statuses_name_unique") => AppError::field("name", NOT_UNIQUE),
        _ => err.into(),
    }
}

/// Malformed id = unknown id = 404 (Laravel's delete answered 500).
fn parse(id: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(id).map_err(|_| AppError::NotFound)
}
