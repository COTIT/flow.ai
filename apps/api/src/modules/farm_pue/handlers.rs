//! HTTP edge for /v1/workspace/{workspace}/farm/pue-requests and the village energy reads.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};

use crate::{
    AppState,
    error::{AppError, UnprocessableBody},
    http::{
        farm::FarmAccess,
        json::ValidatedJson,
        list::{Page, Paginated},
        response::{Data, Envelope, MessageBody},
    },
    modules::farm_pue::{
        dto::{
            ListQuery, PuePipelineResource, PueRequestCreate, PueRequestResource,
            PueTransitionRequest, VillageEnergyResource,
        },
        service,
    },
};

/// GET /farm/pue-requests -> `{data:[...], meta}`.
#[utoipa::path(
    get, path = "/workspace/{workspace}/farm/pue-requests", tag = "farm_pue",
    operation_id = "farm_pue_requests_list", summary = "List PUE requests",
    description = "A farmer's own requests and every request of the caller's staff villages, \
    newest first. Count open ones with `?status=submitted,under_review` and `meta.total`.",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ListQuery),
    responses(
        (status = 200, body = Paginated<PueRequestResource>, description = "One page"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller has no farm role"),
        (status = 422, body = UnprocessableBody, description = "Malformed village id"),
    ),
    security(("bearer" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    access: FarmAccess,
    Query(query): Query<ListQuery>,
) -> Result<Json<Paginated<PueRequestResource>>, AppError> {
    let page = Page::parse(query.page.as_deref(), query.page_size.as_deref());
    let pick = query.pick(page.size, page.offset())?;
    let (rows, total) = service::list(&state, &access, &pick).await?;
    let data = rows.into_iter().map(Into::into).collect();
    Ok(Json(Paginated::new(data, total, page)))
}

/// GET /farm/pue-requests/{request}.
#[utoipa::path(
    get, path = "/workspace/{workspace}/farm/pue-requests/{request}", tag = "farm_pue",
    operation_id = "farm_pue_requests_get", summary = "Get a PUE request",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("request" = Uuid, Path, description = "Request id"),
    ),
    responses(
        (status = 200, body = Envelope<PueRequestResource>, description = "The request with its estimate"),
        (status = 404, body = MessageBody, description = "Unknown, or not the caller's to see"),
    ),
    security(("bearer" = []))
)]
pub async fn get(
    State(state): State<AppState>,
    access: FarmAccess,
    Path((_, id)): Path<(String, String)>,
) -> Result<Data<PueRequestResource>, AppError> {
    Ok(Data(service::get(&state, &access, &id).await?.into()))
}

/// POST /farm/pue-requests -> 201 created, 200 replayed.
#[utoipa::path(
    post, path = "/workspace/{workspace}/farm/pue-requests", tag = "farm_pue",
    operation_id = "farm_pue_requests_create", summary = "Request a machine",
    description = "Refused with `a request may only be created as draft or submitted`, \
    `this equipment is not offered for rent` or `this equipment is not offered for sale`.",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = PueRequestCreate,
    responses(
        (status = 200, body = Envelope<PueRequestResource>, description = "Replayed: same id, same body"),
        (status = 201, body = Envelope<PueRequestResource>, description = "Created, with its estimate"),
        (status = 403, body = MessageBody, description = "Caller has no person record and is not staff"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller has no farm role"),
        (status = 409, body = MessageBody, description = "The id holds a different request, or is used elsewhere"),
        (status = 422, body = UnprocessableBody, description = "Invalid input, or a RUAHA rule refused it"),
    ),
    security(("bearer" = []))
)]
pub async fn create(
    State(state): State<AppState>,
    access: FarmAccess,
    ValidatedJson(input): ValidatedJson<PueRequestCreate>,
) -> Result<(StatusCode, Data<PueRequestResource>), AppError> {
    let (row, new) = service::create(&state, &access, input).await?;
    let status = if new {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((status, Data(row.into())))
}

/// POST /farm/pue-requests/{request}/transitions.
#[utoipa::path(
    post, path = "/workspace/{workspace}/farm/pue-requests/{request}/transitions", tag = "farm_pue",
    operation_id = "farm_pue_requests_transition", summary = "Move a PUE request",
    description = "draft → submitted|withdrawn, submitted → under_review|withdrawn, \
    under_review → approved|rejected; anything else is `illegal transition {from} -> {to}`. \
    Review states need ops or admin (`only ops or admin may review a request`). Approve and \
    reject stamp `decided_at` and `decided_by` with the caller.",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("request" = Uuid, Path, description = "Request id"),
    ),
    request_body = PueTransitionRequest,
    responses(
        (status = 200, body = Envelope<PueRequestResource>, description = "Moved"),
        (status = 403, body = MessageBody, description = "Review states need ops or admin"),
        (status = 404, body = MessageBody, description = "Unknown, or not the caller's"),
        (status = 422, body = UnprocessableBody, description = "Unknown action or illegal transition"),
    ),
    security(("bearer" = []))
)]
pub async fn transition(
    State(state): State<AppState>,
    access: FarmAccess,
    Path((_, id)): Path<(String, String)>,
    ValidatedJson((to, note)): ValidatedJson<PueTransitionRequest>,
) -> Result<Data<PueRequestResource>, AppError> {
    let row = service::transition(&state, &access, &id, to, note.as_deref()).await?;
    Ok(Data(row.into()))
}

/// GET /farm/villages/{village}/energy.
#[utoipa::path(
    get, path = "/workspace/{workspace}/farm/villages/{village}/energy", tag = "farm_pue",
    operation_id = "farm_pue_village_energy", summary = "Village energy",
    description = "Staff of the village. Prospective (submitted, under review) and approved \
    load are never summed; peak = raw × simultaneity; headroom = capacity − approved peak.",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("village" = Uuid, Path, description = "Village id"),
    ),
    responses(
        (status = 200, body = Envelope<VillageEnergyResource>, description = "The village's load"),
        (status = 403, body = MessageBody, description = "Caller is not staff"),
        (status = 404, body = MessageBody, description = "Unknown village, or not one of the caller's"),
    ),
    security(("bearer" = []))
)]
pub async fn energy(
    State(state): State<AppState>,
    access: FarmAccess,
    Path((_, village)): Path<(String, String)>,
) -> Result<Data<VillageEnergyResource>, AppError> {
    let (id, row) = service::energy(&state, &access, &village).await?;
    Ok(Data(VillageEnergyResource::new(id, row)))
}

/// GET /farm/villages/{village}/pue-pipeline.
#[utoipa::path(
    get, path = "/workspace/{workspace}/farm/villages/{village}/pue-pipeline", tag = "farm_pue",
    operation_id = "farm_pue_village_pipeline", summary = "Village PUE pipeline",
    description = "Staff of the village. Requests per status and currency; buy requests add \
    price × quantity, rent requests rent per day × quantity. INDICATIVE, not financed value.",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("village" = Uuid, Path, description = "Village id"),
    ),
    responses(
        (status = 200, body = Envelope<Vec<PuePipelineResource>>, description = "Rows by status"),
        (status = 403, body = MessageBody, description = "Caller is not staff"),
        (status = 404, body = MessageBody, description = "Unknown village, or not one of the caller's"),
    ),
    security(("bearer" = []))
)]
pub async fn pipeline(
    State(state): State<AppState>,
    access: FarmAccess,
    Path((_, village)): Path<(String, String)>,
) -> Result<Data<Vec<PuePipelineResource>>, AppError> {
    let rows = service::pipeline(&state, &access, &village).await?;
    Ok(Data(rows.into_iter().map(Into::into).collect()))
}
