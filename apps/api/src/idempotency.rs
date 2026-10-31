//! Idempotency-Key on money writes (the SPA sends one on every POST and DELETE and reuses it
//! on retry). The first answer to a key is stored by the transaction that does the write and
//! replayed byte for byte for the same request (method, path, body); another request under
//! the key is refused. A parallel copy waits on the key's row until the first commits, then
//! replays. Keys live per workspace for a day. No header: no dedupe.

use axum::{
    body::{Body, Bytes},
    extract::{FromRequest, FromRequestParts, Request},
    http::{HeaderValue, Method, StatusCode, header, request::Parts},
    response::{IntoResponse, Response},
};
use serde::de::DeserializeOwned;
use sha2::{Digest, Sha256};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::{
    error::AppError,
    http::json::{Validate, ValidatedJson},
};

const HEADER: &str = "idempotency-key";
const REUSED: &str = "Idempotency-Key reused with different request payload";
const BAD_KEY: &str = "The Idempotency-Key header must be 1 to 255 visible characters.";

/// A request's key and the hash of what it was sent with.
pub struct Key {
    key: String,
    hash: [u8; 32],
}

/// An answer to store and replay: status and JSON body.
#[derive(Debug, PartialEq, Eq)]
pub struct Answer {
    pub status: u16,
    pub body: Vec<u8>,
}

impl Answer {
    /// 200 `{"message": "ok"}`.
    pub fn ok() -> Self {
        Self {
            status: 200,
            body: br#"{"message":"ok"}"#.to_vec(),
        }
    }

    /// 201 with `body` as JSON.
    pub fn created(body: &serde_json::Value) -> Self {
        Self {
            status: 201,
            body: body.to_string().into_bytes(),
        }
    }
}

impl IntoResponse for Answer {
    fn into_response(self) -> Response {
        let status = StatusCode::from_u16(self.status).unwrap_or(StatusCode::OK);
        (
            status,
            [(header::CONTENT_TYPE, "application/json")],
            self.body,
        )
            .into_response()
    }
}

/// Who sent a keyed request: a member or an app key. An answer replays for its actor only.
#[derive(Clone, Copy, Debug)]
pub enum Actor {
    User(Uuid),
    AppKey(Uuid),
}

/// The stored answer when `key` was used before by this actor for this request; None when it
/// is fresh (or there is no key). Run it first in the write's transaction: a copy in flight
/// holds the key's row until it commits, so this waits for it and then replays its answer.
pub async fn replay(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    actor: Actor,
    key: Option<&Key>,
) -> Result<Option<Answer>, AppError> {
    let (user_id, app_key_id) = match actor {
        Actor::User(id) => (Some(id), None),
        Actor::AppKey(id) => (None, Some(id)),
    };
    let Some(key) = key else {
        return Ok(None);
    };
    // Twice at most: a prune may remove the row between the insert and the read.
    for _ in 0..2 {
        let fresh = sqlx::query_scalar!(
            r#"INSERT INTO idempotency_keys (workspace_id, key, user_id, app_key_id, request_hash,
                                             status, response)
               VALUES ($1, $2, $3, $5, $4, 0, '')
               ON CONFLICT (workspace_id, key) DO NOTHING
               RETURNING true AS "fresh!""#,
            workspace_id,
            key.key,
            user_id,
            &key.hash[..],
            app_key_id,
        )
        .fetch_optional(&mut *conn)
        .await?
        .is_some();
        if fresh {
            return Ok(None);
        }
        let Some(stored) = sqlx::query!(
            "SELECT user_id, app_key_id, request_hash, status, response FROM idempotency_keys
             WHERE workspace_id = $1 AND key = $2",
            workspace_id,
            key.key,
        )
        .fetch_optional(&mut *conn)
        .await?
        else {
            continue;
        };
        // Another actor's key, or another request under it: never their answer.
        if stored.user_id != user_id
            || stored.app_key_id != app_key_id
            || stored.request_hash != key.hash
        {
            return Err(AppError::Unprocessable(REUSED.into()));
        }
        let status = u16::try_from(stored.status)
            .ok()
            .filter(|s| (200..300).contains(s))
            .ok_or_else(|| AppError::Internal("idempotency key without an answer".into()))?;
        return Ok(Some(Answer {
            status,
            body: stored.response,
        }));
    }
    Err(AppError::Internal("idempotency key vanished twice".into()))
}

