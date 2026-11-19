//! HTTP edge for /v1/workspace/{workspace}/sales-returns and /legacy-return-costs. Every route
//! needs a `salesreturn.*` permission (legacy return costs: `accounting.post`), checked before
//! the body or any replay is read. Writes take the C/A envelope plus a required
//! Idempotency-Key and replay permanently by `client_ref`.

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
        purchasing::dto::{CancelPayload, PageQuery},
        sales_returns::{
            dto::{
                ActionReview, FingerprintPayload, LegacyCostPayload, LegacyCostResource,
                PostReview, RestockPayload, RestockReversePayload, RestockReviewBody,
                ReturnCreatePayload, ReturnUpdatePayload, ReversePayload, ReverseReviewBody,
                SalesReturnResource,
            },
            service,
        },
    },
};

permissions! {
    List => "salesreturn.list",
    Get => "salesreturn.get",
    CreateReturn => "salesreturn.create",
    Update => "salesreturn.update",
    Cancel => "salesreturn.cancel",
    Post => "salesreturn.post",
    Reverse => "salesreturn.reverse",
    AccountingPost => "accounting.post",
}

/// A path id; malformed reads as missing.
fn id(raw: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(raw).map_err(|_| AppError::NotFound)
}

/// sales_returns_list: List customer returns.
#[utoipa::path(
    get, path = "/workspace/{workspace}/sales-returns", tag = "sales_returns",
    operation_id = "sales_returns_list",
    summary = "List customer returns",
    params(("workspace" = Uuid, Path, description = "Workspace id"), PageQuery),
    responses(
        (status = 200, body = Paginated<SalesReturnResource>, description = "Success, newest first"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn sales_returns_list(
    State(state): State<AppState>,
    access: Can<List>,
    Query(q): Query<PageQuery>,
) -> Result<Json<Paginated<SalesReturnResource>>, AppError> {
    Ok(Json(service::list(&state, &access, &q).await?))
}

/// sales_returns_create: Draft a credit of a recognised sale (reserves capacity only).
#[utoipa::path(
    post, path = "/workspace/{workspace}/sales-returns", tag = "sales_returns",
    operation_id = "sales_returns_create",
    summary = "Draft a customer credit (reserves capacity only)",
    request_body = Create<ReturnCreatePayload>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 201, body = Envelope<SalesReturnResource>, description = "Created"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or sale, or not a member"),
        (status = 409, body = MessageBody, description = "Reused client_ref"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, more than is left to credit, or `unsupported_active_erp_operation` (setup book)"),
    ),
    security(("bearer" = []))
)]
pub async fn sales_returns_create(
    State(state): State<AppState>,
    access: Can<CreateReturn>,
    cmd: Command<Create<ReturnCreatePayload>>,
) -> Result<Answer, AppError> {
    service::create(&state, &access, cmd).await
}

/// sales_returns_get: Read a customer return.
#[utoipa::path(
    get, path = "/workspace/{workspace}/sales-returns/{return}", tag = "sales_returns",
    operation_id = "sales_returns_get",
    summary = "Read a customer return",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("return" = Uuid, Path, description = "Return id")),
    responses(
        (status = 200, body = Envelope<SalesReturnResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or return, or not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn sales_returns_get(
    State(state): State<AppState>,
    access: Can<Get>,
    Path((_, ret)): Path<(String, String)>,
) -> Result<Data<SalesReturnResource>, AppError> {
    let mut conn = state.db.acquire().await?;
    let mut r = service::get(&mut conn, access.workspace_id, id(&ret)?).await?;
    service::hide_costs(&mut r, &access);
    Ok(Data(r))
}

/// sales_returns_update: Replace a draft's date, reason and lines.
#[utoipa::path(
    patch, path = "/workspace/{workspace}/sales-returns/{return}", tag = "sales_returns",
    operation_id = "sales_returns_update",
    summary = "Replace a draft's date, reason and lines",
    request_body = Action<ReturnUpdatePayload>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("return" = Uuid, Path, description = "Return id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 200, body = Envelope<SalesReturnResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or return, or not a member"),
        (status = 409, body = MessageBody, description = "`stale_document` or reused client_ref"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, more than is left to credit, or not a draft"),
    ),
    security(("bearer" = []))
)]
pub async fn sales_returns_update(
    State(state): State<AppState>,
    access: Can<Update>,
    Path((_, ret)): Path<(String, String)>,
    cmd: Command<Action<ReturnUpdatePayload>>,
) -> Result<Answer, AppError> {
    service::update(&state, &access, id(&ret)?, cmd).await
}

