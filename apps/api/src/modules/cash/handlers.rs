//! HTTP edge for till cash under /v1/workspace/{workspace}/till: the till opens and closes its
//! drawer shift and moves cash to and from vaults; office (`cashregister.*`) manages vaults and
//! reads shift history.

use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
};

use crate::{
    AppState,
    error::{AppError, ValidationErrorBody},
    http::{
        json::ValidatedJson,
        list::{Page, Paginated},
        response::{Data, Envelope, MessageBody},
        workspace::{Can, permissions},
    },
    idempotency::{Answer, Command},
    modules::{
        accounting::dto::Create,
        cash::{
            dto::{
                AdjustReviewResource, CashAdjustmentResource, CashPlaceResource, CountQuote,
                CountRequest, CreateVaultRequest, OfficeAdjustPayload, OfficeReviewRequest, Place,
                ShiftQuery, ShiftResource, TillAdjustPayload, TillReviewRequest, TransferRequest,
                TransferResource, VaultResource,
            },
            places::{self, Approver},
            service,
        },
        till::{privileged::PrivilegedTill, session::TillSession},
    },
};

permissions! {
    CashList => "cashregister.list",
    CashCreate => "cashregister.create",
    CashAdjust => "cashregister.adjust",
    AccountingRead => "accounting.read",
}

const NO_ADJUST: &str = "This action requires the cashregister.adjust permission.";
const OFFICE_ONLY: &str = "Vaults are adjusted from the Office.";

/// GET /till/shift -> `{data: shift|null}`.
#[utoipa::path(
    get, path = "/workspace/{workspace}/till/shift", tag = "till",
    operation_id = "till_shift_current", summary = "The register's open shift",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    responses(
        (status = 200, body = Envelope<Option<ShiftResource>>, description = "The open shift with expected cash so far, or null"),
        (status = 401, body = MessageBody, description = "No live till session"),
        (status = 404, body = MessageBody, description = "Workspace is not the session's"),
    ),
    security(("till_session" = []))
)]
pub async fn current(
    State(state): State<AppState>,
    session: TillSession,
) -> Result<Data<Option<ShiftResource>>, AppError> {
    let row = service::current(&state, &session.row).await?;
    Ok(Data(row.map(Into::into)))
}

/// POST /till/shift/open -> 201 `{data: shift}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/till/shift/open", tag = "till",
    operation_id = "till_shift_open", summary = "Open the drawer with a counted float",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = CountRequest,
    responses(
        (status = 201, body = Envelope<ShiftResource>, description = "Opened"),
        (status = 401, body = MessageBody, description = "No live till session"),
        (status = 404, body = MessageBody, description = "Workspace is not the session's"),
        (status = 409, body = MessageBody, description = "A shift is already open on this register"),
        (status = 422, body = ValidationErrorBody, description = "Invalid count"),
    ),
    security(("till_session" = []))
)]
pub async fn open(
    State(state): State<AppState>,
    session: TillSession,
    ValidatedJson(count): ValidatedJson<CountRequest>,
) -> Result<(StatusCode, Data<ShiftResource>), AppError> {
    let row = service::open(&state, &session.row, count).await?;
    Ok((StatusCode::CREATED, Data(row.into())))
}

/// POST /till/shift/close -> `{data: shift}` with counted, expected and difference.
#[utoipa::path(
    post, path = "/workspace/{workspace}/till/shift/close", tag = "till",
    operation_id = "till_shift_close", summary = "Close the drawer with a count",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = CountRequest,
    responses(
        (status = 200, body = Envelope<ShiftResource>, description = "Closed; `difference` = counted − expected"),
        (status = 401, body = MessageBody, description = "No live till session"),
        (status = 404, body = MessageBody, description = "Workspace is not the session's"),
        (status = 409, body = MessageBody, description = "No shift is open on this register"),
        (status = 422, body = ValidationErrorBody, description = "Invalid count"),
    ),
    security(("till_session" = []))
)]
pub async fn close(
    State(state): State<AppState>,
    session: TillSession,
    ValidatedJson(count): ValidatedJson<CountRequest>,
) -> Result<Data<ShiftResource>, AppError> {
    Ok(Data(
        service::close(&state, &session.row, count).await?.into(),
    ))
}

/// POST /till/shift/count-quote -> `{data: {total, expected, difference}}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/till/shift/count-quote", tag = "till",
    operation_id = "till_shift_count_quote", summary = "Total a count and compare it with the drawer",
    description = "The till's live figures while the cashier counts: the count's total and, with a shift open, the expected cash and the difference, in minor units. Books nothing.",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = CountRequest,
    responses(
        (status = 200, body = Envelope<CountQuote>, description = "The count's figures"),
        (status = 401, body = MessageBody, description = "No live till session"),
        (status = 404, body = MessageBody, description = "Workspace is not the session's"),
        (status = 422, body = ValidationErrorBody, description = "Invalid count"),
    ),
    security(("till_session" = []))
)]
pub async fn count_quote(
    State(state): State<AppState>,
    session: TillSession,
    ValidatedJson(count): ValidatedJson<CountRequest>,
) -> Result<Data<CountQuote>, AppError> {
    Ok(Data(service::quote(&state, &session.row, count).await?))
}

