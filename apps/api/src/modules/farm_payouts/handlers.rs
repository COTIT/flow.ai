//! HTTP edge for /v1/workspace/{workspace}/farm/payouts and /farm/cash.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};

use crate::{
    AppState,
    error::{AppError, ValidationErrorBody},
    http::{
        farm::FarmAccess,
        json::{Rules, ValidatedJson},
        list::{self, Page, Paginated},
        response::{Data, Envelope, MessageBody},
    },
    modules::farm_payouts::{
        dto::{
            CashDayResource, CashMovementRequest, CashMovementResource, CreatePayoutRequest,
            DayQuery, PayoutQuery, PayoutResource, VoidPayoutRequest,
        },
        service,
    },
};

/// POST /farm/payouts -> 201 paid, 200 replayed.
#[utoipa::path(
    post, path = "/workspace/{workspace}/farm/payouts", tag = "farm_payouts",
    operation_id = "farm_payouts_create", summary = "Pay a farmer",
    description = "Staff of the farmer's village. Whoever registered or verified the farmer's household, or belongs to it, may not pay. Above the workspace's `farm.payout_officer_limit` only ops or admin pay. Never more than the passbook balance in that currency. Writes the payout, a passbook debit and a cash ledger row in one transaction. The same `client_ref` and body replay (200).",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = CreatePayoutRequest,
    responses(
        (status = 200, body = Envelope<PayoutResource>, description = "Replayed: this client_ref was used with this body"),
        (status = 201, body = Envelope<PayoutResource>, description = "Paid"),
        (status = 403, body = MessageBody, description = "Not staff, four-eyes conflict, or above the officer limit"),
        (status = 404, body = MessageBody, description = "Unknown farmer, or outside the caller's villages or workspace"),
        (status = 409, body = MessageBody, description = "client_ref used with another body or by another member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, or more than the balance"),
    ),
    security(("bearer" = []))
)]
pub async fn pay(
    State(state): State<AppState>,
    access: FarmAccess,
    ValidatedJson(input): ValidatedJson<CreatePayoutRequest>,
) -> Result<(StatusCode, Data<PayoutResource>), AppError> {
    let (p, paid) = service::pay(&state, &access, input).await?;
    let status = if paid {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((status, Data(p.into())))
}

/// POST /farm/payouts/{payout}/void.
#[utoipa::path(
    post, path = "/workspace/{workspace}/farm/payouts/{payout}/void", tag = "farm_payouts",
    operation_id = "farm_payouts_void", summary = "Void a payout",
    description = "ops or admin, with a reason. Adds a passbook credit and a cash row giving the amount back to the payer; nothing is edited.",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("payout" = Uuid, Path, description = "Payout id"),
    ),
    request_body = VoidPayoutRequest,
    responses(
        (status = 200, body = Envelope<PayoutResource>, description = "Voided"),
        (status = 403, body = MessageBody, description = "Not ops or admin"),
        (status = 404, body = MessageBody, description = "Unknown payout"),
        (status = 409, body = MessageBody, description = "Already void"),
        (status = 422, body = ValidationErrorBody, description = "Reason missing or too long"),
    ),
    security(("bearer" = []))
)]
pub async fn void(
    State(state): State<AppState>,
    access: FarmAccess,
    Path((_, id)): Path<(String, String)>,
    ValidatedJson(reason): ValidatedJson<VoidPayoutRequest>,
) -> Result<Data<PayoutResource>, AppError> {
    Ok(Data(
        service::void(&state, &access, &id, reason).await?.into(),
    ))
}

