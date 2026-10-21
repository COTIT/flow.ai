//! Boot: env -> config -> DB pool -> migrations -> serve, graceful stop on SIGINT/SIGTERM.

use std::{net::SocketAddr, time::Duration};

use genesy_360_api::{
    AppState, RateLimit, Slots, WorkspaceSlots, build_app, cleanup, config::Config, db,
    healthcheck, mail::Mailer, warm_up,
};
use sqlx::postgres::PgConnectOptions;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    // `genesy-360-api healthcheck`: compose healthcheck for the shell-less image. Needs only
    // BIND_ADDR's port, not the full config (secrets, DB), and exits right away.
    if std::env::args().nth(1).as_deref() == Some("healthcheck") {
        let port = std::env::var("BIND_ADDR")
            .ok()
            .and_then(|addr| addr.parse::<SocketAddr>().ok())
            .map_or(8080, |addr| addr.port());
        std::process::exit(if healthcheck::probe(port).await { 0 } else { 1 });
    }

    // .env optional: dev convenience, prod injects real env.
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let config = Config::from_env().expect("config");
    let base: PgConnectOptions = config.database_url.parse().expect("DATABASE_URL");
    // Migrations are embedded in the binary and run on their own connection, before the pool.
    db::migrate(&base).await.expect("run migrations");
    let health_db = db::health(base.clone())
        .await
        .expect("connect postgres (health)");
    let background_db = db::background(base.clone())
        .await
        .expect("connect postgres (background)");
    let db = db::connect(base, config.db_max_connections)
        .await
        .expect("connect postgres");
    warm_up().await;
    cleanup::spawn(background_db);

    if config.credentials_key.is_none() {
        tracing::warn!("CREDENTIALS_KEY not set: integration credentials can't be stored");
    }
    let listener = tokio::net::TcpListener::bind(config.bind_addr)
        .await
        .expect("bind listener");
    tracing::info!(addr = %config.bind_addr, "listening");

    let state = AppState {
        db,
        health_db,
        token_ttl: chrono::Duration::days(config.token_ttl_days),
        mailer: Mailer::smtp(&config.smtp_url, &config.mail_from).expect("mailer config"),
        frontend_url: config.frontend_url.as_str().into(),
        api_public_url: config.api_public_url.as_str().into(),
        rate_limit: RateLimit {
            burst: config.rate_limit_burst,
            replenish: Duration::from_secs(config.rate_limit_replenish_secs),
            trust_forwarded: config.trust_proxy_headers,
        },
        invites_per_hour: config.invites_per_hour,
        private_rate_limit: RateLimit {
            burst: config.private_rate_limit_burst,
            replenish: Duration::from_millis(config.private_rate_limit_replenish_ms),
            trust_forwarded: config.trust_proxy_headers,
        },
        private_ip_rate_limit: RateLimit {
            burst: config.private_ip_rate_limit_burst,
            replenish: Duration::from_millis(config.private_ip_rate_limit_replenish_ms),
            trust_forwarded: config.trust_proxy_headers,
        },
        slots: Slots::new(WorkspaceSlots {
            per_workspace: config.workspace_max_inflight,
            per_user: config.user_max_inflight,
            // Same as the pool's acquire timeout: a queued request gives up when a pooled one would.
            wait: db::ACQUIRE_TIMEOUT,
        }),
        request_timeout: Duration::from_secs(config.request_timeout_secs),
        body_deadline: Duration::from_secs(config.request_body_deadline_secs),
        credentials: config.credentials_key.map(std::sync::Arc::new),
    };
    // connect info: rate limit keys on peer IP.
    axum::serve(
        listener,
        build_app(state).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await
    .expect("server");
}

// Docker stop sends SIGTERM, terminal sends SIGINT. Either one drains in-flight requests.
// A listener that fails to register waits forever: completing would shut the server down at boot.
async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(e) = tokio::signal::ctrl_c().await {
            tracing::error!(error = %e, "SIGINT listener failed");
            std::future::pending::<()>().await;
        }
    };
    #[cfg(unix)]
    let term = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut sig) => {
                sig.recv().await;
            }
            Err(e) => {
                tracing::error!(error = %e, "SIGTERM listener failed");
                std::future::pending::<()>().await;
            }
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = term => {},
    }
    tracing::info!("shutting down");
}
