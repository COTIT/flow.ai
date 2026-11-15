//! HTTP edge for /v1/workspace/{workspace}/tax-profiles: `taxprofile.*` permission, call
//! service, wrap.

use axum::{
    Json,
    extract::{Path, Query, State},
};
use uuid::Uuid;

use crate::{
    AppState,
    error::{AppError, ValidationErrorBody},
    http::{
        list::{Page, Paginated},
        response::{Data, Envelope, MessageBody},
        workspace::{Can, permissions},
    },
    idempotency::{Answer, Command},
    modules::{
        accounting::dto::{Action, Create},
        tax_profiles::{
            dto::{
                TaxProfileCreate, TaxProfileListQuery, TaxProfilePatch, TaxProfileResource,
                TaxProfileReview, TaxProfileVerify,
            },
            service,
        },
    },
};

permissions! {
    Read => "taxprofile.read",
    Configure => "taxprofile.configure",
    Verify => "taxprofile.verify",
}

/// `{workspace}` + `{tax_profile}`; a malformed id is a 404.
type ProfilePath = Path<(String, String)>;

fn id(raw: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(raw).map_err(|_| AppError::NotFound)
}

/// tax_profiles_list: List tax profiles.
#[utoipa::path(
    get, path = "/workspace/{workspace}/tax-profiles", tag = "tax_profiles",
    operation_id = "tax_profiles_list", summary = "List tax profiles",
    params(("workspace" = Uuid, Path, description = "Workspace id"), TaxProfileListQuery),
    responses(
        (status = 200, body = Paginated<TaxProfileResource>, description = "One page, newest first"),
        (status = 403, body = MessageBody, description = "Missing `taxprofile.read` or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or not a member"),
        (status = 422, body = ValidationErrorBody, description = "Bad `country` or `state` filter"),
    ),
    security(("bearer" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    access: Can<Read>,
    Query(q): Query<TaxProfileListQuery>,
) -> Result<Json<Paginated<TaxProfileResource>>, AppError> {
    let (country, st) = q.filters()?;
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    let (rows, total) = service::list(
        &state,
        access.workspace_id,
        country.as_deref(),
        st.as_deref(),
        page,
    )
    .await?;
    Ok(Json(Paginated::new(rows, total, page)))
}

/// tax_profiles_create: Create a draft tax profile, or a correction of a verified one.
#[utoipa::path(
    post, path = "/workspace/{workspace}/tax-profiles", tag = "tax_profiles",
    operation_id = "tax_profiles_create",
    summary = "Create a draft tax profile, or a correction of a verified one",
    request_body = Create<TaxProfileCreate>,
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("Idempotency-Key" = String, Header, description = "Required; client retry identity")),
    responses(
        (status = 201, body = Envelope<TaxProfileResource>, description = "Created draft"),
        (status = 403, body = MessageBody, description = "Missing `taxprofile.configure` or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or not a member"),
        (status = 409, body = MessageBody, description = "Reused client_ref, or `tax_profile_correction_exists`"),
        (status = 422, body = ValidationErrorBody, description = "Invalid or unsupported input"),
    ),
    security(("bearer" = []))
)]
pub async fn create(
    State(state): State<AppState>,
    access: Can<Configure>,
    cmd: Command<Create<TaxProfileCreate>>,
) -> Result<Answer, AppError> {
    service::create(&state, &access, cmd).await
}

