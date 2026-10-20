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
        response::Data,
        workspace::{Can, WorkspaceAccess, permissions},
    },
    modules::relations::{
        dto::{
            CreateRelationRequest, IndexFlags, RelationCreated, RelationIndexResource,
            RelationResource,
        },
        repo::ListFilter,
        search,
        service::{self, GetFlags},
    },
};

permissions! {
    RelationCreate => "relation.create",
    RelationList => "relation.list",
}

/// `{workspace}` + `{relation}`; the id stays raw so a malformed one is a 404.
type RelationPath = Path<(String, String)>;

/// GET /relations/{id} query (Laravel GetRelationRequest). Unknown params ignored.
#[derive(Deserialize)]
pub struct GetQuery {
    slim: Option<String>,
    include_unpaid_invoices: Option<String>,
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
pub fn relation_page(page: Option<&str>, limit: Option<&str>) -> Page {
    let size = limit
        .and_then(|l| l.trim().parse::<i64>().ok())
        .filter(|n| *n > 0)
        .map_or(30, |n| if n < 200 { n } else { 50 });
    Page {
        number: Page::parse(page, None).number,
        size,
    }
}

/// Trimmed optional filter, capped like Laravel's request rules; None when blank.
fn filter(
    r: &mut crate::http::json::Rules,
    key: &str,
    v: Option<&str>,
    max: usize,
) -> Option<String> {
    let v = v.map(str::trim).filter(|v| !v.is_empty())?;
    r.max_chars(key, v, max);
    Some(v.to_owned())
}

/// Shared by GET /relations and `/searches?domain=relation`.
pub async fn page_of(
    state: &AppState,
    workspace_id: uuid::Uuid,
    q: Option<&str>,
    suppliers: bool,
    include_advice: bool,
    filters: [Option<&str>; 3],
    page: Page,
) -> Result<Paginated<RelationIndexResource>, AppError> {
    let mut r = crate::http::json::Rules::default();
    let last_name = filter(&mut r, "last_name", filters[0], 255).map(|l| l.to_lowercase());
    let zipcode =
        filter(&mut r, "zipcode", filters[1], 10).and_then(|z| search::compact_zipcode(&z));
    let company = filter(&mut r, "company_name", filters[2], 255).map(|c| c.to_lowercase());
    r.into_result()?;
    // `q` emptied by cleanup ("null undefined") answers an empty page unless a filter is set.
    let query = match search::parse(q)? {
        Some(query) => query,
        None if last_name.is_none() && zipcode.is_none() && company.is_none() => {
            return Ok(Paginated::new(Vec::new(), 0, page));
        }
        None => search::Query::default(),
    };
    let f = ListFilter {
        suppliers,
        last_name: last_name.as_deref(),
        zipcode: zipcode.as_deref(),
        company_name: company.as_deref(),
        q: &query,
        limit: page.size,
        offset: page.offset(),
    };
    let flags = IndexFlags {
        suppliers,
        include_advice,
    };
    let (data, total) = service::list(state, workspace_id, &f, flags).await?;
    Ok(Paginated::new(data, total, page))
}

/// GET /relations -> `{data:[...], meta}`.
pub async fn list(
    State(state): State<AppState>,
    access: Can<RelationList>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Paginated<RelationIndexResource>>, AppError> {
    let on = |v: &Option<String>| v.as_deref().is_some_and(flag);
    let page = relation_page(query.page.as_deref(), query.limit.as_deref());
    Ok(Json(
        page_of(
            &state,
            access.workspace_id,
            query.q.as_deref(),
            on(&query.suppliers),
            on(&query.include_advice),
            [
                query.last_name.as_deref(),
                query.zipcode.as_deref(),
                query.company_name.as_deref(),
            ],
            page,
        )
        .await?,
    ))
}
