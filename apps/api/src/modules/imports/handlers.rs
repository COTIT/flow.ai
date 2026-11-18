//! Import handlers: extract, call the service, wrap the answer.
use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde_json::Value;
use uuid::Uuid;

use super::{
    dto::{ApplyPayload, ChunkPayload, CreatePayload, ListQuery, ValidatePayload},
    service,
};
use crate::{
    AppState,
    error::{AppError, ValidationErrorBody},
    http::{
        list::Paginated,
        response::{Data, Envelope, MessageBody},
        workspace::{Can, permissions},
    },
    idempotency::{Answer, Command},
    modules::accounting::dto::{Action, Create},
};

permissions! {
    ImportList => "import.list",
    ImportGet => "import.get",
    ImportCreate => "import.create",
    ImportUpload => "import.upload",
    ImportValidate => "import.validate",
    ImportApply => "import.apply",
}

type BatchPath = Path<(Uuid, Uuid)>;

/// imports_list: List import batches, newest first.
#[utoipa::path(
    get, path = "/workspace/{workspace}/imports", tag = "imports", operation_id = "imports_list",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ListQuery),
    responses(
        (status = 200, body = Paginated<Value>, description = "Batches"),
        (status = 403, body = MessageBody, description = "Missing `import.list`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    access: Can<ImportList>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Paginated<Value>>, AppError> {
    Ok(Json(
        service::list(&state.db, access.workspace_id, &q).await?,
    ))
}

/// imports_create: Start an import batch (products or relations).
#[utoipa::path(
    post, path = "/workspace/{workspace}/imports", tag = "imports", operation_id = "imports_create",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    request_body = Create<CreatePayload>,
    responses(
        (status = 201, body = Envelope<Value>, description = "Batch created (or replayed by client_ref)"),
        (status = 403, body = MessageBody, description = "Missing `import.create`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or not a member"),
        (status = 409, body = MessageBody, description = "client_ref reused with another body"),
        (status = 422, body = ValidationErrorBody, description = "Invalid profile, limits or mapping"),
    ),
    security(("bearer" = []))
)]
pub async fn create(
    State(state): State<AppState>,
    access: Can<ImportCreate>,
    cmd: Command<Create<CreatePayload>>,
) -> Result<Answer, AppError> {
    service::create(&state, &access, cmd).await
}

/// imports_get: One batch with row counts and allowed actions.
#[utoipa::path(
    get, path = "/workspace/{workspace}/imports/{import}", tag = "imports", operation_id = "imports_get",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("import" = Uuid, Path, description = "Batch id")),
    responses(
        (status = 200, body = Envelope<Value>, description = "Batch"),
        (status = 403, body = MessageBody, description = "Missing `import.get`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or batch"),
    ),
    security(("bearer" = []))
)]
pub async fn get(
    State(state): State<AppState>,
    access: Can<ImportGet>,
    Path((_, id)): BatchPath,
) -> Result<Data<Value>, AppError> {
    let mut conn = state.db.acquire().await?;
    Ok(Data(
        service::batch(&mut conn, access.workspace_id, id).await?,
    ))
}

/// imports_upload: Upload one chunk (≤ 20 rows, ≤ 60 KiB); same rows replay, other rows 409.
#[utoipa::path(
    put, path = "/workspace/{workspace}/imports/{import}/chunks/{index}", tag = "imports", operation_id = "imports_upload",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("import" = Uuid, Path, description = "Batch id"), ("index" = i64, Path, description = "Chunk index 0..24"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    request_body = Action<ChunkPayload>,
    responses(
        (status = 200, body = Envelope<Value>, description = "Batch after the chunk"),
        (status = 403, body = MessageBody, description = "Missing `import.upload`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or batch"),
        (status = 409, body = MessageBody, description = "`stale_document` or the index holds other rows"),
        (status = 413, body = MessageBody, description = "Chunk above 60 KiB"),
        (status = 422, body = ValidationErrorBody, description = "Rows, columns or cells outside the limits"),
    ),
    security(("bearer" = []))
)]
pub async fn upload(
    State(state): State<AppState>,
    access: Can<ImportUpload>,
    Path((_, id, index)): Path<(Uuid, Uuid, i64)>,
    cmd: Command<Action<ChunkPayload>>,
) -> Result<Answer, AppError> {
    service::upload(&state, &access, id, index, cmd).await
}

/// imports_validate: Validate the complete revision; returns the validation fingerprint.
#[utoipa::path(
    post, path = "/workspace/{workspace}/imports/{import}/validate", tag = "imports", operation_id = "imports_validate",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("import" = Uuid, Path, description = "Batch id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    request_body = Action<ValidatePayload>,
    responses(
        (status = 200, body = Envelope<Value>, description = "Validated batch"),
        (status = 403, body = MessageBody, description = "Missing `import.validate`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or batch"),
        (status = 409, body = MessageBody, description = "`stale_document`"),
        (status = 422, body = ValidationErrorBody, description = "Incomplete revision"),
    ),
    security(("bearer" = []))
)]
pub async fn validate(
    State(state): State<AppState>,
    access: Can<ImportValidate>,
    Path((_, id)): BatchPath,
    cmd: Command<Action<ValidatePayload>>,
) -> Result<Answer, AppError> {
    service::validate(&state, &access, id, cmd).await
}

/// imports_apply: Queue the valid rows for native writes (202); invalid rows never apply.
#[utoipa::path(
    post, path = "/workspace/{workspace}/imports/{import}/apply", tag = "imports", operation_id = "imports_apply",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("import" = Uuid, Path, description = "Batch id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    request_body = Action<ApplyPayload>,
    responses(
        (status = 202, body = Envelope<Value>, description = "Apply queued"),
        (status = 403, body = MessageBody, description = "Missing `import.apply` or the native create/update permission"),
        (status = 404, body = MessageBody, description = "Unknown workspace or batch"),
        (status = 409, body = MessageBody, description = "`stale_document`: version or fingerprint changed"),
        (status = 422, body = ValidationErrorBody, description = "No valid rows"),
    ),
    security(("bearer" = []))
)]
pub async fn apply(
    State(state): State<AppState>,
    access: Can<ImportApply>,
    Path((_, id)): BatchPath,
    cmd: Command<Action<ApplyPayload>>,
) -> Result<Answer, AppError> {
    service::apply(&state, &access, id, cmd).await
}

/// imports_rows: Rows of the current revision (review before apply, results after).
#[utoipa::path(
    get, path = "/workspace/{workspace}/imports/{import}/rows", tag = "imports", operation_id = "imports_rows",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("import" = Uuid, Path, description = "Batch id"), ListQuery),
    responses(
        (status = 200, body = Paginated<Value>, description = "Rows"),
        (status = 403, body = MessageBody, description = "Missing `import.get`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or batch"),
        (status = 422, body = ValidationErrorBody, description = "Unknown status filter"),
    ),
    security(("bearer" = []))
)]
pub async fn rows(
    State(state): State<AppState>,
    access: Can<ImportGet>,
    Path((_, id)): BatchPath,
    Query(q): Query<ListQuery>,
) -> Result<Json<Paginated<Value>>, AppError> {
    Ok(Json(
        service::rows(&state.db, access.workspace_id, id, &q).await?,
    ))
}