/// tax_profiles_get: Get a tax profile.
#[utoipa::path(
    get, path = "/workspace/{workspace}/tax-profiles/{tax_profile}", tag = "tax_profiles",
    operation_id = "tax_profiles_get", summary = "Get a tax profile",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("tax_profile" = Uuid, Path, description = "Tax profile id"),
    ),
    responses(
        (status = 200, body = Envelope<TaxProfileResource>, description = "The profile"),
        (status = 403, body = MessageBody, description = "Missing `taxprofile.read` or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or profile, or not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn get(
    State(state): State<AppState>,
    access: Can<Read>,
    Path((_, raw)): ProfilePath,
) -> Result<Data<TaxProfileResource>, AppError> {
    Ok(Data(
        service::get(&state, access.workspace_id, id(&raw)?).await?,
    ))
}

/// tax_profiles_update: Change a draft tax profile.
#[utoipa::path(
    patch, path = "/workspace/{workspace}/tax-profiles/{tax_profile}", tag = "tax_profiles",
    operation_id = "tax_profiles_update", summary = "Change a draft tax profile",
    request_body = Action<TaxProfilePatch>,
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("tax_profile" = Uuid, Path, description = "Tax profile id"),
        ("Idempotency-Key" = String, Header, description = "Required; client retry identity"),
    ),
    responses(
        (status = 200, body = Envelope<TaxProfileResource>, description = "Updated draft"),
        (status = 403, body = MessageBody, description = "Missing `taxprofile.configure` or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or profile, or not a member"),
        (status = 409, body = MessageBody, description = "`stale_document`, `tax_profile_verified`, or reused client_ref"),
        (status = 422, body = ValidationErrorBody, description = "Invalid or unsupported input"),
    ),
    security(("bearer" = []))
)]
pub async fn update(
    State(state): State<AppState>,
    access: Can<Configure>,
    Path((_, raw)): ProfilePath,
    cmd: Command<Action<TaxProfilePatch>>,
) -> Result<Answer, AppError> {
    service::update(&state, &access, id(&raw)?, cmd).await
}

/// tax_profiles_verify_review: Review a draft before verifying it.
#[utoipa::path(
    post, path = "/workspace/{workspace}/tax-profiles/{tax_profile}/verify/review", tag = "tax_profiles",
    operation_id = "tax_profiles_verify_review", summary = "Review a draft before verifying it",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("tax_profile" = Uuid, Path, description = "Tax profile id"),
    ),
    responses(
        (status = 200, body = Envelope<TaxProfileReview>, description = "Normalized profile and fingerprint"),
        (status = 403, body = MessageBody, description = "Missing `taxprofile.verify` or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or profile, or not a member"),
        (status = 409, body = MessageBody, description = "`tax_profile_verified`"),
    ),
    security(("bearer" = []))
)]
pub async fn verify_review(
    State(state): State<AppState>,
    access: Can<Verify>,
    Path((_, raw)): ProfilePath,
) -> Result<Data<TaxProfileReview>, AppError> {
    Ok(Data(
        service::review(&state, access.workspace_id, id(&raw)?).await?,
    ))
}

/// tax_profiles_verify: Verify a reviewed draft; it never changes afterwards.
#[utoipa::path(
    post, path = "/workspace/{workspace}/tax-profiles/{tax_profile}/verify", tag = "tax_profiles",
    operation_id = "tax_profiles_verify",
    summary = "Verify a reviewed draft; it never changes afterwards",
    request_body = Action<TaxProfileVerify>,
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("tax_profile" = Uuid, Path, description = "Tax profile id"),
        ("Idempotency-Key" = String, Header, description = "Required; client retry identity"),
    ),
    responses(
        (status = 200, body = Envelope<TaxProfileResource>, description = "Verified profile"),
        (status = 403, body = MessageBody, description = "Missing `taxprofile.verify` or inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or profile, or not a member"),
        (status = 409, body = MessageBody, description = "`stale_document`, `tax_profile_verified`, or reused client_ref"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, or `tax_profile_overlap`"),
    ),
    security(("bearer" = []))
)]
pub async fn verify(
    State(state): State<AppState>,
    access: Can<Verify>,
    Path((_, raw)): ProfilePath,
    cmd: Command<Action<TaxProfileVerify>>,
) -> Result<Answer, AppError> {
    service::verify(&state, &access, id(&raw)?, cmd).await
}
