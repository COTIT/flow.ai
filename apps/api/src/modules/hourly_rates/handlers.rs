//! HTTP edge for /v1/workspace/{workspace}/hourly-rates: `hourlyrate.*` permission
//! (via `Can`), call service, wrap.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};

use crate::{
    AppState,
    error::{AppError, ValidationErrorBody},
    http::{
        json::ValidatedJson,
        list::{Page, Paginated, search},
        response::{Data, Envelope, Message, MessageBody},
        workspace::{Can, permissions},
    },
    modules::hourly_rates::{
        dto::{
            CreateHourlyRateRequest, HourlyRateCreated, HourlyRateResource, ListQuery,
            UpdateHourlyRateRequest,
        },
        service,
    },
};

// Laravel checked `hourlyrate.view` for one row; genesy uses the catalog's `get`.
permissions! {
    HourlyRateCreate => "hourlyrate.create",
    HourlyRateList => "hourlyrate.list",
    HourlyRateGet => "hourlyrate.get",
    HourlyRateUpdate => "hourlyrate.update",
    HourlyRateDelete => "hourlyrate.delete",
}

/// `{workspace}` + `{hourly_rate}`; the id stays raw so a malformed one is a 404.
type RatePath = Path<(String, String)>;

/// GET /hourly-rates -> `{data:[...], meta}`.
#[utoipa::path(
    get, path = "/workspace/{workspace}/hourly-rates", tag = "hourly_rates",
    operation_id = "hourly_rates_list", summary = "List hourly rates",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ListQuery,
    ),
    responses(
        (status = 200, body = Paginated<HourlyRateResource>, description = "One page of hourly rates"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `hourlyrate.list`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Search text too long"),
    ),
    security(("bearer" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    access: Can<HourlyRateList>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Paginated<HourlyRateResource>>, AppError> {
    let pattern = search(query.q.as_deref())?;
    let page = Page::parse(query.page.as_deref(), query.page_size.as_deref());
    let (rows, total) = service::list(
        &state,
        access.workspace_id,
        pattern.as_deref(),
        page.size,
        page.offset(),
    )
    .await?;
    let data = rows.into_iter().map(Into::into).collect();
    Ok(Json(Paginated::new(data, total, page)))
}

/// GET /hourly-rates/{hourly_rate}.
#[utoipa::path(
    get, path = "/workspace/{workspace}/hourly-rates/{hourly_rate}", tag = "hourly_rates",
    operation_id = "hourly_rates_get", summary = "Get an hourly rate",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("hourly_rate" = Uuid, Path, description = "Hourly rate id"),
    ),
    responses(
        (status = 200, body = Envelope<HourlyRateResource>, description = "The hourly rate"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `hourlyrate.get`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or hourly rate, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn get(
    State(state): State<AppState>,
    access: Can<HourlyRateGet>,
    Path((_, id)): RatePath,
) -> Result<Data<HourlyRateResource>, AppError> {
    Ok(Data(
        service::get(&state, access.workspace_id, &id).await?.into(),
    ))
}

/// POST /hourly-rates -> 201 `{data:{hourly_rate_id}}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/hourly-rates", tag = "hourly_rates",
    operation_id = "hourly_rates_create", summary = "Create an hourly rate",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = CreateHourlyRateRequest,
    responses(
        (status = 201, body = Envelope<HourlyRateCreated>, description = "Created; returns the new id"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `hourlyrate.create`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    ),
    security(("bearer" = []))
)]
pub async fn create(
    State(state): State<AppState>,
    access: Can<HourlyRateCreate>,
    ValidatedJson(input): ValidatedJson<CreateHourlyRateRequest>,
) -> Result<(StatusCode, Data<HourlyRateCreated>), AppError> {
    let hourly_rate_id = service::create(&state, access.workspace_id, input).await?;
    Ok((
        StatusCode::CREATED,
        Data(HourlyRateCreated { hourly_rate_id }),
    ))
}

/// POST /hourly-rates/{hourly_rate} -> `{"message":"ok"}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/hourly-rates/{hourly_rate}", tag = "hourly_rates",
    operation_id = "hourly_rates_update", summary = "Update an hourly rate (both prices always required)",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("hourly_rate" = Uuid, Path, description = "Hourly rate id"),
    ),
    request_body = UpdateHourlyRateRequest,
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `hourlyrate.update`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or hourly rate, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    ),
    security(("bearer" = []))
)]
pub async fn update(
    State(state): State<AppState>,
    access: Can<HourlyRateUpdate>,
    Path((_, id)): RatePath,
    ValidatedJson(patch): ValidatedJson<UpdateHourlyRateRequest>,
) -> Result<Message, AppError> {
    service::update(&state, access.workspace_id, &id, patch).await?;
    Ok(Message("ok"))
}

/// DELETE /hourly-rates/{hourly_rate} -> `{"message":"ok"}`.
#[utoipa::path(
    delete, path = "/workspace/{workspace}/hourly-rates/{hourly_rate}", tag = "hourly_rates",
    operation_id = "hourly_rates_delete", summary = "Delete an hourly rate",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("hourly_rate" = Uuid, Path, description = "Hourly rate id"),
    ),
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `hourlyrate.delete`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or hourly rate, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn delete(
    State(state): State<AppState>,
    access: Can<HourlyRateDelete>,
    Path((_, id)): RatePath,
) -> Result<Message, AppError> {
    service::delete(&state, access.workspace_id, &id).await?;
    Ok(Message("ok"))
}
