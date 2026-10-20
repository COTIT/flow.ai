//! HTTP edge for /v1/workspace/{workspace}/hourly-rates: `hourlyrate.*` permission
//! (via `Can`), call service, wrap.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};

use crate::{
    AppState,
    error::AppError,
    http::{
        json::ValidatedJson,
        list::{Page, Paginated, search},
        response::{Data, Message},
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
pub async fn delete(
    State(state): State<AppState>,
    access: Can<HourlyRateDelete>,
    Path((_, id)): RatePath,
) -> Result<Message, AppError> {
    service::delete(&state, access.workspace_id, &id).await?;
    Ok(Message("ok"))
}