/// sales_returns_cancel: Cancel a draft, freeing its capacity.
#[utoipa::path(
    post, path = "/workspace/{workspace}/sales-returns/{return}/cancel", tag = "sales_returns",
    operation_id = "sales_returns_cancel",
    summary = "Cancel a draft",
    request_body = Action<CancelPayload>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("return" = Uuid, Path, description = "Return id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 200, body = Envelope<SalesReturnResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or return, or not a member"),
        (status = 409, body = MessageBody, description = "`stale_document` or reused client_ref"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, or not a draft"),
    ),
    security(("bearer" = []))
)]
pub async fn sales_returns_cancel(
    State(state): State<AppState>,
    access: Can<Cancel>,
    Path((_, ret)): Path<(String, String)>,
    cmd: Command<Action<CancelPayload>>,
) -> Result<Answer, AppError> {
    service::cancel(&state, &access, id(&ret)?, cmd).await
}

/// sales_returns_post_review: Preview posting a draft credit (no writes).
#[utoipa::path(
    post, path = "/workspace/{workspace}/sales-returns/{return}/post/review", tag = "sales_returns",
    operation_id = "sales_returns_post_review",
    summary = "Preview posting a draft credit (no writes)",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("return" = Uuid, Path, description = "Return id")),
    responses(
        (status = 200, body = Envelope<PostReview>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or return, or not a member"),
        (status = 422, body = ValidationErrorBody, description = "Not a draft, accounting date not the book day, or `unsupported_active_erp_operation` (setup book)"),
    ),
    security(("bearer" = []))
)]
pub async fn sales_returns_post_review(
    State(state): State<AppState>,
    access: Can<Post>,
    Path((_, ret)): Path<(String, String)>,
) -> Result<Data<PostReview>, AppError> {
    Ok(Data(
        service::post_review(&state, access.workspace_id, id(&ret)?).await?,
    ))
}

/// sales_returns_post: Post a reviewed credit: journal, customer credit item, application.
#[utoipa::path(
    post, path = "/workspace/{workspace}/sales-returns/{return}/post", tag = "sales_returns",
    operation_id = "sales_returns_post",
    summary = "Post a reviewed draft credit",
    request_body = Action<FingerprintPayload>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("return" = Uuid, Path, description = "Return id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 200, body = Envelope<SalesReturnResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or return, or not a member"),
        (status = 409, body = MessageBody, description = "`stale_document` (version or review changed) or reused client_ref"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, not a draft, or `accounting_not_configured` / `accounting_period_closed` / `unsupported_active_erp_operation`"),
    ),
    security(("bearer" = []))
)]
pub async fn sales_returns_post(
    State(state): State<AppState>,
    access: Can<Post>,
    Path((_, ret)): Path<(String, String)>,
    cmd: Command<Action<FingerprintPayload>>,
) -> Result<Answer, AppError> {
    service::post(&state, &access, id(&ret)?, cmd).await
}

/// sales_returns_restock_review: Preview putting credited goods back (no writes).
#[utoipa::path(
    post, path = "/workspace/{workspace}/sales-returns/{return}/restock/review", tag = "sales_returns",
    operation_id = "sales_returns_restock_review",
    summary = "Preview a restock (no writes)",
    request_body = RestockReviewBody,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("return" = Uuid, Path, description = "Return id")),
    responses(
        (status = 200, body = Envelope<ActionReview>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or return, or not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, not posted, another warehouse than the sale's, or more than is left to put back"),
    ),
    security(("bearer" = []))
)]
pub async fn sales_returns_restock_review(
    State(state): State<AppState>,
    access: Can<Post>,
    Path((_, ret)): Path<(String, String)>,
    ValidatedJson(lines): ValidatedJson<RestockReviewBody>,
) -> Result<Data<ActionReview>, AppError> {
    Ok(Data(
        service::restock_review(&state, access.workspace_id, id(&ret)?, &lines).await?,
    ))
}

/// sales_returns_restock: Put credited goods back on stock at the original booked cost.
#[utoipa::path(
    post, path = "/workspace/{workspace}/sales-returns/{return}/restock", tag = "sales_returns",
    operation_id = "sales_returns_restock",
    summary = "Put credited goods back on stock (separate effect)",
    request_body = Action<RestockPayload>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("return" = Uuid, Path, description = "Return id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 200, body = Envelope<SalesReturnResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or return, or not a member"),
        (status = 409, body = MessageBody, description = "`stale_document` (version or review changed) or reused client_ref"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, not posted, or `legacy_return_cost_required` / `receipt_booking_required` / `backdated_valuation`"),
    ),
    security(("bearer" = []))
)]
pub async fn sales_returns_restock(
    State(state): State<AppState>,
    access: Can<Post>,
    Path((_, ret)): Path<(String, String)>,
    cmd: Command<Action<RestockPayload>>,
) -> Result<Answer, AppError> {
    service::restock(&state, &access, id(&ret)?, cmd).await
}

