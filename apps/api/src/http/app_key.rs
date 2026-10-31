//! AppKey extractor: `Authorization: Bearer gsk_…` -> live app_keys row (workspace + scopes),
//! else 401. Routes check scopes with `AppKey::require` (403).

use axum::{
    extract::FromRequestParts,
    http::{HeaderMap, header::AUTHORIZATION, request::Parts},
};
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::{slots::Slot, workspace::NOT_ALLOWED},
    modules::{
        app_keys::{repo, secret},
        auth::token,
    },
};

/// App calling with its key. The workspace comes from the key, never from the request.
#[derive(Debug)]
pub struct AppKey {
    pub id: Uuid,
    pub workspace_id: Uuid,
    scopes: Vec<String>,
    /// Same in-flight cap as members, with the key in the user's place.
    _slot: Slot,
}

impl AppKey {
    /// 403 unless the key holds `scope`.
    pub fn require(&self, scope: &str) -> Result<(), AppError> {
        if self.scopes.iter().any(|s| s == scope) {
            Ok(())
        } else {
            Err(AppError::Forbidden(NOT_ALLOWED))
        }
    }
}

impl FromRequestParts<AppState> for AppKey {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, AppError> {
        let plain = bearer(&parts.headers).ok_or(AppError::Unauthorized)?;
        let key = repo::find_live(&state.db, &token::hash(plain))
            .await?
            .ok_or(AppError::Unauthorized)?;
        let slot = state.slots.take(key.id, key.workspace_id).await?;
        Ok(Self {
            id: key.id,
            workspace_id: key.workspace_id,
            scopes: key.scopes,
            _slot: slot,
        })
    }
}

/// Key from `Authorization: Bearer gsk_…`; anything else (user tokens included) is None.
pub(crate) fn bearer(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(AUTHORIZATION)?.to_str().ok()?;
    let (scheme, key) = value.split_once(' ')?;
    let key = key.trim();
    (scheme.eq_ignore_ascii_case("bearer") && secret::looks_valid(key)).then_some(key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::{app_keys::repo::NewKey, auth::repo as tokens};
    use axum::{
        Router,
        body::{Body, to_bytes},
        http::{Request, StatusCode},
        routing::get,
    };
    use sqlx::PgPool;
    use tower::ServiceExt;

    async fn call(pool: PgPool, auth: Option<&str>) -> (StatusCode, String) {
        let app = Router::new()
            .route(
                "/probe",
                get(|k: AppKey| async move {
                    k.require(secret::CATALOG_READ)?;
                    Ok::<_, AppError>(k.workspace_id.to_string())
                }),
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

    async fn workspace(pool: &PgPool, email: &str) -> (Uuid, Uuid) {
        sqlx::query_as(
            "WITH u AS (INSERT INTO users (email, first_name, last_name, password_hash)
                        VALUES ($1, 'O', 'W', 'h') RETURNING id)
             INSERT INTO workspaces (name, created_by_user_id)
             SELECT 'acme', id FROM u RETURNING id, created_by_user_id",
        )
        .bind(email)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    /// Returns (workspace, key id, plaintext key) for a key holding `scopes`.
    async fn key_with(pool: &PgPool, scopes: &[&str]) -> (Uuid, Uuid, String) {
        let (ws, user) = workspace(pool, "o@x.io").await;
        let (plain, hash, prefix) = secret::generate().unwrap();
        let scopes: Vec<String> = scopes.iter().map(|s| (*s).to_owned()).collect();
        let id = repo::insert(
            pool,
            &NewKey {
                workspace_id: ws,
                name: "Web shop",
                kind: "shop",
                scopes: &scopes,
                key_hash: &hash,
                prefix: &prefix,
                created_by: Some(user),
            },
        )
        .await
        .unwrap()
        .id;
        (ws, id, plain.expose().to_owned())
    }

    async fn last_used(pool: &PgPool, id: Uuid) -> Option<chrono::DateTime<chrono::Utc>> {
        sqlx::query_scalar("SELECT last_used_at FROM app_keys WHERE id = $1")
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    #[sqlx::test]
    async fn valid_key_yields_its_workspace(pool: PgPool) {
        let (ws, _, plain) = key_with(&pool, &[secret::CATALOG_READ]).await;

        let (status, body) = call(pool, Some(&format!("Bearer {plain}"))).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, ws.to_string());
    }

    #[sqlx::test]
    async fn missing_malformed_or_unknown_key_is_401(pool: PgPool) {
        let (_, _, plain) = key_with(&pool, &[secret::CATALOG_READ]).await;
        let (unknown, _, _) = secret::generate().unwrap();

        assert_eq!(call(pool.clone(), None).await.0, StatusCode::UNAUTHORIZED);
        for header in [
            format!("Basic {plain}"),
            format!("Bearer {}", &plain[..20]),
            format!("Bearer {}", &plain[4..]),
            format!("Bearer {}", unknown.expose()),
        ] {
            assert_eq!(
                call(pool.clone(), Some(&header)).await.0,
                StatusCode::UNAUTHORIZED,
                "{header}"
            );
        }
    }

    #[sqlx::test]
    async fn revoked_key_is_401(pool: PgPool) {
        let (_, id, plain) = key_with(&pool, &[secret::CATALOG_READ]).await;
        sqlx::query("UPDATE app_keys SET revoked_at = now() WHERE id = $1")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();

        let (status, _) = call(pool, Some(&format!("Bearer {plain}"))).await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[sqlx::test]
    async fn key_of_deleted_workspace_is_401(pool: PgPool) {
        let (ws, _, plain) = key_with(&pool, &[secret::CATALOG_READ]).await;
        sqlx::query("UPDATE workspaces SET deleted_at = now() WHERE id = $1")
            .bind(ws)
            .execute(&pool)
            .await
            .unwrap();

        let (status, _) = call(pool, Some(&format!("Bearer {plain}"))).await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[sqlx::test]
    async fn user_token_is_no_app_key(pool: PgPool) {
        let (_, user) = workspace(&pool, "u@x.io").await;
        let (plain, hash) = token::generate().unwrap();
        tokens::insert(
            &pool,
            user,
            &hash,
            chrono::Utc::now() + chrono::Duration::days(1),
        )
        .await
        .unwrap();

        let (status, _) = call(pool, Some(&format!("Bearer {}", plain.expose()))).await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[sqlx::test]
    async fn key_without_the_scope_is_403(pool: PgPool) {
        let (_, _, plain) = key_with(&pool, &["other:scope"]).await;

        let (status, body) = call(pool, Some(&format!("Bearer {plain}"))).await;

        assert_eq!(status, StatusCode::FORBIDDEN);
        assert!(body.contains(NOT_ALLOWED), "{body}");
    }

    #[sqlx::test]
    async fn last_used_is_stamped_at_most_once_a_minute(pool: PgPool) {
        let (_, id, plain) = key_with(&pool, &[secret::CATALOG_READ]).await;
        let header = format!("Bearer {plain}");
        assert_eq!(last_used(&pool, id).await, None);

        call(pool.clone(), Some(&header)).await;
        let first = last_used(&pool, id).await.expect("stamped on first use");
        call(pool.clone(), Some(&header)).await;
        assert_eq!(
            last_used(&pool, id).await,
            Some(first),
            "not again within a minute"
        );

        sqlx::query(
            "UPDATE app_keys SET last_used_at = now() - interval '61 seconds' WHERE id = $1",
        )
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
        call(pool.clone(), Some(&header)).await;
        assert!(last_used(&pool, id).await.unwrap() > first - chrono::Duration::seconds(1));
        assert_ne!(last_used(&pool, id).await, Some(first));
    }
}
