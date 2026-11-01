//! HTTP edge for /v1/workspace/{workspace}/farm/buyers, /buyer-demands and /opportunities.

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
    modules::farm_market::{
        dto::{
            AvailableQuery, BuyerDemandRequest, BuyerDemandResource, BuyerQuery, BuyerRequest,
            BuyerResource, DemandMatchResource, DemandQuery, HarvestAvailableResource,
            OpportunityQuery, OpportunityRequest, OpportunityResource, OpportunityStatusRequest,
            SupplyLineRequest, VillageSupplyResource,
        },
        service,
    },
};

/// 201 for a new row, 200 for a replay.
fn created(new: bool) -> StatusCode {
    if new {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    }
}

/// GET /farm/buyers.
#[utoipa::path(
    get, path = "/workspace/{workspace}/farm/buyers", tag = "farm_market",
    operation_id = "farm_market_buyers_list", summary = "List buyers",
    description = "ops or admin: the order book is not a farmer or officer surface.",
    params(("workspace" = Uuid, Path, description = "Workspace id"), BuyerQuery),
    responses(
        (status = 200, body = Paginated<BuyerResource>, description = "One page, by name"),
        (status = 403, body = MessageBody, description = "Caller is not ops or admin"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller has no farm role"),
    ),
    security(("bearer" = []))
)]
pub async fn list_buyers(
    State(state): State<AppState>,
    access: FarmAccess,
    Query(query): Query<BuyerQuery>,
) -> Result<Json<Paginated<BuyerResource>>, AppError> {
    let page = Page::parse(query.page.as_deref(), query.page_size.as_deref());
    let (rows, total) =
        service::buyers(&state, &access, query.active(), page.size, page.offset()).await?;
    let data = rows.into_iter().map(Into::into).collect();
    Ok(Json(Paginated::new(data, total, page)))
}

/// POST /farm/buyers.
#[utoipa::path(
    post, path = "/workspace/{workspace}/farm/buyers", tag = "farm_market",
    operation_id = "farm_market_buyers_create", summary = "Add a buyer",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = BuyerRequest,
    responses(
        (status = 200, body = Envelope<BuyerResource>, description = "Replayed: same id, same body"),
        (status = 201, body = Envelope<BuyerResource>, description = "Created"),
        (status = 403, body = MessageBody, description = "Caller is not ops or admin"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller has no farm role"),
        (status = 409, body = MessageBody, description = "The id holds a different buyer, or is used elsewhere"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input or name taken"),
    ),
    security(("bearer" = []))
)]
pub async fn create_buyer(
    State(state): State<AppState>,
    access: FarmAccess,
    ValidatedJson(input): ValidatedJson<BuyerRequest>,
) -> Result<(StatusCode, Data<BuyerResource>), AppError> {
    let (row, new) = service::create_buyer(&state, &access, input).await?;
    Ok((created(new), Data(row.into())))
}

/// GET /farm/buyer-demands.
#[utoipa::path(
    get, path = "/workspace/{workspace}/farm/buyer-demands", tag = "farm_market",
    operation_id = "farm_market_demands_list", summary = "List buyer demand",
    description = "ops or admin. By window start. Count open demand with `?status=open` and \
    `meta.total`.",
    params(("workspace" = Uuid, Path, description = "Workspace id"), DemandQuery),
    responses(
        (status = 200, body = Paginated<BuyerDemandResource>, description = "One page"),
        (status = 403, body = MessageBody, description = "Caller is not ops or admin"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller has no farm role"),
    ),
    security(("bearer" = []))
)]
pub async fn list_demands(
    State(state): State<AppState>,
    access: FarmAccess,
    Query(query): Query<DemandQuery>,
) -> Result<Json<Paginated<BuyerDemandResource>>, AppError> {
    let page = Page::parse(query.page.as_deref(), query.page_size.as_deref());
    let (rows, total) =
        service::demands(&state, &access, query.statuses(), page.size, page.offset()).await?;
    let data = rows.into_iter().map(Into::into).collect();
    Ok(Json(Paginated::new(data, total, page)))
}