/// sales_returns_reverse_review: Preview reversing a posted credit (no writes).
#[utoipa::path(
    post, path = "/workspace/{workspace}/sales-returns/{return}/reverse/review", tag = "sales_returns",
    operation_id = "sales_returns_reverse_review",
    summary = "Preview reversing a credit (no writes)",
    request_body = ReverseReviewBody,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("return" = Uuid, Path, description = "Return id")),
    responses(
        (status = 200, body = Envelope<ActionReview>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or return, or not a member"),
        (status = 422, body = ValidationErrorBody, description = "Not posted, date not the book day, or `dependent_effects_exist` (refunded or applied elsewhere)"),
    ),
    security(("bearer" = []))
)]
pub async fn sales_returns_reverse_review(
    State(state): State<AppState>,
    access: Can<Reverse>,
    Path((_, ret)): Path<(String, String)>,
    ValidatedJson(date): ValidatedJson<ReverseReviewBody>,
) -> Result<Data<ActionReview>, AppError> {
    Ok(Data(
        service::reverse_review(&state, access.workspace_id, id(&ret)?, date).await?,
    ))
}

/// sales_returns_reverse: Reverse a posted credit (financial only; restocks stay).
#[utoipa::path(
    post, path = "/workspace/{workspace}/sales-returns/{return}/reverse", tag = "sales_returns",
    operation_id = "sales_returns_reverse",
    summary = "Reverse a posted credit (financial only)",
    request_body = Action<ReversePayload>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("return" = Uuid, Path, description = "Return id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 200, body = Envelope<SalesReturnResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or return, or not a member"),
        (status = 409, body = MessageBody, description = "`stale_document` or reused client_ref"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, not posted, or `dependent_effects_exist`"),
    ),
    security(("bearer" = []))
)]
pub async fn sales_returns_reverse(
    State(state): State<AppState>,
    access: Can<Reverse>,
    Path((_, ret)): Path<(String, String)>,
    cmd: Command<Action<ReversePayload>>,
) -> Result<Answer, AppError> {
    service::reverse(&state, &access, id(&ret)?, cmd).await
}

/// sales_return_restocks_reverse_review: Preview undoing a restock (no writes).
#[utoipa::path(
    post, path = "/workspace/{workspace}/sales-returns/{return}/restocks/{restock}/reverse/review", tag = "sales_return_restocks",
    operation_id = "sales_return_restocks_reverse_review",
    summary = "Preview undoing a restock (no writes)",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("return" = Uuid, Path, description = "Return id"), ("restock" = Uuid, Path, description = "Restock id")),
    responses(
        (status = 200, body = Envelope<ActionReview>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace, return or restock, or not a member"),
        (status = 422, body = ValidationErrorBody, description = "Already reversed, or `dependent_effects_exist` (stock valued since)"),
    ),
    security(("bearer" = []))
)]
pub async fn sales_return_restocks_reverse_review(
    State(state): State<AppState>,
    access: Can<Reverse>,
    Path((_, ret, restock)): Path<(String, String, String)>,
) -> Result<Data<ActionReview>, AppError> {
    Ok(Data(
        service::restock_reverse_review(&state, &access, (id(&ret)?, id(&restock)?)).await?,
    ))
}

/// sales_return_restocks_reverse: Undo a restock exactly (physical only; the credit stays).
#[utoipa::path(
    post, path = "/workspace/{workspace}/sales-returns/{return}/restocks/{restock}/reverse", tag = "sales_return_restocks",
    operation_id = "sales_return_restocks_reverse",
    summary = "Undo a restock (physical only)",
    request_body = Action<RestockReversePayload>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("return" = Uuid, Path, description = "Return id"), ("restock" = Uuid, Path, description = "Restock id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 200, body = Envelope<SalesReturnResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace, return or restock, or not a member"),
        (status = 409, body = MessageBody, description = "`stale_document` or reused client_ref"),
        (status = 422, body = ValidationErrorBody, description = "Already reversed, `dependent_effects_exist`, or `insufficient_stock` (goods no longer available)"),
    ),
    security(("bearer" = []))
)]
pub async fn sales_return_restocks_reverse(
    State(state): State<AppState>,
    access: Can<Reverse>,
    Path((_, ret, restock)): Path<(String, String, String)>,
    cmd: Command<Action<RestockReversePayload>>,
) -> Result<Answer, AppError> {
    service::restock_reverse(&state, &access, (id(&ret)?, id(&restock)?), cmd).await
}

/// legacy_return_costs_create: Record a reviewed cost for returns of a sale with no booked cost.
#[utoipa::path(
    post, path = "/workspace/{workspace}/legacy-return-costs", tag = "legacy_return_costs",
    operation_id = "legacy_return_costs_create",
    summary = "Record a reviewed legacy return cost",
    request_body = Create<LegacyCostPayload>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 201, body = Envelope<LegacyCostResource>, description = "Created"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace, order or line, or not a member"),
        (status = 409, body = MessageBody, description = "Reused client_ref"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, more than left stock, another product, or `unsupported_active_erp_operation` (setup book)"),
    ),
    security(("bearer" = []))
)]
pub async fn legacy_return_costs_create(
    State(state): State<AppState>,
    access: Can<AccountingPost>,
    cmd: Command<Create<LegacyCostPayload>>,
) -> Result<Answer, AppError> {
    service::legacy_create(&state, &access, cmd).await
}
