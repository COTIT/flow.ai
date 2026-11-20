//! HTTP edge for /v1/workspace/{workspace}/supplier-bills, /supplier-payments,
//! /purchase-returns and /purchase-cost-adjustments. Each route checks its permission before
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
        payables::{
            bills, costs,
            dto::{
                AllocatePayload, AllocateReview, BillLandedCandidates, BillMatchCandidates,
                BillPayload, BillPostReview, CostAdjustmentPayload, CostAdjustmentResource,
                CostAdjustmentReview, CostListQuery, LandedCandidatesQuery, ListQuery,
                MatchPayload, PaymentPayload, PaymentReview, PaymentTarget, PurchaseReturnResource,
                ReturnCandidates, ReturnCandidatesQuery, ReturnPayload, ReturnPostReview,
                ReturnReverseBody, ReverseBody, ReversePayload, ReverseReview, ReviewedPayload,
                SupplierBillResource, SupplierPaymentResource,
            },
            payments, returns,
        },
    },
};

permissions! {
    BillList => "supplierbill.list",
    BillGet => "supplierbill.get",
    BillCreate => "supplierbill.create",
    BillUpdate => "supplierbill.update",
    BillMatch => "supplierbill.match",
    BillPost => "supplierbill.post",
    BillReverse => "supplierbill.reverse",
    PayList => "supplierpayment.list",
    PayGet => "supplierpayment.get",
    PayCreate => "supplierpayment.create",
    PayAllocate => "supplierpayment.allocate",
    PayReverse => "supplierpayment.reverse",
    RetList => "purchasereturn.list",
    RetGet => "purchasereturn.get",
    RetCreate => "purchasereturn.create",
    RetPost => "purchasereturn.post",
    RetReverse => "purchasereturn.reverse",
    AccRead => "accounting.read",
    AccJournal => "accounting.journal_create",
    AccPost => "accounting.post",
    AccReverse => "accounting.reverse",
}

/// A path id; malformed reads as missing.
fn id(raw: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(raw).map_err(|_| AppError::NotFound)
}

/// supplier_bills_list: List supplier bills.
#[utoipa::path(
    get, path = "/workspace/{workspace}/supplier-bills", tag = "supplier_bills",
    operation_id = "supplier_bills_list",
    summary = "List supplier bills",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ListQuery),
    responses(
        (status = 200, body = Paginated<SupplierBillResource>, description = "Success, newest first"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn bills_list(
    State(state): State<AppState>,
    access: Can<BillList>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Paginated<SupplierBillResource>>, AppError> {
    Ok(Json(
        bills::list(
            &state,
            access.workspace_id,
            &q,
            access.allows("accounting.read"),
        )
        .await?,
    ))
}

/// supplier_bills_create: Create a draft supplier bill.
#[utoipa::path(
    post, path = "/workspace/{workspace}/supplier-bills", tag = "supplier_bills",
    operation_id = "supplier_bills_create",
    summary = "Create a draft supplier bill",
    request_body = Create<BillPayload>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 201, body = Envelope<SupplierBillResource>, description = "Created"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or not a member"),
        (status = 409, body = MessageBody, description = "Reused client_ref with another request"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input or duplicate supplier invoice number"),
    ),
    security(("bearer" = []))
)]
pub async fn bills_create(
    State(state): State<AppState>,
    access: Can<BillCreate>,
    cmd: Command<Create<BillPayload>>,
) -> Result<Answer, AppError> {
    bills::create(&state, &access, cmd).await
}

/// supplier_bills_get: Read a supplier bill.
#[utoipa::path(
    get, path = "/workspace/{workspace}/supplier-bills/{bill}", tag = "supplier_bills",
    operation_id = "supplier_bills_get",
    summary = "Read a supplier bill",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("bill" = Uuid, Path, description = "Supplier bill id")),
    responses(
        (status = 200, body = Envelope<SupplierBillResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or record, or not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn bills_get(
    State(state): State<AppState>,
    access: Can<BillGet>,
    Path((_, raw)): Path<(String, String)>,
) -> Result<Data<SupplierBillResource>, AppError> {
    let mut conn = state.db.acquire().await?;
    Ok(Data(
        bills::get(
            &mut conn,
            access.workspace_id,
            id(&raw)?,
            access.allows("accounting.read"),
        )
        .await?,
    ))
}

/// supplier_bills_update: Replace a draft bill's fields and lines.
#[utoipa::path(
    patch, path = "/workspace/{workspace}/supplier-bills/{bill}", tag = "supplier_bills",
    operation_id = "supplier_bills_update",
    summary = "Replace a draft bill's fields and lines",
    request_body = Action<BillPayload>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("bill" = Uuid, Path, description = "Supplier bill id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 200, body = Envelope<SupplierBillResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or record, or not a member"),
        (status = 409, body = MessageBody, description = "`stale_document` or reused client_ref"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, not a draft, or duplicate supplier invoice number"),
    ),
    security(("bearer" = []))
)]
pub async fn bills_update(
    State(state): State<AppState>,
    access: Can<BillUpdate>,
    Path((_, raw)): Path<(String, String)>,
    cmd: Command<Action<BillPayload>>,
) -> Result<Answer, AppError> {
    bills::update(&state, &access, id(&raw)?, cmd).await
}

