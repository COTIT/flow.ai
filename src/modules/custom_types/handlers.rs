//! HTTP edge for /v1/workspace/{workspace}/custom-types: `customtype.*` permission
//! (via `Can`), call service, wrap.

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
    modules::custom_types::{
        dto::{
            CreateCustomTypeRequest, CustomTypeCreated, CustomTypeResource, ListQuery,
            UpdateCustomTypeRequest, domain_filter, flag_filter,
        },
        repo::ListFilter,
        service,
    },
};

// The catalog has `customtype.view` (no `get`), and Laravel checked `view`.
permissions! {
    CustomTypeCreate => "customtype.create",
    CustomTypeList => "customtype.list",
    CustomTypeView => "customtype.view",
    CustomTypeUpdate => "customtype.update",
    CustomTypeDelete => "customtype.delete",
}

/// `{workspace}` + `{custom_type}`; the id stays raw so a malformed one is a 404.
type TypePath = Path<(String, String)>;

/// GET /custom-types -> `{data:[...], meta}`. `default_for_work_order` and
/// `is_leave_hours_type` only filter hour types, `is_supplier` only relation types.
pub async fn list(
    State(state): State<AppState>,
    access: Can<CustomTypeList>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Paginated<CustomTypeResource>>, AppError> {
    let pattern = search(query.q.as_deref())?;
    let domain = domain_filter(query.domain.as_deref())?;
    let hours = domain.as_deref() == Some("hour_registration_type");
    let relation = domain.as_deref() == Some("relation");
    let page = Page::parse(query.page.as_deref(), query.page_size.as_deref());
    let filter = ListFilter {
        domain: domain.as_deref(),
        pattern: pattern.as_deref(),
        default_for_work_order: flag_filter(query.default_for_work_order.as_deref(), hours),
        is_leave_hours_type: flag_filter(query.is_leave_hours_type.as_deref(), hours),
        is_supplier: flag_filter(query.is_supplier.as_deref(), relation),
        limit: page.size,
        offset: page.offset(),
    };
    let (rows, total) = service::list(&state, access.workspace_id, &filter).await?;
    let data = rows.into_iter().map(Into::into).collect();
    Ok(Json(Paginated::new(data, total, page)))
}

/// GET /custom-types/{custom_type}.
pub async fn get(
    State(state): State<AppState>,
    access: Can<CustomTypeView>,
    Path((_, id)): TypePath,
) -> Result<Data<CustomTypeResource>, AppError> {
    Ok(Data(
        service::get(&state, access.workspace_id, &id).await?.into(),
    ))
}

/// POST /custom-types -> 201 `{data:{custom_type_id}}`.
pub async fn create(
    State(state): State<AppState>,
    access: Can<CustomTypeCreate>,
    ValidatedJson(input): ValidatedJson<CreateCustomTypeRequest>,
) -> Result<(StatusCode, Data<CustomTypeCreated>), AppError> {
    let custom_type_id = service::create(&state, access.workspace_id, input).await?;
    Ok((
        StatusCode::CREATED,
        Data(CustomTypeCreated { custom_type_id }),
    ))
}

/// POST /custom-types/{custom_type} -> `{"message":"ok"}`.
pub async fn update(
    State(state): State<AppState>,
    access: Can<CustomTypeUpdate>,
    Path((_, id)): TypePath,
    ValidatedJson(patch): ValidatedJson<UpdateCustomTypeRequest>,
) -> Result<Message, AppError> {
    service::update(&state, access.workspace_id, &id, patch).await?;
    Ok(Message("ok"))
}

/// DELETE /custom-types/{custom_type} -> `{"message":"ok"}`.
pub async fn delete(
    State(state): State<AppState>,
    access: Can<CustomTypeDelete>,
    Path((_, id)): TypePath,
) -> Result<Message, AppError> {
    service::delete(&state, access.workspace_id, &id).await?;
    Ok(Message("ok"))
}
