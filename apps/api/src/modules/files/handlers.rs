//! HTTP edge for /v1/workspace/{workspace}/files (`file.*`).

use axum::{
    Json,
    extract::{FromRequest, Multipart, Path, Query, Request, State},
    http::{StatusCode, header::CONTENT_TYPE},
};
use tokio::sync::Semaphore;

use crate::{
    AppState,
    error::{AppError, UnprocessableBody},
    http::{
        json::ValidatedJson,
        list::{Page, Paginated},
        response::{Data, Envelope, Message, MessageBody},
        workspace::{Can, permissions},
    },
    modules::files::{
        dto::{self, FileCreated, FileResource, UploadForm, UploadRequest},
        service::{self, NewFile},
    },
};

permissions! {
    FileList => "file.list",
    FileGet => "file.get",
    FileCreate => "file.create",
    FileDelete => "file.delete",
}

/// Uploads buffered at once server-wide (each holds at most ~30 MB at its peak).
static UPLOADS: Semaphore = Semaphore::const_new(4);

/// Paging query.
#[derive(serde::Deserialize, Default, utoipa::IntoParams)]
#[serde(default)]
#[into_params(parameter_in = Query)]
pub struct ListQuery {
    /// 1-based; junk falls back to 1.
    #[param(value_type = Option<i64>)]
    page: Option<String>,
    /// Default 50, max 100.
    #[param(value_type = Option<i64>)]
    page_size: Option<String>,
}

/// GET /files.
#[utoipa::path(
    get, path = "/workspace/{workspace}/files", tag = "files",
    operation_id = "files_list", summary = "List files",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ListQuery),
    responses(
        (status = 200, body = Paginated<FileResource>, description = "One page of files"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `file.list`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    access: Can<FileList>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Paginated<FileResource>>, AppError> {
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    let (rows, total) =
        service::list(&state, access.workspace_id, page.size, page.offset()).await?;
    Ok(Json(Paginated::new(rows, total, page)))
}

/// GET /files/{id}.
#[utoipa::path(
    get, path = "/workspace/{workspace}/files/{file}", tag = "files",
    operation_id = "files_get", summary = "Get a file and a fresh download URL",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("file" = Uuid, Path, description = "File id")),
    responses(
        (status = 200, body = Envelope<FileResource>, description = "The file"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `file.get`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or file, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn get(
    State(state): State<AppState>,
    access: Can<FileGet>,
    Path((_, id)): Path<(String, String)>,
) -> Result<Data<FileResource>, AppError> {
    Ok(Data(service::get(&state, access.workspace_id, &id).await?))
}

/// POST /files -> 201 `{data:{file_id}}`, from a multipart form (vimdesk-web) or base64 JSON
/// (Laravel's other form). The permission check runs first, then the workspace's heavy slot
/// and a server-wide upload slot (10 s wait each, else 503); only then is the body read.
#[utoipa::path(
    post, path = "/workspace/{workspace}/files", tag = "files",
    operation_id = "files_create", summary = "Upload a file (multipart form or base64 JSON)",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body(content((UploadForm = "multipart/form-data"), (UploadRequest = "application/json"))),
    responses(
        (status = 201, body = Envelope<FileCreated>, description = "Stored; returns the new id"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `file.create`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = UnprocessableBody, description = "Invalid form or fields, file type not allowed, or file over 10 MB"),
    ),
    security(("bearer" = []))
)]
pub async fn create(
    State(state): State<AppState>,
    access: Can<FileCreate>,
    req: Request,
) -> Result<(StatusCode, Data<FileCreated>), AppError> {
    let _workspace = state.slots.heavy(access.workspace_id).await?;
    let _upload = tokio::time::timeout(std::time::Duration::from_secs(10), UPLOADS.acquire())
        .await
        .map_err(|_| AppError::Overloaded)?
        .map_err(|_| AppError::Overloaded)?;
    let multipart = req
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("multipart/form-data"));
    let upload = if multipart {
        let form = Multipart::from_request(req, &state)
            .await
            .map_err(|e| AppError::field("upload", e.body_text()))?;
        dto::from_multipart(form).await?
    } else {
        ValidatedJson::<UploadRequest>::from_request(req, &state)
            .await?
            .0
    };
    let file_id = service::create(
        &state,
        access.workspace_id,
        access.user_id,
        NewFile {
            document_name: &upload.document_name,
            description: upload.description.as_deref(),
            meta: upload.meta.as_ref(),
            bytes: upload.bytes,
        },
    )
    .await?;
    Ok((StatusCode::CREATED, Data(FileCreated { file_id })))
}

/// DELETE /files/{id}.
#[utoipa::path(
    delete, path = "/workspace/{workspace}/files/{file}", tag = "files",
    operation_id = "files_delete", summary = "Delete a file",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("file" = Uuid, Path, description = "File id")),
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `file.delete`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or file, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn delete(
    State(state): State<AppState>,
    access: Can<FileDelete>,
    Path((_, id)): Path<(String, String)>,
) -> Result<Message, AppError> {
    service::delete(&state, access.workspace_id, &id).await?;
    Ok(Message("ok"))
}
