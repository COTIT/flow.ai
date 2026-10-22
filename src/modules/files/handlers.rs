//! HTTP edge for /v1/workspace/{workspace}/files (`file.*`).

use axum::{
    Json,
    extract::{Path, Query, Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
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
        dto::{FileCreated, FileResource, UploadRequest},
        service::{self, NewFile},
    },
};

permissions! {
    FileList => "file.list",
    FileGet => "file.get",
    FileCreate => "file.create",
    FileDelete => "file.delete",
}

/// Uploads buffered at once (each up to ~25 MB of body and bytes), whatever the workspace.
static UPLOADS: Semaphore = Semaphore::const_new(4);

/// Route layer: an upload waits up to 10 s for an upload slot before its body is read, else
/// 503; so concurrent uploads can't hold more than `UPLOADS` bodies in memory.
pub async fn upload_slot(req: Request, next: Next) -> Result<Response, AppError> {
    let permit = tokio::time::timeout(std::time::Duration::from_secs(10), UPLOADS.acquire())
        .await
        .map_err(|_| AppError::Overloaded)?
        .map_err(|_| AppError::Overloaded)?;
    let res = next.run(req).await;
    drop(permit);
    Ok(res)
}

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

/// POST /files -> 201 `{data:{file_id}}`.
pub async fn create(
    State(state): State<AppState>,
    access: Can<FileCreate>,
    ValidatedJson(upload): ValidatedJson<UploadRequest>,
) -> Result<(StatusCode, Data<FileCreated>), AppError> {
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
