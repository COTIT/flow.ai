//! HTTP edge for .../shelves/{shelf}/bins and GET /warehouses/{warehouse}/bins. Reads take
//! `warehouse.get`, writes `warehouse.update` (no Laravel catalog names for bins).

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
    modules::warehouse_bins::{
        dto::{BinCreated, BinHierarchyResource, BinResource, CreateBinRequest, UpdateBinRequest},
        service,
    },
};

permissions! {
    HierarchyRead => "warehouse.get",
    HierarchyWrite => "warehouse.update",
}

type ShelfPath = Path<(String, String, String, String, String)>;
type BinPath = Path<(String, String, String, String, String, String)>;

/// Paging params; unknown ones ignored.
#[derive(serde::Deserialize)]
pub struct ListQuery {
    page: Option<String>,
    page_size: Option<String>,
}

/// GET /bins -> `{data:[...], meta}`.
pub async fn list(
    State(state): State<AppState>,
    access: Can<HierarchyRead>,
    Path((_, wh, s, r, sh)): ShelfPath,
    Query(q): Query<ListQuery>,
) -> Result<Json<Paginated<BinResource>>, AppError> {
    let shelf = service::shelf(&state, access.workspace_id, ((&wh, &s, &r), &sh)).await?;
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    let (rows, total) = service::list(
        &state,
        access.workspace_id,
        shelf.id,
        page.size,
        page.offset(),
    )
    .await?;
    let data = rows
        .into_iter()
        .map(|b| BinResource::new(b, false))
        .collect();
    Ok(Json(Paginated::new(data, total, page)))
}

/// GET /bins/{bin}, with `stocks`.
pub async fn get(
    State(state): State<AppState>,
    access: Can<HierarchyRead>,
    Path((_, wh, s, r, sh, id)): BinPath,
) -> Result<Data<BinResource>, AppError> {
    let bin = service::find(&state, access.workspace_id, ((&wh, &s, &r), &sh), &id).await?;
    Ok(Data(BinResource::new(bin, true)))
}

/// GET /warehouses/{warehouse}/bins -> `{data:[...]}` (not paginated, as in Laravel), plus
/// `meta.truncated` only when the cap cut the list.
pub async fn by_warehouse(
    State(state): State<AppState>,
    access: Can<HierarchyRead>,
    Path((_, wh)): Path<(String, String)>,
) -> Result<Json<WarehouseBins>, AppError> {
    let (rows, truncated) = service::of_warehouse(&state, access.workspace_id, &wh).await?;
    Ok(Json(WarehouseBins {
        data: rows.into_iter().map(Into::into).collect(),
        meta: truncated.then_some(Truncated { truncated: true }),
    }))
}

/// Body of `GET /warehouses/{warehouse}/bins`.
#[derive(serde::Serialize)]
pub struct WarehouseBins {
    data: Vec<BinHierarchyResource>,
    #[serde(skip_serializing_if = "Option::is_none")]
    meta: Option<Truncated>,
}

#[derive(serde::Serialize)]
struct Truncated {
    truncated: bool,
}

/// POST /bins -> 201 `{data:{bin_id}}`.
pub async fn create(
    State(state): State<AppState>,
    access: Can<HierarchyWrite>,
    Path((_, wh, s, r, sh)): ShelfPath,
    ValidatedJson(fields): ValidatedJson<CreateBinRequest>,
) -> Result<(StatusCode, Data<BinCreated>), AppError> {
    let shelf = service::shelf(&state, access.workspace_id, ((&wh, &s, &r), &sh)).await?;
    let bin_id = service::create(&state, &shelf, fields).await?;
    Ok((StatusCode::CREATED, Data(BinCreated { bin_id })))
}

/// POST /bins/{bin} -> `{"message":"ok"}`.
pub async fn update(
    State(state): State<AppState>,
    access: Can<HierarchyWrite>,
    Path((_, wh, s, r, sh, id)): BinPath,
    ValidatedJson(fields): ValidatedJson<UpdateBinRequest>,
) -> Result<Message, AppError> {
    let shelf = service::shelf(&state, access.workspace_id, ((&wh, &s, &r), &sh)).await?;
    service::update(&state, access.workspace_id, &shelf, &id, fields).await?;
    Ok(Message("ok"))
}

/// DELETE /bins/{bin} -> `{"message":"ok"}`.
pub async fn delete(
    State(state): State<AppState>,
    access: Can<HierarchyWrite>,
    Path((_, wh, s, r, sh, id)): BinPath,
) -> Result<Message, AppError> {
    let shelf = service::shelf(&state, access.workspace_id, ((&wh, &s, &r), &sh)).await?;
    service::delete(&state, access.workspace_id, &shelf, &id).await?;
    Ok(Message("ok"))
}
