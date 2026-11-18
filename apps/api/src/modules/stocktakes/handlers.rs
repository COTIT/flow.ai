//! HTTP edge for /v1/workspace/{workspace}/stocktakes. Every route needs a `stocktake.*`
//! permission (checked before the body or any replay is read). Writes take the C/A envelope
//! plus a required Idempotency-Key and replay permanently by `client_ref`.

use axum::{
    Json,
    extract::{Path, Query, State},
};
use uuid::Uuid;

use crate::{
    AppState,
    error::{AppError, ValidationErrorBody},
    http::{
        json::ValidatedJson,
        list::Paginated,
        response::{Data, Envelope, MessageBody},
        workspace::{Can, permissions},
    },
    idempotency::{Answer, Command},
    modules::{
        accounting::dto::{Action, Create},
        stocktakes::{
            dto::{
                CountPayload, CreatePayload, PageQuery, PostReview, PostReviewBody, StartPayload,
                StocktakeCancelPayload, StocktakePostPayload, StocktakeResource,
            },
            service,
        },
    },
};

permissions! {
    List => "stocktake.list",
    Get => "stocktake.get",
    CreateSt => "stocktake.create",
    Start => "stocktake.start",
    Count => "stocktake.count",
    Post => "stocktake.post",
    Cancel => "stocktake.cancel",
}

/// A path id; malformed reads as missing.
fn id(raw: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(raw).map_err(|_| AppError::NotFound)
}

/// stocktakes_list: List stocktakes.
#[utoipa::path(
    get, path = "/workspace/{workspace}/stocktakes", tag = "stocktakes",
    operation_id = "stocktakes_list",
    summary = "List stocktakes",
    params(("workspace" = Uuid, Path, description = "Workspace id"), PageQuery),
    responses(
        (status = 200, body = Paginated<StocktakeResource>, description = "Success, newest first; no lines"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    access: Can<List>,
    Query(q): Query<PageQuery>,
) -> Result<Json<Paginated<StocktakeResource>>, AppError> {
    Ok(Json(service::list(&state, access.workspace_id, &q).await?))
}

/// stocktakes_create: Create a draft stocktake for a warehouse.
#[utoipa::path(
    post, path = "/workspace/{workspace}/stocktakes", tag = "stocktakes",
    operation_id = "stocktakes_create",
    summary = "Create a draft stocktake for a warehouse",
    request_body = Create<CreatePayload>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 201, body = Envelope<StocktakeResource>, description = "Created"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or not a member"),
        (status = 409, body = MessageBody, description = "Reused client_ref with another request"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input or warehouse"),
    ),
    security(("bearer" = []))
)]
pub async fn create(
    State(state): State<AppState>,
    access: Can<CreateSt>,
    cmd: Command<Create<CreatePayload>>,
) -> Result<Answer, AppError> {
    service::create(&state, &access, cmd).await
}

/// stocktakes_get: Read a stocktake with its lines.
#[utoipa::path(
    get, path = "/workspace/{workspace}/stocktakes/{stocktake}", tag = "stocktakes",
    operation_id = "stocktakes_get",
    summary = "Read a stocktake with its lines",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("stocktake" = Uuid, Path, description = "Stocktake id")),
    responses(
        (status = 200, body = Envelope<StocktakeResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or record, or not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn get(
    State(state): State<AppState>,
    access: Can<Get>,
    Path((_, raw)): Path<(String, String)>,
) -> Result<Data<StocktakeResource>, AppError> {
    let mut conn = state.db.acquire().await?;
    Ok(Data(
        service::get(&mut conn, access.workspace_id, id(&raw)?).await?,
    ))
}

/// stocktakes_start: Pause the warehouse and freeze its stock.
#[utoipa::path(
    post, path = "/workspace/{workspace}/stocktakes/{stocktake}/start", tag = "stocktakes",
    operation_id = "stocktakes_start",
    summary = "Pause the warehouse and freeze its stock",
    request_body = Action<StartPayload>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("stocktake" = Uuid, Path, description = "Stocktake id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 200, body = Envelope<StocktakeResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or record, or not a member"),
        (status = 409, body = MessageBody, description = "`stale_document`, `warehouse_paused` (another count runs) or reused client_ref"),
        (status = 422, body = ValidationErrorBody, description = "Not a draft, or `receipt_booking_required`"),
    ),
    security(("bearer" = []))
)]
pub async fn start(
    State(state): State<AppState>,
    access: Can<Start>,
    Path((_, raw)): Path<(String, String)>,
    cmd: Command<Action<StartPayload>>,
) -> Result<Answer, AppError> {
    service::start(&state, &access, id(&raw)?, cmd).await
}

