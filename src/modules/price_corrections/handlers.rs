//! HTTP edge for /v1/workspace/{workspace}/products/price-corrections (`pricecorrection.*`).

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
        list::{Page, Paginated},
        response::{Data, Message},
        workspace::{Can, permissions},
    },
    modules::price_corrections::{
        dto::{
            CorrectionCreated, CorrectionResource, CreateCorrectionRequest, ListQuery,
            UpdateCorrectionRequest,
        },
        service,
    },
};

permissions! {
    CorrectionList => "pricecorrection.list",
    CorrectionView => "pricecorrection.view",
    CorrectionCreate => "pricecorrection.create",
    CorrectionUpdate => "pricecorrection.update",
    CorrectionDelete => "pricecorrection.delete",
}

type CorrectionPath = Path<(String, String)>;

/// GET /price-corrections.
pub async fn list(
    State(state): State<AppState>,
    access: Can<CorrectionList>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Paginated<CorrectionResource>>, AppError> {
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    let (rows, total) =
        service::list(&state, access.workspace_id, page.size, page.offset()).await?;
    let data = rows.into_iter().map(Into::into).collect();
    Ok(Json(Paginated::new(data, total, page)))
}

/// GET /price-corrections/{id}.
pub async fn get(
    State(state): State<AppState>,
    access: Can<CorrectionView>,
    Path((_, id)): CorrectionPath,
) -> Result<Data<CorrectionResource>, AppError> {
    Ok(Data(
        service::get(&state, access.workspace_id, &id).await?.into(),
    ))
}

/// POST /price-corrections -> 201 `{data:{price_correction_id}}`.
pub async fn create(
    State(state): State<AppState>,
    access: Can<CorrectionCreate>,
    ValidatedJson(input): ValidatedJson<CreateCorrectionRequest>,
) -> Result<(StatusCode, Data<CorrectionCreated>), AppError> {
    let price_correction_id = service::create(&state, access.workspace_id, input).await?;
    Ok((
        StatusCode::CREATED,
        Data(CorrectionCreated {
            price_correction_id,
        }),
    ))
}

/// POST /price-corrections/{id}.
pub async fn update(
    State(state): State<AppState>,
    access: Can<CorrectionUpdate>,
    Path((_, id)): CorrectionPath,
    ValidatedJson(input): ValidatedJson<UpdateCorrectionRequest>,
) -> Result<Message, AppError> {
    service::update(&state, access.workspace_id, &id, input).await?;
    Ok(Message("ok"))
}

/// DELETE /price-corrections/{id}.
pub async fn delete(
    State(state): State<AppState>,
    access: Can<CorrectionDelete>,
    Path((_, id)): CorrectionPath,
) -> Result<Message, AppError> {
    service::delete(&state, access.workspace_id, &id).await?;
    Ok(Message("ok"))
}
