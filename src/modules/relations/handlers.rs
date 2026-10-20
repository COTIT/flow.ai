//! HTTP edge for /v1/workspace/{workspace}/relations: `relation.*` permission (via `Can`),
//! call service, wrap. Reading one relation needs membership only (Laravel had no check).

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::Deserialize;

use crate::{
    AppState,
    error::AppError,
    http::{
        json::ValidatedJson,
        list::{Page, Paginated, flag},
        response::{Data, Message},
        workspace::{Can, WorkspaceAccess, permissions},
    },
    modules::relations::{
        dto::{
            CreateRelationRequest, RelationCreated, RelationIndexResource, RelationResource,
            UpdateRelationRequest,
        },
        service::{self, GetFlags, IndexQuery},
    },
};

permissions! {
    RelationCreate => "relation.create",
    RelationList => "relation.list",
    RelationUpdate => "relation.update",
    RelationDelete => "relation.delete",
}

/// `{workspace}` + `{relation}`; the id stays raw so a malformed one is a 404.
type RelationPath = Path<(String, String)>;

/// GET /relations/{id} query (Laravel GetRelationRequest). Unknown params ignored.
#[derive(Deserialize)]
pub struct GetQuery {
    slim: Option<String>,
    include_unpaid_invoices: Option<String>,
}

/// GET /relations/{id}/financial-summary query.
#[derive(Deserialize)]
pub struct SummaryQuery {
    lite: Option<String>,
}

/// POST /relations (and `/relations/`, which vimdesk-web and the POS call) -> 201
/// `{data:{relation_id}}`.
pub async fn create(
    State(state): State<AppState>,
    access: Can<RelationCreate>,
    ValidatedJson(input): ValidatedJson<CreateRelationRequest>,
) -> Result<(StatusCode, Data<RelationCreated>), AppError> {
    let relation_id = service::create(&state, access.workspace_id, access.user_id, input).await?;
    Ok((StatusCode::CREATED, Data(RelationCreated { relation_id })))
}

/// GET /relations/{relation}?slim=&include_unpaid_invoices= -> `{data: RelationResource}`.
pub async fn get(
    State(state): State<AppState>,
    access: WorkspaceAccess,
    Path((_, id)): RelationPath,
    Query(query): Query<GetQuery>,
) -> Result<Data<RelationResource>, AppError> {
    let flags = GetFlags {
        slim: query.slim.as_deref().is_some_and(flag),
        include_unpaid_invoices: query.include_unpaid_invoices.as_deref().is_none_or(flag),
    };
    Ok(Data(
        service::get(&state, access.workspace_id, &id, flags).await?,
    ))
}

/// GET /relations list query (Laravel ListRelationRequest + ListRelation). Unknown params
/// ignored: vimdesk-web spreads its route query into this call.
#[derive(Deserialize)]
pub struct ListQuery {
    q: Option<String>,
    page: Option<String>,
    limit: Option<String>,
    suppliers: Option<String>,
    include_advice: Option<String>,
    last_name: Option<String>,
    zipcode: Option<String>,
    company_name: Option<String>,
}

/// Laravel `limit` (default 30; 200 or more falls back to 50). Junk or <= 0 = 30.
fn relation_page(page: Option<&str>, limit: Option<&str>) -> Page {
    let size = limit
        .and_then(|l| l.trim().parse::<i64>().ok())
        .filter(|n| *n > 0)
        .map_or(30, |n| if n < 200 { n } else { 50 });
    Page {
        number: Page::parse(page, None).number,
        size,
    }
}

/// GET /relations -> `{data:[...], meta}`.
pub async fn list(
    State(state): State<AppState>,
    access: Can<RelationList>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Paginated<RelationIndexResource>>, AppError> {
    let on = |v: &Option<String>| v.as_deref().is_some_and(flag);
    let iq = IndexQuery {
        q: query.q.as_deref(),
        suppliers: on(&query.suppliers),
        include_advice: on(&query.include_advice),
        last_name: query.last_name.as_deref(),
        zipcode: query.zipcode.as_deref(),
        company_name: query.company_name.as_deref(),
        page: relation_page(query.page.as_deref(), query.limit.as_deref()),
    };
    Ok(Json(service::index(&state, access.workspace_id, iq).await?))
}

/// POST /relations/{relation} -> `{"message":"ok"}`.
pub async fn update(
    State(state): State<AppState>,
    access: Can<RelationUpdate>,
    Path((_, id)): RelationPath,
    ValidatedJson(patch): ValidatedJson<UpdateRelationRequest>,
) -> Result<Message, AppError> {
    service::update(&state, access.workspace_id, &id, patch).await?;
    Ok(Message("ok"))
}

/// DELETE /relations/{relation} -> `{"message":"ok"}`.
pub async fn delete(
    State(state): State<AppState>,
    access: Can<RelationDelete>,
    Path((_, id)): RelationPath,
) -> Result<Message, AppError> {
    service::delete(&state, access.workspace_id, &id).await?;
    Ok(Message("ok"))
}

/// GET /relations/{relation}/financial-summary?lite= (member only, as in Laravel).
pub async fn financial_summary(
    State(state): State<AppState>,
    access: WorkspaceAccess,
    Path((_, id)): RelationPath,
    Query(query): Query<SummaryQuery>,
) -> Result<Data<serde_json::Value>, AppError> {
    let lite = query.lite.as_deref().is_some_and(flag);
    Ok(Data(
        service::financial_summary(&state, access.workspace_id, &id, lite).await?,
    ))
}
