//! HTTP edge for /v1/workspace/{workspace}/erp/product-brands: `productbrand.*` permission
//! (Laravel checked `.view` for one row; genesy uses the catalog's `get`).

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
    modules::product_brands::{
        dto::{BrandCreated, BrandResource, CreateBrandRequest, ListQuery, UpdateBrandRequest},
        service,
    },
};

permissions! {
    BrandCreate => "productbrand.create",
    BrandList => "productbrand.list",
    BrandGet => "productbrand.get",
    BrandUpdate => "productbrand.update",
    BrandDelete => "productbrand.delete",
}

type BrandPath = Path<(String, String)>;

/// GET /product-brands -> `{data:[...], meta}`.
pub async fn list(
    State(state): State<AppState>,
    access: Can<BrandList>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Paginated<BrandResource>>, AppError> {
    let pattern = search(q.q.as_deref())?;
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
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

/// GET /product-brands/{brand}.
pub async fn get(
    State(state): State<AppState>,
    access: Can<BrandGet>,
    Path((_, id)): BrandPath,
) -> Result<Data<BrandResource>, AppError> {
    Ok(Data(
        service::get(&state, access.workspace_id, &id).await?.into(),
    ))
}

/// POST /product-brands -> 201 `{data:{product_brand_id}}`.
pub async fn create(
    State(state): State<AppState>,
    access: Can<BrandCreate>,
    ValidatedJson(input): ValidatedJson<CreateBrandRequest>,
) -> Result<(StatusCode, Data<BrandCreated>), AppError> {
    let product_brand_id = service::create(&state, access.workspace_id, input).await?;
    Ok((StatusCode::CREATED, Data(BrandCreated { product_brand_id })))
}

/// POST /product-brands/{brand} -> `{"message":"ok"}`.
pub async fn update(
    State(state): State<AppState>,
    access: Can<BrandUpdate>,
    Path((_, id)): BrandPath,
    ValidatedJson(input): ValidatedJson<UpdateBrandRequest>,
) -> Result<Message, AppError> {
    service::update(&state, access.workspace_id, &id, input).await?;
    Ok(Message("ok"))
}

/// DELETE /product-brands/{brand} -> `{"message":"ok"}`.
pub async fn delete(
    State(state): State<AppState>,
    access: Can<BrandDelete>,
    Path((_, id)): BrandPath,
) -> Result<Message, AppError> {
    service::delete(&state, access.workspace_id, &id).await?;
    Ok(Message("ok"))
}
