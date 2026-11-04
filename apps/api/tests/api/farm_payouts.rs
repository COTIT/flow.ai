//! Payout desk (G13) and daily cash reconciliation: staff pay a farmer from their passbook
//! balance under the voucher four-eyes rules; voids and every cash movement are new rows.

use crate::common;

use axum::http::{Method, StatusCode};
use chrono::{FixedOffset, Utc};
use common::{
    farm::{Cast, Farm, client_ref},
    send, signup,
};
use serde_json::{Value, json};
use sqlx::PgPool;

struct Desk {
    f: Farm,
    c: Cast,
    tzs: String,
    /// Second Ilundo officer: registered and verified nobody.
    juma: String,
    juma_id: String,
}

async fn desk(pool: PgPool) -> Desk {
    let f = Farm::new(pool).await;
    let c = f.cast().await;
    let tzs = f.tzs().await;
    let ilu = f.ilundo.clone();
    let juma = f
        .staff("juma@ruaha.test", "field_officer", Some(&ilu))
        .await;
    let juma_id = f.user_id("juma@ruaha.test").await;
    credit(&f, &c.neema.person, &tzs, 1_000_000).await;
    Desk {
        f,
        c,
        tzs,
        juma,
        juma_id,
    }
}

/// A sale credit straight into the passbook (sales write these from a later step).
async fn credit(f: &Farm, person: &str, tzs: &str, amount: i64) {
    sqlx::query(
        "INSERT INTO farm_passbook_entries (workspace_id, village_id, person_id, currency_id,
                                            kind, amount)
         SELECT workspace_id, village_id, id, $2::uuid, 'sale_credit', $3 FROM farm_persons
         WHERE id = $1::uuid",
    )
    .bind(person)
    .bind(tzs)
    .bind(amount)
    .execute(&f.pool)
    .await
    .unwrap();
}

fn payout(d: &Desk, amount: i64) -> Value {
    json!({ "client_ref": client_ref(), "person_id": d.c.neema.person, "amount": amount,
            "currency_id": d.tzs, "id_type": "nida", "name_confirmed": true })
}

async fn pay(d: &Desk, body: &Value, token: &str) -> (StatusCode, Value) {
    d.f.call(Method::POST, "/payouts", Some(body.clone()), token)
        .await
}

