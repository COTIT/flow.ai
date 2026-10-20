//! HTTP edge for .../sections/{section}/rows/{row}/shelves. Reads take `warehouse.get`,
//! writes `warehouse.update` (Laravel's shelf update even checked `warehouse.update`).

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
    modules::warehouse_shelves::{
        dto::{CreateShelfRequest, ShelfCreated, ShelfResource, UpdateShelfRequest},
        service,
    },
};

permissions! {
    HierarchyRead => "warehouse.get",
    HierarchyWrite => "warehouse.update",
}

type RowPath = Path<(String, String, String, String)>;
type ShelfPath = Path<(String, String, String, String, String)>;

/// Paging params; unknown ones ignored.
#[derive(serde::Deserialize)]
pub struct ListQuery {
    page: Option<String>,
    page_size: Option<String>,
}

/// GET /shelves -> `{data:[...], meta}`.
pub async fn list(
    State(state): State<AppState>,
    access: Can<HierarchyRead>,
    Path((_, wh, s, r)): RowPath,
    Query(q): Query<ListQuery>,
) -> Result<Json<Paginated<ShelfResource>>, AppError> {
    let (_, row) = service::row(&state, access.workspace_id, (&wh, &s, &r)).await?;
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    let (rows, total) = service::list(
        &state,
        access.workspace_id,
        row.id,
        page.size,
        page.offset(),
    )
    .await?;
    let data = rows.into_iter().map(Into::into).collect();
    Ok(Json(Paginated::new(data, total, page)))
}

/// GET /shelves/{shelf}.
pub async fn get(
    State(state): State<AppState>,
    access: Can<HierarchyRead>,
    Path((_, wh, s, r, id)): ShelfPath,
) -> Result<Data<ShelfResource>, AppError> {
    let shelf = service::find(&state, access.workspace_id, (&wh, &s, &r), &id).await?;
    Ok(Data(shelf.into()))
}

/// POST /shelves -> 201 `{data:{shelf_id}}`.
pub async fn create(
    State(state): State<AppState>,
    access: Can<HierarchyWrite>,
    Path((_, wh, s, r)): RowPath,
    ValidatedJson(fields): ValidatedJson<CreateShelfRequest>,
) -> Result<(StatusCode, Data<ShelfCreated>), AppError> {
    let (section, row) = service::row(&state, access.workspace_id, (&wh, &s, &r)).await?;
    let shelf_id = service::create(&state, access.workspace_id, &section, &row, fields).await?;
    Ok((StatusCode::CREATED, Data(ShelfCreated { shelf_id })))
}

/// POST /shelves/{shelf} -> `{"message":"ok"}`.
pub async fn update(
    State(state): State<AppState>,
    access: Can<HierarchyWrite>,
    Path((_, wh, s, r, id)): ShelfPath,
    ValidatedJson(fields): ValidatedJson<UpdateShelfRequest>,
) -> Result<Message, AppError> {
    let (_, row) = service::row(&state, access.workspace_id, (&wh, &s, &r)).await?;
    service::update(&state, access.workspace_id, row.id, &id, fields).await?;
    Ok(Message("ok"))
}

/// DELETE /shelves/{shelf} -> `{"message":"ok"}`.
pub async fn delete(
    State(state): State<AppState>,
    access: Can<HierarchyWrite>,
    Path((_, wh, s, r, id)): ShelfPath,
) -> Result<Message, AppError> {
    let (_, row) = service::row(&state, access.workspace_id, (&wh, &s, &r)).await?;
    service::delete(&state, access.workspace_id, row.id, &id).await?;
    Ok(Message("ok"))
}