/// supplier_bills_match: Reserve booked receipt lines for a draft bill's goods lines.
#[utoipa::path(
    post, path = "/workspace/{workspace}/supplier-bills/{bill}/match", tag = "supplier_bills",
    operation_id = "supplier_bills_match",
    summary = "Reserve booked receipt lines for a draft bill's goods lines",
    request_body = Action<MatchPayload>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("bill" = Uuid, Path, description = "Supplier bill id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 200, body = Envelope<SupplierBillResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace, bill, bill line or receipt line"),
        (status = 409, body = MessageBody, description = "`stale_document` or reused client_ref"),
        (status = 422, body = ValidationErrorBody, description = "More than the receipt line's unbilled capacity, another product/supplier/currency, not a draft, or `unsupported_active_erp_operation`"),
    ),
    security(("bearer" = []))
)]
pub async fn bills_match(
    State(state): State<AppState>,
    access: Can<BillMatch>,
    Path((_, raw)): Path<(String, String)>,
    cmd: Command<Action<MatchPayload>>,
) -> Result<Answer, AppError> {
    bills::match_lines(&state, &access, id(&raw)?, cmd).await
}

/// supplier_bills_post_review: Preview posting a draft bill (no writes).
#[utoipa::path(
    post, path = "/workspace/{workspace}/supplier-bills/{bill}/post/review", tag = "supplier_bills",
    operation_id = "supplier_bills_post_review",
    summary = "Preview posting a draft bill (no writes)",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("bill" = Uuid, Path, description = "Supplier bill id")),
    responses(
        (status = 200, body = Envelope<BillPostReview>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or record, or not a member"),
        (status = 422, body = ValidationErrorBody, description = "Unmatched goods, no rate, tax profile refused, or `accounting_not_configured` / `unsupported_active_erp_operation`"),
    ),
    security(("bearer" = []))
)]
pub async fn bills_post_review(
    State(state): State<AppState>,
    access: Can<BillPost>,
    Path((_, raw)): Path<(String, String)>,
) -> Result<Data<BillPostReview>, AppError> {
    Ok(Data(bills::post_review(&state, &access, id(&raw)?).await?))
}

/// supplier_bills_post: Post a reviewed draft bill.
#[utoipa::path(
    post, path = "/workspace/{workspace}/supplier-bills/{bill}/post", tag = "supplier_bills",
    operation_id = "supplier_bills_post",
    summary = "Post a reviewed draft bill",
    request_body = Action<ReviewedPayload>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("bill" = Uuid, Path, description = "Supplier bill id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 200, body = Envelope<SupplierBillResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or record, or not a member"),
        (status = 409, body = MessageBody, description = "`stale_document` (version or review) or reused client_ref"),
        (status = 422, body = ValidationErrorBody, description = "As the review, or `accounting_period_closed`"),
    ),
    security(("bearer" = []))
)]
pub async fn bills_post(
    State(state): State<AppState>,
    access: Can<BillPost>,
    Path((_, raw)): Path<(String, String)>,
    cmd: Command<Action<ReviewedPayload>>,
) -> Result<Answer, AppError> {
    bills::post(&state, &access, id(&raw)?, cmd).await
}

/// supplier_bills_reverse_review: Preview reversing a posted bill (no writes).
#[utoipa::path(
    post, path = "/workspace/{workspace}/supplier-bills/{bill}/reverse/review", tag = "supplier_bills",
    operation_id = "supplier_bills_reverse_review",
    summary = "Preview reversing a posted bill (no writes)",
    request_body = ReverseBody,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("bill" = Uuid, Path, description = "Supplier bill id")),
    responses(
        (status = 200, body = Envelope<ReverseReview>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or record, or not a member"),
        (status = 422, body = ValidationErrorBody, description = "Not posted, date before the bill, or `dependent_effects_exist`"),
    ),
    security(("bearer" = []))
)]
pub async fn bills_reverse_review(
    State(state): State<AppState>,
    access: Can<BillReverse>,
    Path((_, raw)): Path<(String, String)>,
    ValidatedJson(input): ValidatedJson<ReverseBody>,
) -> Result<Data<ReverseReview>, AppError> {
    Ok(Data(
        bills::reverse_review(&state, &access, id(&raw)?, &input).await?,
    ))
}

