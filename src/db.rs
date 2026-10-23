//! Postgres connections. Request pool (size from config, per-connection timeouts, fast-fail
//! acquire), the job pool (one connection per worker), a one-connection pool each for /healthz
//! and the rail's control work (scheduler ticks, lease renewals), and boot migrations on their
//! own connection.

use std::time::Duration;

use sqlx::{
    PgPool,
    migrate::MigrateError,
    postgres::{PgConnectOptions, PgPoolOptions},
};

/// A saturated pool fails fast (503) instead of hanging requests for sqlx's 30s default.
pub const ACQUIRE_TIMEOUT: Duration = Duration::from_secs(5);

/// Idle connections close after this (sqlx default 10 min): previews and prod share one
/// Postgres, so a quiet instance shouldn't sit on its peak.
const IDLE_TIMEOUT: Duration = Duration::from_secs(60);

/// How long boot keeps trying to reach Postgres for migrations (restarting server, or the old
/// blue/green color still holding the role's slots) before giving up.
pub const MIGRATE_CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

/// Server-side limits on every request connection, sent as startup options. Numbering keeps
/// its own `SET LOCAL lock_timeout = '5s'`.
const REQUEST_LIMITS: [(&str, &str); 3] = [
    ("statement_timeout", "30s"),
    ("idle_in_transaction_session_timeout", "60s"),
    ("lock_timeout", "10s"),
];

/// The request pool: at most `max` connections, each with the request limits.
pub async fn connect(base: PgConnectOptions, max: u32) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(max)
        .acquire_timeout(ACQUIRE_TIMEOUT)
        .idle_timeout(IDLE_TIMEOUT)
        .connect_with(base.options(REQUEST_LIMITS))
        .await
}

/// One connection for /healthz, so a full request pool can't fail the health check (and get
/// the container pulled from Traefik). Answers inside the 3s probe or reports unhealthy.
pub async fn health(base: PgConnectOptions) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_secs(2))
        .connect_with(base.options([
            ("statement_timeout", "2s"),
            ("lock_timeout", "2s"),
            ("idle_in_transaction_session_timeout", "60s"),
        ]))
        .await
}

/// The job pool: `max` connections, one per worker. Statements get 5 min (a chunk lowers it
/// with `SET LOCAL`), a transaction 10 min: a stuck one can't hold a worker or the vacuum
/// horizon forever.
pub async fn jobs(base: PgConnectOptions, max: u32) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(max)
        .acquire_timeout(ACQUIRE_TIMEOUT)
        .idle_timeout(IDLE_TIMEOUT)
        .connect_with(base.options([
            ("statement_timeout", "5min"),
            ("transaction_timeout", "10min"),
            ("lock_timeout", "10s"),
            ("idle_in_transaction_session_timeout", "60s"),
        ]))
        .await
}

/// One connection for the job rail's control work (scheduler ticks, lease renewals), apart
/// from the job pool so busy workers can't starve it. Its statements are tiny, so short caps:
/// one stuck statement must not hold every renewal past a lease.
pub async fn background(base: PgConnectOptions) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(ACQUIRE_TIMEOUT)
        .idle_timeout(IDLE_TIMEOUT)
        .connect_with(base.options([
            ("statement_timeout", "15s"),
            ("lock_timeout", "5s"),
            ("idle_in_transaction_session_timeout", "60s"),
        ]))
        .await
}

/// Run the embedded migrations on one connection without a statement timeout. A lock wait is
/// capped at 30s, so an ALTER queued behind the old color's transaction fails the boot
/// (compose restarts it) instead of stalling the table. sqlx's session advisory lock makes
/// replicas booting together apply them once.
pub async fn migrate(base: &PgConnectOptions) -> Result<(), MigrateError> {
    migrate_within(base, MIGRATE_CONNECT_TIMEOUT).await
}

/// `migrate`, retrying the connection for up to `connect_for` (sqlx retries refused and
/// too-many-connections errors until then).
pub async fn migrate_within(
    base: &PgConnectOptions,
    connect_for: Duration,
) -> Result<(), MigrateError> {
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(connect_for)
        .connect_with(
            base.clone()
                .options([("statement_timeout", "0"), ("lock_timeout", "30s")]),
        )
        .await?;
    let result = sqlx::migrate!().run(&pool).await;
    pool.close().await;
    result
}
