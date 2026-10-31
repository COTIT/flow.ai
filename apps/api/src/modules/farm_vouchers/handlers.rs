//! HTTP edge for /v1/workspace/{workspace}/farm/vouchers and /farm/redemptions.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};

use crate::{
    AppState,
    error::{AppError, UnprocessableBody, ValidationErrorBody},
    http::{
        farm::FarmAccess,
        json::ValidatedJson,
        list::{Page, Paginated},
        response::{Data, Envelope, MessageBody},
    },
    modules::farm_vouchers::{
        dto::{
            DaysQuery, ListQuery, LookupRequest, LookupResult, MyVoucher, RedeemRequest, Redeemed,
            Redemption, RedemptionTotal, TrailEntry, VoidRequest,
        },
        service,
    },
};

#[utoipa::path(
    get, path = "/workspace/{workspace}/farm/vouchers/mine", tag = "farm_vouchers",
    operation_id = "farm_vouchers_mine", summary = "My household's vouchers, with codes",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ListQuery),
    responses(
        (status = 200, body = Paginated<MyVoucher>, description = "Newest first"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller has no farm role and is not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn mine(
    State(state): State<AppState>,
    access: FarmAccess,
    Query(q): Query<ListQuery>,
) -> Result<Json<Paginated<MyVoucher>>, AppError> {
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    let (rows, total) = service::mine(&state, &access, page.size, page.offset()).await?;
    Ok(Json(Paginated::new(
        rows.into_iter().map(Into::into).collect(),
        total,
        page,
    )))
}

#[utoipa::path(
    post, path = "/workspace/{workspace}/farm/vouchers/lookup", tag = "farm_vouchers",
    operation_id = "farm_vouchers_lookup", summary = "Scan a voucher code",
    description = "Field staff. Every scan is logged; a code of another village answers `found: false` and is logged as refused. At most 30 lookups a minute per account.",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = LookupRequest,
    responses(
        (status = 200, body = Envelope<LookupResult>, description = "What the code is, and whether the caller may redeem it"),
        (status = 403, body = MessageBody, description = "Caller is not field staff"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller has no farm role and is not a member"),
        (status = 422, body = ValidationErrorBody, description = "No code"),
    ),
    security(("bearer" = []))
)]
pub async fn lookup(
    State(state): State<AppState>,
    access: FarmAccess,
    ValidatedJson(code): ValidatedJson<LookupRequest>,
) -> Result<Data<LookupResult>, AppError> {
    Ok(Data(service::lookup(&state, &access, &code).await?))
}

#[utoipa::path(
    post, path = "/workspace/{workspace}/farm/vouchers/redeem", tag = "farm_vouchers",
    operation_id = "farm_vouchers_redeem", summary = "Redeem a voucher face to face",
    description = "Field staff of the voucher's village who neither registered nor verified the household and are not in it; a held voucher needs ops or admin. Record the ID document type checked. Repeating is safe.",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = RedeemRequest,
    responses(
        (status = 200, body = Envelope<Redeemed>, description = "Redeemed (or the caller's earlier redemption)"),
        (status = 403, body = MessageBody, description = "Caller is not field staff"),
        (status = 404, body = MessageBody, description = "No such voucher in the caller's villages"),
        (status = 422, body = UnprocessableBody, description = "Refused: the reason as written"),
    ),
    security(("bearer" = []))
)]
pub async fn redeem(
    State(state): State<AppState>,
    access: FarmAccess,
    ValidatedJson(input): ValidatedJson<RedeemRequest>,
) -> Result<Data<Redeemed>, AppError> {
    Ok(Data(service::redeem(&state, &access, input).await?))
}

#[utoipa::path(
    post, path = "/workspace/{workspace}/farm/vouchers/{voucher}/void", tag = "farm_vouchers",
    operation_id = "farm_vouchers_void", summary = "Void an unredeemed voucher",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("voucher" = Uuid, Path, description = "Voucher id")),
    request_body = VoidRequest,
    responses(
        (status = 204, description = "Voided"),
        (status = 403, body = MessageBody, description = "Caller is not ops or admin"),
        (status = 404, body = MessageBody, description = "No such voucher in the caller's villages"),
        (status = 422, body = MessageBody, description = "Already redeemed or void, or no reason"),
    ),
    security(("bearer" = []))
)]
pub async fn void(
    State(state): State<AppState>,
    access: FarmAccess,
    Path((_, id)): Path<(String, String)>,
    ValidatedJson(reason): ValidatedJson<VoidRequest>,
) -> Result<StatusCode, AppError> {
    service::void(&state, &access, &id, reason).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get, path = "/workspace/{workspace}/farm/vouchers/{voucher}/timeline", tag = "farm_vouchers",
    operation_id = "farm_vouchers_timeline", summary = "A voucher's trail",
    description = "Filtered by who asks: the household sees issued, answered, redeemed and voided; field staff of the village also see registration, verification, publication and scans; ops and admin see everything, refused scans and login history included.",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("voucher" = Uuid, Path, description = "Voucher id")),
    responses(
        (status = 200, body = Envelope<Vec<TrailEntry>>, description = "Oldest first"),
        (status = 404, body = MessageBody, description = "Unknown voucher, or not the caller's to see"),
    ),
    security(("bearer" = []))
)]
pub async fn timeline(
    State(state): State<AppState>,
    access: FarmAccess,
    Path((_, id)): Path<(String, String)>,
) -> Result<Data<Vec<TrailEntry>>, AppError> {
    let rows = service::timeline(&state, &access, &id).await?;
    Ok(Data(rows.into_iter().map(Into::into).collect()))
}

#[utoipa::path(
    get, path = "/workspace/{workspace}/farm/redemptions", tag = "farm_vouchers",
    operation_id = "farm_vouchers_redemptions", summary = "Redemption log for cash reconciliation",
    params(("workspace" = Uuid, Path, description = "Workspace id"), DaysQuery),
    responses(
        (status = 200, body = Paginated<Redemption>, description = "One page, newest first"),
        (status = 403, body = MessageBody, description = "Caller is not ops or admin"),
        (status = 422, body = ValidationErrorBody, description = "Missing or invalid dates"),
    ),
    security(("bearer" = []))
)]
pub async fn redemptions(
    State(state): State<AppState>,
    access: FarmAccess,
    Query(q): Query<DaysQuery>,
) -> Result<Json<Paginated<Redemption>>, AppError> {
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    let (rows, total) =
        service::log(&state, &access, q.from, q.to, page.size, page.offset()).await?;
    Ok(Json(Paginated::new(
        rows.into_iter().map(Into::into).collect(),
        total,
        page,
    )))
}

#[utoipa::path(
    get, path = "/workspace/{workspace}/farm/redemptions/totals", tag = "farm_vouchers",
    operation_id = "farm_vouchers_redemption_totals", summary = "Redemptions per officer per day",
    params(("workspace" = Uuid, Path, description = "Workspace id"), DaysQuery),
    responses(
        (status = 200, body = Envelope<Vec<RedemptionTotal>>, description = "Newest day first; sums in minor units"),
        (status = 403, body = MessageBody, description = "Caller is not ops or admin"),
        (status = 422, body = ValidationErrorBody, description = "Missing or invalid dates"),
    ),
    security(("bearer" = []))
)]
pub async fn totals(
    State(state): State<AppState>,
    access: FarmAccess,
    Query(q): Query<DaysQuery>,
) -> Result<Data<Vec<RedemptionTotal>>, AppError> {
    let rows = service::totals(&state, &access, q.from, q.to).await?;
    Ok(Data(rows.into_iter().map(Into::into).collect()))
}