/// supplier_bills_reverse: Reverse a posted bill (typed correction).
#[utoipa::path(
    post, path = "/workspace/{workspace}/supplier-bills/{bill}/reverse", tag = "supplier_bills",
    operation_id = "supplier_bills_reverse",
    summary = "Reverse a posted bill (typed correction)",
    request_body = Action<ReversePayload>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("bill" = Uuid, Path, description = "Supplier bill id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 200, body = Envelope<SupplierBillResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or record, or not a member"),
        (status = 409, body = MessageBody, description = "`stale_document` (version or review) or reused client_ref"),
        (status = 422, body = ValidationErrorBody, description = "As the review, or `accounting_period_closed`"),
    ),
    security(("bearer" = []))
)]
pub async fn bills_reverse(
    State(state): State<AppState>,
    access: Can<BillReverse>,
    Path((_, raw)): Path<(String, String)>,
    cmd: Command<Action<ReversePayload>>,
) -> Result<Answer, AppError> {
    bills::reverse(&state, &access, id(&raw)?, cmd).await
}

/// supplier_payments_list: List supplier payments and refunds received.
#[utoipa::path(
    get, path = "/workspace/{workspace}/supplier-payments", tag = "supplier_payments",
    operation_id = "supplier_payments_list",
    summary = "List supplier payments and refunds received",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ListQuery),
    responses(
        (status = 200, body = Paginated<SupplierPaymentResource>, description = "Success, newest first"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn payments_list(
    State(state): State<AppState>,
    access: Can<PayList>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Paginated<SupplierPaymentResource>>, AppError> {
    Ok(Json(
        payments::list(
            &state,
            access.workspace_id,
            &q,
            access.allows("accounting.read"),
        )
        .await?,
    ))
}

/// supplier_payments_create_review: Preview a supplier payment or refund received (no writes).
#[utoipa::path(
    post, path = "/workspace/{workspace}/supplier-payments/review", tag = "supplier_payments",
    operation_id = "supplier_payments_create_review",
    summary = "Preview a supplier payment or refund received (no writes)",
    request_body = PaymentPayload,
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    responses(
        (status = 200, body = Envelope<PaymentReview>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace, money place or not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, other currency, not the business day, no rate, or `accounting_not_configured`"),
    ),
    security(("bearer" = []))
)]
pub async fn payments_create_review(
    State(state): State<AppState>,
    access: Can<PayCreate>,
    ValidatedJson(input): ValidatedJson<PaymentPayload>,
) -> Result<Data<PaymentReview>, AppError> {
    Ok(Data(
        payments::create_review(&state, &access, &input).await?,
    ))
}

/// supplier_payments_create: Record a reviewed supplier payment or refund received.
#[utoipa::path(
    post, path = "/workspace/{workspace}/supplier-payments", tag = "supplier_payments",
    operation_id = "supplier_payments_create",
    summary = "Record a reviewed supplier payment or refund received",
    request_body = Create<PaymentPayload>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 201, body = Envelope<SupplierPaymentResource>, description = "Created"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace, money place or not a member"),
        (status = 409, body = MessageBody, description = "`stale_document` (review) or reused client_ref"),
        (status = 422, body = ValidationErrorBody, description = "As the review, or `accounting_period_closed`"),
    ),
    security(("bearer" = []))
)]
pub async fn payments_create(
    State(state): State<AppState>,
    access: Can<PayCreate>,
    cmd: Command<Create<PaymentPayload>>,
) -> Result<Answer, AppError> {
    payments::create(&state, &access, cmd).await
}

