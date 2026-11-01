//! Loan LISTINGS in the resource catalogue (RUAHA `loan_product`): a name, a description and an
//! indicative amount range. No interest, deposit, term, schedule or repayment (research item
//! C). RUAHA ids in docs/ruaha-rls-mapping.md.

mod common;

use axum::http::{Method, StatusCode};
use common::{farm::Farm, send};
use serde_json::{Value, json};
use sqlx::PgPool;

fn listing(tzs: &str) -> Value {
    json!({ "code": "INPUTS", "name_en": "Input loan", "name_sw": "Mkopo wa pembejeo",
        "description_en": "Seed and fertiliser", "indicative_min_minor": 5_000_000,
        "indicative_max_minor": 50_000_000, "currency_id": tzs })
}

#[sqlx::test]
async fn ops_adds_a_loan_listing_and_every_member_reads_it(pool: PgPool) {
    let f = Farm::new(pool).await;
    let tzs = f.tzs().await;
    let (status, res) = f
        .call(Method::POST, "/loan-products", Some(listing(&tzs)), &f.ops)
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let l = &res["data"];
    assert_eq!(l["code"], "INPUTS");
    assert_eq!(l["indicative_min"]["minor"], 5_000_000);
    assert_eq!(l["indicative_max"]["minor"], 50_000_000);
    assert_eq!(l["indicative_max"]["currency"], "TZS");
    assert_eq!(l["is_active"], true);
    let neema = f.farmer("neema@ruaha.test", &f.ilundo).await;
    for token in [&neema, &f.officer_ilu] {
        let list = f.list("/loan-products?active=true", token).await;
        assert_eq!(list["meta"]["total"], 1, "{list}");
    }
}

#[sqlx::test]
async fn a_loan_range_cannot_run_backwards_or_from_zero(pool: PgPool) {
    let f = Farm::new(pool).await;
    let tzs = f.tzs().await;
    for (min, max) in [(50_000_000, 5_000_000), (0, 5_000_000)] {
        let mut body = listing(&tzs);
        body["indicative_min_minor"] = json!(min);
        body["indicative_max_minor"] = json!(max);
        let (status, res) = f
            .call(Method::POST, "/loan-products", Some(body), &f.ops)
            .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
        assert!(
            res["errors"]["indicative_min_minor"].is_array()
                || res["errors"]["indicative_max_minor"].is_array(),
            "{res}"
        );
    }
}

#[sqlx::test]
async fn officers_and_farmers_cannot_add_a_loan_listing(pool: PgPool) {
    let f = Farm::new(pool).await;
    let tzs = f.tzs().await;
    let neema = f.farmer("neema@ruaha.test", &f.ilundo).await;
    for token in [&f.officer_ilu, &neema] {
        let (status, _) = f
            .call(Method::POST, "/loan-products", Some(listing(&tzs)), token)
            .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }
}

#[sqlx::test]
async fn a_loan_listing_is_never_deleted_and_carries_no_finance_terms(pool: PgPool) {
    let f = Farm::new(pool).await;
    let tzs = f.tzs().await;
    let (_, res) = f
        .call(Method::POST, "/loan-products", Some(listing(&tzs)), &f.ops)
        .await;
    let id = res["data"]["id"].as_str().unwrap().to_owned();
    let (status, _) = send(
        &f.app,
        Method::DELETE,
        &f.url(&format!("/loan-products/{id}")),
        None,
        Some(&f.ops),
    )
    .await;
    assert!(
        status == StatusCode::METHOD_NOT_ALLOWED || status == StatusCode::NOT_FOUND,
        "{status}"
    );
    assert_eq!(f.list("/loan-products", &f.ops).await["meta"]["total"], 1);
    // Research item C: nothing here models lending.
    let terms: Vec<String> = sqlx::query_scalar(
        "SELECT column_name::text FROM information_schema.columns
         WHERE table_name = 'farm_loan_products'
           AND column_name ~ '(interest|rate|deposit|term|schedule|repay|tenor|apr)'",
    )
    .fetch_all(&f.pool)
    .await
    .unwrap();
    assert!(terms.is_empty(), "{terms:?}");
    for key in res["data"].as_object().unwrap().keys() {
        assert!(
            !["interest", "rate", "deposit", "term", "schedule", "repay"]
                .iter()
                .any(|t| key.contains(t)),
            "{key}"
        );
    }
}

#[sqlx::test]
async fn a_loan_code_is_unique_and_retries_replay(pool: PgPool) {
    let f = Farm::new(pool).await;
    let tzs = f.tzs().await;
    let mut body = listing(&tzs);
    body["id"] = json!("019a0000-0000-7000-8000-0000000000a9");
    let (status, _) = f
        .call(Method::POST, "/loan-products", Some(body.clone()), &f.ops)
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, _) = f
        .call(Method::POST, "/loan-products", Some(body.clone()), &f.ops)
        .await;
    assert_eq!(status, StatusCode::OK);
    body["name_en"] = json!("Other");
    let (status, _) = f
        .call(Method::POST, "/loan-products", Some(body), &f.ops)
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, res) = f
        .call(Method::POST, "/loan-products", Some(listing(&tzs)), &f.ops)
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert!(res["errors"]["code"].is_array(), "{res}");
}
