//! HTTP edge for /v1/workspace/{workspace}/relations: `relation.*` permission (via `Can`),
//! call service, wrap. Reading one relation needs membership only (Laravel had no check).

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::Deserialize;

use crate::{
    AppState,
    error::AppError,
    http::{
        json::ValidatedJson,
        list::flag,
        response::Data,
        workspace::{Can, WorkspaceAccess, permissions},
    },
    modules::relations::{
        dto::{CreateRelationRequest, RelationCreated, RelationResource},
        service::{self, GetFlags},
    },
};

permissions! {
    RelationCreate => "relation.create",
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