/// POST /till/transfers -> 201 `{data: transfer}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/till/transfers", tag = "till",
    operation_id = "till_transfers_create", summary = "Move cash between drawer and vault",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = TransferRequest,
    responses(
        (status = 201, body = Envelope<TransferResource>, description = "Booked on both sides"),
        (status = 401, body = MessageBody, description = "No live till session"),
        (status = 404, body = MessageBody, description = "Workspace is not the session's"),
        (status = 409, body = MessageBody, description = "No shift is open on this register"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, unknown vault or other currency, or not enough cash at the source"),
    ),
    security(("till_session" = []))
)]
pub async fn transfer(
    State(state): State<AppState>,
    session: TillSession,
    ValidatedJson(input): ValidatedJson<TransferRequest>,
) -> Result<(StatusCode, Data<TransferResource>), AppError> {
    let body = service::transfer(&state, &session.row, input).await?;
    Ok((StatusCode::CREATED, Data(body)))
}

/// GET /till/vaults -> `{data:[...]}`.
#[utoipa::path(
    get, path = "/workspace/{workspace}/till/vaults", tag = "till",
    operation_id = "till_vaults_list", summary = "List vaults with balances",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    responses(
        (status = 200, body = Envelope<Vec<VaultResource>>, description = "Every vault by name"),
        (status = 403, body = MessageBody, description = "Inactive membership, or no `cashregister.list`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn list_vaults(
    State(state): State<AppState>,
    access: Can<CashList>,
) -> Result<Data<Vec<VaultResource>>, AppError> {
    let rows = service::list_vaults(&state, access.workspace_id).await?;
    Ok(Data(rows.into_iter().map(Into::into).collect()))
}

/// POST /till/vaults -> 201 `{data: vault}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/till/vaults", tag = "till",
    operation_id = "till_vaults_create", summary = "Add a vault",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = CreateVaultRequest,
    responses(
        (status = 201, body = Envelope<VaultResource>, description = "Added, empty"),
        (status = 403, body = MessageBody, description = "Inactive membership, or no `cashregister.create`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    ),
    security(("bearer" = []))
)]
pub async fn create_vault(
    State(state): State<AppState>,
    access: Can<CashCreate>,
    ValidatedJson(input): ValidatedJson<CreateVaultRequest>,
) -> Result<(StatusCode, Data<VaultResource>), AppError> {
    let row = service::create_vault(&state, &access, input).await?;
    Ok((StatusCode::CREATED, Data(row.into())))
}

/// GET /till/shifts -> `{data:[...], meta}`.
#[utoipa::path(
    get, path = "/workspace/{workspace}/till/shifts", tag = "till",
    operation_id = "till_shifts_list", summary = "List drawer shifts",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ShiftQuery),
    responses(
        (status = 200, body = Paginated<ShiftResource>, description = "One page of shifts, newest first"),
        (status = 403, body = MessageBody, description = "Inactive membership, or no `cashregister.list`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Malformed `open`, `register_id`, `from` or `to`"),
    ),
    security(("bearer" = []))
)]
pub async fn list_shifts(
    State(state): State<AppState>,
    access: Can<CashList>,
    Query(query): Query<ShiftQuery>,
) -> Result<Json<Paginated<ShiftResource>>, AppError> {
    let page = Page::parse(query.page.as_deref(), query.page_size.as_deref());
    let f = query.filter()?;
    let (rows, total) =
        service::list_shifts(&state, access.workspace_id, &f, page.size, page.offset()).await?;
    let data = rows.into_iter().map(Into::into).collect();
    Ok(Json(Paginated::new(data, total, page)))
}

/// GET /accounting/cash-places -> `{data:[place]}`.
#[utoipa::path(
    get, path = "/workspace/{workspace}/accounting/cash-places", tag = "accounting",
    operation_id = "accounting_cash_places_list", summary = "Drawers and vaults with book cash",
    description = "Balance = reviewed adjustments + posted place movements, minor units. Counts \
        (shift open/close) are observations and never add cash.",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    responses(
        (status = 200, body = Envelope<Vec<CashPlaceResource>>, description = "Every drawer and vault"),
        (status = 403, body = MessageBody, description = "Missing `accounting.read` or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn cash_places(
    State(state): State<AppState>,
    access: Can<AccountingRead>,
) -> Result<Data<Vec<CashPlaceResource>>, AppError> {
    Ok(Data(places::list(&state, access.workspace_id).await?))
}

/// POST /accounting/cash-adjustments/review -> `{data: review}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/accounting/cash-adjustments/review", tag = "accounting",
    operation_id = "accounting_cash_adjustments_create_review",
    summary = "Preview a cash adjustment (no writes)",
    request_body = OfficeReviewRequest,
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    responses(
        (status = 200, body = Envelope<AdjustReviewResource>, description = "Expected, counted, difference, journal plan and fingerprint"),
        (status = 403, body = MessageBody, description = "Missing `cashregister.adjust` or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, nothing to adjust, or `unsupported_active_erp_operation`"),
    ),
    security(("bearer" = []))
)]
pub async fn office_adjust_review(
    State(state): State<AppState>,
    access: Can<CashAdjust>,
    ValidatedJson(input): ValidatedJson<OfficeReviewRequest>,
) -> Result<Data<AdjustReviewResource>, AppError> {
    Ok(Data(
        places::review(&state, access.workspace_id, &input).await?,
    ))
}

