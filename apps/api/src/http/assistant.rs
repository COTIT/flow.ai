//! Signed bridge controls and durable admission before ordinary business handlers.

use crate::{
    AppError, AppState,
    http::{
        auth::AuthUser,
        json::{Validate, ValidatedJson},
    },
    modules::{
        assistant::{repo, service},
        permissions::repo as permissions,
    },
};
use axum::{
    Router,
    body::{Body, Bytes},
    extract::{FromRequest, FromRequestParts, OriginalUri, Request, State},
    http::{HeaderMap, request::Parts},
    middleware::{self, Next},
    response::{IntoResponse, Response},
};
use hmac::{Hmac, KeyInit, Mac};
use serde::de::DeserializeOwned;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{fmt, sync::Arc};
use uuid::Uuid;

/// Server-only key; Debug masked, allocation wiped on drop.
pub struct AssistantSecret(String);
impl AssistantSecret {
    /// At least 32 bytes, never echoing invalid secret.
    pub fn new(value: String) -> Result<Self, String> {
        if value.len() < 32 {
            Err("Assistant secrets must contain at least 32 bytes.".into())
        } else {
            Ok(Self(value))
        }
    }
    /// Secret only for signature/transport construction; never log or serialize.
    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}
impl fmt::Debug for AssistantSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AssistantSecret(***)")
    }
}
impl Drop for AssistantSecret {
    fn drop(&mut self) {
        zeroize::Zeroize::zeroize(&mut self.0);
    }
}

fn verify(
    secret: Option<&Arc<AssistantSecret>>,
    headers: &HeaderMap,
    method: &str,
    path: &str,
    body: &[u8],
) -> Result<(), AppError> {
    let secret = secret.ok_or(AppError::Unavailable("Assistant bridge is not configured."))?;
    let timestamp = headers
        .get("Assistant-Bridge-Timestamp")
        .and_then(|v| v.to_str().ok())
        .ok_or(AppError::Forbidden(
            "Trusted assistant bridge signature required.",
        ))?;
    let seconds = timestamp
        .parse::<i64>()
        .map_err(|_| AppError::Forbidden("Invalid assistant bridge signature."))?;
    if chrono::Utc::now().timestamp().abs_diff(seconds) > 60 {
        return Err(AppError::Forbidden("Expired assistant bridge signature."));
    }
    let signature = headers
        .get("Assistant-Bridge-Signature")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| hex::decode(v).ok())
        .ok_or(AppError::Forbidden("Invalid assistant bridge signature."))?;
    let digest = hex::encode(Sha256::digest(body));
    let message = format!("{timestamp}\n{method}\n{path}\n{digest}");
    let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(secret.expose().as_bytes())
        .map_err(|_| AppError::Internal("assistant bridge key failed".into()))?;
    mac.update(message.as_bytes());
    mac.verify_slice(&signature)
        .map_err(|_| AppError::Forbidden("Invalid assistant bridge signature."))
}

/// Signature-verified and normally validated JSON, preserving original body bytes.
pub struct SignedJson<T: Validate>(pub T::Valid);
impl<T> FromRequest<AppState> for SignedJson<T>
where
    T: DeserializeOwned + Validate,
{
    type Rejection = AppError;
    async fn from_request(req: Request, state: &AppState) -> Result<Self, AppError> {
        let (parts, body) = req.into_parts();
        let path = parts
            .extensions
            .get::<OriginalUri>()
            .map_or(&parts.uri, |u| &u.0)
            .to_string();
        let bytes = Bytes::from_request(Request::from_parts(parts.clone(), body), state)
            .await
            .map_err(|e| {
                if e.status() == axum::http::StatusCode::PAYLOAD_TOO_LARGE {
                    AppError::PayloadTooLarge
                } else if crate::http::timeout::is_body_timeout(&e) {
                    AppError::BodyTimeout
                } else {
                    AppError::field("body", "Invalid assistant request body.")
                }
            })?;
        verify(
            state.assistant_bridge_secret.as_ref(),
            &parts.headers,
            parts.method.as_str(),
            &path,
            &bytes,
        )?;
        let ValidatedJson(input) =
            ValidatedJson::<T>::from_request(Request::from_parts(parts, Body::from(bytes)), state)
                .await?;
        Ok(Self(input))
    }
}

