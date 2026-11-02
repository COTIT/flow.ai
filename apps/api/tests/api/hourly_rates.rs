//! /v1/workspace/{ws}/hourly-rates: CRUD with an optional VAT group of the same workspace;
//! prices printed like Laravel MoneyHelper (vimdesk-web settings/hourly-rate).

use crate::common;

use axum::http::{Method, StatusCode};
use common::{Api, send, signup};
use serde_json::{Value, json};
use sqlx::PgPool;

const BASE: &str = "/hourly-rates";
const ID: &str = "hourly_rate_id";

async fn api(pool: PgPool) -> Api {
    Api::new(pool, BASE).await
}

async fn vat_group(a: &Api, ws: &str, token: &str) -> String {
    let uri = format!("/v1/workspace/{ws}/settings/vat-groups");
    let body = json!({ "name": "High", "vat_amount": "21", "country": "NL" });
    let (status, res) = send(&a.app, Method::POST, &uri, Some(body), Some(token)).await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    res["data"]["vat_group_id"].as_str().unwrap().to_owned()
}

fn rate(vat: &str) -> Value {
    json!({
        "title": "rate 1", "description": "hourly rate", "consumer_price": "1000.00",
        "business_price": "800.00", "vat_group_id": vat, "ledger_number": "test",
        "purchase_price": "40.12", "meta": { "k": 1 },
    })
}

#[sqlx::test]
async fn create_then_get_in_hourly_rate_resource_shape(pool: PgPool) {
    let a = api(pool).await;
    let vat = vat_group(&a, &a.ws, &a.owner).await;

    let id = a.create(rate(&vat), ID).await;

    let r = a.get(&id).await;
    let keys: Vec<&str> = r.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(
        keys,
        [
            "business_price",
            "consumer_price",
            "created_at",
            "description",
            "id",
            "last_updated_at",
            "ledger_number",
            "meta",
            "purchase_price",
            "relationships",
            "title"
        ]
    );
    assert_eq!(r["title"], "rate 1");
    assert_eq!(r["description"], "hourly rate");
    // Laravel test expectations: formatDecimals on the raw decimal column.
    assert_eq!(r["consumer_price"], "1000");
    assert_eq!(r["business_price"], "800");
    assert_eq!(r["purchase_price"], "40.12");
    assert_eq!(r["ledger_number"], "test");
    assert_eq!(r["meta"], json!({ "k": 1 }));
    let g = &r["relationships"]["vat_group"];
    assert_eq!(g["id"], vat.as_str());
    assert_eq!(g["vat_amount"], "21");
    assert_eq!(g["country"], "NL");
}

#[sqlx::test]
async fn zero_and_null_prices_print_like_laravel(pool: PgPool) {
    let a = api(pool).await;
    let vat = vat_group(&a, &a.ws, &a.owner).await;
    let mut body = rate(&vat);
    body["consumer_price"] = json!(0);
    body["business_price"] = json!("12.345");
    body["purchase_price"] = Value::Null;

    let id = a.create(body, ID).await;

    let r = a.get(&id).await;
    // Uncast DB string "0.00000000" was truthy in PHP: "0". A null price was falsy: "0.00".
    assert_eq!(r["consumer_price"], "0");
    assert_eq!(r["business_price"], "12.35");
    assert_eq!(r["purchase_price"], "0.00");
}

#[sqlx::test]
async fn validation_uses_laravel_wording(pool: PgPool) {
    let a = api(pool).await;

    let (status, res) = a.call(Method::POST, "", Some(json!({})), &a.owner).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"],
        json!({
            "business_price": ["The business price field is required."],
            "consumer_price": ["The consumer price field is required."],
            "description": ["The description field is required."],
            "title": ["The title field is required."],
            "vat_group_id": ["The vat group id field is required."],
        })
    );

    let (status, res) = a
        .call(
            Method::POST,
            "",
            Some(json!({
                "title": "t", "description": "d", "consumer_price": -1, "business_price": "x",
                "purchase_price": "-0.01", "ledger_number": "n".repeat(56),
                "vat_group_id": "nope",
            })),
            &a.owner,
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"],
        json!({
            "business_price": ["The business price must be a number."],
            "consumer_price": ["The consumer price must be at least 0."],
            "ledger_number": ["The ledger number must not be greater than 55 characters."],
            "purchase_price": ["The purchase price must be at least 0."],
            "vat_group_id": ["The vat group id is an invalid id for the vat_group domain."],
        })
    );
}

