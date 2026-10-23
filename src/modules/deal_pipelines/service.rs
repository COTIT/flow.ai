//! Deal pipeline rules. Column writes of one board run under its row lock: positions stay
//! distinct, a custom status is used once per board, and one column at most is won or lost.

use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    modules::deal_pipelines::{
        dto::{
            FLAG_TAKEN, NewPipeline, NewStatus, PipelinePatch, PipelineResource,
            STATUS_IN_PIPELINE, STATUS_MISSING, StatusPatch,
        },
        repo::{self, PipelineRow, StatusRow},
    },
};

/// One page of boards with their columns, plus the total. Columns load in one query.
pub async fn list(
    state: &AppState,
    workspace_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<(Vec<PipelineResource>, i64), AppError> {
    let (rows, mut total) = repo::list(&state.db, workspace_id, limit, offset).await?;
    if rows.is_empty() && offset > 0 {
        total = repo::count(&state.db, workspace_id).await?;
    }
    Ok((with_statuses(state, workspace_id, rows).await?, total))
}

/// One board with its columns.
pub async fn get(
    state: &AppState,
    workspace_id: Uuid,
    id: &str,
) -> Result<PipelineResource, AppError> {
    let row = repo::find(&state.db, workspace_id, parse(id)?)
        .await?
        .ok_or(AppError::NotFound)?;
    let mut one = with_statuses(state, workspace_id, vec![row]).await?;
    one.pop()
        .ok_or_else(|| AppError::Internal("pipeline vanished between reads".into()))
}

/// New board; returns its id.
pub async fn create(
    state: &AppState,
    workspace_id: Uuid,
    input: NewPipeline,
) -> Result<Uuid, AppError> {
    Ok(repo::insert(&state.db, workspace_id, &input).await?)
}

/// Patch a live board of this workspace, else 404.
pub async fn update(
    state: &AppState,
    workspace_id: Uuid,
    id: &str,
    patch: PipelinePatch,
) -> Result<(), AppError> {
    repo::update(&state.db, workspace_id, parse(id)?, &patch)
        .await?
        .then_some(())
        .ok_or(AppError::NotFound)
}

/// New column on the board in the path (Laravel used the body's `deal_pipeline_id`).
pub async fn create_status(
    state: &AppState,
    workspace_id: Uuid,
    pipeline: &str,
    input: NewStatus,
) -> Result<Uuid, AppError> {
    let pipeline = parse(pipeline)?;
    let mut tx = state.db.begin().await?;
    if !repo::lock(&mut tx, workspace_id, pipeline).await? {
        return Err(AppError::NotFound);
    }
    if !repo::custom_status_exists(&mut *tx, workspace_id, input.custom_status_id).await? {
        return Err(AppError::field("custom_status_id", STATUS_MISSING));
    }
    if repo::custom_status_used(&mut *tx, workspace_id, pipeline, input.custom_status_id).await? {
        return Err(AppError::field("custom_status_id", STATUS_IN_PIPELINE));
    }
    if repo::flag_taken(&mut *tx, workspace_id, pipeline, &input.props, None).await? {
        return Err(AppError::field("properties", FLAG_TAKEN));
    }
    repo::shift_from(&mut tx, workspace_id, pipeline, input.position, None).await?;
    let id = repo::insert_status(&mut tx, workspace_id, pipeline, &input)
        .await
        .map_err(taken)?;
    tx.commit().await?;
    Ok(id)
}

/// Patch a column of the board in the path, else 404. Its own won/lost flag doesn't count
/// against it (Laravel's check did, so the won column couldn't be saved).
pub async fn update_status(
    state: &AppState,
    workspace_id: Uuid,
    pipeline: &str,
    id: &str,
    patch: StatusPatch,
) -> Result<(), AppError> {
    let (pipeline, id) = (parse(pipeline)?, parse(id)?);
    let mut tx = state.db.begin().await?;
    if !repo::lock(&mut tx, workspace_id, pipeline).await?
        || !repo::status_exists(&mut *tx, workspace_id, pipeline, id).await?
    {
        return Err(AppError::NotFound);
    }
    if let Some(props) = &patch.props
        && repo::flag_taken(&mut *tx, workspace_id, pipeline, props, Some(id)).await?
    {
        return Err(AppError::field("properties", FLAG_TAKEN));
    }
    if let Some(position) = patch.position {
        repo::shift_from(&mut tx, workspace_id, pipeline, position, Some(id)).await?;
    }
    repo::update_status(&mut tx, workspace_id, pipeline, id, &patch)
        .await
        .map_err(taken)?;
    tx.commit().await?;
    Ok(())
}

/// Soft delete a column of the board in the path, else 404.
// shortcut: no deals table yet, so nothing moves to a fallback column; B8-2 adds the move.
pub async fn delete_status(
    state: &AppState,
    workspace_id: Uuid,
    pipeline: &str,
    id: &str,
) -> Result<(), AppError> {
    let (pipeline, id) = (parse(pipeline)?, parse(id)?);
    let mut tx = state.db.begin().await?;
    if !repo::lock(&mut tx, workspace_id, pipeline).await?
        || !repo::soft_delete_status(&mut tx, workspace_id, pipeline, id).await?
    {
        return Err(AppError::NotFound);
    }
    tx.commit().await?;
    Ok(())
}

/// Attach each board's columns (already in position order) to its row.
async fn with_statuses(
    state: &AppState,
    workspace_id: Uuid,
    rows: Vec<PipelineRow>,
) -> Result<Vec<PipelineResource>, AppError> {
    let ids: Vec<Uuid> = rows.iter().map(|p| p.id).collect();
    let mut statuses = repo::statuses_for(&state.db, workspace_id, &ids)
        .await?
        .into_iter()
        .peekable();
    let mut out = Vec::with_capacity(rows.len());
    // Boards (`ORDER BY id`) and columns (`ORDER BY deal_pipeline_id`) share one order, so one
    // pass pairs them.
    for p in rows {
        let mut mine: Vec<StatusRow> = Vec::new();
        while let Some(s) = statuses.next_if(|s| s.deal_pipeline_id == p.id) {
            mine.push(s);
        }
        out.push(PipelineResource::new(p, mine));
    }
    Ok(out)
}

/// The unique indexes behind the pre-checks, for a write the lock didn't cover.
fn taken(e: sqlx::Error) -> AppError {
    match e.as_database_error().and_then(|d| d.constraint()) {
        Some("deal_pipeline_statuses_custom_status_once") => {
            AppError::field("custom_status_id", STATUS_IN_PIPELINE)
        }
        Some("deal_pipeline_statuses_one_won" | "deal_pipeline_statuses_one_lost") => {
            AppError::field("properties", FLAG_TAKEN)
        }
        _ => e.into(),
    }
}

/// Malformed id = unknown id = 404.
fn parse(id: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(id).map_err(|_| AppError::NotFound)
}
