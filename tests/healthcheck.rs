//! `genesy-360-api healthcheck` self-probe: the distroless image has no shell or curl, so the
//! binary checks its own /healthz for the compose healthcheck.

mod common;

use std::{net::SocketAddr, time::Duration};

use genesy_360_api::{build_app, healthcheck::probe, mail::Mailer};
use sqlx::{PgPool, postgres::PgPoolOptions};

/// Serve `pool` on an ephemeral localhost port, return the port.
async fn serve(pool: PgPool) -> u16 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let app = build_app(common::state(pool, Mailer::capture().0));
    tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });
    port
}

#[sqlx::test]
async fn healthy_server_passes(pool: PgPool) {
    let port = serve(pool).await;

    assert!(probe(port).await);
}

#[tokio::test]
async fn server_without_db_fails() {
    let pool = PgPoolOptions::new()
        .acquire_timeout(Duration::from_millis(300))
        .connect_lazy("postgres://nobody:nothing@127.0.0.1:1/none")
        .unwrap();
    let port = serve(pool).await;

    assert!(!probe(port).await, "503 from /healthz must fail the probe");
}

#[tokio::test]
async fn nothing_listening_fails() {
    // Bind then drop: the port is free, so the connect is refused.
    let port = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap()
        .local_addr()
        .unwrap()
        .port();

    assert!(!probe(port).await);
}