/// POST /farm/cash/movements.
#[utoipa::path(
    post, path = "/workspace/{workspace}/farm/cash/movements", tag = "farm_payouts",
    operation_id = "farm_payouts_cash_movement", summary = "Record cash handed out, back, or counted",
    description = "ops or admin record `float_out` and `float_back`; an officer records their own `count` (ops and admin any). The staff user must be staff of the village.",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = CashMovementRequest,
    responses(
        (status = 201, body = Envelope<CashMovementResource>, description = "Recorded"),
        (status = 403, body = MessageBody, description = "Not allowed for this kind"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or no farm role"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    ),
    security(("bearer" = []))
)]
pub async fn move_cash(
    State(state): State<AppState>,
    access: FarmAccess,
    ValidatedJson(input): ValidatedJson<CashMovementRequest>,
) -> Result<(StatusCode, Data<CashMovementResource>), AppError> {
    let m = service::move_cash(&state, &access, input).await?;
    Ok((StatusCode::CREATED, Data(m)))
}

/// GET /farm/cash/reconciliation.
#[utoipa::path(
    get, path = "/workspace/{workspace}/farm/cash/reconciliation", tag = "farm_payouts",
    operation_id = "farm_payouts_reconciliation", summary = "Daily cash reconciliation",
    description = "Per officer and currency for one Tanzania day: opening, floats, payouts, voids, returns, expected, counted and variance. ops and admin see every officer; an officer sees their own.",
    params(("workspace" = Uuid, Path, description = "Workspace id"), DayQuery),
    responses(
        (status = 200, body = Envelope<Vec<CashDayResource>>, description = "One row per officer and currency"),
        (status = 403, body = MessageBody, description = "Not staff"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or no farm role"),
        (status = 422, body = ValidationErrorBody, description = "Missing or bad day"),
    ),
    security(("bearer" = []))
)]
pub async fn reconciliation(
    State(state): State<AppState>,
    access: FarmAccess,
    Query(q): Query<DayQuery>,
) -> Result<Data<Vec<CashDayResource>>, AppError> {
    let rows = service::reconciliation(&state, &access, q.day, q.village_id).await?;
    Ok(Data(rows.into_iter().map(Into::into).collect()))
}

/// GET /farm/payouts -> `{data:[...], meta}`.
#[utoipa::path(
    get, path = "/workspace/{workspace}/farm/payouts", tag = "farm_payouts",
    operation_id = "farm_payouts_list", summary = "List payouts",
    description = "Newest first. ops and admin see every payout, a field officer those of their villages, a farmer their own. A farmer naming another person is 403.",
    params(("workspace" = Uuid, Path, description = "Workspace id"), PayoutQuery),
    responses(
        (status = 200, body = Paginated<PayoutResource>, description = "One page"),
        (status = 403, body = MessageBody, description = "A farmer asking for another person's payouts"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or no farm role"),
        (status = 422, body = ValidationErrorBody, description = "Malformed `person_id`"),
    ),
    security(("bearer" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    access: FarmAccess,
    Query(query): Query<PayoutQuery>,
) -> Result<Json<Paginated<PayoutResource>>, AppError> {
    let page = Page::parse(query.page.as_deref(), query.page_size.as_deref());
    let mut r = Rules::default();
    let person = list::id(&mut r, "person_id", query.person_id.as_deref());
    r.into_result()?;
    let (rows, total) = service::list(&state, &access, person, page.size, page.offset()).await?;
    let data = rows.into_iter().map(Into::into).collect();
    Ok(Json(Paginated::new(data, total, page)))
}

/// GET /farm/payouts/{payout}.
#[utoipa::path(
    get, path = "/workspace/{workspace}/farm/payouts/{payout}", tag = "farm_payouts",
    operation_id = "farm_payouts_get", summary = "Get a payout",
    description = "Staff of the farmer's village, or the farmer paid.",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("payout" = Uuid, Path, description = "Payout id"),
    ),
    responses(
        (status = 200, body = Envelope<PayoutResource>, description = "The payout"),
        (status = 403, body = MessageBody, description = "Another farmer's payout"),
        (status = 404, body = MessageBody, description = "Unknown payout, or outside the caller's villages or workspace"),
    ),
    security(("bearer" = []))
)]
pub async fn get(
    State(state): State<AppState>,
    access: FarmAccess,
    Path((_, id)): Path<(String, String)>,
) -> Result<Data<PayoutResource>, AppError> {
    Ok(Data(service::get(&state, &access, &id).await?.into()))
}
