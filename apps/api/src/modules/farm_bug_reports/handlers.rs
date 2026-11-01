//! HTTP edge for /v1/workspace/{workspace}/farm/bug-reports.

use axum::{
    Json,
    extract::{FromRequest, Multipart, Path, Query, Request, State},
    http::{StatusCode, header::CONTENT_TYPE},
};

use crate::{
    AppState,
    error::{AppError, UnprocessableBody},
    http::{
        farm::FarmAccess,
        list::{Page, Paginated},
        response::{Data, Envelope, MessageBody},
    },
    modules::{
        farm_bug_reports::{
            dto::{BugReportFiled, BugReportForm, BugReportResource, from_multipart},
            service,
        },
        farm_catalogue::dto::ListQuery,
    },
};

/// POST /farm/bug-reports (multipart) -> 201, or 200 for a retried id.
#[utoipa::path(
    post, path = "/workspace/{workspace}/farm/bug-reports", tag = "farm_bug_reports",
    operation_id = "farm_bug_reports_create", summary = "File a bug report",
    description = "Anyone with a farm role, always under their own name. Multipart: `route`, \
    `user_agent`, `viewport`, `description`, `actions` (JSON array, at most 30), and \
    `screenshot` and/or `photo` (JPEG, PNG, WebP or HEIC, 5 MB each). Optional `id` for retries.",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body(content = BugReportForm, content_type = "multipart/form-data"),
    responses(
        (status = 200, body = Envelope<BugReportFiled>, description = "Retried: filed before"),
        (status = 201, body = Envelope<BugReportFiled>, description = "Filed"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller has no farm role"),
        (status = 409, body = MessageBody, description = "The id is someone else's report"),
        (status = 422, body = UnprocessableBody, description = "Not multipart, unknown field, missing or invalid field or image"),
    ),
    security(("bearer" = []))
)]
pub async fn create(
    State(state): State<AppState>,
    access: FarmAccess,
    req: Request,
) -> Result<(StatusCode, Data<BugReportFiled>), AppError> {
    let multipart = req
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("multipart/form-data"));
    if !multipart {
        return Err(AppError::field(
            "form",
            "Send the report as a multipart form.",
        ));
    }
    let form = Multipart::from_request(req, &state)
        .await
        .map_err(|e| AppError::field("form", e.body_text()))?;
    let (id, r#ref, new) = service::create(&state, &access, from_multipart(form).await?).await?;
    let status = if new {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((status, Data(BugReportFiled::new(id, r#ref))))
}

/// GET /farm/bug-reports.
#[utoipa::path(
    get, path = "/workspace/{workspace}/farm/bug-reports", tag = "farm_bug_reports",
    operation_id = "farm_bug_reports_list", summary = "List bug reports",
    description = "Farm admin only; not even the reporter reads a report back. Newest first, \
    without image links.",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ListQuery),
    responses(
        (status = 200, body = Paginated<BugReportResource>, description = "One page"),
        (status = 403, body = MessageBody, description = "Caller is not farm admin"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller has no farm role"),
    ),
    security(("bearer" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    access: FarmAccess,
    Query(query): Query<ListQuery>,
) -> Result<Json<Paginated<BugReportResource>>, AppError> {
    let page = Page::parse(query.page.as_deref(), query.page_size.as_deref());
    let (rows, total) = service::list(&state, &access, page.size, page.offset()).await?;
    let data = rows
        .into_iter()
        .map(|r| BugReportResource::new(r, None, None))
        .collect();
    Ok(Json(Paginated::new(data, total, page)))
}

/// GET /farm/bug-reports/{report}.
#[utoipa::path(
    get, path = "/workspace/{workspace}/farm/bug-reports/{report}", tag = "farm_bug_reports",
    operation_id = "farm_bug_reports_get", summary = "Get a bug report",
    description = "Farm admin only, with 10-minute image links.",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("report" = Uuid, Path, description = "Report id"),
    ),
    responses(
        (status = 200, body = Envelope<BugReportResource>, description = "The report"),
        (status = 403, body = MessageBody, description = "Caller is not farm admin"),
        (status = 404, body = MessageBody, description = "Unknown workspace or report"),
    ),
    security(("bearer" = []))
)]
pub async fn get(
    State(state): State<AppState>,
    access: FarmAccess,
    Path((_, id)): Path<(String, String)>,
) -> Result<Data<BugReportResource>, AppError> {
    Ok(Data(service::get(&state, &access, &id).await?))
}
