//! `{"data": ...}` success envelope, same as vimdesk-api ApiResponse.

use axum::{
    Json,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use utoipa::ToSchema;

/// Wrap a body as `{"data": body}`. Pair with a status tuple for non-200: `(StatusCode::CREATED, Data(x))`.
pub struct Data<T>(pub T);

/// Wire shape of [`Data`] and its OpenAPI schema; one type, so docs and output can't drift.
#[derive(Serialize, ToSchema)]
#[schema(description = "Success body: the result under `data`.")]
pub struct Envelope<T> {
    data: T,
}

/// `{"message": ...}` body for actions with nothing to return. Laravel used `{"message":"ok"}`.
pub struct Message(pub &'static str);

/// Wire shape of [`Message`]; also the schema of every `{"message"}` error body.
#[derive(Serialize, ToSchema)]
#[schema(description = "A short message: `ok` after an action, or what went wrong.")]
pub struct MessageBody {
    message: &'static str,
    /// Stable machine-readable reason, when the caller must distinguish setup from retry.
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<&'static str>,
}

impl IntoResponse for Message {
    fn into_response(self) -> Response {
        Json(MessageBody {
            message: self.0,
            code: None,
        })
        .into_response()
    }
}

impl<T: Serialize> IntoResponse for Data<T> {
    fn into_response(self) -> Response {
        Json(Envelope { data: self.0 }).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::to_bytes, http::StatusCode};
    use serde_json::{Value, json};

    #[tokio::test]
    async fn wraps_body_under_data() {
        let res = Data(json!({ "a": 1 })).into_response();

        assert_eq!(res.status(), StatusCode::OK);
        let body: Value =
            serde_json::from_slice(&to_bytes(res.into_body(), usize::MAX).await.unwrap()).unwrap();
        assert_eq!(body, json!({ "data": { "a": 1 } }));
    }

    #[tokio::test]
    async fn message_body() {
        let res = Message("ok").into_response();

        assert_eq!(res.status(), StatusCode::OK);
        let body: Value =
            serde_json::from_slice(&to_bytes(res.into_body(), usize::MAX).await.unwrap()).unwrap();
        assert_eq!(body, json!({ "message": "ok" }));
    }
}
