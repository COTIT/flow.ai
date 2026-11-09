//! HTTP edge for /v1/workspace/{workspace}/scheduled-reports (any member, own rows only) and
//! the public /v1/scheduled-reports/unsubscribe.

use axum::{
    extract::{Path, State},
    http::StatusCode,
};

use crate::{
    AppState,
    error::{AppError, ValidationErrorBody},
    http::{
        json::ValidatedJson,
        response::{Data, Envelope, Message, MessageBody},
        workspace::WorkspaceAccess,
    },
    modules::scheduled_reports::{
        dto::{
            CreateScheduledReportRequest, ScheduledReport, UnsubscribeRequest, Unsubscribed,
            UpdateScheduledReportRequest,
        },
        service,
    },
};

/// GET /scheduled-reports -> `{data:[...]}`, the caller's own.
#[utoipa::path(
    get, path = "/workspace/{workspace}/scheduled-reports", tag = "scheduled_reports",
    operation_id = "scheduled_reports_list",
    summary = "My scheduled reports (recurring emails, schedules, weekly summaries; geplande rapporten; ripoti zilizopangwa)",
    description = "The caller's own scheduled reports in this workspace (at most 10), oldest first, with the next run, the last outcome and why one is paused. Read-only.",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    responses(
        (status = 200, body = Envelope<Vec<ScheduledReport>>, description = "The caller's schedules"),
        (status = 403, body = MessageBody, description = "Inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    access: WorkspaceAccess,
) -> Result<Data<Vec<ScheduledReport>>, AppError> {
    Ok(Data(service::list(&state, &access).await?))
}

/// POST /scheduled-reports -> 201 `{data}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/scheduled-reports", tag = "scheduled_reports",
    operation_id = "scheduled_reports_create",
    summary = "Email me a report on a schedule (every day / Monday / month: sales summary, to-do, overdue invoices, low stock; elke maandag, wekelijks; kila Jumatatu, kila wiki)",
    description = "Mails one read-only report to the caller's own login email, around `hour` local time (workspace `general.timezone`, UTC when unset), every day, one ISO weekday (1 = Monday) or one day of the month (1-28). There is no recipient field: it is always the caller. `overdue_invoices` and `stock` only mail when something is overdue, sold out or at/below the workspace low-stock threshold, so they work as a daily alert. Each run rechecks membership and the report's permission and pauses (`access_lost`) instead of sending. At most 10 per member per workspace.",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = CreateScheduledReportRequest,
    responses(
        (status = 201, body = Envelope<ScheduledReport>, description = "Scheduled; `next_run_at` is the first send"),
        (status = 403, body = MessageBody, description = "Inactive membership, or missing the report's list permission (`order.list`, `invoice.list`, `stock.list`)"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Unknown report or frequency, hour outside 0..23, weekday/day_of_month missing or not for this frequency, or 10 schedules already"),
    ),
    security(("bearer" = []))
)]
pub async fn create(
    State(state): State<AppState>,
    access: WorkspaceAccess,
    ValidatedJson(input): ValidatedJson<CreateScheduledReportRequest>,
) -> Result<(StatusCode, Data<ScheduledReport>), AppError> {
    let s = service::create(&state, &access, input).await?;
    Ok((StatusCode::CREATED, Data(s)))
}

/// POST /scheduled-reports/{schedule} -> 200 `{data}`: pause or resume.
#[utoipa::path(
    post, path = "/workspace/{workspace}/scheduled-reports/{schedule}", tag = "scheduled_reports",
    operation_id = "scheduled_reports_update",
    summary = "Pause or resume one of my scheduled reports (stop / restart the emails)",
    description = "`active: false` pauses (`paused_reason` `user`); `active: true` resumes from the next slot after now and rechecks the report's permission. Only the caller's own schedule.",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("schedule" = Uuid, Path, description = "Scheduled report id"),
    ),
    request_body = UpdateScheduledReportRequest,
    responses(
        (status = 200, body = Envelope<ScheduledReport>, description = "Updated schedule"),
        (status = 403, body = MessageBody, description = "Inactive membership, or resuming without the report's permission"),
        (status = 404, body = MessageBody, description = "Unknown workspace or schedule, or not the caller's"),
        (status = 422, body = ValidationErrorBody, description = "`active` missing or not a boolean"),
    ),
    security(("bearer" = []))
)]
pub async fn update(
    State(state): State<AppState>,
    access: WorkspaceAccess,
    Path((_, id)): Path<(String, String)>,
    ValidatedJson(active): ValidatedJson<UpdateScheduledReportRequest>,
) -> Result<Data<ScheduledReport>, AppError> {
    Ok(Data(
        service::set_active(&state, &access, &id, active).await?,
    ))
}

/// DELETE /scheduled-reports/{schedule} -> `{"message":"ok"}`.
#[utoipa::path(
    delete, path = "/workspace/{workspace}/scheduled-reports/{schedule}", tag = "scheduled_reports",
    operation_id = "scheduled_reports_delete",
    summary = "Delete one of my scheduled reports",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("schedule" = Uuid, Path, description = "Scheduled report id"),
    ),
    responses(
        (status = 200, body = MessageBody, description = "Deleted"),
        (status = 403, body = MessageBody, description = "Inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or schedule, or not the caller's"),
    ),
    security(("bearer" = []))
)]
pub async fn delete(
    State(state): State<AppState>,
    access: WorkspaceAccess,
    Path((_, id)): Path<(String, String)>,
) -> Result<Message, AppError> {
    service::delete(&state, &access, &id).await?;
    Ok(Message("ok"))
}

/// POST /scheduled-reports/unsubscribe (public, rate limited) -> `{data:{report}}`.
#[utoipa::path(
    post, path = "/scheduled-reports/unsubscribe", tag = "scheduled_reports",
    operation_id = "scheduled_reports_unsubscribe",
    summary = "Stop a scheduled report from its mail link",
    description = "Pauses the one schedule whose mail carried `token` (`paused_reason` `unsubscribed`; a schedule already paused keeps its reason). Again is fine. Links work while the run is logged (90 days).",
    request_body = UnsubscribeRequest,
    responses(
        (status = 200, body = Envelope<Unsubscribed>, description = "Paused"),
        (status = 404, body = MessageBody, description = "Unknown or expired link"),
        (status = 422, body = ValidationErrorBody, description = "`token` missing"),
    ),
)]
pub async fn unsubscribe(
    State(state): State<AppState>,
    ValidatedJson(token): ValidatedJson<UnsubscribeRequest>,
) -> Result<Data<Unsubscribed>, AppError> {
    let report = service::unsubscribe(&state, &token).await?;
    Ok(Data(Unsubscribed { report }))
}
