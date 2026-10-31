//! HTTP edge for /v1/workspace/{workspace}/settings/pdf-templates (`pdftemplate.*`).

use axum::{
    Json,
    extract::{Path, Query, State},
    http::header,
    response::{IntoResponse, Response},
};
use serde_json::Value;

use crate::{
    AppState,
    error::{AppError, UnprocessableBody, ValidationErrorBody},
    http::{
        json::{Rules, ValidatedJson},
        list::{self, Page, Paginated},
        response::{Data, Envelope, Message, MessageBody},
        workspace::{Can, permissions},
    },
    modules::{
        files::dto::FileResource,
        pdf_templates::{
            dto::{
                AvailableFields, CreateTemplateRequest, DOMAINS, ListQuery, PdfTemplateResource,
                TemplateCreated, UpdateTemplateRequest,
            },
            fields,
            repo::ListFilter,
            service,
        },
    },
};

permissions! {
    TemplateList => "pdftemplate.list",
    TemplateView => "pdftemplate.view",
    TemplateCreate => "pdftemplate.create",
    TemplateUpdate => "pdftemplate.update",
    TemplateDelete => "pdftemplate.delete",
}

/// GET /settings/pdf-templates (Laravel checked `view` and `list`).
#[utoipa::path(
    get, path = "/workspace/{workspace}/settings/pdf-templates", tag = "pdf_templates",
    operation_id = "pdf_templates_list", summary = "List PDF templates",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ListQuery),
    responses(
        (status = 200, body = Paginated<PdfTemplateResource>, description = "One page of templates"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `pdftemplate.list` / `pdftemplate.view`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Unknown domain or search text too long"),
    ),
    security(("bearer" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    access: Can<TemplateList>,
    _view: Can<TemplateView>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Paginated<Value>>, AppError> {
    let mut r = Rules::default();
    let domain = q
        .domain
        .as_deref()
        .map(str::trim)
        .filter(|d| !d.is_empty())
        .map(str::to_lowercase);
    if let Some(d) = &domain
        && !DOMAINS.contains(&d.as_str())
    {
        r.add("domain", "The selected domain is invalid.");
    }
    r.into_result()?;
    let search = list::search(q.q.as_deref())?;
    let blank = |v: &Option<String>| {
        v.as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    };
    let (relation_type, language, custom_type) = (
        blank(&q.relation_type),
        blank(&q.language),
        blank(&q.relation_custom_type_id),
    );
    let filter = ListFilter {
        domain: domain.as_deref(),
        search: search.as_deref(),
        relation_type: relation_type.as_deref(),
        language: language.as_deref(),
        relation_custom_type_id: custom_type.as_deref(),
    };
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    let (rows, total) = service::list(
        &state,
        access.workspace_id,
        &filter,
        page.size,
        page.offset(),
    )
    .await?;
    Ok(Json(Paginated::new(rows, total, page)))
}

/// GET /settings/pdf-templates/available-fields/{domain}: Laravel's field and variable lists,
/// served as generated (their key order is the SPA's display order).
#[utoipa::path(
    get, path = "/workspace/{workspace}/settings/pdf-templates/available-fields/{domain}", tag = "pdf_templates",
    operation_id = "pdf_templates_available_fields", summary = "Fields and variables a template of a domain may use",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("domain" = String, Path, description = "Template domain, e.g. `invoice`")),
    responses(
        (status = 200, body = Envelope<AvailableFields>, description = "The catalogue; both `[]` for an unknown domain"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `pdftemplate.view`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn available_fields(
    _access: Can<TemplateView>,
    Path((_, domain)): Path<(String, String)>,
) -> Response {
    let body = match fields::raw(&domain) {
        Some(raw) => format!("{{\"data\":{}}}", raw.trim_end()),
        None => r#"{"data":{"fields":[],"variables":[]}}"#.to_owned(),
    };
    ([(header::CONTENT_TYPE, "application/json")], body).into_response()
}

/// GET /settings/pdf-templates/{id}.
#[utoipa::path(
    get, path = "/workspace/{workspace}/settings/pdf-templates/{template}", tag = "pdf_templates",
    operation_id = "pdf_templates_get", summary = "Get a PDF template",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("template" = Uuid, Path, description = "PDF template id")),
    responses(
        (status = 200, body = Envelope<PdfTemplateResource>, description = "The template"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `pdftemplate.view`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or template, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn get(
    State(state): State<AppState>,
    access: Can<TemplateView>,
    Path((_, id)): Path<(String, String)>,
) -> Result<Data<Value>, AppError> {
    Ok(Data(service::get(&state, access.workspace_id, &id).await?))
}

/// POST /settings/pdf-templates -> `{data:{pdf_template_id}}` (200, as Laravel).
#[utoipa::path(
    post, path = "/workspace/{workspace}/settings/pdf-templates", tag = "pdf_templates",
    operation_id = "pdf_templates_create", summary = "Create a PDF template",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = CreateTemplateRequest,
    responses(
        (status = 200, body = Envelope<TemplateCreated>, description = "Created; returns the new id"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `pdftemplate.create`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, unknown column field, file or relation custom type"),
    ),
    security(("bearer" = []))
)]
pub async fn create(
    State(state): State<AppState>,
    access: Can<TemplateCreate>,
    ValidatedJson(input): ValidatedJson<CreateTemplateRequest>,
) -> Result<Data<TemplateCreated>, AppError> {
    let pdf_template_id = service::create(&state, access.workspace_id, input).await?;
    Ok(Data(TemplateCreated { pdf_template_id }))
}

/// POST /settings/pdf-templates/{id}.
#[utoipa::path(
    post, path = "/workspace/{workspace}/settings/pdf-templates/{template}", tag = "pdf_templates",
    operation_id = "pdf_templates_update", summary = "Update a PDF template",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("template" = Uuid, Path, description = "PDF template id")),
    request_body = UpdateTemplateRequest,
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `pdftemplate.update`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or template, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, unknown column field, file or relation custom type"),
    ),
    security(("bearer" = []))
)]
pub async fn update(
    State(state): State<AppState>,
    access: Can<TemplateUpdate>,
    Path((_, id)): Path<(String, String)>,
    ValidatedJson(input): ValidatedJson<UpdateTemplateRequest>,
) -> Result<Message, AppError> {
    service::update(&state, access.workspace_id, &id, input).await?;
    Ok(Message("ok"))
}

