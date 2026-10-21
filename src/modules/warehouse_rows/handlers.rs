//! HTTP edge for .../warehouses/{warehouse}/sections/{section}/rows. Reads take
//! `warehouse.get`, writes `warehouse.update` (no Laravel catalog names here).

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
    modules::warehouse_rows::{
        dto::{CreateRowRequest, RowCreated, RowResource, UpdateRowRequest},
        service,
    },
};

permissions! {
    HierarchyRead => "warehouse.get",
    HierarchyWrite => "warehouse.update",
}

type SectionPath = Path<(String, String, String)>;
type RowPath = Path<(String, String, String, String)>;

/// Paging params; unknown ones ignored.
#[derive(serde::Deserialize)]
pub struct ListQuery {
    page: Option<String>,
    page_size: Option<String>,
}

/// GET /rows -> `{data:[...], meta}`.
pub async fn list(
    State(state): State<AppState>,
    access: Can<HierarchyRead>,
    Path((_, wh, s)): SectionPath,
    Query(q): Query<ListQuery>,
) -> Result<Json<Paginated<RowResource>>, AppError> {
    let section = service::section(&state, access.workspace_id, &wh, &s).await?;
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    let (rows, total) = service::list(
        &state,
        access.workspace_id,
        section.id,
        page.size,
        page.offset(),
    )
    .await?;
    let ids: Vec<_> = rows.iter().map(|r| r.id).collect();
    let occupied = service::occupied(&state, access.workspace_id, &ids).await?;
    let data = rows
        .into_iter()
        .map(|r| {
            let n = occupied.get(&r.id).copied().unwrap_or_default();
            RowResource::new(r, access.workspace_id, &section, n)
        })
        .collect();
    Ok(Json(Paginated::new(data, total, page)))
}

/// GET /rows/{row}.
pub async fn get(
    State(state): State<AppState>,
    access: Can<HierarchyRead>,
    Path((_, wh, s, id)): RowPath,
) -> Result<Data<RowResource>, AppError> {
    let (section, row) = service::find(&state, access.workspace_id, (&wh, &s, &id)).await?;
    let occupied = service::occupied(&state, access.workspace_id, &[row.id]).await?;
    let n = occupied.get(&row.id).copied().unwrap_or_default();
    Ok(Data(RowResource::new(
        row,
        access.workspace_id,
        &section,
        n,
    )))
}

/// POST /rows -> 201 `{data:{row_id, warehouse_section_row_id}}`.
pub async fn create(
    State(state): State<AppState>,
    access: Can<HierarchyWrite>,
    Path((_, wh, s)): SectionPath,
    ValidatedJson(fields): ValidatedJson<CreateRowRequest>,
) -> Result<(StatusCode, Data<RowCreated>), AppError> {
    let section = service::section(&state, access.workspace_id, &wh, &s).await?;
    let id = service::create(&state, access.workspace_id, &section, fields).await?;
    Ok((
        StatusCode::CREATED,
        Data(RowCreated {
            row_id: id,
            warehouse_section_row_id: id,
        }),
    ))
}

/// POST /rows/{row} -> `{"message":"ok"}`.
pub async fn update(
    State(state): State<AppState>,
    access: Can<HierarchyWrite>,
    Path((_, wh, s, id)): RowPath,
    ValidatedJson(fields): ValidatedJson<UpdateRowRequest>,
) -> Result<Message, AppError> {
    let section = service::section(&state, access.workspace_id, &wh, &s).await?;
    service::update(&state, access.workspace_id, &section, &id, fields).await?;
    Ok(Message("ok"))
}

/// DELETE /rows/{row} -> `{"message":"ok"}`.
pub async fn delete(
    State(state): State<AppState>,
    access: Can<HierarchyWrite>,
    Path((_, wh, s, id)): RowPath,
) -> Result<Message, AppError> {
    let section = service::section(&state, access.workspace_id, &wh, &s).await?;
    service::delete(&state, access.workspace_id, &section, &id).await?;
    Ok(Message("ok"))
}
