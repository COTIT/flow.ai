//! Owner/admin design HTTP boundary; app keys can read published designs only.

use super::{dto::*, preview_link, service};
use crate::{
    AppError, AppState,
    error::{UnprocessableBody, ValidationErrorBody},
    http::{
        app_key::AppKey,
        json::ValidatedJson,
        list::{Page, Paginated},
        response::{Data, Envelope, MessageBody},
        workspace::{Can, permissions},
    },
    idempotency::{Answer, Keyed},
    modules::app_keys::secret::CATALOG_READ,
};
use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde_json::Value;

permissions! {WorkspaceAdmin=>"workspace_admin"}

/// Read current draft state; ownership remains relational.
#[utoipa::path(get,path="/workspace/{workspace}/storefront/pages/home",tag="storefront_design",operation_id="storefront_design_get",params(("workspace"=uuid::Uuid,Path)),responses((status=200,body=Envelope<StorefrontDesignState>,description="Current draft state and optimistic page version"),(status=401,body=MessageBody,description="Missing, invalid or expired login or app key"),(status=403,body=MessageBody,description="Inactive member, missing workspace admin rights or catalog scope"),(status=404,body=MessageBody,description="Unknown workspace or revision, or resource outside caller scope")),security(("bearer"=[])))]
pub async fn get(
    State(state): State<AppState>,
    access: Can<WorkspaceAdmin>,
) -> Result<Data<StorefrontDesignState>, AppError> {
    Ok(Data(service::state(&state.db, access.workspace_id).await?))
}

/// Save new immutable revision; keyed retries replay original answer.
#[utoipa::path(post,path="/workspace/{workspace}/storefront/pages/home/revisions",tag="storefront_design",operation_id="storefront_design_save",params(("workspace"=uuid::Uuid,Path),("Idempotency-Key"=String,Header,description="Required retry key; reuse only with identical payload")),request_body=StorefrontDesignSave,responses((status=201,body=Envelope<StorefrontDesignState>,description="New immutable draft and updated page state"),(status=401,body=MessageBody,description="Missing, invalid or expired login or app key"),(status=403,body=MessageBody,description="Inactive member, missing workspace admin rights or catalog scope"),(status=404,body=MessageBody,description="Unknown workspace or revision, or resource outside caller scope"),(status=409,body=MessageBody,description="Stale page version or revision is not current unpublished draft"),(status=413,body=MessageBody,description="Request exceeds API body limit"),(status=422,body=UnprocessableBody,description="Invalid document, revision query, category binding or idempotency key")),security(("bearer"=[])))]
pub async fn save(
    State(state): State<AppState>,
    access: Can<WorkspaceAdmin>,
    Keyed { input, key }: Keyed<StorefrontDesignSave>,
) -> Result<Answer, AppError> {
    service::save(&state.db, &access, input, key).await
}

