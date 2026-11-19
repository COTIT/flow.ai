//! HTTP edge for privileged Till returns and refunds under
//! /v1/workspace/{workspace}/till/sales/{order}: member bearer + `X-Till-Session` (D14), and
//! the member's own `cashregister.refund`. Writes take the C envelope plus a required
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
    },
    idempotency::{Answer, Command},
    modules::{
        accounting::dto::Create,
        purchasing::dto::PageQuery,
        sales_returns::dto::SalesReturnResource,
        till::privileged::PrivilegedTill,
        till_refunds::{
            dto::{
                LookupQuery, RefundPayload, RefundReview, RefundReviewRequest, ReturnPayload,
                ReturnReview, ReturnReviewRequest, TillRefundResource, TillSaleLookup,
            },
            service,
        },
    },
};

const NO_REFUND: &str = "This action requires the cashregister.refund permission.";

/// The member must hold `cashregister.refund` (a till PIN or member link never counts).
fn refunder(p: &PrivilegedTill) -> Result<(), AppError> {
    if p.member.allows("cashregister.refund") {
        Ok(())
    } else {
        Err(AppError::Forbidden(NO_REFUND))
    }
}

/// A path id; malformed reads as missing.
fn id(raw: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(raw).map_err(|_| AppError::NotFound)
}

/// till_sales_lookup: a till sale by receipt number, with what is left to return and refund.
#[utoipa::path(
    get, path = "/workspace/{workspace}/till/sales/lookup", tag = "till",
    operation_id = "till_sales_lookup",
    summary = "Find a till sale by receipt number (privileged)",
    description = "Member bearer + `X-Till-Session`. Any register of the workspace may look up; \
        `register_id` says which drawer a cash refund must use. `returnable` and `refundable` \
        are what a return and refund accept now.",
    params(("workspace" = Uuid, Path, description = "Workspace id"), LookupQuery),
    responses(
        (status = 200, body = Envelope<TillSaleLookup>, description = "The sale, its lines and credits"),
        (status = 401, body = MessageBody, description = "No live member login or till session"),
        (status = 403, body = MessageBody, description = "Missing `cashregister.refund`"),
        (status = 404, body = MessageBody, description = "Not a member, other workspace, or no till sale with that number"),
        (status = 422, body = ValidationErrorBody, description = "No number given"),
    ),
    security(("bearer" = [], "till_privileged" = []))
)]
pub async fn till_sales_lookup(
    State(state): State<AppState>,
    p: PrivilegedTill,
    Query(q): Query<LookupQuery>,
) -> Result<Data<TillSaleLookup>, AppError> {
    refunder(&p)?;
    let number = q.number.as_deref().map(str::trim).unwrap_or_default();
    if number.is_empty() {
        return Err(AppError::field("number", "Enter the receipt number."));
    }
    Ok(Data(
        service::lookup(&state, p.member.workspace_id, number).await?,
    ))
}

/// till_sales_return_review: preview a till return (no writes).
#[utoipa::path(
    post, path = "/workspace/{workspace}/till/sales/{order}/returns/review", tag = "till",
    operation_id = "till_sales_return_review",
    summary = "Preview a till return (no writes)",
    description = "Member bearer + `X-Till-Session`. Credits sold lines of a till sale at \
        their original money (receipt credit, no customer needed), optional restock.",
    request_body = ReturnReviewRequest,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("order" = Uuid, Path, description = "Till sale (order) id")),
    responses(
        (status = 200, body = Envelope<ReturnReview>, description = "The credits it posts and the review fingerprint"),
        (status = 401, body = MessageBody, description = "No live member login or till session"),
        (status = 403, body = MessageBody, description = "Missing `cashregister.refund`"),
        (status = 404, body = MessageBody, description = "Not a member, other workspace, or not a till sale"),
        (status = 422, body = ValidationErrorBody, description = "Invalid lines, over what is left, another warehouse, or `unsupported_active_erp_operation`"),
    ),
    security(("bearer" = [], "till_privileged" = []))
)]
pub async fn till_sales_return_review(
    State(state): State<AppState>,
    Path((_, order)): Path<(String, String)>,
    p: PrivilegedTill,
    ValidatedJson(input): ValidatedJson<ReturnReviewRequest>,
) -> Result<Data<ReturnReview>, AppError> {
    refunder(&p)?;
    Ok(Data(
        service::return_review(&state, &p, id(&order)?, &input).await?,
    ))
}

/// till_sales_return: post a reviewed till return.
#[utoipa::path(
    post, path = "/workspace/{workspace}/till/sales/{order}/returns", tag = "till",
    operation_id = "till_sales_return",
    summary = "Return till sale lines as a customer credit",
    description = "Member bearer + `X-Till-Session` (D14). One posted receipt credit per \
        recognised sale (and restock when asked), exactly as the review showed.",
    request_body = Create<ReturnPayload>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("order" = Uuid, Path, description = "Till sale (order) id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 201, body = Envelope<Vec<SalesReturnResource>>, description = "Posted credits"),
        (status = 401, body = MessageBody, description = "No live member login or till session"),
        (status = 403, body = MessageBody, description = "Missing `cashregister.refund`"),
        (status = 404, body = MessageBody, description = "Not a member, other workspace, or not a till sale"),
        (status = 409, body = MessageBody, description = "`stale_document` or reused client_ref"),
        (status = 422, body = ValidationErrorBody, description = "Invalid lines, over what is left, or `unsupported_active_erp_operation` / `accounting_period_closed`"),
    ),
    security(("bearer" = [], "till_privileged" = []))
)]
pub async fn till_sales_return(
    State(state): State<AppState>,
    Path((_, order)): Path<(String, String)>,
    p: PrivilegedTill,
    cmd: Command<Create<ReturnPayload>>,
) -> Result<Answer, AppError> {
    refunder(&p)?;
    service::return_create(&state, &p, id(&order)?, cmd).await
}

