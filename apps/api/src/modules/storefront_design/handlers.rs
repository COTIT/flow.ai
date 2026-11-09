//! Owner/admin design HTTP boundary; app keys can read published designs only.

use super::{dto::*, service};
use crate::{
    AppError, AppState,
    error::{UnprocessableBody, ValidationErrorBody},
    http::{
        app_key::AppKey,
        list::{Page, Paginated},
        response::{Data, Envelope, MessageBody},
        workspace::{Can, permissions},
    },
    idempotency::{Answer, Keyed},
    modules::app_keys::secret::CATALOG_READ,
};
use axum::{
    Json,
    extract::{Query, State},
};
use serde_json::Value;

permissions! {WorkspaceAdmin=>"workspace_admin"}

/// Read current draft state; ownership remains relational.
#[utoipa::path(get,path="/workspace/{workspace}/storefront/pages/home",tag="storefront-design",operation_id="storefront_design_get",params(("workspace"=uuid::Uuid,Path)),responses((status=200,body=Envelope<StorefrontDesignState>,description="Current draft state and optimistic page version"),(status=401,body=MessageBody,description="Missing, invalid or expired login or app key"),(status=403,body=MessageBody,description="Inactive member, missing workspace admin rights or catalog scope"),(status=404,body=MessageBody,description="Unknown workspace or revision, or resource outside caller scope")),security(("bearer"=[])))]
pub async fn get(
    State(state): State<AppState>,
    access: Can<WorkspaceAdmin>,
) -> Result<Data<StorefrontDesignState>, AppError> {
    Ok(Data(service::state(&state.db, access.workspace_id).await?))
}

/// Save new immutable revision; keyed retries replay original answer.
#[utoipa::path(post,path="/workspace/{workspace}/storefront/pages/home/revisions",tag="storefront-design",operation_id="storefront_design_save",params(("workspace"=uuid::Uuid,Path),("Idempotency-Key"=String,Header,description="Required retry key; reuse only with identical payload")),request_body=StorefrontDesignSave,responses((status=201,body=Envelope<StorefrontDesignState>,description="New immutable draft and updated page state"),(status=401,body=MessageBody,description="Missing, invalid or expired login or app key"),(status=403,body=MessageBody,description="Inactive member, missing workspace admin rights or catalog scope"),(status=404,body=MessageBody,description="Unknown workspace or revision, or resource outside caller scope"),(status=409,body=MessageBody,description="Stale page version or revision is not current unpublished draft"),(status=413,body=MessageBody,description="Request exceeds API body limit"),(status=422,body=UnprocessableBody,description="Invalid document, revision query, category binding or idempotency key")),security(("bearer"=[])))]
pub async fn save(
    State(state): State<AppState>,
    access: Can<WorkspaceAdmin>,
    Keyed { input, key }: Keyed<StorefrontDesignSave>,
) -> Result<Answer, AppError> {
    service::save(&state.db, &access, input, key).await
}

