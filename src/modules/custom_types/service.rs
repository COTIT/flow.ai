//! Custom type rules: name unique per (workspace, domain), case-insensitive; the domain is
//! fixed after create; `properties` follow the domain.

use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::json::Rules,
    modules::custom_types::{
        dto::{CustomTypePatch, NOT_UNIQUE, NewCustomType, properties},
        repo::{self, CustomTypeRow, ListFilter},
    },
};

/// One list page plus the total matching count.
pub async fn list(
    state: &AppState,
    workspace_id: Uuid,
    f: &ListFilter<'_>,
) -> Result<(Vec<CustomTypeRow>, i64), AppError> {
    let (rows, mut total) = repo::list(&state.db, workspace_id, f).await?;
    if rows.is_empty() && f.offset > 0 {
        total = repo::count(&state.db, workspace_id, f).await?;
    }
    Ok((rows, total))
}

pub async fn get(
    state: &AppState,
    workspace_id: Uuid,
    id: &str,
) -> Result<CustomTypeRow, AppError> {
    repo::find(&state.db, workspace_id, parse(id)?)
        .await?
        .ok_or(AppError::NotFound)
}

pub async fn create(
    state: &AppState,
    workspace_id: Uuid,
    input: NewCustomType,
) -> Result<Uuid, AppError> {
    unique(state, workspace_id, &input.domain, &input.name, None).await?;
    let ids = repo::insert_many(
        &state.db,
        workspace_id,
        &[input.name.as_str()],
        &[input.domain.as_str()],
        &[input.properties],
        input.meta.as_ref(),
    )
    .await
    .map_err(taken)?;
    ids.into_iter()
        .next()
        .ok_or_else(|| AppError::Internal("insert returned no id".into()))
}

/// Unknown id -> 404 before anything else (Laravel answered 422 `domain` here).
pub async fn update(
    state: &AppState,
    workspace_id: Uuid,
    id: &str,
    patch: CustomTypePatch,
) -> Result<(), AppError> {
    let row = get(state, workspace_id, id).await?;
    let props = match patch.properties {
        None => None,
        Some(given) => {
            let mut r = Rules::default();
            let kept = properties(&mut r, &row.domain, given);
            r.into_result()?;
            Some(kept)
        }
    };
    if let Some(name) = &patch.name {
        unique(state, workspace_id, &row.domain, name, Some(row.id)).await?;
    }
    let changed = repo::update(
        &state.db,
        workspace_id,
        row.id,
        patch.name.as_deref(),
        props.as_ref().map(Option::as_ref),
        patch.meta.as_ref().map(Option::as_ref),
    )
    .await
    .map_err(taken)?;
    changed.then_some(()).ok_or(AppError::NotFound)
}

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
        Some("custom_types_name_unique") => AppError::field("name", NOT_UNIQUE),
        _ => err.into(),
    }
}

/// Malformed id = unknown id = 404.
fn parse(id: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(id).map_err(|_| AppError::NotFound)
}