/// POST /accounting/cash-adjustments -> 201 `{data: adjustment}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/accounting/cash-adjustments", tag = "accounting",
    operation_id = "accounting_cash_adjustments_create", summary = "Post a reviewed cash adjustment",
    description = "Member authority. Moves the place by counted − expected in every book; in an \
        active book also posts cash_over_short/cash once.",
    request_body = Create<OfficeAdjustPayload>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 201, body = Envelope<CashAdjustmentResource>, description = "Posted"),
        (status = 403, body = MessageBody, description = "Missing `cashregister.adjust` or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or not a member"),
        (status = 409, body = MessageBody, description = "`stale_document` or reused client_ref"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, nothing to adjust, or `accounting_period_closed` / `unsupported_active_erp_operation`"),
    ),
    security(("bearer" = []))
)]
pub async fn office_adjust(
    State(state): State<AppState>,
    access: Can<CashAdjust>,
    cmd: Command<Create<OfficeAdjustPayload>>,
) -> Result<Answer, AppError> {
    let by = Approver {
        member: access.user_id,
        till_user_id: None,
    };
    places::adjust(
        &state,
        access.workspace_id,
        "accounting_cash_adjustments_create",
        by,
        cmd,
    )
    .await
}

/// Privileged Till context allowed to adjust its own drawer.
fn till_adjuster(p: &PrivilegedTill, place: Place) -> Result<Approver, AppError> {
    if !p.member.allows("cashregister.adjust") {
        return Err(AppError::Forbidden(NO_ADJUST));
    }
    match place {
        Place::Drawer(id) if id == p.till.register_id => Ok(Approver {
            member: p.member.user_id,
            till_user_id: Some(p.till.till_user_id),
        }),
        Place::Drawer(_) => Err(AppError::NotFound),
        Place::Vault(_) => Err(AppError::field("target_cash_place", OFFICE_ONLY)),
    }
}

/// POST /till/cash-adjustments/review -> `{data: review}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/till/cash-adjustments/review", tag = "till",
    operation_id = "till_cash_adjustments_create_review",
    summary = "Preview a drawer adjustment (no writes)",
    description = "Member bearer + `X-Till-Session`. Only the session's own drawer.",
    request_body = TillReviewRequest,
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    responses(
        (status = 200, body = Envelope<AdjustReviewResource>, description = "Expected, counted, difference, journal plan and fingerprint"),
        (status = 401, body = MessageBody, description = "No live member login or till session"),
        (status = 403, body = MessageBody, description = "Missing `cashregister.adjust`"),
        (status = 404, body = MessageBody, description = "Not a member, other workspace, or another register's drawer"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, a vault, nothing to adjust, or `unsupported_active_erp_operation`"),
    ),
    security(("bearer" = [], "till_privileged" = []))
)]
pub async fn till_adjust_review(
    State(state): State<AppState>,
    p: PrivilegedTill,
    ValidatedJson(input): ValidatedJson<TillReviewRequest>,
) -> Result<Data<AdjustReviewResource>, AppError> {
    till_adjuster(&p, input.place)?;
    Ok(Data(
        places::review(&state, p.member.workspace_id, &input).await?,
    ))
}

/// POST /till/cash-adjustments -> 201 `{data: adjustment}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/till/cash-adjustments", tag = "till",
    operation_id = "till_cash_adjustments_create", summary = "Post a reviewed drawer adjustment",
    description = "Member bearer + `X-Till-Session` (D14): a till PIN or member link never \
        counts. Accounting date = today in the book timezone.",
    request_body = Create<TillAdjustPayload>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 201, body = Envelope<CashAdjustmentResource>, description = "Posted"),
        (status = 401, body = MessageBody, description = "No live member login or till session"),
        (status = 403, body = MessageBody, description = "Missing `cashregister.adjust`"),
        (status = 404, body = MessageBody, description = "Not a member, other workspace, or another register's drawer"),
        (status = 409, body = MessageBody, description = "`stale_document` or reused client_ref"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, a vault, nothing to adjust, or `accounting_period_closed` / `unsupported_active_erp_operation`"),
    ),
    security(("bearer" = [], "till_privileged" = []))
)]
pub async fn till_adjust(
    State(state): State<AppState>,
    p: PrivilegedTill,
    cmd: Command<Create<TillAdjustPayload>>,
) -> Result<Answer, AppError> {
    let by = till_adjuster(&p, cmd.input.payload.review.place)?;
    places::adjust(
        &state,
        p.member.workspace_id,
        "till_cash_adjustments_create",
        by,
        cmd,
    )
    .await
}
