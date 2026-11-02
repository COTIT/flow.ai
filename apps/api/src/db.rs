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

/// Server-side limits on every request connection, sent as startup options. Relation/product
/// numbering and stock locks set their own `SET LOCAL lock_timeout = '5s'` (it lasts to
/// commit, so an invoice send with stock lines waits 5s on the counter, else 10s).
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
pub async fn migrate(base: &PgConnectOptions) -> Result<Migrated, MigrateError> {
    migrate_within(base, MIGRATE_CONNECT_TIMEOUT).await
}

/// `migrate`, retrying the connection for up to `connect_for` (sqlx retries refused and
/// too-many-connections errors until then).
pub async fn migrate_within(
    base: &PgConnectOptions,
    connect_for: Duration,
) -> Result<Migrated, MigrateError> {
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(connect_for)
        .connect_with(
            base.clone()
                .options([("statement_timeout", "0"), ("lock_timeout", "30s")]),
        )
        .await?;
    let result = run_counted(&pool).await;
    pool.close().await;
    result
}

/// What one `migrate` run did, read from sqlx's own `_sqlx_migrations` ledger.
#[derive(Debug, PartialEq, Eq)]
pub struct Migrated {
    /// Migrations this run applied.
    pub applied: i64,
    /// Newest applied version afterwards; None only for a DB with no migrations.
    pub version: Option<i64>,
}

impl std::fmt::Display for Migrated {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let version = self
            .version
            .map_or_else(|| "none".to_owned(), |v| v.to_string());
        match self.applied {
            0 => write!(f, "migrations up to date ({version})"),
            1 => write!(f, "applied 1 migration, now at {version}"),
            n => write!(f, "applied {n} migrations, now at {version}"),
        }
    }
}

async fn run_counted(pool: &PgPool) -> Result<Migrated, MigrateError> {
    let (before, _) = ledger(pool).await?;
    sqlx::migrate!().run(pool).await?;
    let (after, version) = ledger(pool).await?;
    Ok(Migrated {
        applied: after - before,
        version,
    })
}

/// (applied count, newest version); (0, None) before sqlx has created its table.
async fn ledger(pool: &PgPool) -> Result<(i64, Option<i64>), sqlx::Error> {
    let exists: bool = sqlx::query_scalar("SELECT to_regclass('_sqlx_migrations') IS NOT NULL")
        .fetch_one(pool)
        .await?;
    if !exists {
        return Ok((0, None));
    }
    sqlx::query_as("SELECT count(*), max(version) FROM _sqlx_migrations WHERE success")
        .fetch_one(pool)
        .await
}

#[cfg(test)]
mod tests {
    use super::Migrated;

    #[test]
    fn one_migration_is_singular() {
        let one = Migrated {
            applied: 1,
            version: Some(20261102022109),
        };
        assert_eq!(
            one.to_string(),
            "applied 1 migration, now at 20261102022109"
        );
    }
}
