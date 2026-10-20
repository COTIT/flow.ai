//! HTTP edge for /v1/workspace/{workspace}/products: `product.*` permission (via `Can`), call
//! service, wrap. Laravel's barcode search had no check; genesy asks for `product.list`.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::{
        json::ValidatedJson,
        list::{Page, Paginated, search as pattern_of},
        response::Data,
        workspace::{Can, permissions},
    },
    modules::products::{
        dto::{CreateProductRequest, HistoryQuery, ListQuery, SearchBody, UpdateProductRequest},
        repo::ListFilter,
        service::{self, Actor},
    },
};

permissions! {
    ProductCreate => "product.create",
    ProductList => "product.list",
    ProductView => "product.view",
    ProductUpdate => "product.update",
    ProductDelete => "product.delete",
    ProductDuplicate => "product.duplicate",
}

type ProductPath = Path<(String, String)>;

/// Created / duplicated: `{data:{product_id}}`.
fn created(id: Uuid) -> (StatusCode, Data<Value>) {
    (StatusCode::CREATED, Data(json!({ "product_id": id })))
}

/// GET /products -> `{data:[...], meta}`, newest first, status `active` unless asked.
pub async fn list(
    State(state): State<AppState>,
    access: Can<ProductList>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Paginated<Value>>, AppError> {
    let pattern = pattern_of(q.q.as_deref().or(q.search.as_deref()))?;
    // Laravel: `limit`, else `page_size`; vimdesk-web and the RN app send `per_page`.
    let size = q
        .limit
        .as_deref()
        .or(q.per_page.as_deref())
        .or(q.page_size.as_deref());
    let page = Page::parse(q.page.as_deref(), size);
    let supplier = match q
        .supplier_id
        .as_deref()
        .or(q.supplier_relation_id.as_deref())
    {
        Some(s) if !s.trim().is_empty() => Some(Uuid::parse_str(s.trim()).map_err(|_| {
            AppError::field("supplier_id", "The supplier id must be a valid UUID.")
        })?),
        _ => None,
    };
    let exclude: Vec<Uuid> = q
        .exclude_ids
        .as_deref()
        .unwrap_or_default()
        .split(',')
        .filter_map(|s| Uuid::parse_str(s.trim()).ok())
        .collect();
    if exclude.len() > 100 {
        return Err(AppError::field(
            "exclude_ids",
            "The exclude ids must not have more than 100 items.",
        ));
    }
    let status = q
        .status
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("active");
    let ean13 = q.ean13.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let filter = ListFilter {
        status,
        pattern: pattern.as_deref(),
        supplier,
        ean13,
        exclude: &exclude,
        limit: page.size,
        offset: page.offset(),
    };
    let (data, total) = service::list(&state, access.workspace_id, &filter).await?;
    Ok(Json(Paginated::new(data, total, page)))
}

/// GET /products/{product}.
pub async fn get(
    State(state): State<AppState>,
    access: Can<ProductView>,
    Path((_, id)): ProductPath,
) -> Result<Data<Value>, AppError> {
    Ok(Data(service::get(&state, access.workspace_id, &id).await?))
}

/// POST /products/search `{query}` -> one product by barcode, EAN or id, else 404.
pub async fn search(
    State(state): State<AppState>,
    access: Can<ProductList>,
    ValidatedJson(query): ValidatedJson<SearchBody>,
) -> Result<Data<Value>, AppError> {
    Ok(Data(
        service::search(&state, access.workspace_id, &query).await?,
    ))
}

/// POST /products -> 201 `{data:{product_id}}`.
pub async fn create(
    State(state): State<AppState>,
    access: Can<ProductCreate>,
    ValidatedJson(fields): ValidatedJson<CreateProductRequest>,
) -> Result<(StatusCode, Data<Value>), AppError> {
    let actor = Actor {
        user_id: access.user_id,
        flow: "manual",
    };
    let id = service::create(&state, access.workspace_id, fields, actor).await?;
    Ok(created(id))
}

/// POST /products/{product} -> `{"message":"ok"}`.
pub async fn update(
    State(state): State<AppState>,
    access: Can<ProductUpdate>,
    Path((_, id)): ProductPath,
    ValidatedJson(fields): ValidatedJson<UpdateProductRequest>,
) -> Result<crate::http::response::Message, AppError> {
    let actor = Actor {
        user_id: access.user_id,
        flow: "manual",
    };
    service::update(&state, access.workspace_id, &id, fields, actor).await?;
    Ok(crate::http::response::Message("ok"))
}

/// DELETE /products/{product} -> `{"data":{"message":"The Product has been deleted"}}`.
pub async fn delete(
    State(state): State<AppState>,
    access: Can<ProductDelete>,
    Path((_, id)): ProductPath,
) -> Result<Data<Value>, AppError> {
    service::delete(&state, access.workspace_id, &id).await?;
    Ok(Data(json!({ "message": "The Product has been deleted" })))
}

/// POST /products/{product}/duplicate -> 201 `{data:{product_id}}`.
pub async fn duplicate(
    State(state): State<AppState>,
    access: Can<ProductDuplicate>,
    Path((_, id)): ProductPath,
) -> Result<(StatusCode, Data<Value>), AppError> {
    let id = service::duplicate(&state, access.workspace_id, &id, access.user_id).await?;
    Ok(created(id))
}

/// GET /products/{product}/price-history -> `{data:[...], meta}`; `per_page` default 25.
pub async fn price_history(
    State(state): State<AppState>,
    access: Can<ProductView>,
    Path((_, id)): ProductPath,
    Query(q): Query<HistoryQuery>,
) -> Result<Json<Paginated<Value>>, AppError> {
    let mut page = Page::parse(q.page.as_deref(), q.per_page.as_deref());
    if q.per_page
        .as_deref()
        .and_then(|s| s.trim().parse::<i64>().ok())
        .is_none_or(|n| n < 1)
    {
        page.size = 25;
    }
    let (data, total) = service::history(&state, access.workspace_id, &id, page).await?;
    Ok(Json(Paginated::new(data, total, page)))
}

/// GET /products/{product}/sale-statics: sales figures from invoices. No invoices exist until
/// the final batch, so the answer is Laravel's empty-result shape.
pub async fn sale_statics(
    State(state): State<AppState>,
    access: Can<ProductView>,
    Path((_, id)): ProductPath,
) -> Result<Data<Value>, AppError> {
    service::exists(&state, access.workspace_id, &id).await?;
    Ok(Data(json!({
        "units_sold": 0, "revenue": 0.0, "avg_monthly_sales": null, "last_sale": null,
        "customer_orders": 0, "last_order": null, "months_active": null,
    })))
}

/// GET /products/{product}/linked-products: links can't be created in genesy yet, so none.
pub async fn linked_products(
    State(state): State<AppState>,
    access: Can<ProductView>,
    Path((_, id)): ProductPath,
) -> Result<Data<Value>, AppError> {
    service::exists(&state, access.workspace_id, &id).await?;
    Ok(Data(json!({ "products": [] })))
}

/// POST /products/{product}/sync-product-stock-locations-from-external-service: Picqer isn't
/// ported, which is Laravel's "credentials not configured" case: `{"message":"ok"}`.
pub async fn sync_external(
    State(state): State<AppState>,
    access: Can<ProductView>,
    Path((_, id)): ProductPath,
) -> Result<crate::http::response::Message, AppError> {
    service::exists(&state, access.workspace_id, &id).await?;
    Ok(crate::http::response::Message("ok"))
}