/// supplier_payments_get: Read a supplier payment.
#[utoipa::path(
    get, path = "/workspace/{workspace}/supplier-payments/{payment}", tag = "supplier_payments",
    operation_id = "supplier_payments_get",
    summary = "Read a supplier payment",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("payment" = Uuid, Path, description = "Supplier payment id")),
    responses(
        (status = 200, body = Envelope<SupplierPaymentResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or record, or not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn payments_get(
    State(state): State<AppState>,
    access: Can<PayGet>,
    Path((_, raw)): Path<(String, String)>,
) -> Result<Data<SupplierPaymentResource>, AppError> {
    let mut conn = state.db.acquire().await?;
    Ok(Data(
        payments::get(
            &mut conn,
            access.workspace_id,
            id(&raw)?,
            access.allows("accounting.read"),
        )
        .await?,
    ))
}

/// supplier_payments_allocate_review: Preview applying a payment (no writes).
#[utoipa::path(
    post, path = "/workspace/{workspace}/supplier-payments/{payment}/allocate/review", tag = "supplier_payments",
    operation_id = "supplier_payments_allocate_review",
    summary = "Preview applying a payment (no writes)",
    request_body = AllocatePayload,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("payment" = Uuid, Path, description = "Supplier payment id")),
    responses(
        (status = 200, body = Envelope<AllocateReview>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace, payment or open item"),
        (status = 422, body = ValidationErrorBody, description = "Another supplier/currency/kind of item, more than open, or not posted"),
    ),
    security(("bearer" = []))
)]
pub async fn payments_allocate_review(
    State(state): State<AppState>,
    access: Can<PayAllocate>,
    Path((_, raw)): Path<(String, String)>,
    ValidatedJson(input): ValidatedJson<AllocatePayload>,
) -> Result<Data<AllocateReview>, AppError> {
    Ok(Data(
        payments::allocate_review(&state, &access, id(&raw)?, &input).await?,
    ))
}

/// supplier_payments_allocate: Apply a payment to a bill (or a refund to a credit).
#[utoipa::path(
    post, path = "/workspace/{workspace}/supplier-payments/{payment}/allocate", tag = "supplier_payments",
    operation_id = "supplier_payments_allocate",
    summary = "Apply a payment to a bill (or a refund to a credit)",
    request_body = Action<AllocatePayload>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("payment" = Uuid, Path, description = "Supplier payment id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 200, body = Envelope<SupplierPaymentResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace, payment or open item"),
        (status = 409, body = MessageBody, description = "`stale_document` (version or review) or reused client_ref"),
        (status = 422, body = ValidationErrorBody, description = "As the review, or `accounting_period_closed`"),
    ),
    security(("bearer" = []))
)]
pub async fn payments_allocate(
    State(state): State<AppState>,
    access: Can<PayAllocate>,
    Path((_, raw)): Path<(String, String)>,
    cmd: Command<Action<AllocatePayload>>,
) -> Result<Answer, AppError> {
    payments::allocate(&state, &access, id(&raw)?, cmd).await
}

/// supplier_payments_reverse_review: Preview reversing an unapplied payment (no writes).
#[utoipa::path(
    post, path = "/workspace/{workspace}/supplier-payments/{payment}/reverse/review", tag = "supplier_payments",
    operation_id = "supplier_payments_reverse_review",
    summary = "Preview reversing an unapplied payment (no writes)",
    request_body = ReverseBody,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("payment" = Uuid, Path, description = "Supplier payment id")),
    responses(
        (status = 200, body = Envelope<ReverseReview>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or record, or not a member"),
        (status = 422, body = ValidationErrorBody, description = "Not posted, not the business day, or `dependent_effects_exist` (applied)"),
    ),
    security(("bearer" = []))
)]
pub async fn payments_reverse_review(
    State(state): State<AppState>,
    access: Can<PayReverse>,
    Path((_, raw)): Path<(String, String)>,
    ValidatedJson(input): ValidatedJson<ReverseBody>,
) -> Result<Data<ReverseReview>, AppError> {
    Ok(Data(
        payments::reverse_review(&state, &access, id(&raw)?, &input).await?,
    ))
}

/// supplier_payments_reverse: Reverse an erroneous, unapplied payment record.
#[utoipa::path(
    post, path = "/workspace/{workspace}/supplier-payments/{payment}/reverse", tag = "supplier_payments",
    operation_id = "supplier_payments_reverse",
    summary = "Reverse an erroneous, unapplied payment record",
    request_body = Action<ReversePayload>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("payment" = Uuid, Path, description = "Supplier payment id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 200, body = Envelope<SupplierPaymentResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or record, or not a member"),
        (status = 409, body = MessageBody, description = "`stale_document` (version or review) or reused client_ref"),
        (status = 422, body = ValidationErrorBody, description = "As the review, or `accounting_period_closed`"),
    ),
    security(("bearer" = []))
)]
pub async fn payments_reverse(
    State(state): State<AppState>,
    access: Can<PayReverse>,
    Path((_, raw)): Path<(String, String)>,
    cmd: Command<Action<ReversePayload>>,
) -> Result<Answer, AppError> {
    payments::reverse(&state, &access, id(&raw)?, cmd).await
}