async fn balance(d: &Desk) -> i64 {
    let (status, res) =
        d.f.call(
            Method::GET,
            &format!("/persons/{}/passbook", d.c.neema.person),
            None,
            &d.f.ops,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    res["data"][0]["balance_after"]["minor"].as_i64().unwrap()
}

fn today() -> String {
    let eat = FixedOffset::east_opt(3 * 3600).unwrap();
    Utc::now().with_timezone(&eat).date_naive().to_string()
}

#[sqlx::test]
async fn an_officer_pays_from_the_balance_and_a_retry_pays_once(pool: PgPool) {
    let d = desk(pool).await;
    let body = payout(&d, 300_000);

    let (status, res) = pay(&d, &body, &d.juma).await;

    assert_eq!(status, StatusCode::CREATED, "{res}");
    assert_eq!(res["data"]["amount"]["minor"], 300_000);
    assert_eq!(res["data"]["amount"]["currency"], "TZS");
    assert_eq!(res["data"]["paid_by"], d.juma_id.as_str());
    assert_eq!(balance(&d).await, 700_000);

    let (status, again) = pay(&d, &body, &d.juma).await;
    assert_eq!(status, StatusCode::OK, "{again}");
    assert_eq!(again["data"], res["data"]);
    let mut other = body.clone();
    other["amount"] = json!(300_001);
    let (status, _) = pay(&d, &other, &d.juma).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(balance(&d).await, 700_000, "paid once");
}

#[sqlx::test]
async fn whoever_registered_verified_or_belongs_to_the_household_cannot_pay(pool: PgPool) {
    let d = desk(pool).await;
    // Salima registered Neema's household.
    let (status, res) = pay(&d, &payout(&d, 1_000), &d.f.officer_ilu).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{res}");
    assert_eq!(
        res["message"],
        "you registered this household, so another staff member must pay this farmer"
    );

    d.f.verify("households", &d.c.neema.household, &d.f.ops)
        .await;
    let (status, res) = pay(&d, &payout(&d, 1_000), &d.f.ops).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{res}");
    assert_eq!(
        res["message"],
        "you verified this household, so another staff member must pay this farmer"
    );

    // Juma turns out to be Rehema, of Neema's household.
    d.f.link("juma@ruaha.test", &d.c.rehema.person).await;
    let (status, res) = pay(&d, &payout(&d, 1_000), &d.juma).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{res}");
    assert_eq!(res["message"], "you cannot pay your own household");
    assert_eq!(balance(&d).await, 1_000_000);
}

#[sqlx::test]
async fn never_more_than_the_balance_and_never_from_a_negative_one(pool: PgPool) {
    let d = desk(pool).await;
    let (status, res) = pay(&d, &payout(&d, 1_000_001), &d.juma).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(
        res["errors"]["amount"][0],
        "The amount must not be greater than the balance of 1000000."
    );
    let (status, _) = pay(&d, &payout(&d, 1_000_000), &d.juma).await;
    assert_eq!(status, StatusCode::CREATED, "the whole balance");

    credit(&d.f, &d.c.neema.person, &d.tzs, -5).await;
    let (status, res) = pay(&d, &payout(&d, 1), &d.juma).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");

    for (field, bad) in [
        ("amount", json!(0)),
        ("id_type", json!("passport")),
        ("name_confirmed", json!(false)),
    ] {
        let mut b = payout(&d, 1);
        b[field] = bad;
        let (status, res) = pay(&d, &b, &d.juma).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{field}: {res}");
        assert!(res["errors"][field].is_array(), "{field}: {res}");
    }
}

#[sqlx::test]
async fn officers_pay_up_to_the_workspace_limit_ops_above_it(pool: PgPool) {
    let d = desk(pool).await;
    let (status, res) = send(
        &d.f.app,
        Method::POST,
        &format!("/v1/workspace/{}/settings", d.f.ws),
        Some(json!({ "farm": { "payout_officer_limit": 200_000 } })),
        Some(&d.f.owner),
    )
    .await;
    assert!(status.is_success(), "{res}");

    let (status, res) = pay(&d, &payout(&d, 200_000), &d.juma).await;
    assert_eq!(status, StatusCode::CREATED, "at the limit: {res}");
    let (status, res) = pay(&d, &payout(&d, 200_001), &d.juma).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "above it: {res}");
    assert_eq!(
        res["message"],
        "this payout is above the officer limit: ops or admin must pay it"
    );
    let (status, res) = pay(&d, &payout(&d, 200_001), &d.f.ops).await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
}

#[sqlx::test]
async fn parallel_payouts_never_overdraw(pool: PgPool) {
    let d = desk(pool).await;
    let mut set = tokio::task::JoinSet::new();
    for _ in 0..5 {
        let (app, url, token, body) = (
            d.f.app.clone(),
            d.f.url("/payouts"),
            d.juma.clone(),
            payout(&d, 300_000),
        );
        set.spawn(async move { send(&app, Method::POST, &url, Some(body), Some(&token)).await });
    }
    let results = set.join_all().await;
    let paid = results
        .iter()
        .filter(|(s, _)| *s == StatusCode::CREATED)
        .count();
    assert_eq!(paid, 3, "{results:?}");
    assert_eq!(balance(&d).await, 100_000);
}

#[sqlx::test]
async fn only_staff_of_the_village_pay_and_nothing_crosses_workspaces(pool: PgPool) {
    let d = desk(pool).await;
    let (status, _) = pay(&d, &payout(&d, 1), &d.f.officer_mga).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "other village");
    let neema =
        d.f.farmer_for("neema@ruaha.test", &d.f.ilundo, &d.c.neema.person)
            .await;
    let (status, _) = pay(&d, &payout(&d, 1), &neema).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "a farmer pays nobody");

    let other = Farm::new_named(d.f.pool.clone(), "else.test", "elsewhere").await;
    let their_tzs = other.tzs().await;
    let (status, _) = other
        .call(Method::POST, "/payouts", Some(payout(&d, 1)), &other.ops)
        .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "our farmer from their workspace"
    );
    let mut b = payout(&d, 1);
    b["currency_id"] = json!(their_tzs);
    let (status, res) = pay(&d, &b, &d.juma).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert!(res["errors"]["currency_id"].is_array());

    let (outsider, _) = signup(&d.f.app, "out@else2.test", "outside").await;
    let (status, _) =
        d.f.call(
            Method::GET,
            &format!("/cash/reconciliation?day={}", today()),
            None,
            &outsider,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(balance(&d).await, 1_000_000);
}

