//! HTTP edge for /v1/workspace/{workspace}/currency-rates. Reads and convert are open to every
//! active member; setting a rate needs `currency.update`.

use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
};

use crate::{
    AppState,
    error::{AppError, ValidationErrorBody},
    http::{
        json::{Rules, Validate, ValidatedJson},
        list::{Page, Paginated},
        response::{Data, Envelope, MessageBody},
        workspace::{Can, WorkspaceAccess, permissions},
    },
    modules::currency_rates::{
        dto::{
            Conversion, ConvertQuery, CreateRateRequest, ListQuery, RateCreated, RateResource,
            currency_id,
        },
        service,
    },
};

permissions! {
    CurrencyUpdate => "currency.update",
}

/// GET /currency-rates -> `{data:[...], meta}`.
#[utoipa::path(
    get, path = "/workspace/{workspace}/currency-rates", tag = "currency_rates",
    operation_id = "currency_rates_list",
    summary = "List the workspace's exchange rates (history, newest first)",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ListQuery),
    responses(
        (status = 200, body = Paginated<RateResource>, description = "One page of rates"),
        (status = 403, body = MessageBody, description = "Membership is inactive"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Malformed currency id"),
    ),
    security(("bearer" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    access: WorkspaceAccess,
    Query(query): Query<ListQuery>,
) -> Result<Json<Paginated<RateResource>>, AppError> {
    let mut r = Rules::default();
    let filter = |r: &mut Rules, field, v: Option<String>| {
        v.filter(|v| !v.trim().is_empty())
            .and_then(|v| currency_id(r, field, Some(v)))
    };
    let from = filter(&mut r, "from_currency_id", query.from_currency_id);
    let to = filter(&mut r, "to_currency_id", query.to_currency_id);
    r.into_result()?;
    let page = Page::parse(query.page.as_deref(), query.page_size.as_deref());
    let (rows, total) = service::list(
        &state,
        access.workspace_id,
        from,
        to,
        page.size,
        page.offset(),
    )
    .await?;
    let data = rows.into_iter().map(Into::into).collect();
    Ok(Json(Paginated::new(data, total, page)))
}

/// POST /currency-rates -> 201 `{data:{currency_rate_id}}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/currency-rates", tag = "currency_rates",
    operation_id = "currency_rates_create",
    summary = "Set an exchange rate for a currency pair from a day on (never edits old rates)",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = CreateRateRequest,
    responses(
        (status = 201, body = Envelope<RateCreated>, description = "Stored; returns the new id"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `currency.update`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, unknown currency or future date"),
    ),
    security(("bearer" = []))
)]
pub async fn create(
    State(state): State<AppState>,
    access: Can<CurrencyUpdate>,
    ValidatedJson(input): ValidatedJson<CreateRateRequest>,
) -> Result<(StatusCode, Data<RateCreated>), AppError> {
    let currency_rate_id =
        service::create(&state, access.workspace_id, access.user_id, input).await?;
    Ok((StatusCode::CREATED, Data(RateCreated { currency_rate_id })))
}

/// GET /currency-rates/convert -> `{data:{...}}`.
#[utoipa::path(
    get, path = "/workspace/{workspace}/currency-rates/convert", tag = "currency_rates",
    operation_id = "currency_rates_convert",
    summary = "Convert an amount to another currency at the workspace's own exchange rate",
    description = "Read only. Uses the latest rate staff set on or before `on`; no market rates. \
                   Say the answer uses the workspace's own rate as of `rate_effective_on`. 422 when \
                   no rate exists for the pair (either direction).",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ConvertQuery),
    responses(
        (status = 200, body = Envelope<Conversion>, description = "The converted amount and the rate used"),
        (status = 403, body = MessageBody, description = "Membership is inactive"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, unknown currency or no rate"),
    ),
    security(("bearer" = []))
)]
pub async fn convert(
    State(state): State<AppState>,
    access: WorkspaceAccess,
    Query(query): Query<ConvertQuery>,
) -> Result<Data<Conversion>, AppError> {
    let mut r = Rules::default();
    let input = query.validate(&mut r);
    r.into_result()?;
    let input = input.ok_or(AppError::NotFound)?;
    Ok(Data(
        service::conversion(&state, access.workspace_id, input).await?,
    ))
}