/// GET /farm/buyer-demands/{demand}.
#[utoipa::path(
    get, path = "/workspace/{workspace}/farm/buyer-demands/{demand}", tag = "farm_market",
    operation_id = "farm_market_demands_get", summary = "Get a buyer demand",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("demand" = Uuid, Path, description = "Demand id"),
    ),
    responses(
        (status = 200, body = Envelope<BuyerDemandResource>, description = "The demand"),
        (status = 403, body = MessageBody, description = "Caller is not ops or admin"),
        (status = 404, body = MessageBody, description = "Unknown workspace or demand"),
    ),
    security(("bearer" = []))
)]
pub async fn get_demand(
    State(state): State<AppState>,
    access: FarmAccess,
    Path((_, id)): Path<(String, String)>,
) -> Result<Data<BuyerDemandResource>, AppError> {
    Ok(Data(service::demand(&state, &access, &id).await?.into()))
}

/// POST /farm/buyer-demands.
#[utoipa::path(
    post, path = "/workspace/{workspace}/farm/buyer-demands", tag = "farm_market",
    operation_id = "farm_market_demands_create", summary = "Record a buyer demand",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = BuyerDemandRequest,
    responses(
        (status = 200, body = Envelope<BuyerDemandResource>, description = "Replayed: same id, same body"),
        (status = 201, body = Envelope<BuyerDemandResource>, description = "Created"),
        (status = 403, body = MessageBody, description = "Caller is not ops or admin"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller has no farm role"),
        (status = 409, body = MessageBody, description = "The id holds a different demand, or is used elsewhere"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, or unknown buyer, crop or currency"),
    ),
    security(("bearer" = []))
)]
pub async fn create_demand(
    State(state): State<AppState>,
    access: FarmAccess,
    ValidatedJson(input): ValidatedJson<BuyerDemandRequest>,
) -> Result<(StatusCode, Data<BuyerDemandResource>), AppError> {
    let (row, new) = service::create_demand(&state, &access, input).await?;
    Ok((created(new), Data(row.into())))
}

/// POST /farm/buyer-demands/{demand}/opportunities.
#[utoipa::path(
    post, path = "/workspace/{workspace}/farm/buyer-demands/{demand}/opportunities", tag = "farm_market",
    operation_id = "farm_market_opportunities_create", summary = "Propose a village",
    description = "ops or admin (`only project ops or admin may write opportunities`, 403). \
    Refused with `an opportunity must start as proposed` or `opportunity must match the demand \
    crop and project` (422). One opportunity per demand and village.",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("demand" = Uuid, Path, description = "Demand id"),
    ),
    request_body = OpportunityRequest,
    responses(
        (status = 200, body = Envelope<OpportunityResource>, description = "Replayed: same id, same body"),
        (status = 201, body = Envelope<OpportunityResource>, description = "Proposed"),
        (status = 403, body = MessageBody, description = "Caller is not ops or admin"),
        (status = 404, body = MessageBody, description = "Unknown workspace or demand"),
        (status = 409, body = MessageBody, description = "The id holds a different opportunity, or is used elsewhere"),
        (status = 422, body = UnprocessableBody, description = "Invalid input, a RUAHA rule, or the village already has one"),
    ),
    security(("bearer" = []))
)]
pub async fn propose(
    State(state): State<AppState>,
    access: FarmAccess,
    Path((_, demand)): Path<(String, String)>,
    ValidatedJson(input): ValidatedJson<OpportunityRequest>,
) -> Result<(StatusCode, Data<OpportunityResource>), AppError> {
    let (row, new) = service::propose(&state, &access, &demand, input).await?;
    Ok((created(new), Data(row.into())))
}

/// GET /farm/opportunities.
#[utoipa::path(
    get, path = "/workspace/{workspace}/farm/opportunities", tag = "farm_market",
    operation_id = "farm_market_opportunities_list", summary = "List opportunities",
    description = "Staff: the opportunities of their villages (ops and admin: all), newest first.",
    params(("workspace" = Uuid, Path, description = "Workspace id"), OpportunityQuery),
    responses(
        (status = 200, body = Paginated<OpportunityResource>, description = "One page"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller has no farm role"),
        (status = 422, body = UnprocessableBody, description = "Malformed filter id"),
    ),
    security(("bearer" = []))
)]
pub async fn list_opportunities(
    State(state): State<AppState>,
    access: FarmAccess,
    Query(query): Query<OpportunityQuery>,
) -> Result<Json<Paginated<OpportunityResource>>, AppError> {
    let page = Page::parse(query.page.as_deref(), query.page_size.as_deref());
    let filter = query.filter()?;
    let (rows, total) =
        service::opportunities(&state, &access, filter, page.size, page.offset()).await?;
    let data = rows.into_iter().map(Into::into).collect();
    Ok(Json(Paginated::new(data, total, page)))
}