#[sqlx::test]
async fn a_void_adds_reversing_rows_and_nothing_is_ever_edited(pool: PgPool) {
    let d = desk(pool).await;
    let (_, res) = pay(&d, &payout(&d, 300_000), &d.juma).await;
    let id = res["data"]["id"].as_str().unwrap().to_owned();
    let tail = format!("/payouts/{id}/void");

    let (status, _) =
        d.f.call(
            Method::POST,
            &tail,
            Some(json!({ "reason": "wrong farmer" })),
            &d.juma,
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "officers don't void");
    let (status, _) =
        d.f.call(Method::POST, &tail, Some(json!({ "reason": "" })), &d.f.ops)
            .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "a reason is required"
    );
    let (status, res) =
        d.f.call(
            Method::POST,
            &tail,
            Some(json!({ "reason": "wrong farmer" })),
            &d.f.ops,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res["data"]["void"]["reason"], "wrong farmer");
    assert_eq!(balance(&d).await, 1_000_000);
    let (status, _) =
        d.f.call(
            Method::POST,
            &tail,
            Some(json!({ "reason": "again" })),
            &d.f.ops,
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "voided once");

    let kinds: Vec<String> =
        sqlx::query_scalar("SELECT kind FROM farm_passbook_entries ORDER BY created_at, id")
            .fetch_all(&d.f.pool)
            .await
            .unwrap();
    assert_eq!(kinds, ["sale_credit", "payout", "payout_void"]);
    for sql in [
        "UPDATE farm_payouts SET amount = 1",
        "DELETE FROM farm_payouts",
        "UPDATE farm_payout_voids SET reason = 'x'",
        "DELETE FROM farm_payout_voids",
        "UPDATE farm_cash_ledger SET amount = 1",
        "DELETE FROM farm_cash_ledger",
        "TRUNCATE farm_cash_ledger",
    ] {
        assert!(sqlx::query(sql).execute(&d.f.pool).await.is_err(), "{sql}");
    }
}

#[sqlx::test]
async fn the_day_reconciles_float_payouts_returns_and_the_count(pool: PgPool) {
    let d = desk(pool).await;
    let movement = |kind: &str, amount: i64| {
        json!({ "kind": kind, "staff_user_id": d.juma_id, "village_id": d.f.ilundo,
                "currency_id": d.tzs, "amount": amount })
    };
    let (status, res) =
        d.f.call(
            Method::POST,
            "/cash/movements",
            Some(movement("float_out", 1_000_000)),
            &d.f.ops,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let (status, _) =
        d.f.call(
            Method::POST,
            "/cash/movements",
            Some(movement("float_out", 1)),
            &d.juma,
        )
        .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "officers don't hand out floats"
    );

    pay(&d, &payout(&d, 300_000), &d.juma).await;
    let (_, voided) = pay(&d, &payout(&d, 50_000), &d.juma).await;
    let id = voided["data"]["id"].as_str().unwrap();
    d.f.call(
        Method::POST,
        &format!("/payouts/{id}/void"),
        Some(json!({ "reason": "duplicate" })),
        &d.f.ops,
    )
    .await;
    d.f.call(
        Method::POST,
        "/cash/movements",
        Some(movement("float_back", 200_000)),
        &d.f.ops,
    )
    .await;
    let (status, res) =
        d.f.call(
            Method::POST,
            "/cash/movements",
            Some(movement("count", 490_000)),
            &d.juma,
        )
        .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "an officer counts their own cash: {res}"
    );

    let tail = format!(
        "/cash/reconciliation?day={}&village_id={}",
        today(),
        d.f.ilundo
    );
    let (status, res) = d.f.call(Method::GET, &tail, None, &d.f.ops).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    let row = &res["data"][0];
    assert_eq!(row["staff_user_id"], d.juma_id.as_str());
    let m = |k: &str| row[k]["minor"].as_i64();
    assert_eq!(m("opening"), Some(0));
    assert_eq!(m("floats"), Some(1_000_000));
    assert_eq!(row["payouts"], 2);
    assert_eq!(m("paid"), Some(350_000));
    assert_eq!(m("voided"), Some(50_000));
    assert_eq!(m("returned"), Some(200_000));
    assert_eq!(m("expected"), Some(500_000));
    assert_eq!(m("counted"), Some(490_000));
    assert_eq!(m("variance"), Some(-10_000));

    let (_, own) = d.f.call(Method::GET, &tail, None, &d.juma).await;
    assert_eq!(
        own["data"].as_array().unwrap().len(),
        1,
        "an officer sees their own row"
    );
    let (_, theirs) = d.f.call(Method::GET, &tail, None, &d.f.officer_ilu).await;
    assert_eq!(theirs["data"], json!([]), "not another officer's");
}