/// till_refunds_create_review: preview a till refund (no writes).
#[utoipa::path(
    post, path = "/workspace/{workspace}/till/sales/{order}/refunds/review", tag = "till",
    operation_id = "till_refunds_create_review",
    summary = "Preview a till refund (no writes)",
    description = "Member bearer + `X-Till-Session`. Cash: the sale's own register, open \
        shift, enough drawer, a multiple of the cash step. card_recorded: any register, the \
        terminal's refund reference, confirmed.",
    request_body = RefundReviewRequest,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("order" = Uuid, Path, description = "Till sale (order) id")),
    responses(
        (status = 200, body = Envelope<RefundReview>, description = "Refundable, journal plan and fingerprint"),
        (status = 401, body = MessageBody, description = "No live member login or till session"),
        (status = 403, body = MessageBody, description = "Missing `cashregister.refund`"),
        (status = 404, body = MessageBody, description = "Not a member, other workspace, not a till sale, or cash on another register"),
        (status = 409, body = MessageBody, description = "Cash refund without an open drawer shift"),
        (status = 422, body = ValidationErrorBody, description = "Not this sale's credit, over the refundable amount, off the cash step, not enough drawer, missing terminal reference, or `unsupported_active_erp_operation`"),
    ),
    security(("bearer" = [], "till_privileged" = []))
)]
pub async fn till_refunds_create_review(
    State(state): State<AppState>,
    Path((_, order)): Path<(String, String)>,
    p: PrivilegedTill,
    ValidatedJson(input): ValidatedJson<RefundReviewRequest>,
) -> Result<Data<RefundReview>, AppError> {
    refunder(&p)?;
    Ok(Data(
        service::refund_review(&state, &p, id(&order)?, &input).await?,
    ))
}

/// till_refunds_create: pay out a reviewed refund of a sale's customer credit.
#[utoipa::path(
    post, path = "/workspace/{workspace}/till/sales/{order}/refunds", tag = "till",
    operation_id = "till_refunds_create",
    summary = "Refund a customer credit at the till",
    description = "Member bearer + `X-Till-Session` (D14). Journal DR receivables / CR cash \
        (drawer refund row) or provider_clearing (card_recorded: no drawer movement, no \
        gateway call). Amount in integer minor units.",
    request_body = Create<RefundPayload>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("order" = Uuid, Path, description = "Till sale (order) id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 201, body = Envelope<TillRefundResource>, description = "Refunded"),
        (status = 401, body = MessageBody, description = "No live member login or till session"),
        (status = 403, body = MessageBody, description = "Missing `cashregister.refund`"),
        (status = 404, body = MessageBody, description = "Not a member, other workspace, not a till sale, or cash on another register"),
        (status = 409, body = MessageBody, description = "`stale_document`, reused client_ref, or no open drawer shift"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, over the refundable amount, or `unsupported_active_erp_operation` / `accounting_period_closed`"),
    ),
    security(("bearer" = [], "till_privileged" = []))
)]
pub async fn till_refunds_create(
    State(state): State<AppState>,
    Path((_, order)): Path<(String, String)>,
    p: PrivilegedTill,
    cmd: Command<Create<RefundPayload>>,
) -> Result<Answer, AppError> {
    refunder(&p)?;
    service::refund_create(&state, &p, id(&order)?, cmd).await
}

/// till_refunds_list: refunds of a till sale.
#[utoipa::path(
    get, path = "/workspace/{workspace}/till/sales/{order}/refunds", tag = "till",
    operation_id = "till_refunds_list",
    summary = "List a till sale's refunds",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("order" = Uuid, Path, description = "Till sale (order) id"), PageQuery),
    responses(
        (status = 200, body = Paginated<TillRefundResource>, description = "Oldest first"),
        (status = 401, body = MessageBody, description = "No live member login or till session"),
        (status = 403, body = MessageBody, description = "Missing `cashregister.refund`"),
        (status = 404, body = MessageBody, description = "Not a member, other workspace, or not a till sale"),
    ),
    security(("bearer" = [], "till_privileged" = []))
)]
pub async fn till_refunds_list(
    State(state): State<AppState>,
    Path((_, order)): Path<(String, String)>,
    p: PrivilegedTill,
    Query(q): Query<PageQuery>,
) -> Result<Json<Paginated<TillRefundResource>>, AppError> {
    refunder(&p)?;
    Ok(Json(
        service::list(&state, p.member.workspace_id, id(&order)?, &q).await?,
    ))
}

/// till_refunds_get: one refund of a till sale.
#[utoipa::path(
    get, path = "/workspace/{workspace}/till/sales/{order}/refunds/{refund}", tag = "till",
    operation_id = "till_refunds_get",
    summary = "Get a till refund",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("order" = Uuid, Path, description = "Till sale (order) id"), ("refund" = Uuid, Path, description = "Refund id")),
    responses(
        (status = 200, body = Envelope<TillRefundResource>, description = "The refund"),
        (status = 401, body = MessageBody, description = "No live member login or till session"),
        (status = 403, body = MessageBody, description = "Missing `cashregister.refund`"),
        (status = 404, body = MessageBody, description = "Not a member, other workspace, or unknown"),
    ),
    security(("bearer" = [], "till_privileged" = []))
)]
pub async fn till_refunds_get(
    State(state): State<AppState>,
    Path((_, order, refund)): Path<(String, String, String)>,
    p: PrivilegedTill,
) -> Result<Data<TillRefundResource>, AppError> {
    refunder(&p)?;
    Ok(Data(
        service::get(&state, p.member.workspace_id, id(&order)?, id(&refund)?).await?,
    ))
}
