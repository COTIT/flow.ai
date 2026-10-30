//! Request time limits: a request body must arrive within its deadline (408), and a whole
//! request within the request timeout (503), so a slow client or a stuck query can't hold a
//! request slot or a pooled connection for long.

mod common;

use std::{
    net::SocketAddr,
    time::{Duration, Instant},
};

use axum::{
    Router,
    body::Body,
    extract::ConnectInfo,
    http::{HeaderMap, Method, Request, StatusCode},
};
use genesy_360_api::{Slots, WorkspaceSlots, build_app, db, mail::Mailer};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tower::ServiceExt;

async fn send(app: &Router, req: Request<Body>) -> (StatusCode, HeaderMap, Value) {
    let mut req = req;
    req.extensions_mut().insert(ConnectInfo(
        "203.0.113.7:4000".parse::<SocketAddr>().unwrap(),
    ));
    let res = app.clone().oneshot(req).await.unwrap();
    let (status, headers) = (res.status(), res.headers().clone());
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        headers,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

/// A client that sends the start of its JSON body and then stalls gets 408 at the deadline
/// (real server and socket, as hyper reads the body).
#[sqlx::test]
async fn a_stalled_body_gets_408_at_the_deadline(pool: PgPool) {
    let mut state = common::state(pool, Mailer::capture().0);
    state.body_deadline = Duration::from_millis(300);
    let app = build_app(state);
    let (token, ws) = common::signup(&app, "ada@example.com", "acme").await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });

    let mut socket = tokio::net::TcpStream::connect(addr).await.unwrap();
    let head = format!(
        "POST /v1/workspace/{ws}/relations HTTP/1.1\r\nhost: test\r\n\
         authorization: Bearer {token}\r\ncontent-type: application/json\r\n\
         content-length: 100\r\n\r\n{{\"company_name\":"
    );
    socket.write_all(head.as_bytes()).await.unwrap();
    let started = Instant::now();
    let mut response = vec![0u8; 4096];
    let n = tokio::time::timeout(Duration::from_secs(3), socket.read(&mut response))
        .await
        .expect("answered before the client sent the rest")
        .unwrap();
    let text = String::from_utf8_lossy(&response[..n]);
    assert!(text.starts_with("HTTP/1.1 408"), "{text}");
    assert!(
        text.contains("The request body took too long to arrive."),
        "{text}"
    );
    assert!(started.elapsed() < Duration::from_secs(3));

    // Item totals take bodies up to 2 MiB: twice the deadline. A body that would be too slow
    // elsewhere (450 ms here) still arrives in time.
    let body = r#"{"items":[{"id":1,"quantity":1,"price":10,"vat":21}]}"#;
    let (first, rest) = body.split_at(10);
    let mut socket = tokio::net::TcpStream::connect(addr).await.unwrap();
    let head = format!(
        "POST /v1/workspace/{ws}/calculate-items HTTP/1.1\r\nhost: test\r\n\
         authorization: Bearer {token}\r\ncontent-type: application/json\r\n\
         content-length: {}\r\n\r\n{first}",
        body.len()
    );
    socket.write_all(head.as_bytes()).await.unwrap();
    tokio::time::sleep(Duration::from_millis(450)).await;
    socket.write_all(rest.as_bytes()).await.unwrap();
    let n = tokio::time::timeout(Duration::from_secs(3), socket.read(&mut response))
        .await
        .unwrap()
        .unwrap();
    let text = String::from_utf8_lossy(&response[..n]);
    assert!(text.starts_with("HTTP/1.1 200"), "{text}");
}

/// A request stuck past the request timeout answers 503 with Retry-After, and its workspace
/// slot is free again at once.
#[sqlx::test]
async fn a_stuck_request_gets_503_and_frees_its_slot(pool: PgPool) {
    let app_pool = db::connect((*pool.connect_options()).clone(), 20)
        .await
        .unwrap();
    let mut state = common::state(app_pool, Mailer::capture().0);
    state.slots = Slots::new(WorkspaceSlots {
        per_workspace: 1,
        per_user: 10,
        wait: Duration::from_millis(100),
    });
    // Setup and the follow-up run on the default timeout (signup's argon2 is slow in a debug
    // build on CI); only the stuck request gets the short one. Both share pool and slots.
    let app = build_app(state.clone());
    state.request_timeout = Duration::from_millis(500);
    let short = build_app(state);
    let (token, ws) = common::signup(&app, "ada@example.com", "acme").await;

    // Workspace numbering lock held elsewhere: the create waits on it (lock timeout 5s).
    let mut holder = pool.begin().await.unwrap();
    sqlx::query(
        "SELECT pg_advisory_xact_lock(hashtextextended('relations.relation_number|' || $1, 0))",
    )
    .bind(&ws)
    .execute(&mut *holder)
    .await
    .unwrap();
    let req = Request::post(format!("/v1/workspace/{ws}/relations"))
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(
            json!({ "company_name": "C", "type": "business" }).to_string(),
        ))
        .unwrap();
    let started = Instant::now();
    let (status, headers, body) = send(&short, req).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(
        body,
        json!({ "message": "The server is busy, please try again." })
    );
    assert_eq!(headers["retry-after"], "5");
    let took = started.elapsed();
    assert!(
        took >= Duration::from_millis(450) && took < Duration::from_secs(3),
        "{took:?}"
    );

    let req = Request::builder()
        .method(Method::GET)
        .uri(format!("/v1/workspace/{ws}/relations"))
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&app, req).await.0, StatusCode::OK, "the slot is free");
    holder.rollback().await.unwrap();
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM relations WHERE workspace_id = $1::uuid")
            .bind(&ws)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 0, "the cancelled create rolled back");
}
