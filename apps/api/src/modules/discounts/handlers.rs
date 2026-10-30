//! HTTP edge for /v1/workspace/{workspace}/products/discounts (`discount.*`).

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
    modules::discounts::{
        dto::{
            CreateDiscountRequest, DiscountCreated, DiscountResource, ListQuery,
            UpdateDiscountRequest,
        },
        service,
    },
};

permissions! {
    DiscountList => "discount.list",
    DiscountView => "discount.view",
    DiscountCreate => "discount.create",
    DiscountUpdate => "discount.update",
    DiscountDelete => "discount.delete",
}

type DiscountPath = Path<(String, String)>;

/// GET /discounts (`?discount_group_id=`).
pub async fn list(
    State(state): State<AppState>,
    access: Can<DiscountList>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Paginated<DiscountResource>>, AppError> {
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    let (rows, total) = service::list(
        &state,
        access.workspace_id,
        q.discount_group_id.as_deref(),
        page.size,
        page.offset(),
    )
    .await?;
    let data = rows.into_iter().map(Into::into).collect();
    Ok(Json(Paginated::new(data, total, page)))
}

/// GET /discounts/{id}.
pub async fn get(
    State(state): State<AppState>,
    access: Can<DiscountView>,
    Path((_, id)): DiscountPath,
) -> Result<Data<DiscountResource>, AppError> {
    Ok(Data(
        service::get(&state, access.workspace_id, &id).await?.into(),
    ))
}

/// POST /discounts -> 201 `{data:{discount_id}}`.
pub async fn create(
    State(state): State<AppState>,
    access: Can<DiscountCreate>,
    ValidatedJson(input): ValidatedJson<CreateDiscountRequest>,
) -> Result<(StatusCode, Data<DiscountCreated>), AppError> {
    let discount_id = service::create(&state, access.workspace_id, input).await?;
    Ok((StatusCode::CREATED, Data(DiscountCreated { discount_id })))
}

/// POST /discounts/{id}.
pub async fn update(
    State(state): State<AppState>,
    access: Can<DiscountUpdate>,
    Path((_, id)): DiscountPath,
    ValidatedJson(input): ValidatedJson<UpdateDiscountRequest>,
) -> Result<Message, AppError> {
    service::update(&state, access.workspace_id, &id, input).await?;
    Ok(Message("ok"))
}

/// DELETE /discounts/{id}.
pub async fn delete(
    State(state): State<AppState>,
    access: Can<DiscountDelete>,
    Path((_, id)): DiscountPath,
) -> Result<Message, AppError> {
    service::delete(&state, access.workspace_id, &id).await?;
    Ok(Message("ok"))
}
