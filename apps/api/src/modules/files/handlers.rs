//! HTTP edge for /v1/workspace/{workspace}/files (`file.*`).

use axum::{
    Json,
    extract::{FromRequest, Multipart, Path, Query, Request, State},
    http::{StatusCode, header::CONTENT_TYPE},
};
use tokio::sync::Semaphore;

use crate::{
    AppState,
    error::AppError,
    http::{
        json::ValidatedJson,
        list::{Page, Paginated},
        response::{Data, Message},
        workspace::{Can, permissions},
    },
    modules::files::{
        dto::{self, FileCreated, FileResource, UploadRequest},
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
#[derive(serde::Deserialize, Default)]
#[serde(default)]
pub struct ListQuery {
    page: Option<String>,
    page_size: Option<String>,
}

/// GET /files.
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
pub async fn delete(
    State(state): State<AppState>,
    access: Can<FileDelete>,
    Path((_, id)): Path<(String, String)>,
) -> Result<Message, AppError> {
    service::delete(&state, access.workspace_id, &id).await?;
    Ok(Message("ok"))
}
