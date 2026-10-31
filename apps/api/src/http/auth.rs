//! AuthUser extractor: `Authorization: Bearer <token>` -> live access_tokens row, else 401.
//! Phone-only accounts (farmers) get 403 from it: the account routes it guards (profile,
//! invites, email verification) need an email. Farm routes, workspace routes and logout take
//! [`AnyUser`] / [`AuthUser::any`] instead.

use axum::{
    extract::FromRequestParts,
    http::{HeaderMap, header::AUTHORIZATION, request::Parts},
};
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::workspace::NOT_ALLOWED,
    modules::auth::{repo as tokens, token},
};

/// Logged-in caller. Put it in a handler's args to require auth.
#[derive(Debug, Clone, Copy)]
pub struct AuthUser {
    pub user_id: Uuid,
    /// Token used for this request; logout deletes it.
    pub token_id: Uuid,
}

impl AuthUser {
    /// Any signed-in account, phone-only included.
    pub async fn any(parts: &Parts, state: &AppState) -> Result<Self, AppError> {
        Ok(Self::lookup(parts, state).await?.0)
    }

    async fn lookup(parts: &Parts, state: &AppState) -> Result<(Self, bool), AppError> {
        let plain = bearer(&parts.headers).ok_or(AppError::Unauthorized)?;
        let row = tokens::find_active_by_hash(&state.db, &token::hash(plain))
            .await?
            .ok_or(AppError::Unauthorized)?;
        let user = Self {
            user_id: row.user_id,
            token_id: row.id,
        };
        Ok((user, row.phone_only))
    }
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, AppError> {
        let (user, phone_only) = Self::lookup(parts, state).await?;
        if phone_only {
            return Err(AppError::Forbidden(NOT_ALLOWED));
        }
        Ok(user)
    }
}

/// Any signed-in account, phone-only (farmer) accounts included.
#[derive(Debug, Clone, Copy)]
pub struct AnyUser(pub AuthUser);

impl FromRequestParts<AppState> for AnyUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, AppError> {
        Ok(Self(AuthUser::any(parts, state).await?))
    }
}

/// Token from `Authorization: Bearer <t>`. Scheme case-insensitive (RFC 9110). Malformed tokens
/// rejected here so garbage never costs a DB round trip. The private rate limit keys on the
/// same value.
pub(crate) fn bearer(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = value.split_once(' ')?;
    let token = token.trim();
    (scheme.eq_ignore_ascii_case("bearer") && token::looks_valid(token)).then_some(token)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::{
        auth::{repo as tokens, token},
        users::repo::{self as users, NewUser},
    };
    use axum::{
        Router,
        body::{Body, to_bytes},
        http::{Request, StatusCode},
        routing::get,
    };
    use chrono::{Duration, Utc};
    use sqlx::PgPool;
    use tower::ServiceExt;

    async fn call(pool: PgPool, auth: Option<&str>) -> (StatusCode, String) {
        let app = Router::new()
            .route(
                "/probe",
                get(|u: AuthUser| async move { u.user_id.to_string() }),
            )
            .with_state(crate::http::test_state(pool));
        let mut req = Request::get("/probe");
        if let Some(value) = auth {
            req = req.header("authorization", value);
        }
        let res = app.oneshot(req.body(Body::empty()).unwrap()).await.unwrap();
        let status = res.status();
        let body = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        (status, String::from_utf8(body.to_vec()).unwrap())
    }

    /// Returns (user_id, plaintext token) for a token valid `ttl` from now.
    async fn token_for(pool: &PgPool, ttl: Duration) -> (Uuid, String) {
        let user_id = users::insert(
            pool,
            &NewUser {
                email: "u@x.io",
                first_name: "U",
                middle_name: None,
                last_name: "S",
                password_hash: "h",
            },
        )
        .await
        .unwrap();
        let (plain, hash) = token::generate().unwrap();
        tokens::insert(pool, user_id, &hash, Utc::now() + ttl)
            .await
            .unwrap();
        (user_id, plain.expose().to_owned())
    }

    #[sqlx::test]
    async fn valid_bearer_yields_user(pool: PgPool) {
        let (user_id, plain) = token_for(&pool, Duration::days(1)).await;

        let (status, body) = call(pool, Some(&format!("Bearer {plain}"))).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, user_id.to_string());
    }

    #[sqlx::test]
    async fn scheme_is_case_insensitive(pool: PgPool) {
        let (_, plain) = token_for(&pool, Duration::days(1)).await;

        let (status, _) = call(pool, Some(&format!("bearer {plain}"))).await;

        assert_eq!(status, StatusCode::OK);
    }

    #[sqlx::test]
    async fn missing_header_is_401(pool: PgPool) {
        assert_eq!(call(pool, None).await.0, StatusCode::UNAUTHORIZED);
    }

    #[sqlx::test]
    async fn wrong_scheme_or_empty_is_401(pool: PgPool) {
        let (_, plain) = token_for(&pool, Duration::days(1)).await;

        for header in [
            format!("Basic {plain}"),
            "Bearer ".into(),
            "Bearer".into(),
            plain,
        ] {
            assert_eq!(
                call(pool.clone(), Some(&header)).await.0,
                StatusCode::UNAUTHORIZED,
                "{header}"
            );
        }
    }

    #[sqlx::test]
    async fn unknown_token_is_401(pool: PgPool) {
        token_for(&pool, Duration::days(1)).await;

        let (status, body) = call(pool, Some("Bearer deadbeef")).await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert!(body.contains("You are not authorized"), "{body}");
    }

    #[sqlx::test]
    async fn expired_token_is_401(pool: PgPool) {
        let (_, plain) = token_for(&pool, -Duration::seconds(1)).await;

        assert_eq!(
            call(pool, Some(&format!("Bearer {plain}"))).await.0,
            StatusCode::UNAUTHORIZED
        );
    }
}
