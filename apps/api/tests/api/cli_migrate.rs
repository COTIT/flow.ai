//! `genesy-360-api migrate`: the deploy's own migration step, run as the real binary.

use std::process::{Command, Output};

use sqlx::PgPool;

/// The binary with `migrate`, pointed at database `db` on the test server, nothing else set
/// (no app config: migrate reads only the DB URL).
fn migrate(db: &str) -> Output {
    dotenvy::dotenv().ok();
    let server = std::env::var("DATABASE_URL").expect("DATABASE_URL");
    let (server, _) = server
        .rsplit_once('/')
        .expect("DATABASE_URL has a database");
    Command::new(env!("CARGO_BIN_EXE_genesy-360-api"))
        .arg("migrate")
        .env_clear()
        .env("DATABASE_URL", format!("{server}/{db}"))
        .output()
        .expect("run genesy-360-api")
}

/// The summary line; tracing logs (sqlx notices) also go to stdout before it.
fn last_line(out: &Output) -> String {
    let stdout = String::from_utf8_lossy(&out.stdout);
    stdout.lines().last().unwrap_or_default().to_owned()
}

async fn applied(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM _sqlx_migrations WHERE success")
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Fills an empty database, exits 0, and a second run changes nothing.
#[sqlx::test(migrations = false)]
async fn migrate_fills_an_empty_database_then_is_a_no_op(pool: PgPool) {
    let db = pool.connect_options().get_database().unwrap().to_owned();

    let first = migrate(&db);
    assert!(first.status.success(), "{first:?}");
    let count = applied(&pool).await;
    assert!(count > 10, "{count}");
    let newest: i64 = sqlx::query_scalar("SELECT max(version) FROM _sqlx_migrations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        last_line(&first),
        format!("applied {count} migrations, now at {newest}")
    );

    let second = migrate(&db);
    assert!(second.status.success(), "{second:?}");
    assert_eq!(applied(&pool).await, count);
    assert_eq!(
        last_line(&second),
        format!("migrations up to date ({newest})")
    );
}

/// A failed migration exits non-zero with `migrate:` on stderr, and starts no server.
#[test]
fn migrate_failure_exits_1() {
    let out = migrate("genesy_no_such_database");
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("migrate: "));
}
