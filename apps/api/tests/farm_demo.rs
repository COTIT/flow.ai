//! Demo seed (G15): the RUAHA demo story, built through the API. CLI only
//! (`genesy-360-api seed-farm-demo`); refuses a non-demo workspace of the same name.
//! RUAHA ids in docs/ruaha-rls-mapping.md.

mod common;

use axum::http::{Method, StatusCode};
use common::{send, signup};
use genesy_360_api::{farm_demo, mail::Mailer};
use serde_json::json;
use sqlx::PgPool;

async fn seeded(pool: &PgPool) -> farm_demo::Report {
    farm_demo::run(common::state(pool.clone(), Mailer::capture().0))
        .await
        .unwrap()
}

/// Captured log output of this thread.
#[derive(Clone, Default)]
struct Captured(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

impl std::io::Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[sqlx::test]
async fn the_demo_tells_the_ruaha_story(pool: PgPool) {
    let logs = Captured::default();
    let writer = logs.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_writer(move || writer.clone())
        .finish();
    let guard = tracing::subscriber::set_default(subscriber);
    let report = seeded(&pool).await;
    drop(guard);
    assert!(report.created);
    let (staff_pw, farmer_pw) = (
        report.staff_password.clone().unwrap(),
        report.farmer_password.clone().unwrap(),
    );
    let text = String::from_utf8(logs.0.lock().unwrap().clone()).unwrap();
    assert!(
        !text.contains(&staff_pw) && !text.contains(&farmer_pw),
        "a demo password reached the logs"
    );
    let other = seeded_fresh_passwords_differ(&staff_pw);
    assert!(other, "passwords are random, not constants");
    let app = common::app(pool.clone());
    let (status, s) = send(
        &app,
        Method::POST,
        "/v1/farm/login",
        Some(json!({ "phone": "0712000001", "password": farmer_pw })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{s}");
    let neema = s["data"]["access_token"].as_str().unwrap().to_owned();
    let uri = format!(
        "/v1/workspace/{}/farm/surveys/eligibility",
        report.workspace_id
    );
    let (_, list) = send(&app, Method::GET, &uri, None, Some(&neema)).await;
    let open: Vec<String> = list["data"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["eligible"] == true)
        .map(|e| e["survey_id"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(open.len(), 2, "Neema has two surveys to answer: {list}");
    let titles: Vec<String> = sqlx::query_scalar(
        "SELECT title_en FROM farm_surveys WHERE id = ANY($1::uuid[]) ORDER BY title_en",
    )
    .bind(&open)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(titles, ["Harvest labour", "Maize storage"]);

    let (issued, redeemed, outstanding): (i64, i64, i64) = sqlx::query_as(
        "SELECT count(*), coalesce(sum(amount_minor) FILTER (WHERE status = 'redeemed'), 0)::bigint,
                coalesce(sum(amount_minor) FILTER (WHERE status = 'issued' AND expires_at > now()), 0)::bigint
         FROM farm_vouchers",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(issued, 3, "three vouchers issued");
    let total: i64 = sqlx::query_scalar("SELECT sum(amount_minor)::bigint FROM farm_vouchers")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(total, 1_000_000, "TZS 10,000 issued");
    assert_eq!(redeemed, 200_000, "TZS 2,000 redeemed");
    assert_eq!(outstanding, 800_000, "TZS 8,000 outstanding");
    let by: String = sqlx::query_scalar(
        "SELECT concat_ws(' ', u.first_name, u.last_name) FROM farm_vouchers v JOIN users u ON u.id = v.redeemed_by",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(by, "Juma Ally", "redeemed by Juma");
    let superseded: i64 =
        sqlx::query_scalar("SELECT count(*) FROM farm_harvest_reports WHERE NOT is_current")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(superseded, 1, "one revised estimate stays on record");
}

#[sqlx::test]
async fn the_demo_seeds_energy_and_the_market(pool: PgPool) {
    let report = seeded(&pool).await;
    let app = common::app(pool.clone());
    let (status, res) = send(
        &app,
        Method::POST,
        "/v1/login",
        Some(json!({ "email": "asha.ops@ruaha-demo.test", "password": report.staff_password.unwrap() })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    let ops = res["data"]["access_token"].as_str().unwrap().to_owned();
    let ilundo: String =
        sqlx::query_scalar("SELECT id::text FROM farm_villages WHERE code = 'ILUNDO'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let uri = format!(
        "/v1/workspace/{}/farm/villages/{ilundo}/energy",
        report.workspace_id
    );
    let (status, e) = send(&app, Method::GET, &uri, None, Some(&ops)).await;
    assert_eq!(status, StatusCode::OK, "{e}");
    // (15.0 + 2 × 1.5) × 0.600
    assert_eq!(e["data"]["approved_peak_kw"], "10.800", "{e}");
    assert_eq!(e["data"]["prospective_kw_raw"], "15.000", "{e}");
    let (requests, purchases): (i64, i64) = sqlx::query_as(
        "SELECT count(*), count(*) FILTER (WHERE acquisition = 'buy') FROM farm_pue_requests",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        (requests, purchases),
        (3, 3),
        "every seeded request is a purchase"
    );
    let offered: String =
        sqlx::query_scalar("SELECT offered_quantity_kg::text FROM farm_opportunities")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(offered, "3000.00");
    let loans: i64 = sqlx::query_scalar("SELECT count(*) FROM farm_loan_products")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(loans, 1);
}

/// The staff password is not a fixed string from the source.
fn seeded_fresh_passwords_differ(pw: &str) -> bool {
    pw.starts_with("Demo-")
        && pw.len() == 26
        && !std::fs::read_to_string("src/farm_demo.rs")
            .unwrap()
            .contains(pw)
}

#[sqlx::test]
async fn a_second_run_changes_nothing(pool: PgPool) {
    let first = seeded(&pool).await;
    let persons: i64 = sqlx::query_scalar("SELECT count(*) FROM farm_persons")
        .fetch_one(&pool)
        .await
        .unwrap();
    let again = seeded(&pool).await;
    assert!(!again.created);
    assert!(
        again.staff_password.is_none(),
        "no new passwords on a no-op run"
    );
    assert_eq!(again.workspace_id, first.workspace_id);
    let after: i64 = sqlx::query_scalar("SELECT count(*) FROM farm_persons")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(persons, after);
}

#[sqlx::test]
async fn a_real_workspace_of_that_name_is_never_seeded(pool: PgPool) {
    let app = common::app(pool.clone());
    signup(&app, "real@customer.test", farm_demo::WORKSPACE).await;
    let err = farm_demo::run(common::state(pool.clone(), Mailer::capture().0))
        .await
        .err()
        .expect("refused");
    assert!(err.to_string().contains("not a demo workspace"), "{err}");
    let persons: i64 = sqlx::query_scalar("SELECT count(*) FROM farm_persons")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(persons, 0);
}