#[sqlx::test]
async fn vat_group_must_be_live_and_in_this_workspace(pool: PgPool) {
    let a = api(pool).await;
    let (eve, eve_ws) = signup(&a.app, "eve@example.com", "evil").await;
    let foreign = vat_group(&a, &eve_ws, &eve).await;
    let gone = vat_group(&a, &a.ws, &a.owner).await;
    let (status, _) = send(
        &a.app,
        Method::DELETE,
        &format!("/v1/workspace/{}/settings/vat-groups/{gone}", a.ws),
        None,
        Some(&a.owner),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    for vat in [
        foreign.as_str(),
        gone.as_str(),
        "01900000-0000-7000-8000-000000000001",
    ] {
        let (status, res) = a.call(Method::POST, "", Some(rate(vat)), &a.owner).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{vat}");
        assert_eq!(
            res["errors"]["vat_group_id"],
            json!(["The vat group id is an invalid id for the vat_group domain."])
        );
    }

    // The database refuses a cross-workspace link even if the app check were skipped.
    let err = sqlx::query(
        "INSERT INTO hourly_rates (workspace_id, title, description, consumer_price, vat_group_id)
         VALUES ($1::uuid, 't', 'd', 1, $2::uuid)",
    )
    .bind(&a.ws)
    .bind(&foreign)
    .execute(&a.pool)
    .await
    .unwrap_err();
    assert_eq!(
        err.as_database_error().and_then(|e| e.constraint()),
        Some("hourly_rates_vat_group_fkey")
    );
}

#[sqlx::test]
async fn soft_deleted_vat_group_renders_null(pool: PgPool) {
    let a = api(pool).await;
    let vat = vat_group(&a, &a.ws, &a.owner).await;
    let id = a.create(rate(&vat), ID).await;

    sqlx::query("UPDATE vat_groups SET deleted_at = now() WHERE id = $1::uuid")
        .bind(&vat)
        .execute(&a.pool)
        .await
        .unwrap();

    assert_eq!(
        a.get(&id).await["relationships"],
        json!({ "vat_group": null })
    );
}

#[sqlx::test]
async fn update_patches_prices_required_vat_optional(pool: PgPool) {
    let a = api(pool).await;
    let vat = vat_group(&a, &a.ws, &a.owner).await;
    let id = a.create(rate(&vat), ID).await;

    let (status, res) = a.update(&id, json!({ "title": "rate #1" })).await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "prices required on update"
    );
    assert_eq!(
        res["errors"],
        json!({
            "business_price": ["The business price field is required."],
            "consumer_price": ["The consumer price field is required."],
        })
    );

    let (status, res) = a
        .update(
            &id,
            json!({ "title": "rate #1", "description": "paid hourly", "consumer_price": "2000.00", "business_price": "1800.00" }),
        )
        .await;
    assert_eq!((status, res), (StatusCode::OK, json!({ "message": "ok" })));
    let r = a.get(&id).await;
    assert_eq!(
        (
            r["title"].clone(),
            r["consumer_price"].clone(),
            r["business_price"].clone()
        ),
        (json!("rate #1"), json!("2000"), json!("1800"))
    );
    assert_eq!(
        r["relationships"]["vat_group"]["id"],
        vat.as_str(),
        "absent keeps"
    );
    assert_eq!(r["purchase_price"], "40.12", "absent keeps");

    let (status, _) = a
        .update(
            &id,
            json!({ "consumer_price": 1, "business_price": 1, "vat_group_id": null, "purchase_price": null, "title": null }),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "title null is required"
    );

    let (status, _) = a
        .update(
            &id,
            json!({ "consumer_price": 1, "business_price": 1, "vat_group_id": null, "purchase_price": null }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let r = a.get(&id).await;
    assert_eq!(r["relationships"]["vat_group"], Value::Null, "null clears");
    assert_eq!(r["purchase_price"], "0.00");
}

#[sqlx::test]
async fn list_joins_vat_groups_in_one_query(pool: PgPool) {
    let a = api(pool).await;
    let vat = vat_group(&a, &a.ws, &a.owner).await;
    for i in 0..60 {
        let mut body = rate(&vat);
        body["title"] = json!(format!("rate {i:02}"));
        a.create(body, ID).await;
    }

    let res = a.list("?q=rate%200").await;
    assert_eq!(res["meta"]["total"], 10);

    let res = a.list("?page=2").await;
    assert_eq!(res["meta"]["total"], 60);
    assert_eq!(res["data"].as_array().unwrap().len(), 10);
    assert_eq!(res["data"][0]["title"], "rate 50");
    assert_eq!(
        res["data"][0]["relationships"]["vat_group"]["id"],
        vat.as_str()
    );
}

#[sqlx::test]
async fn delete_bad_ids_and_access(pool: PgPool) {
    let a = api(pool).await;
    let vat = vat_group(&a, &a.ws, &a.owner).await;
    let id = a.create(rate(&vat), ID).await;
    let gone = a.create(rate(&vat), ID).await;
    a.delete(&gone).await;
    let update = json!({ "consumer_price": 1, "business_price": 1, "title": "pwned" });

    a.assert_bad_ids_404(&gone, update.clone()).await;
    a.assert_foreign_404(&id, update).await;
    a.assert_member_forbidden(&id).await;
    a.assert_granted(
        &[
            "hourlyrate.create",
            "hourlyrate.list",
            "hourlyrate.get",
            "hourlyrate.update",
            "hourlyrate.delete",
        ],
        rate(&vat),
        ID,
    )
    .await;
}
