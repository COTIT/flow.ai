//! Source-reviewed native fixture guards; database and token export is unavailable.

#[path = "native_fixture_guards.rs"]
mod guards;

use axum::{
    Json,
    body::{Body, Bytes, to_bytes},
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::oneshot;
use uuid::Uuid;

type ResponseSender = Arc<Mutex<Option<oneshot::Sender<(StatusCode, Bytes)>>>>;

#[derive(Clone)]
struct ResponseCapture {
    path: String,
    body: Value,
    bearer_hash: Vec<u8>,
    claimed: Arc<AtomicBool>,
    sender: ResponseSender,
}

fn response_capture(
    path: String,
    body: Value,
    bearer: &str,
) -> (ResponseCapture, oneshot::Receiver<(StatusCode, Bytes)>) {
    let (sender, receive) = oneshot::channel();
    (
        ResponseCapture {
            path,
            body,
            bearer_hash: Sha256::digest(bearer.as_bytes()).to_vec(),
            claimed: Arc::new(AtomicBool::new(false)),
            sender: Arc::new(Mutex::new(Some(sender))),
        },
        receive,
    )
}

async fn record_response(
    State(capture): State<ResponseCapture>,
    request: Request,
    next: Next,
) -> Response {
    if request.method() != axum::http::Method::POST || request.uri().path() != capture.path {
        return next.run(request).await;
    }
    let refused = || {
        (StatusCode::CONFLICT, Json(json!({"message":"Native exercise request differs from reviewed target or was already dispatched."}))).into_response()
    };
    if request.uri().path_and_query().map(|value| value.as_str()) != Some(capture.path.as_str()) {
        return refused();
    }
    let bearer = request
        .headers()
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));
    if bearer.is_none_or(|value| {
        Sha256::digest(value.as_bytes()).as_slice() != capture.bearer_hash.as_slice()
    }) {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({"message":"Native exercise actor differs from reviewed target."})),
        )
            .into_response();
    }
    let (parts, body) = request.into_parts();
    let Ok(body) = to_bytes(body, 48000).await else {
        return refused();
    };
    if serde_json::from_slice::<Value>(&body).ok().as_ref() != Some(&capture.body)
        || capture
            .claimed
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
    {
        return refused();
    }
    let response = next.run(Request::from_parts(parts, Body::from(body))).await;
    let (parts, body) = response.into_parts();
    let Ok(body) = to_bytes(body, 180000).await else {
        return (
            StatusCode::BAD_GATEWAY,
            Json(json!({"message":"Native exercise response could not be verified."})),
        )
            .into_response();
    };
    if let Some(sender) = capture.sender.lock().unwrap().take() {
        let _ = sender.send((parts.status, body.clone()));
    }
    Response::from_parts(parts, Body::from(body))
}

/// The two source-reviewed prototype cases; no other operation may request export.
pub enum Case<'a> {
    /// Exact product create used by the native variant/history regression.
    ProductCreate,
    /// First EUR60 cash payment on the manual EUR100 invoice.
    CashInvoicePayment { invoice: &'a str },
}

struct Review {
    operation_id: &'static str,
    path: String,
}

