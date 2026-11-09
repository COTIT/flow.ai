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

#[sqlx::test]
async fn village_staff_or_the_farmer_read_passport_consent_nobody_else(pool: PgPool) {
    let (f, c, _) = setup(pool).await;
    let tail = format!("/persons/{}/passport-consent", c.neema.person);
    let neema = f
        .farmer_for("neema@ruaha.test", &f.ilundo, &c.neema.person)
        .await;

    let (status, res) = f.call(Method::GET, &tail, None, &f.officer_ilu).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res["data"]["consent"], false);
    assert!(res["data"]["consent_at"].is_null());

    f.call(Method::POST, &tail, None, &neema).await;
    let (status, res) = f.call(Method::GET, &tail, None, &f.ops).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res["data"]["consent"], true);
    assert!(res["data"]["consent_at"].is_string());
    let (status, _) = f.call(Method::GET, &tail, None, &neema).await;
    assert_eq!(status, StatusCode::OK, "the farmer themself");

    let (status, _) = f.call(Method::GET, &tail, None, &f.officer_mga).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "other village");
    let asha = f
        .farmer_for("asha@ruaha.test", &f.ilundo, &c.asha.person)
        .await;
    let (status, _) = f.call(Method::GET, &tail, None, &asha).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "another farmer");
    let other = Farm::new_named(f.pool.clone(), "else.test", "elsewhere-farm").await;
    let (status, _) = other.call(Method::GET, &tail, None, &other.ops).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "other workspace's staff");
    let (status, _) = f
        .call(
            Method::GET,
            "/persons/not-a-uuid/passport-consent",
            None,
            &f.ops,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

/// Write a line created at `at` (RFC 3339); lines can't be moved once written.
async fn dated(f: &Farm, person: &str, tzs: &str, kind: &str, amount: i64, at: &str) {
    sqlx::query(
        "INSERT INTO farm_passbook_entries (workspace_id, village_id, person_id, currency_id,
                                            kind, amount, created_at)
         SELECT workspace_id, village_id, id, $2::uuid, $3, $4, $5::timestamptz FROM farm_persons
         WHERE id = $1::uuid",
    )
    .bind(person)
    .bind(tzs)
    .bind(kind)
    .bind(amount)
    .bind(at)
    .execute(&f.pool)
    .await
    .unwrap();
}

#[sqlx::test]
async fn a_passbook_reads_one_tanzania_date_range_with_balances_over_every_line(pool: PgPool) {
    let (f, c, tzs) = setup(pool).await;
    for (amount, at) in [
        (100, "2026-09-30T20:59:59Z"), // 30 Sep 23:59 in Tanzania
        (200, "2026-09-30T21:00:00Z"), // 1 Oct 00:00
        (-50, "2026-10-31T20:59:59Z"), // 31 Oct 23:59
        (400, "2026-10-31T21:00:00Z"), // 1 Nov 00:00
    ] {
        let kind = if amount < 0 { "payout" } else { "sale_credit" };
        dated(&f, &c.neema.person, &tzs, kind, amount, at).await;
    }
    let tail = format!("/persons/{}/passbook", c.neema.person);

    let res = f
        .list(&format!("{tail}?from=2026-10-01&to=2026-10-31"), &f.ops)
        .await;
    assert_eq!(
        amounts(&res),
        [(-50, 250), (200, 300)],
        "running balance kept"
    );
    assert_eq!(res["meta"]["total"], 2);
    assert_eq!(res["balances"][0]["minor"], 650, "balance over every line");

    let res = f.list(&format!("{tail}?from=2026-10-01"), &f.ops).await;
    assert_eq!(res["meta"]["total"], 3, "open end");
    let res = f.list(&format!("{tail}?to=2026-09-30"), &f.ops).await;
    assert_eq!(amounts(&res), [(100, 100)], "open start");
    let res = f.list(&format!("{tail}?from=&to="), &f.ops).await;
    assert_eq!(res["meta"]["total"], 4, "blank = no bound");

    for bad in [
        "from=2026-13-01",
        "to=yesterday",
        "from=2026-11-01&to=2026-10-01",
    ] {
        let (status, res) = f
            .call(Method::GET, &format!("{tail}?{bad}"), None, &f.ops)
            .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{bad}: {res}");
    }
}

#[sqlx::test]
async fn staff_see_what_is_owed_per_farmer_and_in_total_within_their_villages(pool: PgPool) {
    let (f, c, tzs) = setup(pool).await;
    entry(&f, &c.neema.person, &tzs, "sale_credit", 5_000).await;
    entry(&f, &c.neema.person, &tzs, "payout", -1_500).await;
    entry(&f, &c.asha.person, &tzs, "sale_credit", 700).await;
    entry(&f, &c.rehema.person, &tzs, "sale_credit", 300).await;
    entry(&f, &c.rehema.person, &tzs, "payout", -300).await; // paid up: not listed
    entry(&f, &c.joseph.person, &tzs, "sale_credit", 900).await; // Mgama

    let owed = |res: &Value| -> Vec<(String, i64)> {
        res["data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| {
                (
                    r["person_id"].as_str().unwrap().to_owned(),
                    r["balance"]["minor"].as_i64().unwrap(),
                )
            })
            .collect()
    };

    let res = f.list("/passbook/balances", &f.ops).await;
    assert_eq!(
        owed(&res),
        [
            (c.neema.person.clone(), 3_500),
            (c.joseph.person.clone(), 900),
            (c.asha.person.clone(), 700),
        ],
        "largest first, zero balances left out"
    );
    assert_eq!(res["totals"][0]["minor"], 5_100);
    assert_eq!(res["totals"][0]["currency"], "TZS");
    assert_eq!(res["meta"]["total"], 3);
    assert!(res["data"][0]["name"].is_string());
    assert!(res["data"][0]["village"].is_string());

    let res = f
        .list(
            &format!("/passbook/balances?village_id={}", f.mgama),
            &f.ops,
        )
        .await;
    assert_eq!(owed(&res), [(c.joseph.person.clone(), 900)]);
    assert_eq!(res["totals"][0]["minor"], 900);

    let res = f.list("/passbook/balances?page_size=1", &f.ops).await;
    assert_eq!(res["data"].as_array().unwrap().len(), 1);
    assert_eq!(res["totals"][0]["minor"], 5_100, "totals over every page");

    let res = f.list("/passbook/balances", &f.officer_mga).await;
    assert_eq!(
        owed(&res),
        [(c.joseph.person.clone(), 900)],
        "own villages only"
    );
    assert_eq!(res["totals"][0]["minor"], 900);
    let res = f
        .list(
            &format!("/passbook/balances?village_id={}", f.ilundo),
            &f.officer_mga,
        )
        .await;
    assert_eq!(res["data"], json!([]), "a village outside the officer's");

    let asha = f
        .farmer_for("asha@ruaha.test", &f.ilundo, &c.asha.person)
        .await;
    let (status, _) = f.call(Method::GET, "/passbook/balances", None, &asha).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "farmers see only their own passbook"
    );
    let (status, _) = f
        .call(Method::GET, "/passbook/balances?village_id=x", None, &f.ops)
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    let other = Farm::new_named(f.pool.clone(), "else.test", "elsewhere-farm").await;
    let res = other.list("/passbook/balances", &other.ops).await;
    assert_eq!(res["data"], json!([]), "another workspace sees none of it");
    assert_eq!(res["totals"], json!([]));
}
