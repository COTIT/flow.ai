//! HTTP edge for /v1/workspace/{workspace}/relations: `relation.*` permission (via `Can`),
//! call service, wrap. Reading one relation needs membership only (Laravel had no check).

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};

use crate::{
    AppState,
    error::{AppError, ValidationErrorBody},
    http::{
        json::ValidatedJson,
        list::{Page, Paginated, flag},
        response::{Data, Envelope, Message, MessageBody},
        workspace::{Can, WorkspaceAccess, permissions},
    },
    modules::relations::{
        dto::{
            CreateRelationRequest, RelationCreated, RelationIndexResource, RelationInput,
            RelationPatchInput, RelationResource, UpdateRelationRequest,
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
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct GetQuery {
    /// Leave out `default_vat_rate` and the discount groups.
    #[param(value_type = Option<bool>)]
    slim: Option<String>,
    /// Default true; false leaves out `relationships.unpaid_invoices`.
    #[param(value_type = Option<bool>)]
    include_unpaid_invoices: Option<String>,
}

/// GET /relations/{id}/financial-summary query.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct SummaryQuery {
    /// Fewer keys (no order statistics).
    #[param(value_type = Option<bool>)]
    lite: Option<String>,
}

/// Placeholder money figures until invoices are ported; keys depend on `lite`.
// Doc-only: names the free-form summary object; never constructed.
#[derive(ToSchema)]
pub struct FinancialSummary {}

/// POST /relations (and `/relations/`, which vimdesk-web and the POS call) -> 201
/// `{data:{relation_id}}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/relations", tag = "relations",
    operation_id = "relations_create", summary = "Create a relation",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = RelationInput,
    responses(
        (status = 201, body = Envelope<RelationCreated>, description = "Created; returns the new id"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `relation.create`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, or unknown relation or source type"),
    ),
    security(("bearer" = []))
)]
pub async fn create(
    State(state): State<AppState>,
    access: Can<RelationCreate>,
    ValidatedJson(input): ValidatedJson<CreateRelationRequest>,
) -> Result<(StatusCode, Data<RelationCreated>), AppError> {
    let relation_id = service::create(&state, access.workspace_id, access.user_id, input).await?;
    Ok((StatusCode::CREATED, Data(RelationCreated { relation_id })))
}

/// GET /relations/{relation}?slim=&include_unpaid_invoices= -> `{data: RelationResource}`.
#[utoipa::path(
    get, path = "/workspace/{workspace}/relations/{relation}", tag = "relations",
    operation_id = "relations_get", summary = "Get a relation",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("relation" = Uuid, Path, description = "Relation id"),
        GetQuery,
    ),
    responses(
        (status = 200, body = Envelope<RelationResource>, description = "The relation"),
        (status = 403, body = MessageBody, description = "Membership is inactive"),
        (status = 404, body = MessageBody, description = "Unknown workspace or relation, or caller not a member"),
    ),
    security(("bearer" = []))
)]
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

// GET /relations list query (Laravel ListRelationRequest + ListRelation). Unknown params
// ignored: vimdesk-web spreads its route query into this call.
/// GET /relations list query.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListQuery {
    /// Search on name, email, phone, relation number and address.
    q: Option<String>,
    /// 1-based; junk falls back to 1.
    #[param(value_type = Option<i64>)]
    page: Option<String>,
    /// Page size, default 30; 200 or more gives 50.
    #[param(value_type = Option<i64>)]
    limit: Option<String>,
    /// Suppliers only (adds `open_orders`).
    #[param(value_type = Option<bool>)]
    suppliers: Option<String>,
    /// Add `has_advice` and `advice_count`.
    #[param(value_type = Option<bool>)]
    include_advice: Option<String>,
    /// Filter on a contact's last name.
    last_name: Option<String>,
    /// Filter on an address zipcode.
    zipcode: Option<String>,
    /// Filter on company name.
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
#[utoipa::path(
    get, path = "/workspace/{workspace}/relations", tag = "relations",
    operation_id = "relations_list", summary = "List relations",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ListQuery,
    ),
    responses(
        (status = 200, body = Paginated<RelationIndexResource>, description = "One page of relations"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `relation.list`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
    ),
    security(("bearer" = []))
)]
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
#[utoipa::path(
    post, path = "/workspace/{workspace}/relations/{relation}", tag = "relations",
    operation_id = "relations_update", summary = "Update a relation (`type` and `relation_type_id` always required)",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("relation" = Uuid, Path, description = "Relation id"),
    ),
    request_body = RelationPatchInput,
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `relation.update`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or relation, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, or unknown relation type"),
    ),
    security(("bearer" = []))
)]
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
#[utoipa::path(
    delete, path = "/workspace/{workspace}/relations/{relation}", tag = "relations",
    operation_id = "relations_delete", summary = "Delete a relation",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("relation" = Uuid, Path, description = "Relation id"),
    ),
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `relation.delete`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or relation, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Relation still has invoices (`errors.relation_id`)"),
    ),
    security(("bearer" = []))
)]
pub async fn delete(
    State(state): State<AppState>,
    access: Can<RelationDelete>,
    Path((_, id)): RelationPath,
) -> Result<Message, AppError> {
    service::delete(&state, access.workspace_id, &id).await?;
    Ok(Message("ok"))
}

/// GET /relations/{relation}/financial-summary?lite= (member only, as in Laravel).
#[utoipa::path(
    get, path = "/workspace/{workspace}/relations/{relation}/financial-summary", tag = "relations",
    operation_id = "relations_financial_summary", summary = "Get a relation's financial summary",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("relation" = Uuid, Path, description = "Relation id"),
        SummaryQuery,
    ),
    responses(
        (status = 200, body = Envelope<FinancialSummary>, description = "Summary figures"),
        (status = 403, body = MessageBody, description = "Membership is inactive"),
        (status = 404, body = MessageBody, description = "Unknown workspace or relation, or caller not a member"),
    ),
    security(("bearer" = []))
)]
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