/// Read bounded metadata without exposing document copies in history rows.
#[utoipa::path(get,path="/workspace/{workspace}/storefront/pages/home/revisions",tag="storefront_design",operation_id="storefront_design_revisions",params(("workspace"=uuid::Uuid,Path),HistoryQuery),responses((status=200,body=Paginated<StorefrontDesignRevisionSummary>,description="One bounded page of revision metadata"),(status=401,body=MessageBody,description="Missing, invalid or expired login or app key"),(status=403,body=MessageBody,description="Inactive member, missing workspace admin rights or catalog scope"),(status=404,body=MessageBody,description="Unknown workspace or revision, or resource outside caller scope")),security(("bearer"=[])))]
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
#[utoipa::path(get,path="/workspace/{workspace}/storefront/pages/home/preview",tag="storefront_design",operation_id="storefront_design_preview",params(("workspace"=uuid::Uuid,Path),PreviewQuery),responses((status=200,body=Envelope<StorefrontDesignPreview>,description="Owned revision with public catalog collections and current page version"),(status=401,body=MessageBody,description="Missing, invalid or expired login or app key"),(status=403,body=MessageBody,description="Inactive member, missing workspace admin rights or catalog scope"),(status=404,body=MessageBody,description="Unknown workspace or revision, or resource outside caller scope"),(status=422,body=ValidationErrorBody,description="Missing or invalid revision_id, or stored revision no longer validates")),security(("bearer"=[])))]
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
#[utoipa::path(post,path="/workspace/{workspace}/storefront/pages/home/restore",tag="storefront_design",operation_id="storefront_design_restore",params(("workspace"=uuid::Uuid,Path),("Idempotency-Key"=String,Header,description="Required retry key; reuse only with identical payload")),request_body=StorefrontDesignSelect,responses((status=201,body=Envelope<StorefrontDesignState>,description="Historical document copied into a new draft"),(status=401,body=MessageBody,description="Missing, invalid or expired login or app key"),(status=403,body=MessageBody,description="Inactive member, missing workspace admin rights or catalog scope"),(status=404,body=MessageBody,description="Unknown workspace or revision, or resource outside caller scope"),(status=409,body=MessageBody,description="Stale page version or revision is not current unpublished draft"),(status=413,body=MessageBody,description="Request exceeds API body limit"),(status=422,body=UnprocessableBody,description="Invalid document, revision query, category binding or idempotency key")),security(("bearer"=[])))]
pub async fn restore(
    State(state): State<AppState>,
    access: Can<WorkspaceAdmin>,
    Keyed { input, key }: Keyed<StorefrontDesignSelect>,
) -> Result<Answer, AppError> {
    service::restore(&state.db, &access, input, key).await
}

/// Publish exactly current reviewed draft; atomically move pointer and append audit.
#[utoipa::path(post,path="/workspace/{workspace}/storefront/pages/home/publish",tag="storefront_design",operation_id="storefront_design_publish",params(("workspace"=uuid::Uuid,Path),("Idempotency-Key"=String,Header,description="Required retry key; reuse only with identical payload")),request_body=StorefrontDesignSelect,responses((status=200,body=Envelope<StorefrontDesignState>,description="Current draft published atomically"),(status=401,body=MessageBody,description="Missing, invalid or expired login or app key"),(status=403,body=MessageBody,description="Inactive member, missing workspace admin rights or catalog scope"),(status=404,body=MessageBody,description="Unknown workspace or revision, or resource outside caller scope"),(status=409,body=MessageBody,description="Stale page version or revision is not current unpublished draft"),(status=413,body=MessageBody,description="Request exceeds API body limit"),(status=422,body=UnprocessableBody,description="Invalid document, revision query, category binding or idempotency key")),security(("bearer"=[])))]
pub async fn publish(
    State(state): State<AppState>,
    access: Can<WorkspaceAdmin>,
    Keyed { input, key }: Keyed<StorefrontDesignSelect>,
) -> Result<Answer, AppError> {
    service::publish(&state.db, &access, input, key).await
}

/// Targeted changes to current draft saved as new draft; no whole-document writes.
#[utoipa::path(post,path="/workspace/{workspace}/storefront/pages/home/edits",tag="storefront_design",operation_id="storefront_design_edit",params(("workspace"=uuid::Uuid,Path),("Idempotency-Key"=String,Header,description="Required retry key; reuse only with identical payload")),request_body=StorefrontDesignEdit,responses((status=201,body=Envelope<StorefrontDesignState>,description="Edited copy of current draft saved as new draft"),(status=401,body=MessageBody,description="Missing, invalid or expired login or app key"),(status=403,body=MessageBody,description="Inactive member, missing workspace admin rights or catalog scope"),(status=404,body=MessageBody,description="Unknown workspace or revision, or resource outside caller scope"),(status=409,body=MessageBody,description="Stale page version or base is not current draft"),(status=413,body=MessageBody,description="Request exceeds API body limit"),(status=422,body=UnprocessableBody,description="Invalid change, resulting document, category binding or idempotency key")),security(("bearer"=[])))]
pub async fn edit(
    State(state): State<AppState>,
    access: Can<WorkspaceAdmin>,
    Keyed { input, key }: Keyed<StorefrontDesignEdit>,
) -> Result<Answer, AppError> {
    service::edit(&state.db, &access, input, key).await
}