/// stocktakes_count: Store counted quantities.
#[utoipa::path(
    put, path = "/workspace/{workspace}/stocktakes/{stocktake}/counts", tag = "stocktakes",
    operation_id = "stocktakes_count",
    summary = "Store counted quantities",
    request_body = Action<CountPayload>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("stocktake" = Uuid, Path, description = "Stocktake id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 200, body = Envelope<StocktakeResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or record, or not a member"),
        (status = 409, body = MessageBody, description = "`stale_document` or reused client_ref"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, unknown line, or not counting"),
    ),
    security(("bearer" = []))
)]
pub async fn count(
    State(state): State<AppState>,
    access: Can<Count>,
    Path((_, raw)): Path<(String, String)>,
    cmd: Command<Action<CountPayload>>,
) -> Result<Answer, AppError> {
    service::count(&state, &access, id(&raw)?, cmd).await
}

/// stocktakes_post_review: Preview posting the count (no writes).
#[utoipa::path(
    post, path = "/workspace/{workspace}/stocktakes/{stocktake}/post/review", tag = "stocktakes",
    operation_id = "stocktakes_post_review",
    summary = "Preview posting the count (no writes)",
    request_body = PostReviewBody,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("stocktake" = Uuid, Path, description = "Stocktake id")),
    responses(
        (status = 200, body = Envelope<PostReview>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or record, or not a member"),
        (status = 422, body = ValidationErrorBody, description = "Not counting, count incomplete or below holds, or `stocktake_value_required` / `stocktake_value_not_allowed` / `backdated_valuation` / `accounting_not_configured` / `accounting_period_closed`"),
    ),
    security(("bearer" = []))
)]
pub async fn post_review(
    State(state): State<AppState>,
    access: Can<Post>,
    Path((_, raw)): Path<(String, String)>,
    ValidatedJson(date): ValidatedJson<PostReviewBody>,
) -> Result<Data<PostReview>, AppError> {
    Ok(Data(
        service::post_review(&state, &access, id(&raw)?, date).await?,
    ))
}

/// stocktakes_post: Book the reviewed count and end the pause.
#[utoipa::path(
    post, path = "/workspace/{workspace}/stocktakes/{stocktake}/post", tag = "stocktakes",
    operation_id = "stocktakes_post",
    summary = "Book the reviewed count and end the pause",
    request_body = Action<StocktakePostPayload>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("stocktake" = Uuid, Path, description = "Stocktake id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 200, body = Envelope<StocktakeResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or record, or not a member"),
        (status = 409, body = MessageBody, description = "`stale_document` (version or review) or reused client_ref"),
        (status = 422, body = ValidationErrorBody, description = "As the review"),
    ),
    security(("bearer" = []))
)]
pub async fn post(
    State(state): State<AppState>,
    access: Can<Post>,
    Path((_, raw)): Path<(String, String)>,
    cmd: Command<Action<StocktakePostPayload>>,
) -> Result<Answer, AppError> {
    service::post(&state, &access, id(&raw)?, cmd).await
}

/// stocktakes_cancel: Cancel; keep the count and end the pause.
#[utoipa::path(
    post, path = "/workspace/{workspace}/stocktakes/{stocktake}/cancel", tag = "stocktakes",
    operation_id = "stocktakes_cancel",
    summary = "Cancel; keep the count and end the pause",
    request_body = Action<StocktakeCancelPayload>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("stocktake" = Uuid, Path, description = "Stocktake id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 200, body = Envelope<StocktakeResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or record, or not a member"),
        (status = 409, body = MessageBody, description = "`stale_document` or reused client_ref"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, or already posted or cancelled"),
    ),
    security(("bearer" = []))
)]
pub async fn cancel(
    State(state): State<AppState>,
    access: Can<Cancel>,
    Path((_, raw)): Path<(String, String)>,
    cmd: Command<Action<StocktakeCancelPayload>>,
) -> Result<Answer, AppError> {
    service::cancel(&state, &access, id(&raw)?, cmd).await
}
