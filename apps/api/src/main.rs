//! Boot: env -> config -> DB pool -> migrations -> serve, graceful stop on SIGINT/SIGTERM.

use std::{net::SocketAddr, time::Duration};

use genesy_360_api::{
    AppState, RateLimit, Slots, WorkspaceSlots, build_app,
    config::Config,
    db, healthcheck, jobs,
    mail::{self, Mailer},
    outbound, payments, pdf, storage, warm_up,
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

    let mut config = Config::from_env().expect("config");
    let base: PgConnectOptions = config.database_url.parse().expect("DATABASE_URL");
    // Migrations are embedded in the binary and run on their own connection, before the pool.
    db::migrate(&base).await.expect("run migrations");
    let health_db = db::health(base.clone())
        .await
        .expect("connect postgres (health)");
    let background_db = db::background(base.clone())
        .await
        .expect("connect postgres (background)");
    let jobs_db = db::jobs(base.clone(), config.db_job_connections)
        .await
        .expect("connect postgres (jobs)");
    let db = db::connect(base, config.db_max_connections)
        .await
        .expect("connect postgres");
    warm_up().await;

    // Operator-configured endpoints may be internal (the sidecar, a local MinIO): trusted.
    // Graph hosts count only when overridden (a local fake); Microsoft's are public anyway.
    let graph_fakes = [&config.graph_login_url, &config.graph_api_url]
        .into_iter()
        .filter(|u| !u.starts_with(mail::graph::LOGIN_URL) && !u.starts_with(mail::graph::API_URL))
        .map(String::as_str);
    let trusted: Vec<String> = [
        config.gotenberg_url.as_deref(),
        config.storage.as_ref().map(|s| s.endpoint.as_str()),
    ]
    .into_iter()
    .flatten()
    .chain(graph_fakes)
    .filter_map(outbound::host_of)
    .collect();
    let http = outbound::Outbound::new(&trusted).expect("outbound http client");
    let pdf = config
        .gotenberg_url
        .as_deref()
        .map(|url| pdf::Renderer::new(url, http.clone()));
    let files = config
        .storage
        .take()
        .map(|s| storage::Store::s3(s, http.clone()).expect("file storage"));
    let job_files = files.clone();
    // Webhook targets are customer URLs: no operator host is trusted for them.
    let job_http = outbound::Outbound::new(&[]).expect("outbound http client");
    let credentials = config.credentials_key.map(std::sync::Arc::new);
    // Checked by Config: fake is refused in a release build.
    let payments = payments::select(config.payments_provider.as_deref(), !cfg!(debug_assertions))
        .expect("payments provider");
    if payments.is_none() {
        tracing::warn!("PAYMENTS_PROVIDER not set: web payments answer 503");
    }
    let job_credentials = credentials.clone();
    if files.is_none() || pdf.is_none() {
        tracing::warn!(
            files = files.is_some(),
            pdf = pdf.is_some(),
            "file storage or PDF rendering not configured: those routes answer 503"
        );
    }
    if credentials.is_none() {
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
        mailer: Mailer::smtp(&config.smtp_url, &config.mail_from)
            .expect("mailer config")
            .with_allowed_domains(&config.mail_allowed_domains)
            .with_trusted_smtp_hosts(&config.mail_trusted_smtp_hosts)
            .with_graph(mail::graph::Client::new(
                &config.graph_login_url,
                &config.graph_api_url,
                http.clone(),
            )),
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
        credentials,
        files,
        pdf,
        payments: payments.clone(),
    };
    // The job rail starts last: a boot step that panics above must not leave claimed jobs
    // behind in a process that dies.
    let (stop_jobs, jobs_stopped) = tokio::sync::watch::channel(false);
    let runner = jobs::runner::Runner {
        db: jobs_db.clone(),
        control: background_db.clone(),
        kinds: jobs::kinds::KINDS.to_vec(),
        workers: usize::try_from(config.db_job_connections).unwrap_or(1),
        cap: i64::from(config.job_max_running_per_workspace),
        lease: jobs::LEASE,
        idle: jobs::IDLE,
        max_run: jobs::MAX_RUN,
        handler: jobs::kinds::handler(jobs::kinds::Ctx {
            db: jobs_db,
            files: job_files,
            http: job_http,
            credentials: job_credentials,
            payments: payments.clone(),
        }),
    }
    .spawn(jobs_stopped.clone());
    let scheduler =
        jobs::scheduler::spawn(background_db, &jobs::kinds::SCHEDULES, jobs_stopped.clone());
    // Starts at the signal and runs alongside the HTTP drain.
    let mut stopped = jobs_stopped;
    let drain_jobs = async move {
        let _ = stopped.wait_for(|stop| *stop).await;
        let drained = tokio::time::timeout(JOB_DRAIN, async {
            if let Err(err) = runner.await {
                tracing::error!(error = %err, "job runner ended abnormally");
            }
            if let Err(err) = scheduler.await {
                tracing::error!(error = %err, "scheduler ended abnormally");
            }
        })
        .await;
        if drained.is_err() {
            tracing::warn!(
                "jobs still running at exit; their leases expire and another instance resumes them"
            );
        }
    };
    // connect info: rate limit keys on peer IP.
    let serve = axum::serve(
        listener,
        build_app(state).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async move {
        shutdown_signal().await;
        // Workers stop claiming now and finish their chunk while requests drain.
        let _ = stop_jobs.send(true);
    });
    let (served, ()) = tokio::join!(std::future::IntoFuture::into_future(serve), drain_jobs);
    served.expect("server");
}

/// Docker waits 10 s after SIGTERM before killing: running chunks get most of it.
const JOB_DRAIN: Duration = Duration::from_secs(8);

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
