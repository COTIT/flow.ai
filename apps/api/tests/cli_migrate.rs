//! `genesy-360-api migrate`: the deploy's own migration step, run as the real binary.

use std::process::{Command, Output};

use sqlx::PgPool;

/// The binary with `migrate`, pointed at database `db` on the test server, nothing else set.
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
        .env("FRONTEND_URL", "https://app.test")
        .env("API_PUBLIC_URL", "https://api.test")
        .env("SMTP_URL", "smtps://smtp.test:465")
        .env("MAIL_FROM", "Genesy <no-reply@genesy.test>")
        .output()
        .expect("run genesy-360-api")
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

    let second = migrate(&db);
    assert!(second.status.success(), "{second:?}");
    assert_eq!(applied(&pool).await, count);
    assert!(String::from_utf8_lossy(&second.stdout).contains("up to date"));
}

/// A failed migration exits non-zero with `migrate:` on stderr, and starts no server.
#[test]
fn migrate_failure_exits_1() {
    let out = migrate("genesy_no_such_database");
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("migrate: "));
}