/// JSON proposals remain ordinary-auth; raw multipart digests require bridge proof.
pub struct ProposalJson(pub crate::modules::assistant::dto::ProposalInput);
impl FromRequest<AppState> for ProposalJson {
    type Rejection = AppError;
    async fn from_request(req: Request, state: &AppState) -> Result<Self, AppError> {
        let (parts, body) = req.into_parts();
        let bytes = Bytes::from_request(Request::from_parts(parts.clone(), body), state)
            .await
            .map_err(|e| {
                if e.status() == axum::http::StatusCode::PAYLOAD_TOO_LARGE {
                    AppError::PayloadTooLarge
                } else if crate::http::timeout::is_body_timeout(&e) {
                    AppError::BodyTimeout
                } else {
                    AppError::field("body", "Invalid assistant request body.")
                }
            })?;
        if serde_json::from_slice::<Value>(&bytes)
            .ok()
            .is_some_and(|v| v["body_sha256"].is_string())
        {
            let path = parts
                .extensions
                .get::<OriginalUri>()
                .map_or(&parts.uri, |u| &u.0)
                .to_string();
            verify(
                state.assistant_bridge_secret.as_ref(),
                &parts.headers,
                parts.method.as_str(),
                &path,
                &bytes,
            )?;
        }
        let ValidatedJson(input) =
            ValidatedJson::<crate::modules::assistant::dto::ProposalInput>::from_request(
                Request::from_parts(parts, Body::from(bytes)),
                state,
            )
            .await?;
        Ok(Self(input))
    }
}

/// Signature for bodyless cleanup controls, which survive expired user logins.
pub struct SignedControl;
impl FromRequestParts<AppState> for SignedControl {
    type Rejection = AppError;
    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, AppError> {
        let path = parts
            .extensions
            .get::<OriginalUri>()
            .map_or(&parts.uri, |u| &u.0)
            .to_string();
        verify(
            state.assistant_bridge_secret.as_ref(),
            &parts.headers,
            parts.method.as_str(),
            &path,
            b"",
        )?;
        Ok(Self)
    }
}

/// Outer to request timeout: durable claim commits before handler future can run.
pub fn apply(router: Router<AppState>, state: AppState) -> Router<AppState> {
    router.layer(middleware::from_fn_with_state(state, guard))
}

async fn guard(State(state): State<AppState>, req: Request, next: Next) -> Response {
    if !req.headers().contains_key("Assistant-Action-Id") {
        return next.run(req).await;
    }
    match admitted(&state, req, next).await {
        Ok(res) => res,
        Err(err) => err.into_response(),
    }
}

fn uuid_header(headers: &HeaderMap, name: &str) -> Result<Uuid, AppError> {
    headers
        .get(name)
        .and_then(|h| h.to_str().ok())
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| AppError::BadRequest(format!("Invalid {name} header.")))
}

// Raw body admission happens before native upload handler's own memory guard.
static RAW_BODIES: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(4);