/// Dry run of targeted edit with live collections and review rows; stores nothing.
#[utoipa::path(post,path="/workspace/{workspace}/storefront/pages/home/edits/preview",tag="storefront_design",operation_id="storefront_design_edit_preview",params(("workspace"=uuid::Uuid,Path)),request_body=StorefrontDesignEdit,responses((status=200,body=Envelope<StorefrontDesignEditPreview>,description="Edited document, public catalog collections and change rows; nothing stored"),(status=401,body=MessageBody,description="Missing, invalid or expired login or app key"),(status=403,body=MessageBody,description="Inactive member, missing workspace admin rights or catalog scope"),(status=404,body=MessageBody,description="Unknown workspace or revision, or resource outside caller scope"),(status=409,body=MessageBody,description="Stale page version or base is not current draft"),(status=413,body=MessageBody,description="Request exceeds API body limit"),(status=422,body=UnprocessableBody,description="Invalid change, resulting document or category binding")),security(("bearer"=[])))]
pub async fn edit_preview(
    State(state): State<AppState>,
    access: Can<WorkspaceAdmin>,
    ValidatedJson(input): ValidatedJson<StorefrontDesignEdit>,
) -> Result<Data<StorefrontDesignEditPreview>, AppError> {
    Ok(Data(
        service::edit_preview(&state, access.workspace_id, input).await?,
    ))
}

/// Introspect only reviewed component capabilities and Rust-generated document schema.
#[utoipa::path(get,path="/workspace/{workspace}/storefront/design-capabilities",tag="storefront_design",operation_id="storefront_design_capabilities",params(("workspace"=uuid::Uuid,Path)),responses((status=200,body=Envelope<Value>,description="Reviewed component registry and Rust-generated document schema"),(status=401,body=MessageBody,description="Missing, invalid or expired login or app key"),(status=403,body=MessageBody,description="Inactive member, missing workspace admin rights or catalog scope"),(status=404,body=MessageBody,description="Unknown workspace or revision, or resource outside caller scope")),security(("bearer"=[])))]
pub async fn capabilities(_access: Can<WorkspaceAdmin>) -> Result<Data<Value>, AppError> {
    Ok(Data(super::dto::capabilities()?))
}

/// Public design retrieval can follow published pointer only.
#[utoipa::path(get,path="/storefront/pages/home",tag="storefront_design",operation_id="storefront_design_published",responses((status=200,body=Envelope<Option<StorefrontPublishedDesign>>,description="Published revision and public catalog collections, or null before publication"),(status=401,body=MessageBody,description="Missing, invalid or expired login or app key"),(status=403,body=MessageBody,description="Inactive member, missing workspace admin rights or catalog scope")),security(("app_key"=[])))]
pub async fn published(
    State(state): State<AppState>,
    key: AppKey,
) -> Result<Data<Option<StorefrontPublishedDesign>>, AppError> {
    key.require(CATALOG_READ)?;
    Ok(Data(service::published(&state, key.workspace_id).await?))
}

/// Issue expiring full-page preview link for current unpublished draft; token shown once.
#[utoipa::path(post,path="/workspace/{workspace}/storefront/pages/home/preview-links",tag="storefront_design",operation_id="storefront_design_preview_link_create",params(("workspace"=uuid::Uuid,Path),("Idempotency-Key"=String,Header,description="Required retry key; a replay answers the same link with token null")),request_body=StorefrontPreviewLinkCreate,responses((status=201,body=Envelope<StorefrontPreviewLinkIssued>,description="New link; token present in this first answer only"),(status=401,body=MessageBody,description="Missing, invalid or expired login or app key"),(status=403,body=MessageBody,description="Inactive member, missing workspace admin rights or catalog scope"),(status=404,body=MessageBody,description="Unknown workspace or revision, or resource outside caller scope"),(status=409,body=MessageBody,description="Revision is not the current unpublished draft, or it has 5 active links"),(status=413,body=MessageBody,description="Request exceeds API body limit"),(status=422,body=ValidationErrorBody,description="Invalid revision id, hours outside 1 to 168, or missing idempotency key")),security(("bearer"=[])))]
pub async fn preview_link_create(
    State(state): State<AppState>,
    access: Can<WorkspaceAdmin>,
    Keyed { input, key }: Keyed<StorefrontPreviewLinkCreate>,
) -> Result<Answer, AppError> {
    preview_link::create(&state.db, &access, input, key).await
}

