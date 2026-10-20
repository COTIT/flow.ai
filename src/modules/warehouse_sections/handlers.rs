//! HTTP edge for /v1/workspace/{workspace}/warehouses/{warehouse}/sections. Laravel had no
//! catalog names for the hierarchy, so reads take `warehouse.get` and writes
//! `warehouse.update` (user decision, docs/api/warehouses.md).

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};

use crate::{
    AppState,
    error::AppError,
    http::{
        json::{Rules, ValidatedJson},
        list::{Paginated, contains_pattern, flag},
        response::{Data, Message},
        workspace::{Can, permissions},
    },
    modules::warehouse_sections::{
        dto::{ListQuery, SectionBody, SectionCreated, SectionResource},
        repo::ListFilter,
        service,
    },
};

permissions! {
    HierarchyRead => "warehouse.get",
    HierarchyWrite => "warehouse.update",
}

type WarehousePath = Path<(String, String)>;
type SectionPath = Path<(String, String, String)>;

/// GET /sections -> `{data:[...], meta}`, sorted by name.
pub async fn list(
    State(state): State<AppState>,
    access: Can<HierarchyRead>,
    Path((_, wh)): WarehousePath,
    Query(q): Query<ListQuery>,
) -> Result<Json<Paginated<SectionResource>>, AppError> {
    let warehouse_id = service::warehouse(&state, access.workspace_id, &wh).await?;
    let page = q.page();
    let search = q.search.as_deref().map(str::trim).filter(|s| !s.is_empty());
    if let Some(s) = search {
        let mut r = Rules::default();
        r.max_chars("search", s, 255);
        r.into_result()?;
    }
    let pattern = search.map(contains_pattern);
    let filter = ListFilter {
        category: blank(&q.category),
        zone: blank(&q.zone),
        priority_level: blank(&q.priority_level),
        restricted_access: blank(&q.restricted_access).map(flag),
        search: pattern.as_deref(),
        limit: page.size,
        offset: page.offset(),
    };
    let (rows, total) = service::list(&state, access.workspace_id, warehouse_id, &filter).await?;
    let data = rows.into_iter().map(Into::into).collect();
    Ok(Json(Paginated::new(data, total, page)))
}

/// GET /sections/{section}.
pub async fn get(
    State(state): State<AppState>,
    access: Can<HierarchyRead>,
    Path((_, wh, id)): SectionPath,
) -> Result<Data<SectionResource>, AppError> {
    Ok(Data(
        service::find(&state, access.workspace_id, &wh, &id)
            .await?
            .into(),
    ))
}

/// POST /sections -> 201 `{data:{section_id}}`.
pub async fn create(
    State(state): State<AppState>,
    access: Can<HierarchyWrite>,
    Path((_, wh)): WarehousePath,
    ValidatedJson(input): ValidatedJson<SectionBody>,
) -> Result<(StatusCode, Data<SectionCreated>), AppError> {
    let warehouse_id = service::warehouse(&state, access.workspace_id, &wh).await?;
    let section_id = service::create(&state, access.workspace_id, warehouse_id, input).await?;
    Ok((StatusCode::CREATED, Data(SectionCreated { section_id })))
}

/// POST /sections/{section} -> `{"message":"ok"}`.
pub async fn update(
    State(state): State<AppState>,
    access: Can<HierarchyWrite>,
    Path((_, wh, id)): SectionPath,
    ValidatedJson(input): ValidatedJson<SectionBody>,
) -> Result<Message, AppError> {
    let warehouse_id = service::warehouse(&state, access.workspace_id, &wh).await?;
    service::update(&state, access.workspace_id, warehouse_id, &id, input).await?;
    Ok(Message("ok"))
}

/// DELETE /sections/{section} -> `{"message":"ok"}`.
pub async fn delete(
    State(state): State<AppState>,
    access: Can<HierarchyWrite>,
    Path((_, wh, id)): SectionPath,
) -> Result<Message, AppError> {
    let warehouse_id = service::warehouse(&state, access.workspace_id, &wh).await?;
    service::delete(&state, access.workspace_id, warehouse_id, &id).await?;
    Ok(Message("ok"))
}

/// Trimmed query value; blank = not sent.
fn blank(v: &Option<String>) -> Option<&str> {
    v.as_deref().map(str::trim).filter(|s| !s.is_empty())
}