/// purchase_returns_list: List purchase returns.
#[utoipa::path(
    get, path = "/workspace/{workspace}/purchase-returns", tag = "purchase_returns",
    operation_id = "purchase_returns_list",
    summary = "List purchase returns",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ListQuery),
    responses(
        (status = 200, body = Paginated<PurchaseReturnResource>, description = "Success, newest first"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn returns_list(
    State(state): State<AppState>,
    access: Can<RetList>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Paginated<PurchaseReturnResource>>, AppError> {
    Ok(Json(
        returns::list(
            &state,
            access.workspace_id,
            &q,
            access.allows("accounting.read"),
        )
        .await?,
    ))
}

/// purchase_returns_create: Create a draft purchase return.
#[utoipa::path(
    post, path = "/workspace/{workspace}/purchase-returns", tag = "purchase_returns",
    operation_id = "purchase_returns_create",
    summary = "Create a draft purchase return",
    request_body = Create<ReturnPayload>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 201, body = Envelope<PurchaseReturnResource>, description = "Created"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace, receipt line or not a member"),
        (status = 409, body = MessageBody, description = "Reused client_ref with another request"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, more than returnable/unbilled, or `accounting_not_configured` / `unsupported_active_erp_operation`"),
    ),
    security(("bearer" = []))
)]
pub async fn returns_create(
    State(state): State<AppState>,
    access: Can<RetCreate>,
    cmd: Command<Create<ReturnPayload>>,
) -> Result<Answer, AppError> {
    returns::create(&state, &access, cmd).await
}

/// purchase_returns_get: Read a purchase return.
#[utoipa::path(
    get, path = "/workspace/{workspace}/purchase-returns/{return}", tag = "purchase_returns",
    operation_id = "purchase_returns_get",
    summary = "Read a purchase return",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("return" = Uuid, Path, description = "Purchase return id")),
    responses(
        (status = 200, body = Envelope<PurchaseReturnResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or record, or not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn returns_get(
    State(state): State<AppState>,
    access: Can<RetGet>,
    Path((_, raw)): Path<(String, String)>,
) -> Result<Data<PurchaseReturnResource>, AppError> {
    let mut conn = state.db.acquire().await?;
    Ok(Data(
        returns::get(
            &mut conn,
            access.workspace_id,
            id(&raw)?,
            access.allows("accounting.read"),
        )
        .await?,
    ))
}

/// purchase_returns_post_review: Preview posting a draft return (no writes).
#[utoipa::path(
    post, path = "/workspace/{workspace}/purchase-returns/{return}/post/review", tag = "purchase_returns",
    operation_id = "purchase_returns_post_review",
    summary = "Preview posting a draft return (no writes)",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("return" = Uuid, Path, description = "Purchase return id")),
    responses(
        (status = 200, body = Envelope<ReturnPostReview>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or record, or not a member"),
        (status = 422, body = ValidationErrorBody, description = "More than returnable/unbilled, `insufficient_stock`, or `accounting_not_configured`"),
    ),
    security(("bearer" = []))
)]
pub async fn returns_post_review(
    State(state): State<AppState>,
    access: Can<RetPost>,
    Path((_, raw)): Path<(String, String)>,
) -> Result<Data<ReturnPostReview>, AppError> {
    Ok(Data(
        returns::post_review(&state, &access, id(&raw)?).await?,
    ))
}

/// purchase_returns_post: Post a reviewed return (goods out at current average).
#[utoipa::path(
    post, path = "/workspace/{workspace}/purchase-returns/{return}/post", tag = "purchase_returns",
    operation_id = "purchase_returns_post",
    summary = "Post a reviewed return (goods out at current average)",
    request_body = Action<ReviewedPayload>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("return" = Uuid, Path, description = "Purchase return id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 200, body = Envelope<PurchaseReturnResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or record, or not a member"),
        (status = 409, body = MessageBody, description = "`stale_document` (version or review) or reused client_ref"),
        (status = 422, body = ValidationErrorBody, description = "As the review, or `receipt_booking_required` / `backdated_valuation` / `accounting_period_closed`"),
    ),
    security(("bearer" = []))
)]
pub async fn returns_post(
    State(state): State<AppState>,
    access: Can<RetPost>,
    Path((_, raw)): Path<(String, String)>,
    cmd: Command<Action<ReviewedPayload>>,
) -> Result<Answer, AppError> {
    returns::post(&state, &access, id(&raw)?, cmd).await
}

