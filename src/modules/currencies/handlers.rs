//! HTTP edge for /v1/workspace/{workspace}/currencies. Reads are open to every active member
//! (vimdesk-web and the POS load the list at boot to format money); writes need `currency.*`.

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
        workspace::{Can, WorkspaceAccess, permissions},
    },
    modules::currencies::{
        dto::{
            CreateCurrencyRequest, CurrencyCreated, CurrencyResource, ListQuery,
            UpdateCurrencyRequest,
        },
        service,
    },
};

permissions! {
    CurrencyCreate => "currency.create",
    CurrencyUpdate => "currency.update",
    CurrencyDelete => "currency.delete",
}

/// `{workspace}` + `{currency}`; the id stays raw so a malformed one is a 404.
type CurrencyPath = Path<(String, String)>;

/// GET /currencies -> `{data:[...], meta}`.
pub async fn list(
    State(state): State<AppState>,
    access: WorkspaceAccess,
    Query(query): Query<ListQuery>,
) -> Result<Json<Paginated<CurrencyResource>>, AppError> {
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

/// GET /currencies/{currency}.
pub async fn get(
    State(state): State<AppState>,
    access: WorkspaceAccess,
    Path((_, id)): CurrencyPath,
) -> Result<Data<CurrencyResource>, AppError> {
    let row = service::get(&state, access.workspace_id, &id).await?;
    Ok(Data(row.into()))
}

/// POST /currencies -> 201 `{data:{currency_id}}`.
pub async fn create(
    State(state): State<AppState>,
    access: Can<CurrencyCreate>,
    ValidatedJson(input): ValidatedJson<CreateCurrencyRequest>,
) -> Result<(StatusCode, Data<CurrencyCreated>), AppError> {
    let currency_id = service::create(&state, access.workspace_id, input).await?;
    Ok((StatusCode::CREATED, Data(CurrencyCreated { currency_id })))
}

/// POST /currencies/{currency} -> `{"message":"ok"}`.
pub async fn update(
    State(state): State<AppState>,
    access: Can<CurrencyUpdate>,
    Path((_, id)): CurrencyPath,
    ValidatedJson(patch): ValidatedJson<UpdateCurrencyRequest>,
) -> Result<Message, AppError> {
    service::update(&state, access.workspace_id, &id, patch).await?;
    Ok(Message("ok"))
}

/// DELETE /currencies/{currency} -> `{"message":"ok"}`.
pub async fn delete(
    State(state): State<AppState>,
    access: Can<CurrencyDelete>,
    Path((_, id)): CurrencyPath,
) -> Result<Message, AppError> {
    service::delete(&state, access.workspace_id, &id).await?;
    Ok(Message("ok"))
}
