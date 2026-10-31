//! A Gotenberg stand-in on 127.0.0.1: answers every HTML render with fixed bytes and keeps the
//! multipart bodies it received.

use std::sync::{Arc, Mutex};

use axum::{Router, body::Bytes, http::StatusCode, routing};

/// What the fake sidecar received, one multipart body per render.
pub type Seen = Arc<Mutex<Vec<String>>>;

/// Start it; `answer` is the PDF it returns. An empty answer plays a failing sidecar (502),
/// `refuse` one that refuses the HTML (400).
pub async fn start(answer: Vec<u8>) -> (String, Seen) {
    let seen: Seen = Arc::default();
    let log = seen.clone();
    let app = Router::new().route(
        "/forms/chromium/convert/html",
        routing::post(move |body: Bytes| {
            let (log, answer) = (log.clone(), answer.clone());
            async move {
                log.lock()
                    .unwrap()
                    .push(String::from_utf8_lossy(&body).into_owned());
                let status = match answer.as_slice() {
                    b"" => StatusCode::BAD_GATEWAY,
                    b"refuse" => StatusCode::BAD_REQUEST,
                    _ => StatusCode::OK,
                };
                (status, answer)
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (url, seen)
}
