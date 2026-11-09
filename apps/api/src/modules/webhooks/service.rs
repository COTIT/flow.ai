//! Webhook endpoint rules: admins only, secret generated here, sealed, shown once; delivery
//! history per endpoint.

use base64::{Engine, engine::general_purpose::STANDARD};
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::workspace::WorkspaceAccess,
    modules::{
        auth::token::PlainSecret,
        webhooks::{
            dto::NewEndpointInput,
            repo::{self, DeliveryRow, EndpointPatch, EndpointRow, NewEndpoint},
        },
    },
};

/// Endpoints per workspace: each one costs a delivery job per event on the shared rail.
pub const MAX_ENDPOINTS: i64 = 10;

/// Standard Webhooks secret prefix.
pub const SECRET_PREFIX: &str = "whsec_";

/// Path the secret is sealed under (with the workspace), so a sealed value can't be moved.
pub fn seal_path(endpoint: Uuid) -> String {
    format!("webhook:{endpoint}")
}

/// Register an endpoint: the row and the plaintext secret. 503 without `CREDENTIALS_KEY`.
pub async fn create(
    state: &AppState,
    access: &WorkspaceAccess,
    input: NewEndpointInput,
) -> Result<(EndpointRow, PlainSecret), AppError> {
    let key = state.credentials.as_ref().ok_or(AppError::Unavailable(
        "Webhooks are not configured on this server.",
    ))?;
    // shortcut: two concurrent creates can pass the cap by one; add a lock if that matters.
    if repo::count(&state.db, access.workspace_id).await? >= MAX_ENDPOINTS {
        return Err(AppError::field(
            "url",
            "A workspace can have at most 10 webhook endpoints.",
        ));
    }
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| AppError::Internal(format!("os rng: {e}")))?;
    let secret = PlainSecret::new(format!("{SECRET_PREFIX}{}", STANDARD.encode(bytes)));
    zeroize::Zeroize::zeroize(&mut bytes);
    let id = Uuid::now_v7();
    let sealed = key.seal(access.workspace_id, &seal_path(id), secret.expose())?;
    let row = repo::insert(
        &state.db,
        &NewEndpoint {
            id,
            workspace_id: access.workspace_id,
            url: &input.url,
            events: &input.events,
            description: input.description.as_deref(),
            secret_sealed: &sealed,
            secret_key_id: key.id(),
            created_by: access.user_id,
        },
    )
    .await?;
    Ok((row, secret))
}

fn parse(id: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(id).map_err(|_| AppError::NotFound)
}

/// One endpoint of this workspace, else 404.
pub async fn get(state: &AppState, workspace_id: Uuid, id: &str) -> Result<EndpointRow, AppError> {
    repo::find(&state.db, workspace_id, parse(id)?)
        .await?
        .ok_or(AppError::NotFound)
}

/// One page, newest first, with the total.
pub async fn list(
    state: &AppState,
    workspace_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<(Vec<EndpointRow>, i64), AppError> {
    Ok(repo::list(&state.db, workspace_id, limit, offset).await?)
}

/// One page of an endpoint's deliveries; an endpoint outside this workspace is 404.
pub async fn deliveries(
    state: &AppState,
    workspace_id: Uuid,
    id: &str,
    status: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<DeliveryRow>, i64), AppError> {
    let endpoint = get(state, workspace_id, id).await?;
    Ok(repo::deliveries(&state.db, workspace_id, endpoint.id, status, limit, offset).await?)
}

/// Apply a patch, else 404.
pub async fn update(
    state: &AppState,
    workspace_id: Uuid,
    id: &str,
    patch: EndpointPatch,
) -> Result<EndpointRow, AppError> {
    repo::update(&state.db, workspace_id, parse(id)?, &patch)
        .await?
        .ok_or(AppError::NotFound)
}

/// Delete, else 404.
pub async fn delete(state: &AppState, workspace_id: Uuid, id: &str) -> Result<(), AppError> {
    repo::delete(&state.db, workspace_id, parse(id)?)
        .await?
        .then_some(())
        .ok_or(AppError::NotFound)
}
