//! Row rules: reached only through a live section of a live warehouse; code unique per
//! section.

use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    modules::{
        warehouse_rows::{
            dto::{CODE_TAKEN, Fields},
            repo::{self, RowRow},
        },
        warehouse_sections::{repo::SectionRow, service as sections},
    },
};

/// The path's section, else 404.
pub async fn section(
    state: &AppState,
    workspace_id: Uuid,
    warehouse_id: &str,
    section_id: &str,
) -> Result<SectionRow, AppError> {
    sections::find(state, workspace_id, warehouse_id, section_id).await
}

/// The path's row under the path's section, else 404.
pub async fn find(
    state: &AppState,
    workspace_id: Uuid,
    path: (&str, &str, &str),
) -> Result<(SectionRow, RowRow), AppError> {
    let s = section(state, workspace_id, path.0, path.1).await?;
    let row = repo::find(&state.db, workspace_id, s.id, sections::parse(path.2)?)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok((s, row))
}

/// One page plus total.
pub async fn list(
    state: &AppState,
    workspace_id: Uuid,
    section_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<(Vec<RowRow>, i64), AppError> {
    let (rows, mut total) = repo::list(&state.db, workspace_id, section_id, limit, offset).await?;
    if rows.is_empty() && offset > 0 {
        total = repo::count(&state.db, workspace_id, section_id).await?;
    }
    Ok((rows, total))
}

/// Insert; returns the new id.
pub async fn create(
    state: &AppState,
    workspace_id: Uuid,
    s: &SectionRow,
    f: Fields,
) -> Result<Uuid, AppError> {
    let code = f.row_code.as_deref().unwrap_or_default();
    if repo::code_taken(&state.db, s.id, code, None).await? {
        return Err(AppError::field("row_code", CODE_TAKEN));
    }
    repo::insert(&state.db, workspace_id, s.warehouse_id, s.id, &f)
        .await
        .map_err(code_race)
}

/// Patch; 404 when the row is gone.
pub async fn update(
    state: &AppState,
    workspace_id: Uuid,
    s: &SectionRow,
    id: &str,
    f: Fields,
) -> Result<(), AppError> {
    let id = sections::parse(id)?;
    if let Some(code) = f.row_code.as_deref()
        && repo::code_taken(&state.db, s.id, code, Some(id)).await?
    {
        return Err(AppError::field("row_code", CODE_TAKEN));
    }
    repo::update(&state.db, workspace_id, s.id, id, &f)
        .await
        .map_err(code_race)?
        .then_some(())
        .ok_or(AppError::NotFound)
}

/// Soft delete, else 404.
pub async fn delete(
    state: &AppState,
    workspace_id: Uuid,
    s: &SectionRow,
    id: &str,
) -> Result<(), AppError> {
    repo::soft_delete(&state.db, workspace_id, s.id, sections::parse(id)?)
        .await?
        .then_some(())
        .ok_or(AppError::NotFound)
}

/// The unique index caught a race the pre-check missed: same 422.
fn code_race(e: sqlx::Error) -> AppError {
    match e.as_database_error().and_then(|d| d.constraint()) {
        Some("warehouse_section_rows_code_unique") => AppError::field("row_code", CODE_TAKEN),
        _ => AppError::Database(e),
    }
}
