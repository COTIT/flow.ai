//! Scheduled report rules: own rows only, report permission on create and resume, at most
//! `MAX_PER_MEMBER` per member per workspace, next slot from the workspace zone.

use chrono::Utc;
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::workspace::{NOT_ALLOWED, WorkspaceAccess},
    modules::{
        auth::token,
        scheduled_reports::{
            dto::{NewSchedule, ScheduledReport},
            repo,
        },
    },
};

/// Most schedules one member keeps in one workspace.
pub const MAX_PER_MEMBER: i64 = 10;

/// The list permission a report reads under; the to-do list follows the reader's own.
pub fn permission(report: &str) -> Option<&'static str> {
    match report {
        "sales_summary" => Some("order.list"),
        "overdue_invoices" => Some("invoice.list"),
        "stock" => Some("stock.list"),
        _ => None,
    }
}

fn may_read(access: &WorkspaceAccess, report: &str) -> Result<(), AppError> {
    match permission(report) {
        Some(p) if !access.allows(p) => Err(AppError::Forbidden(NOT_ALLOWED)),
        _ => Ok(()),
    }
}

fn parse(id: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(id).map_err(|_| AppError::NotFound)
}

/// The caller's schedules here.
pub async fn list(
    state: &AppState,
    access: &WorkspaceAccess,
) -> Result<Vec<ScheduledReport>, AppError> {
    let zone = repo::zone(&state.db, access.workspace_id).await?;
    let rows = repo::list(&state.db, access.workspace_id, access.user_id).await?;
    Ok(rows
        .into_iter()
        .map(|r| ScheduledReport::new(r, &zone))
        .collect())
}

async fn one(
    state: &AppState,
    access: &WorkspaceAccess,
    id: Uuid,
) -> Result<ScheduledReport, AppError> {
    list(state, access)
        .await?
        .into_iter()
        .find(|s| s.id() == id)
        .ok_or(AppError::NotFound)
}

/// New schedule, first slot after now.
pub async fn create(
    state: &AppState,
    access: &WorkspaceAccess,
    input: NewSchedule,
) -> Result<ScheduledReport, AppError> {
    may_read(access, &input.report)?;
    let (ws, user) = (access.workspace_id, access.user_id);
    let mut tx = state.db.begin().await?;
    // Serialize one member's creates so two at once can't pass the cap together.
    sqlx::query!(
        "SELECT pg_advisory_xact_lock(hashtextextended($1::uuid::text || $2::uuid::text, 0))",
        ws,
        user,
    )
    .execute(&mut *tx)
    .await?;
    if repo::count(&mut *tx, ws, user).await? >= MAX_PER_MEMBER {
        return Err(AppError::field(
            "report",
            format!("You can keep at most {MAX_PER_MEMBER} scheduled reports in this workspace."),
        ));
    }
    let next = repo::next_run(&mut *tx, ws, &input.rule, Utc::now()).await?;
    let id = repo::insert(&mut *tx, ws, user, &input.report, &input.rule, next).await?;
    tx.commit().await?;
    one(state, access, id).await
}

/// Pause (`user`) or resume. Resume rechecks the report's permission and restarts from the
/// next slot after now.
pub async fn set_active(
    state: &AppState,
    access: &WorkspaceAccess,
    id: &str,
    active: bool,
) -> Result<ScheduledReport, AppError> {
    let id = parse(id)?;
    let (ws, user) = (access.workspace_id, access.user_id);
    let s = repo::owned(&state.db, ws, user, id)
        .await?
        .ok_or(AppError::NotFound)?;
    if active {
        may_read(access, &s.report)?;
        let next = repo::next_run(&state.db, ws, &s.rule(), Utc::now()).await?;
        repo::set_active(&state.db, ws, user, id, None, Some(next)).await?;
    } else {
        repo::set_active(&state.db, ws, user, id, Some("user"), None).await?;
    }
    one(state, access, id).await
}

/// Delete the caller's schedule, else 404.
pub async fn delete(state: &AppState, access: &WorkspaceAccess, id: &str) -> Result<(), AppError> {
    let id = parse(id)?;
    repo::delete(&state.db, access.workspace_id, access.user_id, id)
        .await?
        .then_some(())
        .ok_or(AppError::NotFound)
}

/// Pause the schedule a mailed link belongs to; its report. Unknown link: 404.
pub async fn unsubscribe(state: &AppState, plain: &str) -> Result<String, AppError> {
    if !token::looks_valid(plain) {
        return Err(AppError::NotFound);
    }
    repo::unsubscribe(&state.db, &token::hash(plain))
        .await?
        .ok_or(AppError::NotFound)
}