/// DELETE /settings/pdf-templates/{id}.
#[utoipa::path(
    delete, path = "/workspace/{workspace}/settings/pdf-templates/{template}", tag = "pdf_templates",
    operation_id = "pdf_templates_delete", summary = "Delete a PDF template",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("template" = Uuid, Path, description = "PDF template id")),
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `pdftemplate.delete`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or template, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn delete(
    State(state): State<AppState>,
    access: Can<TemplateDelete>,
    Path((_, id)): Path<(String, String)>,
) -> Result<Message, AppError> {
    service::delete(&state, access.workspace_id, &id).await?;
    Ok(Message("ok"))
}

/// GET /settings/pdf-templates/{id}/preview -> `{data: FileResource}` of a new PDF.
#[utoipa::path(
    get, path = "/workspace/{workspace}/settings/pdf-templates/{template}/preview", tag = "pdf_templates",
    operation_id = "pdf_templates_preview", summary = "Render a sample PDF of the template",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("template" = Uuid, Path, description = "PDF template id")),
    responses(
        (status = 200, body = Envelope<FileResource>, description = "The rendered PDF, stored as a file"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `pdftemplate.view`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or template, or caller not a member"),
        (status = 422, body = UnprocessableBody, description = "No sample document to render, or the renderer refused the template"),
    ),
    security(("bearer" = []))
)]
pub async fn preview(
    State(state): State<AppState>,
    access: Can<TemplateView>,
    Path((_, id)): Path<(String, String)>,
) -> Result<Data<FileResource>, AppError> {
    Ok(Data(
        service::preview(&state, access.workspace_id, access.user_id, &id).await?,
    ))
}
