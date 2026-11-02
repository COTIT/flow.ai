//! Farmer passbook (append-only credits and debits, running balance per currency) and the
//! passport consent flag.

use crate::common;

use axum::http::{Method, StatusCode};
use common::{
    farm::{Cast, Farm},
    signup,
};
use serde_json::{Value, json};
use sqlx::PgPool;

async fn setup(pool: PgPool) -> (Farm, Cast, String) {
    let f = Farm::new(pool).await;
    let c = f.cast().await;
    let tzs = f.tzs().await;
    (f, c, tzs)
}

/// Write an entry straight to the table (credits come from sales, debits from payouts).
async fn entry(f: &Farm, person: &str, tzs: &str, kind: &str, amount: i64) {
    sqlx::query(
        "INSERT INTO farm_passbook_entries (workspace_id, village_id, person_id, currency_id,
                                            kind, amount)
         SELECT workspace_id, village_id, id, $2::uuid, $3, $4 FROM farm_persons
         WHERE id = $1::uuid",
    )
    .bind(person)
    .bind(tzs)
    .bind(kind)
    .bind(amount)
    .execute(&f.pool)
    .await
    .unwrap();
}

fn amounts(res: &Value) -> Vec<(i64, i64)> {
    res["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                e["amount"]["minor"].as_i64().unwrap(),
                e["balance_after"]["minor"].as_i64().unwrap(),
            )
        })
        .collect()
}

#[sqlx::test]
async fn a_farmer_reads_their_passbook_newest_first_with_a_running_balance(pool: PgPool) {
    let (f, c, tzs) = setup(pool).await;
    entry(&f, &c.neema.person, &tzs, "sale_credit", 100_000).await;
    entry(&f, &c.neema.person, &tzs, "payout", -40_000).await;
    entry(&f, &c.neema.person, &tzs, "sale_credit", 2_550).await;
    entry(&f, &c.asha.person, &tzs, "sale_credit", 999).await;
    let neema = f
        .farmer_for("neema@ruaha.test", &f.ilundo, &c.neema.person)
        .await;

    let (status, res) = f.call(Method::GET, "/passbook/mine", None, &neema).await;

    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(
        amounts(&res),
        [(2_550, 62_550), (-40_000, 60_000), (100_000, 100_000)]
    );
    let first = &res["data"][0];
    assert_eq!(first["kind"], "sale_credit");
    assert_eq!(first["amount"]["currency"], "TZS");
    assert_eq!(first["amount"]["decimals"], 2);
    assert_eq!(res["meta"]["total"], 3);

    let (_, page) = f
        .call(
            Method::GET,
            "/passbook/mine?page=2&page_size=2",
            None,
            &neema,
        )
        .await;
    assert_eq!(
        amounts(&page),
        [(100_000, 100_000)],
        "balance survives paging"
    );
}

#[sqlx::test]
async fn staff_of_the_village_read_a_persons_passbook_nobody_else(pool: PgPool) {
    let (f, c, tzs) = setup(pool).await;
    entry(&f, &c.neema.person, &tzs, "sale_credit", 500).await;
    let tail = format!("/persons/{}/passbook", c.neema.person);

    let (status, res) = f.call(Method::GET, &tail, None, &f.officer_ilu).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(amounts(&res), [(500, 500)]);
    let (status, _) = f.call(Method::GET, &tail, None, &f.ops).await;
    assert_eq!(status, StatusCode::OK);

    let (status, _) = f.call(Method::GET, &tail, None, &f.officer_mga).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "other village");
    let asha = f
        .farmer_for("asha@ruaha.test", &f.ilundo, &c.asha.person)
        .await;
    let (status, _) = f.call(Method::GET, &tail, None, &asha).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "another farmer");
    let (_, mine) = f.call(Method::GET, "/passbook/mine", None, &asha).await;
    assert_eq!(mine["data"], json!([]));

    let (outsider, _) = signup(&f.app, "out@else.test", "elsewhere").await;
    let (status, _) = f.call(Method::GET, &tail, None, &outsider).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "other workspace");
    let other = Farm::new_named(f.pool.clone(), "else.test", "elsewhere-farm").await;
    let (status, _) = other
        .call(
            Method::GET,
            &format!("/persons/{}/passbook", c.neema.person),
            None,
            &other.ops,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "other workspace's staff");

    let (status, _) = f.call(Method::GET, "/passbook/mine", None, &f.ops).await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "staff account is no registered person"
    );
}

#[sqlx::test]
async fn passbook_entries_are_append_only(pool: PgPool) {
    let (f, c, tzs) = setup(pool).await;
    entry(&f, &c.neema.person, &tzs, "sale_credit", 500).await;
    for sql in [
        "UPDATE farm_passbook_entries SET amount = 1",
        "DELETE FROM farm_passbook_entries",
        "TRUNCATE farm_passbook_entries",
    ] {
        assert!(sqlx::query(sql).execute(&f.pool).await.is_err(), "{sql}");
    }
    let zero = sqlx::query(
        "INSERT INTO farm_passbook_entries (workspace_id, village_id, person_id, currency_id,
                                            kind, amount)
         SELECT workspace_id, village_id, id, $2::uuid, 'sale_credit', 0 FROM farm_persons
         WHERE id = $1::uuid",
    )
    .bind(&c.neema.person)
    .bind(&tzs)
    .execute(&f.pool)
    .await;
    assert!(zero.is_err(), "a zero line means nothing");
}

#[sqlx::test]
async fn passport_consent_is_off_by_default_and_the_farmer_or_village_staff_set_it(pool: PgPool) {
    let (f, c, _) = setup(pool).await;
    let tail = format!("/persons/{}/passport-consent", c.neema.person);
    let neema = f
        .farmer_for("neema@ruaha.test", &f.ilundo, &c.neema.person)
        .await;

    let off: bool =
        sqlx::query_scalar("SELECT passport_profile_consent FROM farm_persons WHERE id = $1::uuid")
            .bind(&c.neema.person)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert!(!off);

    let (status, res) = f.call(Method::POST, &tail, None, &neema).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res["data"]["consent"], true);
    assert!(res["data"]["consent_at"].is_string());

    let (status, res) = f.call(Method::DELETE, &tail, None, &f.officer_ilu).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res["data"]["consent"], false);
    assert!(res["data"]["consent_at"].is_null());

    let (status, _) = f.call(Method::POST, &tail, None, &f.officer_mga).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "other village");
    let asha = f
        .farmer_for("asha@ruaha.test", &f.ilundo, &c.asha.person)
        .await;
    let (status, _) = f.call(Method::POST, &tail, None, &asha).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "another farmer");
}