/// purchase_returns_reverse_review: Preview reversing a posted return (no writes).
#[utoipa::path(
    post, path = "/workspace/{workspace}/purchase-returns/{return}/reverse/review", tag = "purchase_returns",
    operation_id = "purchase_returns_reverse_review",
    summary = "Preview reversing a posted return (no writes)",
    request_body = ReturnReverseBody,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("return" = Uuid, Path, description = "Purchase return id")),
    responses(
        (status = 200, body = Envelope<ReverseReview>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or record, or not a member"),
        (status = 422, body = ValidationErrorBody, description = "Not posted, one component only (`unsupported_active_erp_operation`), or `dependent_effects_exist`"),
    ),
    security(("bearer" = []))
)]
pub async fn returns_reverse_review(
    State(state): State<AppState>,
    access: Can<RetReverse>,
    Path((_, raw)): Path<(String, String)>,
    ValidatedJson(input): ValidatedJson<ReturnReverseBody>,
) -> Result<Data<ReverseReview>, AppError> {
    Ok(Data(
        returns::reverse_review(&state, &access, id(&raw)?, &input).await?,
    ))
}

/// purchase_returns_reverse: Reverse a posted return (goods back, credit closed).
#[utoipa::path(
    post, path = "/workspace/{workspace}/purchase-returns/{return}/reverse", tag = "purchase_returns",
    operation_id = "purchase_returns_reverse",
    summary = "Reverse a posted return (goods back, credit closed)",
    request_body = Action<ReturnReverseBody>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("return" = Uuid, Path, description = "Purchase return id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 200, body = Envelope<PurchaseReturnResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or record, or not a member"),
        (status = 409, body = MessageBody, description = "`stale_document` (version or review) or reused client_ref"),
        (status = 422, body = ValidationErrorBody, description = "As the review, or `backdated_valuation` / `accounting_period_closed`"),
    ),
    security(("bearer" = []))
)]
pub async fn returns_reverse(
    State(state): State<AppState>,
    access: Can<RetReverse>,
    Path((_, raw)): Path<(String, String)>,
    cmd: Command<Action<ReturnReverseBody>>,
) -> Result<Answer, AppError> {
    returns::reverse(&state, &access, id(&raw)?, cmd).await
}

/// purchase_cost_adjustments_list: List purchase cost adjustments.
#[utoipa::path(
    get, path = "/workspace/{workspace}/purchase-cost-adjustments", tag = "purchase_cost_adjustments",
    operation_id = "purchase_cost_adjustments_list",
    summary = "List purchase cost adjustments",
    params(("workspace" = Uuid, Path, description = "Workspace id"), CostListQuery),
    responses(
        (status = 200, body = Paginated<CostAdjustmentResource>, description = "Success, newest first"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or not a member"),
        (status = 422, body = ValidationErrorBody, description = "`accounting_not_configured`"),
    ),
    security(("bearer" = []))
)]
pub async fn costs_list(
    State(state): State<AppState>,
    access: Can<AccRead>,
    Query(q): Query<CostListQuery>,
) -> Result<Json<Paginated<CostAdjustmentResource>>, AppError> {
    Ok(Json(costs::list(&state, access.workspace_id, &q).await?))
}

/// purchase_cost_adjustments_create: Draft a split of a bill's pending purchase cost variance.
#[utoipa::path(
    post, path = "/workspace/{workspace}/purchase-cost-adjustments", tag = "purchase_cost_adjustments",
    operation_id = "purchase_cost_adjustments_create",
    summary = "Draft a split of a bill's pending purchase cost variance",
    request_body = Create<CostAdjustmentPayload>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 201, body = Envelope<CostAdjustmentResource>, description = "Created"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace, bill or not a member"),
        (status = 409, body = MessageBody, description = "Reused client_ref with another request"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, bill not posted, line not on the bill, or `accounting_not_configured`"),
    ),
    security(("bearer" = []))
)]
pub async fn costs_create(
    State(state): State<AppState>,
    access: Can<AccJournal>,
    cmd: Command<Create<CostAdjustmentPayload>>,
) -> Result<Answer, AppError> {
    costs::create(&state, &access, cmd).await
}