/// GET /farm/opportunities/{opportunity}.
#[utoipa::path(
    get, path = "/workspace/{workspace}/farm/opportunities/{opportunity}", tag = "farm_market",
    operation_id = "farm_market_opportunities_get", summary = "Get an opportunity",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("opportunity" = Uuid, Path, description = "Opportunity id"),
    ),
    responses(
        (status = 200, body = Envelope<OpportunityResource>, description = "The opportunity"),
        (status = 404, body = MessageBody, description = "Unknown, or not the caller's to see"),
    ),
    security(("bearer" = []))
)]
pub async fn get_opportunity(
    State(state): State<AppState>,
    access: FarmAccess,
    Path((_, id)): Path<(String, String)>,
) -> Result<Data<OpportunityResource>, AppError> {
    Ok(Data(
        service::opportunity(&state, &access, &id).await?.into(),
    ))
}

/// POST /farm/opportunities/{opportunity}/status.
#[utoipa::path(
    post, path = "/workspace/{workspace}/farm/opportunities/{opportunity}/status", tag = "farm_market",
    operation_id = "farm_market_opportunities_status", summary = "Move an opportunity",
    description = "ops or admin. Anything outside the machine is `illegal opportunity \
    transition {from} -> {to}` (422).",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("opportunity" = Uuid, Path, description = "Opportunity id"),
    ),
    request_body = OpportunityStatusRequest,
    responses(
        (status = 200, body = Envelope<OpportunityResource>, description = "Moved"),
        (status = 403, body = MessageBody, description = "Staff of the village who are not ops or admin"),
        (status = 404, body = MessageBody, description = "Unknown, or not the caller's to see"),
        (status = 422, body = UnprocessableBody, description = "Unknown status or illegal transition"),
    ),
    security(("bearer" = []))
)]
pub async fn set_status(
    State(state): State<AppState>,
    access: FarmAccess,
    Path((_, id)): Path<(String, String)>,
    ValidatedJson(to): ValidatedJson<OpportunityStatusRequest>,
) -> Result<Data<OpportunityResource>, AppError> {
    Ok(Data(
        service::set_status(&state, &access, &id, &to).await?.into(),
    ))
}

/// POST /farm/opportunities/{opportunity}/supply.
#[utoipa::path(
    post, path = "/workspace/{workspace}/farm/opportunities/{opportunity}/supply", tag = "farm_market",
    operation_id = "farm_market_supply_create", summary = "Attach supply to an opportunity",
    description = "ops or admin (`only project ops or admin may attach supply`, 403). Refused \
    (422) with `supply must reference a current, undeleted expected harvest report`, `supply \
    requires a live opportunity`, `supply must match the opportunity village, crop and harvest \
    cycle`, or `over-commitment: {available} kg available, {committed} kg already committed, \
    {requested} kg requested`. Returns the opportunity with its lines and re-summed offer.",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("opportunity" = Uuid, Path, description = "Opportunity id"),
    ),
    request_body = SupplyLineRequest,
    responses(
        (status = 200, body = Envelope<OpportunityResource>, description = "Replayed: same id, same line"),
        (status = 201, body = Envelope<OpportunityResource>, description = "Attached"),
        (status = 403, body = MessageBody, description = "Caller sees it but is not ops or admin"),
        (status = 404, body = MessageBody, description = "Unknown, or not the caller's to see"),
        (status = 409, body = MessageBody, description = "The id holds a different line, or is used elsewhere"),
        (status = 422, body = UnprocessableBody, description = "Invalid input or a RUAHA supply rule"),
    ),
    security(("bearer" = []))
)]
pub async fn attach(
    State(state): State<AppState>,
    access: FarmAccess,
    Path((_, id)): Path<(String, String)>,
    ValidatedJson(input): ValidatedJson<SupplyLineRequest>,
) -> Result<(StatusCode, Data<OpportunityResource>), AppError> {
    let (row, new) = service::attach(&state, &access, &id, input).await?;
    Ok((created(new), Data(row.into())))
}