/// `(meta.total, person ids)` of the payout list `token` sees under `query`.
async fn listed(d: &Desk, token: &str, query: &str) -> (StatusCode, i64, Vec<String>) {
    let (status, res) =
        d.f.call(Method::GET, &format!("/payouts{query}"), None, token)
            .await;
    let people = res["data"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .map(|r| r["person_id"].as_str().unwrap().to_owned())
                .collect()
        })
        .unwrap_or_default();
    (status, res["meta"]["total"].as_i64().unwrap_or(-1), people)
}

#[sqlx::test]
async fn payouts_are_listed_to_staff_of_the_village_and_to_the_farmer_paid(pool: PgPool) {
    let d = desk(pool).await;
    credit(&d.f, &d.c.asha.person, &d.tzs, 500_000).await;
    let (s, neema_paid) = pay(&d, &payout(&d, 300_000), &d.juma).await;
    assert_eq!(s, StatusCode::CREATED, "{neema_paid}");
    let mut b = payout(&d, 100_000);
    b["person_id"] = json!(d.c.asha.person);
    let (s, asha_paid) = pay(&d, &b, &d.juma).await;
    assert_eq!(s, StatusCode::CREATED, "{asha_paid}");
    let (neema_id, asha_id) = (
        neema_paid["data"]["id"].as_str().unwrap().to_owned(),
        asha_paid["data"]["id"].as_str().unwrap().to_owned(),
    );
    let neema_person = d.c.neema.person.clone();

    let (s, total, _) = listed(&d, &d.f.ops, "").await;
    assert_eq!((s, total), (StatusCode::OK, 2), "ops: every payout");
    assert_eq!(listed(&d, &d.juma, "").await.1, 2, "officer of Ilundo");
    let (_, total, people) = listed(&d, &d.f.ops, &format!("?person_id={neema_person}")).await;
    assert_eq!((total, people), (1, vec![neema_person.clone()]));
    assert_eq!(
        listed(&d, &d.f.officer_mga, "").await.1,
        0,
        "officer elsewhere"
    );

    let neema =
        d.f.farmer_for("neema@ruaha.test", &d.f.ilundo, &d.c.neema.person)
            .await;
    let (s, total, people) = listed(&d, &neema, "").await;
    assert_eq!(
        (s, total, people),
        (StatusCode::OK, 1, vec![neema_person.clone()]),
        "her own only"
    );
    let (s, _, _) = listed(&d, &neema, &format!("?person_id={}", d.c.asha.person)).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "another farmer's payouts");

    let get = |token: String, id: String| {
        let f = &d.f;
        async move {
            f.call(Method::GET, &format!("/payouts/{id}"), None, &token)
                .await
        }
    };
    let (s, res) = get(neema.clone(), neema_id.clone()).await;
    assert_eq!(s, StatusCode::OK, "{res}");
    assert_eq!(res["data"]["amount"]["minor"], 300_000);
    assert_eq!(res["data"]["void"], Value::Null);
    assert_eq!(
        get(neema.clone(), asha_id.clone()).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        get(d.f.officer_mga.clone(), neema_id.clone()).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        get(d.f.ops.clone(), asha_id.clone()).await.0,
        StatusCode::OK
    );

    let other = Farm::new_named(d.f.pool.clone(), "else.test", "elsewhere").await;
    let (s, _) = other
        .call(
            Method::GET,
            &format!("/payouts/{neema_id}"),
            None,
            &other.ops,
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND, "another workspace");
    let (s, res) = other.call(Method::GET, "/payouts", None, &other.ops).await;
    assert_eq!(
        (s, res["meta"]["total"].clone()),
        (StatusCode::OK, json!(0))
    );
    let (s, _) =
        d.f.call(Method::GET, "/payouts?person_id=nope", None, &d.f.ops)
            .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);

    // The passbook carries the balance per currency next to its lines.
    for (token, path) in [
        (d.f.ops.clone(), format!("/persons/{neema_person}/passbook")),
        (neema.clone(), "/passbook/mine".to_owned()),
    ] {
        let (s, res) = d.f.call(Method::GET, &path, None, &token).await;
        assert_eq!(s, StatusCode::OK, "{path}: {res}");
        assert_eq!(res["balances"][0]["minor"], 700_000, "{path}");
        assert_eq!(res["balances"][0]["currency"], "TZS");
        assert_eq!(res["balances"].as_array().unwrap().len(), 1);
    }
}
