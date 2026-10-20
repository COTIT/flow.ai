//! HTTP edge for /v1/workspace/{workspace}/settings/vat-groups: `vatgroup.*` permission
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
    modules::vat_groups::{
        dto::{
            CreateVatGroupRequest, ListQuery, UpdateVatGroupRequest, VatGroupCreated,
            VatGroupResource,
        },
        repo::ListFilter,
        service,
    },
};

// Laravel checked `vatgroup.view` for one row; genesy uses the catalog's `get`.
permissions! {
    VatGroupCreate => "vatgroup.create",
    VatGroupList => "vatgroup.list",
    VatGroupGet => "vatgroup.get",
    VatGroupUpdate => "vatgroup.update",
    VatGroupDelete => "vatgroup.delete",
}

/// `{workspace}` + `{vat_group}`; the id stays raw so a malformed one is a 404.
type VatGroupPath = Path<(String, String)>;

/// GET /settings/vat-groups -> `{data:[...], meta}`.
pub async fn list(
    State(state): State<AppState>,
    access: Can<VatGroupList>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Paginated<VatGroupResource>>, AppError> {
    let pattern = search(query.q.as_deref())?;
    let country = query.country()?;
    let page = Page::parse(query.page.as_deref(), query.page_size.as_deref());
    let filter = ListFilter {
        country: country.as_deref(),
        pattern: pattern.as_deref(),
        limit: page.size,
        offset: page.offset(),
    };
    let (rows, total) = service::list(&state, access.workspace_id, &filter).await?;
    let data = rows.into_iter().map(Into::into).collect();
    Ok(Json(Paginated::new(data, total, page)))
}

/// GET /settings/vat-groups/{vat_group}.
pub async fn get(
    State(state): State<AppState>,
    access: Can<VatGroupGet>,
    Path((_, id)): VatGroupPath,
) -> Result<Data<VatGroupResource>, AppError> {
    Ok(Data(
        service::get(&state, access.workspace_id, &id).await?.into(),
    ))
}

/// POST /settings/vat-groups -> 201 `{data:{vat_group_id}}`.
pub async fn create(
    State(state): State<AppState>,
    access: Can<VatGroupCreate>,
    ValidatedJson(input): ValidatedJson<CreateVatGroupRequest>,
) -> Result<(StatusCode, Data<VatGroupCreated>), AppError> {
    let vat_group_id = service::create(&state, access.workspace_id, input).await?;
    Ok((StatusCode::CREATED, Data(VatGroupCreated { vat_group_id })))
}

/// POST /settings/vat-groups/{vat_group} -> `{"message":"ok"}`.
pub async fn update(
    State(state): State<AppState>,
    access: Can<VatGroupUpdate>,
    Path((_, id)): VatGroupPath,
    ValidatedJson(patch): ValidatedJson<UpdateVatGroupRequest>,
) -> Result<Message, AppError> {
    service::update(&state, access.workspace_id, &id, patch).await?;
    Ok(Message("ok"))
}

/// DELETE /settings/vat-groups/{vat_group} -> `{"message":"ok"}`.
pub async fn delete(
    State(state): State<AppState>,
    access: Can<VatGroupDelete>,
    Path((_, id)): VatGroupPath,
) -> Result<Message, AppError> {
    service::delete(&state, access.workspace_id, &id).await?;
    Ok(Message("ok"))
}