/// Newest 50 preview links of one revision with status; never tokens.
#[utoipa::path(get,path="/workspace/{workspace}/storefront/pages/home/preview-links",tag="storefront_design",operation_id="storefront_design_preview_links",params(("workspace"=uuid::Uuid,Path),PreviewLinksQuery),responses((status=200,body=Envelope<Vec<StorefrontPreviewLink>>,description="Links newest first; empty for another workspace's revision"),(status=401,body=MessageBody,description="Missing, invalid or expired login or app key"),(status=403,body=MessageBody,description="Inactive member, missing workspace admin rights or catalog scope"),(status=404,body=MessageBody,description="Unknown workspace"),(status=422,body=ValidationErrorBody,description="Missing or invalid revision_id")),security(("bearer"=[])))]
pub async fn preview_links(
    State(state): State<AppState>,
    access: Can<WorkspaceAdmin>,
    query: Result<Query<PreviewLinksQuery>, axum::extract::rejection::QueryRejection>,
) -> Result<Data<Vec<StorefrontPreviewLink>>, AppError> {
    let Query(query) =
        query.map_err(|_| AppError::field("revision_id", "A valid revision UUID is required."))?;
    Ok(Data(
        preview_link::list(&state.db, access.workspace_id, query.revision_id).await?,
    ))
}

/// Revoke preview link now; repeat answers same row.
#[utoipa::path(delete,path="/workspace/{workspace}/storefront/pages/home/preview-links/{link}",tag="storefront_design",operation_id="storefront_design_preview_link_revoke",params(("workspace"=uuid::Uuid,Path),("link"=uuid::Uuid,Path)),responses((status=200,body=Envelope<StorefrontPreviewLink>,description="Revoked link"),(status=401,body=MessageBody,description="Missing, invalid or expired login or app key"),(status=403,body=MessageBody,description="Inactive member, missing workspace admin rights or catalog scope"),(status=404,body=MessageBody,description="Unknown workspace or link, or link outside caller scope")),security(("bearer"=[])))]
pub async fn preview_link_revoke(
    State(state): State<AppState>,
    access: Can<WorkspaceAdmin>,
    Path((_, link)): Path<(uuid::Uuid, uuid::Uuid)>,
) -> Result<Data<StorefrontPreviewLink>, AppError> {
    Ok(Data(preview_link::revoke(&state.db, &access, link).await?))
}

/// Shop server resolves preview token for its own workspace; any dead or foreign token is 404.
#[utoipa::path(post,path="/storefront/preview",tag="storefront_design",operation_id="storefront_design_preview_resolve",request_body=StorefrontPreviewResolve,responses((status=200,body=Envelope<StorefrontPreviewDesign>,description="Unpublished revision with public catalog collections"),(status=401,body=MessageBody,description="Missing, invalid or expired login or app key"),(status=403,body=MessageBody,description="Inactive member, missing workspace admin rights or catalog scope"),(status=404,body=MessageBody,description="Unknown, expired, revoked, published or superseded link, or another workspace's link"),(status=422,body=ValidationErrorBody,description="Body without a token string"),(status=429,body=MessageBody,description="Too many preview resolves for this app key")),security(("app_key"=[])))]
pub async fn preview_resolve(
    State(state): State<AppState>,
    key: AppKey,
    ValidatedJson(input): ValidatedJson<StorefrontPreviewResolve>,
) -> Result<Data<StorefrontPreviewDesign>, AppError> {
    key.require(CATALOG_READ)?;
    Ok(Data(
        preview_link::resolve(&state, key.workspace_id, &input.token).await?,
    ))
}
