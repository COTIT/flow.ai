//! /v1/workspace/{ws}/settings/vat-groups: CRUD, `country` + `q` filters, vat_amount as a
//! Laravel MoneyHelper string (vimdesk-web vat-groups.service.js).

mod common;

use axum::http::{Method, StatusCode};
use common::Api;
use serde_json::{Value, json};
use sqlx::PgPool;

const BASE: &str = "/settings/vat-groups";
const ID: &str = "vat_group_id";

fn high() -> Value {
    json!({ "name": "High Rate", "vat_amount": 21, "default_group": true, "country": "nl", "meta": { "k": 1 } })
}

async fn api(pool: PgPool) -> Api {
    Api::new(pool, BASE).await
}

#[sqlx::test]
async fn create_then_get_in_vat_group_resource_shape(pool: PgPool) {
    let a = api(pool).await;

    let id = a.create(high(), ID).await;

    let g = a.get(&id).await;
    let keys: Vec<&str> = g.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(
        keys,
        [
            "country",
            "created_at",
            "default_group",
            "id",
            "last_updated_at",
            "meta",
            "name",
            "vat_amount"
        ]
    );
    assert_eq!(g["id"], id.as_str());
    assert_eq!(g["name"], "High Rate");
    assert_eq!(g["vat_amount"], "21");
    assert_eq!(g["default_group"], true);
    assert_eq!(g["country"], "NL", "stored uppercase");
    assert_eq!(g["meta"], json!({ "k": 1 }));
}

#[sqlx::test]
async fn vat_amount_prints_like_money_helper(pool: PgPool) {
    let a = api(pool).await;

    for (sent, shown) in [
        (json!(5.5), "5.5"),
        (json!("9.25"), "9.25"),
        (json!("9"), "9"),
        // Laravel cast vat_amount to float, so zero was falsy: "0.00".
        (json!(0), "0.00"),
        (json!("12.345"), "12.35"),
        (json!("100"), "100"),
    ] {
        let id = a
            .create(json!({ "name": "R", "vat_amount": sent }), ID)
            .await;
        assert_eq!(a.get(&id).await["vat_amount"], shown, "{sent}");
    }
}

#[sqlx::test]
async fn rules_and_bounds(pool: PgPool) {
    let a = api(pool).await;

    let (status, res) = a.call(Method::POST, "", Some(json!({})), &a.owner).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"],
        json!({
            "name": ["The name field is required."],
            "vat_amount": ["The vat amount field is required."],
        })
    );

    for (vat, msg) in [
        (
            json!("100.00000001"),
            "The vat amount must not be greater than 100.",
        ),
        (json!(-1), "The vat amount must be at least 0."),
        (
            json!("1.123456789"),
            "The vat amount must have 0-8 decimal places.",
        ),
        (json!("abc"), "The vat amount must be a number."),
    ] {
        let (status, res) = a
            .call(
                Method::POST,
                "",
                Some(json!({ "name": "R", "vat_amount": vat })),
                &a.owner,
            )
            .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{vat}");
        assert_eq!(res["errors"]["vat_amount"], json!([msg]), "{vat}");
    }

    let (status, res) = a
        .call(
            Method::POST,
            "",
            Some(
                json!({ "name": "R", "vat_amount": 9, "country": "NO_LAND", "default_group": "x" }),
            ),
            &a.owner,
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"],
        json!({
            "country": ["country is not valid"],
            "default_group": ["The default group field must be true or false."],
        })
    );

    let id = a
        .create(
            json!({ "name": "é".repeat(255), "vat_amount": "100.00000000" }),
            ID,
        )
        .await;
    let g = a.get(&id).await;
    assert_eq!(
        (g["default_group"].clone(), g["country"].clone()),
        (json!(false), Value::Null)
    );
}

#[sqlx::test]
async fn update_needs_name_and_rate_and_patches_the_rest(pool: PgPool) {
    let a = api(pool).await;
    let id = a.create(high(), ID).await;

    let (status, res) = a.update(&id, json!({ "country": "BE" })).await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "Laravel update rules require both"
    );
    assert_eq!(
        res["errors"],
        json!({
            "name": ["The name field is required."],
            "vat_amount": ["The vat amount field is required."],
        })
    );

    let (status, res) = a
        .update(
            &id,
            json!({ "name": "Vat Group Low", "vat_amount": "9", "country": null }),
        )
        .await;
    assert_eq!((status, res), (StatusCode::OK, json!({ "message": "ok" })));
    let g = a.get(&id).await;
    assert_eq!(g["name"], "Vat Group Low");
    assert_eq!(g["vat_amount"], "9");
    assert_eq!(g["default_group"], true, "absent keeps");
    assert_eq!(g["country"], Value::Null, "null clears");
    assert_eq!(g["meta"], json!({ "k": 1 }));
}

#[sqlx::test]
async fn country_and_search_filters_combine(pool: PgPool) {
    let a = api(pool).await;
    a.create(
        json!({ "name": "NL High", "vat_amount": 21, "country": "NL" }),
        ID,
    )
    .await;
    a.create(
        json!({ "name": "NL Low", "vat_amount": 9, "country": "NL" }),
        ID,
    )
    .await;
    a.create(
        json!({ "name": "BE High", "vat_amount": 21, "country": "BE" }),
        ID,
    )
    .await;

    let names = |res: Value| -> Vec<String> {
        res["data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|g| g["name"].as_str().unwrap().to_owned())
            .collect()
    };
    assert_eq!(names(a.list("?country=nl").await), ["NL High", "NL Low"]);
    assert_eq!(names(a.list("?country=NL&q=high").await), ["NL High"]);
    assert_eq!(names(a.list("?q=HIGH").await), ["NL High", "BE High"]);
    assert_eq!(a.list("").await["meta"]["total"], 3);

    let (status, res) = a
        .call(Method::GET, "?country=NO_LAND", None, &a.owner)
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(res["errors"]["country"], json!(["country is not valid"]));
}

#[sqlx::test]
async fn delete_bad_ids_and_access(pool: PgPool) {
    let a = api(pool).await;
    let id = a.create(high(), ID).await;
    let gone = a.create(high(), ID).await;
    a.delete(&gone).await;
    let update = json!({ "name": "pwned", "vat_amount": 1 });

    a.assert_bad_ids_404(&gone, update.clone()).await;
    a.assert_foreign_404(&id, update).await;
    a.assert_member_forbidden(&id).await;
    a.assert_granted(
        &[
            "vatgroup.create",
            "vatgroup.list",
            "vatgroup.get",
            "vatgroup.update",
            "vatgroup.delete",
        ],
        json!({ "name": "R", "vat_amount": 1 }),
        ID,
    )
    .await;
}