/// Read bounded metadata without exposing document copies in history rows.
#[utoipa::path(get,path="/workspace/{workspace}/storefront/pages/home/revisions",tag="storefront-design",operation_id="storefront_design_revisions",params(("workspace"=uuid::Uuid,Path),HistoryQuery),responses((status=200,body=Paginated<StorefrontDesignRevisionSummary>,description="One bounded page of revision metadata"),(status=401,body=MessageBody,description="Missing, invalid or expired login or app key"),(status=403,body=MessageBody,description="Inactive member, missing workspace admin rights or catalog scope"),(status=404,body=MessageBody,description="Unknown workspace or revision, or resource outside caller scope")),security(("bearer"=[])))]
pub async fn revisions(
    State(state): State<AppState>,
    access: Can<WorkspaceAdmin>,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<Paginated<StorefrontDesignRevisionSummary>>, AppError> {
    let mut page = Page::parse(query.page.as_deref(), query.page_size.as_deref());
    page.size = page.size.min(50);
    let (rows, total) =
        service::history(&state.db, access.workspace_id, page.size, page.offset()).await?;
    Ok(Json(Paginated::new(rows, total, page)))
}

/// Materialize owned revision using same authoritative catalog as public Shop.
#[utoipa::path(get,path="/workspace/{workspace}/storefront/pages/home/preview",tag="storefront-design",operation_id="storefront_design_preview",params(("workspace"=uuid::Uuid,Path),PreviewQuery),responses((status=200,body=Envelope<StorefrontDesignPreview>,description="Owned revision with public catalog collections and current page version"),(status=401,body=MessageBody,description="Missing, invalid or expired login or app key"),(status=403,body=MessageBody,description="Inactive member, missing workspace admin rights or catalog scope"),(status=404,body=MessageBody,description="Unknown workspace or revision, or resource outside caller scope"),(status=422,body=ValidationErrorBody,description="Missing or invalid revision_id, or stored revision no longer validates")),security(("bearer"=[])))]
pub async fn preview(
    State(state): State<AppState>,
    access: Can<WorkspaceAdmin>,
    query: Result<Query<PreviewQuery>, axum::extract::rejection::QueryRejection>,
) -> Result<Data<StorefrontDesignPreview>, AppError> {
    let Query(query) =
        query.map_err(|_| AppError::field("revision_id", "A valid revision UUID is required."))?;
    Ok(Data(
        service::preview(&state, access.workspace_id, query.revision_id).await?,
    ))
}

/// Copy history into new draft; no implicit publication.
#[utoipa::path(post,path="/workspace/{workspace}/storefront/pages/home/restore",tag="storefront-design",operation_id="storefront_design_restore",params(("workspace"=uuid::Uuid,Path),("Idempotency-Key"=String,Header,description="Required retry key; reuse only with identical payload")),request_body=StorefrontDesignSelect,responses((status=201,body=Envelope<StorefrontDesignState>,description="Historical document copied into a new draft"),(status=401,body=MessageBody,description="Missing, invalid or expired login or app key"),(status=403,body=MessageBody,description="Inactive member, missing workspace admin rights or catalog scope"),(status=404,body=MessageBody,description="Unknown workspace or revision, or resource outside caller scope"),(status=409,body=MessageBody,description="Stale page version or revision is not current unpublished draft"),(status=413,body=MessageBody,description="Request exceeds API body limit"),(status=422,body=UnprocessableBody,description="Invalid document, revision query, category binding or idempotency key")),security(("bearer"=[])))]
pub async fn restore(
    State(state): State<AppState>,
    access: Can<WorkspaceAdmin>,
    Keyed { input, key }: Keyed<StorefrontDesignSelect>,
) -> Result<Answer, AppError> {
    service::restore(&state.db, &access, input, key).await
}

/// Publish exactly current reviewed draft; atomically move pointer and append audit.
#[utoipa::path(post,path="/workspace/{workspace}/storefront/pages/home/publish",tag="storefront-design",operation_id="storefront_design_publish",params(("workspace"=uuid::Uuid,Path),("Idempotency-Key"=String,Header,description="Required retry key; reuse only with identical payload")),request_body=StorefrontDesignSelect,responses((status=200,body=Envelope<StorefrontDesignState>,description="Current draft published atomically"),(status=401,body=MessageBody,description="Missing, invalid or expired login or app key"),(status=403,body=MessageBody,description="Inactive member, missing workspace admin rights or catalog scope"),(status=404,body=MessageBody,description="Unknown workspace or revision, or resource outside caller scope"),(status=409,body=MessageBody,description="Stale page version or revision is not current unpublished draft"),(status=413,body=MessageBody,description="Request exceeds API body limit"),(status=422,body=UnprocessableBody,description="Invalid document, revision query, category binding or idempotency key")),security(("bearer"=[])))]
pub async fn publish(
    State(state): State<AppState>,
    access: Can<WorkspaceAdmin>,
    Keyed { input, key }: Keyed<StorefrontDesignSelect>,
) -> Result<Answer, AppError> {
    service::publish(&state.db, &access, input, key).await
}

/// Introspect only reviewed component capabilities and Rust-generated document schema.
#[utoipa::path(get,path="/workspace/{workspace}/storefront/design-capabilities",tag="storefront-design",operation_id="storefront_design_capabilities",params(("workspace"=uuid::Uuid,Path)),responses((status=200,body=Envelope<Value>,description="Reviewed component registry and Rust-generated document schema"),(status=401,body=MessageBody,description="Missing, invalid or expired login or app key"),(status=403,body=MessageBody,description="Inactive member, missing workspace admin rights or catalog scope"),(status=404,body=MessageBody,description="Unknown workspace or revision, or resource outside caller scope")),security(("bearer"=[])))]
pub async fn capabilities(_access: Can<WorkspaceAdmin>) -> Result<Data<Value>, AppError> {
    Ok(Data(super::dto::capabilities()?))
}

/// Public design retrieval can follow published pointer only.
#[utoipa::path(get,path="/storefront/pages/home",tag="storefront-design",operation_id="storefront_design_published",responses((status=200,body=Envelope<Option<StorefrontPublishedDesign>>,description="Published revision and public catalog collections, or null before publication"),(status=401,body=MessageBody,description="Missing, invalid or expired login or app key"),(status=403,body=MessageBody,description="Inactive member, missing workspace admin rights or catalog scope")),security(("app_key"=[])))]
pub async fn published(
    State(state): State<AppState>,
    key: AppKey,
) -> Result<Data<Option<StorefrontPublishedDesign>>, AppError> {
    key.require(CATALOG_READ)?;
    Ok(Data(service::published(&state, key.workspace_id).await?))
}