/// purchase_cost_adjustments_get: Read a purchase cost adjustment.
#[utoipa::path(
    get, path = "/workspace/{workspace}/purchase-cost-adjustments/{adjustment}", tag = "purchase_cost_adjustments",
    operation_id = "purchase_cost_adjustments_get",
    summary = "Read a purchase cost adjustment",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("adjustment" = Uuid, Path, description = "Adjustment id")),
    responses(
        (status = 200, body = Envelope<CostAdjustmentResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or record, or not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn costs_get(
    State(state): State<AppState>,
    access: Can<AccRead>,
    Path((_, raw)): Path<(String, String)>,
) -> Result<Data<CostAdjustmentResource>, AppError> {
    let mut conn = state.db.acquire().await?;
    Ok(Data(
        costs::get(&mut conn, access.workspace_id, id(&raw)?).await?,
    ))
}

/// purchase_cost_adjustments_post_review: Preview posting a cost adjustment (no writes).
#[utoipa::path(
    post, path = "/workspace/{workspace}/purchase-cost-adjustments/{adjustment}/post/review", tag = "purchase_cost_adjustments",
    operation_id = "purchase_cost_adjustments_post_review",
    summary = "Preview posting a cost adjustment (no writes)",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("adjustment" = Uuid, Path, description = "Adjustment id")),
    responses(
        (status = 200, body = Envelope<CostAdjustmentReview>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or record, or not a member"),
        (status = 422, body = ValidationErrorBody, description = "More than or against the pending variance, or not a draft"),
    ),
    security(("bearer" = []))
)]
pub async fn costs_post_review(
    State(state): State<AppState>,
    access: Can<AccPost>,
    Path((_, raw)): Path<(String, String)>,
) -> Result<Data<CostAdjustmentReview>, AppError> {
    Ok(Data(costs::post_review(&state, &access, id(&raw)?).await?))
}

/// purchase_cost_adjustments_post: Post a reviewed cost adjustment.
#[utoipa::path(
    post, path = "/workspace/{workspace}/purchase-cost-adjustments/{adjustment}/post", tag = "purchase_cost_adjustments",
    operation_id = "purchase_cost_adjustments_post",
    summary = "Post a reviewed cost adjustment",
    request_body = Action<ReviewedPayload>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("adjustment" = Uuid, Path, description = "Adjustment id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 200, body = Envelope<CostAdjustmentResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or record, or not a member"),
        (status = 409, body = MessageBody, description = "`stale_document` (version or review) or reused client_ref"),
        (status = 422, body = ValidationErrorBody, description = "As the review, empty stock for an inventory part, or `backdated_valuation` / `accounting_period_closed`"),
    ),
    security(("bearer" = []))
)]
pub async fn costs_post(
    State(state): State<AppState>,
    access: Can<AccPost>,
    Path((_, raw)): Path<(String, String)>,
    cmd: Command<Action<ReviewedPayload>>,
) -> Result<Answer, AppError> {
    costs::post(&state, &access, id(&raw)?, cmd).await
}

/// purchase_cost_adjustments_reverse_review: Preview reversing a cost adjustment (no writes).
#[utoipa::path(
    post, path = "/workspace/{workspace}/purchase-cost-adjustments/{adjustment}/reverse/review", tag = "purchase_cost_adjustments",
    operation_id = "purchase_cost_adjustments_reverse_review",
    summary = "Preview reversing a cost adjustment (no writes)",
    request_body = ReverseBody,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("adjustment" = Uuid, Path, description = "Adjustment id")),
    responses(
        (status = 200, body = Envelope<ReverseReview>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or record, or not a member"),
        (status = 422, body = ValidationErrorBody, description = "Not posted, not the business day, or `dependent_effects_exist`"),
    ),
    security(("bearer" = []))
)]
pub async fn costs_reverse_review(
    State(state): State<AppState>,
    access: Can<AccReverse>,
    Path((_, raw)): Path<(String, String)>,
    ValidatedJson(input): ValidatedJson<ReverseBody>,
) -> Result<Data<ReverseReview>, AppError> {
    Ok(Data(
        costs::reverse_review(&state, &access, id(&raw)?, &input).await?,
    ))
}

/// purchase_cost_adjustments_reverse: Reverse a cost adjustment nothing depends on.
#[utoipa::path(
    post, path = "/workspace/{workspace}/purchase-cost-adjustments/{adjustment}/reverse", tag = "purchase_cost_adjustments",
    operation_id = "purchase_cost_adjustments_reverse",
    summary = "Reverse a cost adjustment nothing depends on",
    request_body = Action<ReversePayload>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("adjustment" = Uuid, Path, description = "Adjustment id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 200, body = Envelope<CostAdjustmentResource>, description = "Success"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or record, or not a member"),
        (status = 409, body = MessageBody, description = "`stale_document` (version or review) or reused client_ref"),
        (status = 422, body = ValidationErrorBody, description = "As the review, or `accounting_period_closed`"),
    ),
    security(("bearer" = []))
)]
pub async fn costs_reverse(
    State(state): State<AppState>,
    access: Can<AccReverse>,
    Path((_, raw)): Path<(String, String)>,
    cmd: Command<Action<ReversePayload>>,
) -> Result<Answer, AppError> {
    costs::reverse(&state, &access, id(&raw)?, cmd).await
}

