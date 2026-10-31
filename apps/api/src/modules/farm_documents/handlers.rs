//! HTTP edge for /v1/workspace/{workspace}/farm/plots/{plot}/documents.

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
        farm_crops::dto::ListQuery,
        farm_documents::{
            dto::{PlotDocumentDownload, PlotDocumentForm, PlotDocumentResource, from_multipart},
            service,
        },
    },
};

/// GET /farm/plots/{plot}/documents.
#[utoipa::path(
    get, path = "/workspace/{workspace}/farm/plots/{plot}/documents", tag = "farm_documents",
    operation_id = "farm_documents_list", summary = "List a plot's title documents",
    description = "Field staff of the plot's village. Farmers see no documents, their own plot's included.",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("plot" = Uuid, Path, description = "Plot id"),
        ListQuery,
    ),
    responses(
        (status = 200, body = Paginated<PlotDocumentResource>, description = "One page, oldest first"),
        (status = 404, body = MessageBody, description = "Unknown plot, or caller not staff of its village"),
    ),
    security(("bearer" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    access: FarmAccess,
    Path((_, plot)): Path<(String, String)>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Paginated<PlotDocumentResource>>, AppError> {
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    let (rows, total) = service::list(&state, &access, &plot, page.size, page.offset()).await?;
    Ok(Json(Paginated::new(
        rows.into_iter().map(Into::into).collect(),
        total,
        page,
    )))
}

/// POST /farm/plots/{plot}/documents (multipart) -> 201, or 200 for a retried id.
#[utoipa::path(
    post, path = "/workspace/{workspace}/farm/plots/{plot}/documents", tag = "farm_documents",
    operation_id = "farm_documents_create", summary = "File a plot title document",
    description = "Field staff of the plot's village. Multipart `file` (JPEG, PNG, WebP, HEIC or \
        PDF, at most 10 MB; the type is read from the bytes) and an optional `id`: a retry with \
        the same id returns the document already filed and stores nothing.",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("plot" = Uuid, Path, description = "Plot id"),
    ),
    request_body(content = PlotDocumentForm, content_type = "multipart/form-data"),
    responses(
        (status = 200, body = Envelope<PlotDocumentResource>, description = "Already filed under this id"),
        (status = 201, body = Envelope<PlotDocumentResource>, description = "Filed"),
        (status = 404, body = MessageBody, description = "Unknown plot, or caller not staff of its village"),
        (status = 409, body = MessageBody, description = "The id is used by another plot's document"),
        (status = 422, body = UnprocessableBody, description = "Not a multipart form, unknown field, file type not allowed, or over 10 MB"),
    ),
    security(("bearer" = []))
)]
pub async fn create(
    State(state): State<AppState>,
    access: FarmAccess,
    Path((_, plot)): Path<(String, String)>,
    req: Request,
) -> Result<(StatusCode, Data<PlotDocumentResource>), AppError> {
    let multipart = req
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("multipart/form-data"));
    if !multipart {
        return Err(AppError::field(
            "file",
            "Send the document as a multipart form.",
        ));
    }
    // The scope check runs first, so nobody outside the village can make us read 10 MB.
    service::list(&state, &access, &plot, 1, 0).await?;
    let form = Multipart::from_request(req, &state)
        .await
        .map_err(|e| AppError::field("file", e.body_text()))?;
    let upload = from_multipart(form).await?;
    let (doc, created) = service::create(&state, &access, &plot, upload).await?;
    let status = if created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((status, Data(doc.into())))
}

/// GET /farm/plots/{plot}/documents/{document}: the document and a 10-minute download link.
#[utoipa::path(
    get, path = "/workspace/{workspace}/farm/plots/{plot}/documents/{document}", tag = "farm_documents",
    operation_id = "farm_documents_get", summary = "Get a title document and a download link",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("plot" = Uuid, Path, description = "Plot id"),
        ("document" = Uuid, Path, description = "Document id"),
    ),
    responses(
        (status = 200, body = Envelope<PlotDocumentDownload>, description = "The document; the link expires at `download_expires_at`"),
        (status = 404, body = MessageBody, description = "Unknown plot or document, or caller not staff of its village"),
    ),
    security(("bearer" = []))
)]
pub async fn get(
    State(state): State<AppState>,
    access: FarmAccess,
    Path((_, plot, id)): Path<(String, String, String)>,
) -> Result<Data<PlotDocumentDownload>, AppError> {
    Ok(Data(service::get(&state, &access, &plot, &id).await?))
}
