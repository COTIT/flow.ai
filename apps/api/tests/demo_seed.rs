//! Staging demo seed (`seed-demo`): refuses without its flag, seeds every part once, and every
//! credential it prints actually logs in. No secret reaches the logs.

mod common;

use std::sync::{Arc, Mutex};

use axum::http::{Method, StatusCode};
use common::send;
use genesy_360_api::{demo, mail::Mailer};
use serde_json::json;
use sqlx::PgPool;

#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

async fn count(pool: &PgPool, q: &str) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(q.to_owned()))
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test]
async fn the_seed_refuses_without_its_flag(pool: PgPool) {
    let state = common::state(pool.clone(), Mailer::capture().0);
    let err = demo::run(state, false).await.err().expect("refused");
    assert!(err.to_string().contains("GENESY_ALLOW_DEMO_SEED"), "{err}");
    assert_eq!(count(&pool, "SELECT count(*) FROM workspaces").await, 0);
}

#[sqlx::test]
async fn the_seed_runs_once_and_every_login_works(pool: PgPool) {
    let logs = Captured::default();
    let writer = logs.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_writer(move || writer.clone())
        .finish();
    let guard = tracing::subscriber::set_default(subscriber);
    let seeded = demo::run(common::state(pool.clone(), Mailer::capture().0), true)
        .await
        .unwrap();
    drop(guard);
    assert!(seeded.created);
    let text = String::from_utf8(logs.0.lock().unwrap().clone()).unwrap();
    for c in &seeded.credentials {
        // A 4-digit PIN occurs by chance in timestamps and ids: look for it as a value.
        let needle = if c.secret.len() < 8 {
            format!("\"{}\"", c.secret)
        } else {
            c.secret.clone()
        };
        assert!(
            !text.contains(&needle),
            "a {} secret reached the logs",
            c.role
        );
    }

    let app = common::app(pool.clone());
    let roles: Vec<&str> = seeded.credentials.iter().map(|c| c.role.as_str()).collect();
    for role in [
        "owner",
        "admin",
        "ops",
        "field_officer",
        "farmer",
        "device key",
        "cashier PIN",
    ] {
        assert!(roles.contains(&role), "{role} in {roles:?}");
    }
    let device = seeded
        .credentials
        .iter()
        .find(|c| c.role == "device key")
        .unwrap()
        .secret
        .clone();
    let shop: String =
        sqlx::query_scalar("SELECT id::text FROM workspaces WHERE name = 'afm-demo'")
            .fetch_one(&pool)
            .await
            .unwrap();
    for c in &seeded.credentials {
        let (status, res) = match c.path {
            "/login" => {
                let body = json!({ "email": c.login, "password": c.secret });
                send(&app, Method::POST, "/v1/login", Some(body), None).await
            }
            "/farm/login" => {
                let body = json!({ "phone": c.login, "password": c.secret });
                send(&app, Method::POST, "/v1/farm/login", Some(body), None).await
            }
            _ if c.role == "cashier PIN" => {
                let id: String =
                    sqlx::query_scalar("SELECT id::text FROM till_users WHERE name = $1")
                        .bind(&c.login)
                        .fetch_one(&pool)
                        .await
                        .unwrap();
                let body = json!({ "till_user_id": id, "pin": c.secret });
                let uri = format!("/v1/workspace/{shop}/till/login");
                send(&app, Method::POST, &uri, Some(body), Some(&device)).await
            }
            _ => {
                let uri = format!("/v1/workspace/{shop}/till/login/users");
                send(&app, Method::GET, &uri, None, Some(&device)).await
            }
        };
        assert_eq!(status, StatusCode::OK, "{} {}: {res}", c.app, c.role);
    }
    let key = seeded.storefront_key.clone().unwrap();
    let (status, res) = send(
        &app,
        Method::GET,
        "/v1/storefront/products",
        None,
        Some(&key),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    let passport = seeded.passport_path.clone().unwrap();
    let code = passport.trim_start_matches("/b/");
    let (status, res) = send(
        &app,
        Method::GET,
        &format!("/v1/public/lots/{code}"),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{res}");

    // The supply chain ran end to end: a live link, farmer credits from the web and till sales,
    // a payout, an invoice, a paid web order.
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM supply_links WHERE accepted_at IS NOT NULL AND revoked_at IS NULL"
        )
        .await,
        1
    );
    assert!(count(&pool, "SELECT count(*) FROM farm_sale_attributions").await >= 2);
    assert_eq!(count(&pool, "SELECT count(*) FROM farm_payouts").await, 1);
    assert_eq!(count(&pool, "SELECT count(*) FROM invoices").await, 1);
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM order_payments WHERE status = 'paid'"
        )
        .await,
        1
    );

    // A second run changes nothing.
    let tables = [
        "workspaces",
        "users",
        "orders",
        "farm_pue_requests",
        "farm_payouts",
        "till_users",
    ];
    let mut before = Vec::new();
    for t in tables {
        before.push(count(&pool, &format!("SELECT count(*) FROM {t}")).await);
    }
    let again = demo::run(common::state(pool.clone(), Mailer::capture().0), true)
        .await
        .unwrap();
    assert!(!again.created && again.credentials.is_empty());
    for (t, n) in tables.iter().zip(before) {
        assert_eq!(
            count(&pool, &format!("SELECT count(*) FROM {t}")).await,
            n,
            "{t}"
        );
    }
}

#[sqlx::test]
async fn a_half_seeded_staging_is_refused(pool: PgPool) {
    genesy_360_api::farm_demo::run(common::state(pool.clone(), Mailer::capture().0))
        .await
        .unwrap();
    let err = demo::run(common::state(pool, Mailer::capture().0), true)
        .await
        .err()
        .expect("refused");
    assert!(err.to_string().contains("wipe the staging volume"), "{err}");
}