/// supplier_bills_match_candidates: Booked receipt lines a draft bill's goods lines may match.
#[utoipa::path(
    get, path = "/workspace/{workspace}/supplier-bills/{bill}/match-candidates", tag = "supplier_bills",
    operation_id = "supplier_bills_match_candidates",
    summary = "Booked receipt lines a draft bill's goods lines may match",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("bill" = Uuid, Path, description = "Supplier bill id")),
    responses(
        (status = 200, body = Envelope<BillMatchCandidates>, description = "Success; matchable left per receipt line, this bill's reservations not counted"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or bill, or not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn bills_match_candidates(
    State(state): State<AppState>,
    access: Can<BillMatch>,
    Path((_, raw)): Path<(String, String)>,
) -> Result<Data<BillMatchCandidates>, AppError> {
    Ok(Data(
        bills::match_candidates(&state, access.workspace_id, id(&raw)?).await?,
    ))
}

/// supplier_bills_landed_candidates: Booked landed components a bill line may clear.
#[utoipa::path(
    get, path = "/workspace/{workspace}/supplier-bills/landed-candidates", tag = "supplier_bills",
    operation_id = "supplier_bills_landed_candidates",
    summary = "Booked landed components a bill line may clear",
    params(("workspace" = Uuid, Path, description = "Workspace id"), LandedCandidatesQuery),
    responses(
        (status = 200, body = Envelope<BillLandedCandidates>, description = "Success; components in the currency on no live bill"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or not a member"),
        (status = 422, body = ValidationErrorBody, description = "`currency_id` missing"),
    ),
    security(("bearer" = []))
)]
pub async fn bills_landed_candidates(
    State(state): State<AppState>,
    access: Can<BillCreate>,
    Query(q): Query<LandedCandidatesQuery>,
) -> Result<Data<BillLandedCandidates>, AppError> {
    let currency = q
        .currency_id
        .ok_or_else(|| AppError::field("currency_id", "Choose a currency."))?;
    Ok(Data(
        bills::landed_candidates(&state, access.workspace_id, currency, q.bill_id).await?,
    ))
}

/// purchase_returns_candidates: Returnable quantities of one booked receipt.
#[utoipa::path(
    get, path = "/workspace/{workspace}/purchase-returns/candidates", tag = "purchase_returns",
    operation_id = "purchase_returns_candidates",
    summary = "Returnable quantities of one booked receipt",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ReturnCandidatesQuery),
    responses(
        (status = 200, body = Envelope<ReturnCandidates>, description = "Success; unbilled and per posted bill match"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or receipt, or not a member"),
        (status = 422, body = ValidationErrorBody, description = "`receipt_id` missing"),
    ),
    security(("bearer" = []))
)]
pub async fn returns_candidates(
    State(state): State<AppState>,
    access: Can<RetCreate>,
    Query(q): Query<ReturnCandidatesQuery>,
) -> Result<Data<ReturnCandidates>, AppError> {
    let receipt = q
        .receipt_id
        .ok_or_else(|| AppError::field("receipt_id", "Choose a receipt."))?;
    Ok(Data(
        returns::candidates(&state, access.workspace_id, receipt).await?,
    ))
}

/// supplier_payments_targets: Open items a supplier payment or refund may be allocated to.
#[utoipa::path(
    get, path = "/workspace/{workspace}/supplier-payments/{payment}/targets", tag = "supplier_payments",
    operation_id = "supplier_payments_targets",
    summary = "Open items a supplier payment or refund may be allocated to",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("payment" = Uuid, Path, description = "Supplier payment id"), ListQuery),
    responses(
        (status = 200, body = Paginated<PaymentTarget>, description = "Success, oldest due first"),
        (status = 403, body = MessageBody, description = "Missing permission or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or payment, or not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn payments_targets(
    State(state): State<AppState>,
    access: Can<PayAllocate>,
    Path((_, raw)): Path<(String, String)>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Paginated<PaymentTarget>>, AppError> {
    Ok(Json(
        payments::targets(&state, access.workspace_id, id(&raw)?, &q).await?,
    ))
}
