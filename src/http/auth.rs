//! AuthUser extractor: `Authorization: Bearer <token>` -> live access_tokens row, else 401.

use axum::{
    extract::FromRequestParts,
    http::{header::AUTHORIZATION, request::Parts},
};
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    modules::auth::{repo as tokens, token},
};

/// Logged-in caller. Put it in a handler's args to require auth.
#[derive(Debug, Clone, Copy)]
pub struct AuthUser {
    pub user_id: Uuid,
    /// Token used for this request; logout deletes it.
    pub token_id: Uuid,
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, AppError> {
        let plain = bearer(parts).ok_or(AppError::Unauthorized)?;
        let row = tokens::find_active_by_hash(&state.db, &token::hash(plain))
            .await?
            .ok_or(AppError::Unauthorized)?;
        Ok(Self {
            user_id: row.user_id,
            token_id: row.id,
        })
    }
}

/// Token from `Authorization: Bearer <t>`. Scheme case-insensitive (RFC 9110). Malformed tokens
/// rejected here so garbage never costs a DB round trip.
fn bearer(parts: &Parts) -> Option<&str> {
    let value = parts.headers.get(AUTHORIZATION)?.to_str().ok()?;
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
            .with_state(AppState {
                health_db: pool.clone(),
                db: pool,
                token_ttl: Duration::days(30),
                mailer: crate::mail::Mailer::capture().0,
                frontend_url: "https://app.test".into(),
                api_public_url: "https://api.test".into(),
                rate_limit: crate::RateLimit {
                    burst: 1000,
                    replenish: std::time::Duration::from_millis(1),
                    trust_forwarded: false,
                },
                invites_per_hour: 1000,
                private_rate_limit: crate::RateLimit {
                    burst: 1000,
                    replenish: std::time::Duration::from_millis(1),
                    trust_forwarded: false,
                },
                workspace_slots: crate::WorkspaceSlots {
                    per_workspace: 1000,
                    wait: std::time::Duration::from_secs(5),
                },
            });
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