async fn admitted(state: &AppState, req: Request, next: Next) -> Result<Response, AppError> {
    let (parts, body) = req.into_parts();
    let action = uuid_header(&parts.headers, "Assistant-Action-Id")?;
    let session = uuid_header(&parts.headers, "Assistant-Session-Id")?;
    let user = AuthUser::any(&parts, state).await?;
    let workspace = repo::session_workspace(&state.db, user.user_id, user.token_id, session)
        .await?
        .ok_or(AppError::NotFound)?;
    let rights = permissions::access(&state.db, workspace, user.user_id)
        .await?
        .ok_or(AppError::NotFound)?;
    if !rights.active {
        return Err(AppError::Forbidden("This action is unauthorized."));
    }
    let owner = repo::Owner {
        workspace_id: workspace,
        user_id: user.user_id,
        login_id: user.token_id,
    };
    let original = parts
        .extensions
        .get::<OriginalUri>()
        .map_or(&parts.uri, |u| &u.0);
    let operation = service::resolve_operation(parts.method.as_str(), original.path())?;
    if !service::reviewable_method(parts.method.as_str(), &operation) {
        return Err(AppError::BadRequest(
            "Action claims require a mutation method.".into(),
        ));
    }
    let path = original.path().to_owned();
    let query = original.query().map(str::to_owned);
    let content_type = parts
        .headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_owned();
    let raw = content_type.starts_with("multipart/form-data;");
    let _raw_permit = if raw {
        Some(
            tokio::time::timeout(state.body_deadline, RAW_BODIES.acquire())
                .await
                .map_err(|_| AppError::Overloaded)?
                .map_err(|_| AppError::Overloaded)?,
        )
    } else {
        None
    };
    let limit = if raw { 10485760 + 64 * 1024 } else { 64 * 1024 };
    let deadline = if raw {
        state.body_deadline * 2
    } else {
        state.body_deadline
    };
    let bytes = tokio::time::timeout(deadline, axum::body::to_bytes(body, limit))
        .await
        .map_err(|_| AppError::BodyTimeout)?
        .map_err(|_| AppError::PayloadTooLarge)?;
    let hash = if raw {
        service::request_hash_raw(
            parts.method.as_str(),
            &path,
            query.as_deref(),
            &content_type,
            &hex::encode(Sha256::digest(&bytes)),
        )
    } else {
        let json: Option<Value> = if bytes.is_empty() {
            None
        } else {
            Some(
                serde_json::from_slice(&bytes)
                    .map_err(|_| AppError::field("body", "Action arguments must be JSON."))?,
            )
        };
        service::request_hash(
            parts.method.as_str(),
            &path,
            query.as_deref(),
            json.as_ref(),
        )
    };
    let needs_facts = crate::modules::assistant::preconditions::requires_facts(&operation)
        || crate::modules::products::approval::requires_facts(&operation)
        || crate::modules::invoices::approval::requires_facts(&operation)
        || crate::modules::storefront_design::approval::requires_facts(&operation)
        || operation == "messages_send";
    let review_facts = if parts.headers.contains_key("Assistant-Batch-Id")
        || parts.headers.contains_key("Assistant-Batch-Token")
    {
        let batch = uuid_header(&parts.headers, "Assistant-Batch-Id")?;
        let proof = parts
            .headers
            .get("Assistant-Batch-Token")
            .and_then(|value| value.to_str().ok())
            .filter(|value| crate::modules::auth::token::looks_valid(value))
            .ok_or(AppError::Conflict(
                "Batch controller proof is missing or invalid.",
            ))?;
        crate::modules::assistant::batch_repo::claim(
            &state.db,
            owner,
            crate::modules::assistant::batch_repo::Claim {
                batch_id: batch,
                action_id: action,
                session_id: session,
                operation: &operation,
                request_hash: &hash,
                run_hash: crate::modules::auth::token::hash(proof),
                needs_facts,
            },
        )
        .await?
    } else {
        repo::claim(
            &state.db,
            owner,
            action,
            session,
            &operation,
            &hash,
            needs_facts,
        )
        .await?
    }
    .ok_or(AppError::Conflict(
        "Action approval is missing, expired, changed, requires a new review or already consumed.",
    ))?;
    let (response, stale) = crate::modules::assistant::preconditions::scope(
        action,
        workspace,
        operation.clone(),
        review_facts,
        async {
            let response = next
                .run(Request::from_parts(parts, Body::from(bytes)))
                .await;
            (
                response,
                crate::modules::assistant::preconditions::is_stale(),
            )
        },
    )
    .await;
    let status = response.status().as_u16();
    let mut evidence = service::evidence(&operation, status);
    if stale {
        evidence["refusal"] = serde_json::json!("review_stale");
    }
    if let Some(receipt) = response
        .extensions()
        .get::<crate::modules::messages::dto::SendReceipt>()
    {
        evidence["delivery"] = serde_json::json!(receipt);
    }
    if let Some(receipt) = response
        .extensions()
        .get::<crate::modules::invoices::delivery::Receipt>()
    {
        evidence["delivery"] = serde_json::json!(receipt);
    }
    if let Some(receipt) = response
        .extensions()
        .get::<crate::modules::teams::dto::PermissionChangeReceipt>()
    {
        evidence["permission_changes"] = serde_json::json!({
            "status": receipt.status,
            "applied_count": receipt.applied.len(),
            "not_applied_count": receipt.not_applied.len(),
            "ignored_count": receipt.ignored_count,
        });
    }
    if let Some(receipt) = response
        .extensions()
        .get::<crate::modules::product_variants::dto::GenerationReceipt>()
    {
        let status = match receipt.status {
            crate::modules::product_variants::dto::GenerationReceiptStatus::Applied => "applied",
            crate::modules::product_variants::dto::GenerationReceiptStatus::Partial => "partial",
        };
        evidence["generation"] = service::generation_evidence(
            status,
            receipt.requested,
            receipt.created,
            receipt.skipped,
        );
    }
    if let Some(receipt) = response
        .extensions()
        .get::<crate::modules::orders::dto::RefundReceipt>()
    {
        evidence["refund"] = serde_json::json!(receipt);
    }
    // Finalizer failure cannot turn consumed action into retryable work.
    if repo::finish(&state.db, owner, action, i32::from(status), &evidence)
        .await
        .is_err()
    {
        tracing::warn!(action_id=%action,"assistant outcome persistence failed");
    }
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn secret_debug_masks_value() {
        let secret = AssistantSecret::new("x".repeat(32)).unwrap();
        assert_eq!(format!("{secret:?}"), "AssistantSecret(***)");
        assert!(AssistantSecret::new("short".into()).is_err());
    }
    #[test]
    fn signature_binds_method_path_and_body() {
        let key = Arc::new(AssistantSecret::new("x".repeat(32)).unwrap());
        let stamp = chrono::Utc::now().timestamp().to_string();
        let body = b"{}";
        let message = format!(
            "{stamp}\nPOST\n/v1/x\n{}",
            hex::encode(Sha256::digest(body))
        );
        let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(key.expose().as_bytes()).unwrap();
        mac.update(message.as_bytes());
        let mut headers = HeaderMap::new();
        headers.insert("Assistant-Bridge-Timestamp", stamp.parse().unwrap());
        headers.insert(
            "Assistant-Bridge-Signature",
            hex::encode(mac.finalize().into_bytes()).parse().unwrap(),
        );
        assert!(verify(Some(&key), &headers, "POST", "/v1/x", body).is_ok());
        assert!(verify(Some(&key), &headers, "DELETE", "/v1/x", body).is_err());
        assert!(verify(Some(&key), &headers, "POST", "/v1/y", body).is_err());
        assert!(verify(Some(&key), &headers, "POST", "/v1/x", b"[]").is_err());
    }
}