/// Store the answer of a fresh key, in the same transaction as the write.
pub async fn finish(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    key: Option<&Key>,
    answer: &Answer,
) -> Result<(), sqlx::Error> {
    let Some(key) = key else {
        return Ok(());
    };
    sqlx::query!(
        "UPDATE idempotency_keys SET status = $3, response = $4
         WHERE workspace_id = $1 AND key = $2",
        workspace_id,
        key.key,
        i16::try_from(answer.status).unwrap_or(200),
        &answer.body,
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Drop keys older than a day; how many.
pub async fn prune(db: &PgPool) -> Result<u64, sqlx::Error> {
    Ok(
        sqlx::query!("DELETE FROM idempotency_keys WHERE created_at < now() - interval '1 day'")
            .execute(db)
            .await?
            .rows_affected(),
    )
}

/// The request's key, bound to method + path + `body`; None without the header.
fn key_of(
    method: &Method,
    path: &str,
    raw: Option<&HeaderValue>,
    body: &[u8],
) -> Result<Option<Key>, AppError> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let key = raw
        .to_str()
        .ok()
        .map(str::trim)
        .filter(|k| (1..=255).contains(&k.len()))
        .ok_or_else(|| AppError::Unprocessable(BAD_KEY.into()))?;
    let mut h = Sha256::new();
    h.update(method.as_str());
    h.update(b"\n");
    h.update(path);
    h.update(b"\n");
    h.update(body);
    Ok(Some(Key {
        key: key.to_owned(),
        hash: h.finalize().into(),
    }))
}

/// A validated JSON body (as `ValidatedJson`) plus its Idempotency-Key.
pub struct Keyed<T: Validate> {
    pub input: T::Valid,
    pub key: Option<Key>,
}

impl<S, T> FromRequest<S> for Keyed<T>
where
    S: Send + Sync,
    T: DeserializeOwned + Validate,
{
    type Rejection = AppError;

    async fn from_request(req: Request, state: &S) -> Result<Self, AppError> {
        let (method, uri, headers) = (
            req.method().clone(),
            req.uri().clone(),
            req.headers().clone(),
        );
        // The request itself is read: its extensions carry the route's body limit.
        let bytes = Bytes::from_request(req, state).await.map_err(|rejection| {
            match rejection.status() {
                StatusCode::PAYLOAD_TOO_LARGE => AppError::PayloadTooLarge,
                _ if crate::http::timeout::is_body_timeout(&rejection) => AppError::BodyTimeout,
                _ => AppError::field("body", rejection.body_text()),
            }
        })?;
        let key = key_of(&method, uri.path(), headers.get(HEADER), &bytes)?;
        // Same head, read bytes: ValidatedJson's content-type check and errors apply as is.
        let mut again = Request::new(Body::from(bytes));
        *again.method_mut() = method;
        *again.uri_mut() = uri;
        *again.headers_mut() = headers;
        let ValidatedJson(input) = ValidatedJson::<T>::from_request(again, state).await?;
        Ok(Self { input, key })
    }
}

/// The Idempotency-Key of a write without a body (DELETE).
pub struct KeyOnly(pub Option<Key>);

impl<S: Send + Sync> FromRequestParts<S> for KeyOnly {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, AppError> {
        key_of(
            &parts.method,
            parts.uri.path(),
            parts.headers.get(HEADER),
            b"",
        )
        .map(Self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(
        method: &str,
        path: &str,
        raw: Option<&str>,
        body: &[u8],
    ) -> Result<Option<Key>, AppError> {
        let raw = raw.map(|r| HeaderValue::from_str(r).unwrap());
        key_of(
            &Method::from_bytes(method.as_bytes()).unwrap(),
            path,
            raw.as_ref(),
            body,
        )
    }

    #[test]
    fn a_key_is_bound_to_method_path_and_body() {
        let hash = |m, p, b: &[u8]| key(m, p, Some("k"), b).unwrap().unwrap().hash;
        let base = hash("POST", "/a", b"{}");
        assert_eq!(base, hash("POST", "/a", b"{}"));
        assert_ne!(base, hash("DELETE", "/a", b"{}"));
        assert_ne!(base, hash("POST", "/b", b"{}"));
        assert_ne!(base, hash("POST", "/a", b"{ }"));
    }

    #[test]
    fn no_header_no_key_and_bad_keys_are_refused() {
        assert!(key("POST", "/a", None, b"").unwrap().is_none());
        for bad in ["", "   ", &"k".repeat(256)] {
            assert!(key("POST", "/a", Some(bad), b"").is_err(), "{bad}");
        }
    }
}