fn reviewed_request(case: Case<'_>, workspace: &str, body: &Value) -> Result<Review, &'static str> {
    Uuid::parse_str(workspace).map_err(|_| "invalid synthetic workspace")?;
    let (operation_id, path, expected) = match case {
        Case::ProductCreate => (
            "products_create",
            format!("/v1/workspace/{workspace}/products"),
            json!({"name":"A","price":"220","gross_price":"200"}),
        ),
        Case::CashInvoicePayment { invoice } => {
            Uuid::parse_str(invoice).map_err(|_| "invalid synthetic invoice")?;
            (
                "invoice_payments_create",
                format!("/v1/workspace/{workspace}/invoices/{invoice}/payments"),
                json!({"date":"2026-10-23","amount":"60","payment_method":"cash"}),
            )
        }
    };
    if body != &expected {
        return Err("capture request differs from reviewed exact synthetic case");
    }
    Ok(Review { operation_id, path })
}

/// Ordinary runs do nothing. Export remains blocked pending explicit approval review.
pub async fn capture(_pool: &PgPool, case: Case<'_>, workspace: &str, _bearer: &str, body: &Value) {
    if std::env::var_os("ASSISTANT_NATIVE_FIXTURE_DIR").is_none() {
        return;
    }
    let _ = reviewed_request(case, workspace, body).unwrap_or_else(|message| panic!("{message}"));
    // Exporting database state and owned test bearer was rejected by automatic approval review.
    panic!(
        "native fixture export unavailable: explicit approval for synthetic database and bearer artifacts is required"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const WORKSPACE: &str = "01900000-0000-7000-8000-000000000001";

    #[test]
    fn reviewed_capture_rejects_changed_scope_and_invalid_target() {
        let product = json!({"name":"A","price":"220","gross_price":"200"});
        let review = reviewed_request(Case::ProductCreate, WORKSPACE, &product).unwrap();
        assert_eq!(review.operation_id, "products_create");
        assert!(reviewed_request(Case::ProductCreate, "other", &product).is_err());
        assert!(
            reviewed_request(
                Case::ProductCreate,
                WORKSPACE,
                &json!({"name":"B","price":"220","gross_price":"200"})
            )
            .is_err()
        );
        let payment = json!({"date":"2026-10-23","amount":"60","payment_method":"cash"});
        assert!(
            reviewed_request(
                Case::CashInvoicePayment { invoice: WORKSPACE },
                WORKSPACE,
                &payment
            )
            .is_ok()
        );
        assert!(
            reviewed_request(
                Case::CashInvoicePayment {
                    invoice: "not-an-invoice"
                },
                WORKSPACE,
                &payment
            )
            .is_err()
        );
        assert!(
            reviewed_request(
                Case::CashInvoicePayment { invoice: WORKSPACE },
                WORKSPACE,
                &json!({"date":"2026-10-23","amount":"60","payment_method":"bank"})
            )
            .is_err()
        );
    }

    #[tokio::test]
    async fn native_capture_returns_actual_handler_reply_once() {
        use axum::{Router, body::Body, middleware, response::IntoResponse, routing::post};
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };
        use tower::ServiceExt;
        let calls = Arc::new(AtomicUsize::new(0));
        let handler_calls = calls.clone();
        let (capture, receive) = response_capture(
            "/target".into(),
            json!({"name":"A"}),
            "owned-synthetic-token",
        );
        let app = Router::new()
            .route(
                "/target",
                post(move || {
                    let calls = handler_calls.clone();
                    async move {
                        calls.fetch_add(1, Ordering::SeqCst);
                        (
                            axum::http::StatusCode::CREATED,
                            axum::Json(json!({"data":{"product_id":"one"}})),
                        )
                            .into_response()
                    }
                }),
            )
            .layer(middleware::from_fn_with_state(capture, record_response));
        let request = || {
            axum::http::Request::builder()
                .method("POST")
                .uri("/target")
                .header("authorization", "Bearer owned-synthetic-token")
                .body(Body::from(r#"{"name":"A"}"#))
                .unwrap()
        };
        assert_eq!(app.clone().oneshot(request()).await.unwrap().status(), 201);
        assert_eq!(app.oneshot(request()).await.unwrap().status(), 409);
        let (status, body) = receive.await.unwrap();
        assert_eq!(status, 201);
        assert_eq!(
            serde_json::from_slice::<Value>(&body).unwrap()["data"]["product_id"],
            "one"
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn changed_native_body_never_reaches_business_handler() {
        use axum::{Router, body::Body, middleware, routing::post};
        use tower::ServiceExt;
        let (capture, mut receive) = response_capture(
            "/target".into(),
            json!({"name":"A"}),
            "owned-synthetic-token",
        );
        let app = Router::new()
            .route(
                "/target",
                post(|| async { StatusCode::INTERNAL_SERVER_ERROR }),
            )
            .layer(middleware::from_fn_with_state(capture, record_response));
        let request = axum::http::Request::builder()
            .method("POST")
            .uri("/target")
            .header("authorization", "Bearer owned-synthetic-token")
            .body(Body::from(r#"{"name":"B"}"#))
            .unwrap();
        assert_eq!(app.oneshot(request).await.unwrap().status(), 409);
        assert!(receive.try_recv().is_err());
    }
}