/// GET /farm/harvest-available.
#[utoipa::path(
    get, path = "/workspace/{workspace}/farm/harvest-available", tag = "farm_market",
    operation_id = "farm_market_harvest_available", summary = "Harvest available to promise",
    description = "Current expected figures with committed and available kg. Staff read their \
    villages; a farmer reads their own cycles. Committed counts every figure of the cycle's \
    expected series on live opportunities.",
    params(("workspace" = Uuid, Path, description = "Workspace id"), AvailableQuery),
    responses(
        (status = 200, body = Paginated<HarvestAvailableResource>, description = "One page, by harvest start"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller has no farm role"),
        (status = 422, body = UnprocessableBody, description = "Malformed filter id"),
    ),
    security(("bearer" = []))
)]
pub async fn available(
    State(state): State<AppState>,
    access: FarmAccess,
    Query(query): Query<AvailableQuery>,
) -> Result<Json<Paginated<HarvestAvailableResource>>, AppError> {
    let page = Page::parse(query.page.as_deref(), query.page_size.as_deref());
    let (village, crop) = query.filter()?;
    let (rows, total) =
        service::available(&state, &access, village, crop, page.size, page.offset()).await?;
    let data = rows.into_iter().map(Into::into).collect();
    Ok(Json(Paginated::new(data, total, page)))
}

/// GET /farm/villages/{village}/supply.
#[utoipa::path(
    get, path = "/workspace/{workspace}/farm/villages/{village}/supply", tag = "farm_market",
    operation_id = "farm_market_village_supply", summary = "Village supply by crop and month",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("village" = Uuid, Path, description = "Village id"),
    ),
    responses(
        (status = 200, body = Envelope<Vec<VillageSupplyResource>>, description = "Rows by month"),
        (status = 403, body = MessageBody, description = "Caller is not staff"),
        (status = 404, body = MessageBody, description = "Unknown village, or not one of the caller's"),
    ),
    security(("bearer" = []))
)]
pub async fn village_supply(
    State(state): State<AppState>,
    access: FarmAccess,
    Path((_, village)): Path<(String, String)>,
) -> Result<Data<Vec<VillageSupplyResource>>, AppError> {
    let rows = service::village_supply(&state, &access, &village).await?;
    Ok(Data(rows.into_iter().map(Into::into).collect()))
}

/// GET /farm/buyer-demands/{demand}/matches.
#[utoipa::path(
    get, path = "/workspace/{workspace}/farm/buyer-demands/{demand}/matches", tag = "farm_market",
    operation_id = "farm_market_demand_matches", summary = "Villages that could cover a demand",
    description = "ops or admin. One row per village whose current expected harvests of the \
    demand's crop overlap its window; each harvest counted once. A comparison, not an allocation.",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("demand" = Uuid, Path, description = "Demand id"),
    ),
    responses(
        (status = 200, body = Envelope<Vec<DemandMatchResource>>, description = "One row per village"),
        (status = 403, body = MessageBody, description = "Caller is not ops or admin"),
        (status = 404, body = MessageBody, description = "Unknown workspace or demand"),
    ),
    security(("bearer" = []))
)]
pub async fn demand_matches(
    State(state): State<AppState>,
    access: FarmAccess,
    Path((_, demand)): Path<(String, String)>,
) -> Result<Data<Vec<DemandMatchResource>>, AppError> {
    let rows = service::demand_matches(&state, &access, &demand).await?;
    Ok(Data(rows.into_iter().map(Into::into).collect()))
}

/// GET /farm/villages/{village}/demand-matches.
#[utoipa::path(
    get, path = "/workspace/{workspace}/farm/villages/{village}/demand-matches", tag = "farm_market",
    operation_id = "farm_market_village_matches", summary = "Open demand a village could cover",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("village" = Uuid, Path, description = "Village id"),
    ),
    responses(
        (status = 200, body = Envelope<Vec<DemandMatchResource>>, description = "One row per open demand"),
        (status = 403, body = MessageBody, description = "Caller is not ops or admin"),
        (status = 404, body = MessageBody, description = "Unknown village"),
    ),
    security(("bearer" = []))
)]
pub async fn village_matches(
    State(state): State<AppState>,
    access: FarmAccess,
    Path((_, village)): Path<(String, String)>,
) -> Result<Data<Vec<DemandMatchResource>>, AppError> {
    let rows = service::village_matches(&state, &access, &village).await?;
    Ok(Data(rows.into_iter().map(Into::into).collect()))
}
