//! Till catalog: what a register sells (store-only products included), priced in its currency
//! the way `POST /till/sales` books them, with the register warehouse's stock, barcode lookup
//! and delta sync by `updated_since`.

use crate::common;

use axum::http::{Method, StatusCode};
use common::{Api, invoicing::stock, send};
use serde_json::{Value, json};
use sqlx::PgPool;

const PIN: &str = "4821";

struct W {
    api: Api,
    /// Till token on the store register.
    t: String,
    shop: String,
    back: String,
    honey: String,
    wax: String,
    shop_key: String,
}

/// Acme: a store warehouse "Shop" (default) and a back store; honey (€12.50 excl, 9 % VAT,
/// EAN 8712345678906, 10 in the shop, 4 in the back) and store-only wax (€3, 21 %, 5 in the
/// shop); one register on the shop; a web shop key.
async fn world(pool: PgPool) -> W {
    let api = Api::new(pool, "/till").await;
    let vat = |rate: &'static str| {
        let api = api.with_base("/settings/vat-groups");
        async move {
            api.create(
                json!({ "name": rate, "vat_amount": rate, "country": "NL" }),
                "vat_group_id",
            )
            .await
        }
    };
    let (low, high) = (vat("9").await, vat("21").await);
    let warehouses = api.with_base("/warehouses");
    let shop = warehouses
        .create(
            json!({ "name": "Shop", "default_warehouse": true, "is_store": true }),
            "warehouse_id",
        )
        .await;
    let back = warehouses
        .create(json!({ "name": "Back", "is_store": true }), "warehouse_id")
        .await;
    let products = api.with_base("/products");
    let honey = products
        .create(
            json!({ "name": "Honey", "price": "12.50", "vat_group_id": low,
                    "ean13": "8712345678906" }),
            "product_id",
        )
        .await;
    let wax = products
        .create(
            json!({ "name": "Wax", "price": "3", "vat_group_id": high, "store_only": true }),
            "product_id",
        )
        .await;
    stock(&api, &honey, &shop, 10).await;
    stock(&api, &honey, &back, 4).await;
    stock(&api, &wax, &shop, 5).await;
    let t = session(&api, &shop).await;
    let (status, res) = api
        .with_base("/app-keys")
        .call(
            Method::POST,
            "",
            Some(json!({ "name": "Shop", "kind": "shop", "scopes": ["catalog:read"] })),
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let shop_key = res["data"]["key"].as_str().unwrap().to_owned();
    W {
        api,
        t,
        shop,
        back,
        honey,
        wax,
        shop_key,
    }
}

async fn session(api: &Api, warehouse: &str) -> String {
    let (_, reg) = api
        .call(
            Method::POST,
            "/registers",
            Some(json!({ "name": "Till 1", "warehouse_id": warehouse })),
            &api.owner,
        )
        .await;
    let key = reg["data"]["key"].as_str().unwrap().to_owned();
    let (_, user) = api
        .call(
            Method::POST,
            "/users",
            Some(json!({ "name": "Ann", "pin": PIN })),
            &api.owner,
        )
        .await;
    let (status, res) = api
        .call(
            Method::POST,
            "/login",
            Some(json!({ "till_user_id": user["data"]["id"], "pin": PIN })),
            &key,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    res["data"]["token"].as_str().unwrap().to_owned()
}

async fn catalog(w: &W, query: &str) -> Value {
    let (status, res) = w
        .api
        .call(Method::GET, &format!("/catalog{query}"), None, &w.t)
        .await;
    assert_eq!(status, StatusCode::OK, "{query}: {res}");
    res
}

fn ids(res: &Value) -> Vec<&str> {
    res["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["id"].as_str().unwrap())
        .collect()
}

fn row<'a>(res: &'a Value, id: &str) -> &'a Value {
    res["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == id)
        .unwrap_or_else(|| panic!("{id} not in {res}"))
}

async fn delta(w: &W, since: &str) -> Value {
    catalog(w, &format!("?updated_since={}", since.replace('+', "%2B"))).await
}

async fn edit(w: &W, product: &str, body: Value) {
    let (status, res) = w
        .api
        .with_base("/products")
        .call(
            Method::POST,
            &format!("/{product}"),
            Some(body),
            &w.api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
}

#[sqlx::test]
async fn the_till_sees_store_only_products_priced_like_its_sales(pool: PgPool) {
    let w = world(pool).await;
    let res = catalog(&w, "").await;
    let mut want = [w.honey.as_str(), w.wax.as_str()];
    want.sort_unstable();
    assert_eq!(ids(&res), want);
    assert_eq!(res["meta"]["total"], 2);
    assert!(res["next_since"].is_string(), "{res}");

    let honey = row(&res, &w.honey);
    assert_eq!(honey["vat_rate"], "9");
    assert_eq!(honey["unit_price"], 1250);
    assert_eq!(honey["ean13"], "8712345678906");
    assert_eq!(honey["store_only"], false);
    assert_eq!(honey["sellable"], true);
    assert_eq!(honey["stock"], "10.00", "the register's warehouse only");
    let variants = honey["variants"].as_array().unwrap();
    assert_eq!(variants.len(), 1);
    assert_eq!(variants[0]["unit_price"], 1250);
    let wax = row(&res, &w.wax);
    assert_eq!(
        (wax["store_only"].clone(), wax["vat_rate"].clone()),
        (json!(true), json!("21"))
    );

    // What the catalog says is what a sale books: no price mismatch.
    let (status, sold) = w
        .api
        .call(
            Method::POST,
            "/sales",
            Some(json!({
                "client_ref": "c1",
                "lines": [{ "product_id": w.honey, "quantity": "1",
                            "unit_price": honey["unit_price"] }],
                "payment": { "method": "card" },
            })),
            &w.t,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{sold}");
    assert_eq!(sold["data"]["price_mismatch"], 0, "{sold}");

    // The web shop never sees a store-only product.
    let (_, web) = send(
        &w.api.app,
        Method::GET,
        "/v1/storefront/products",
        None,
        Some(&w.shop_key),
    )
    .await;
    assert_eq!(ids(&web), [w.honey.as_str()]);
    let (status, _) = send(
        &w.api.app,
        Method::GET,
        &format!("/v1/storefront/products/{}", w.wax),
        None,
        Some(&w.shop_key),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn a_scanned_code_finds_its_product(pool: PgPool) {
    let w = world(pool).await;
    let variant = w
        .api
        .with_base("/products")
        .call(
            Method::POST,
            &format!("/{}/variants", w.wax),
            Some(
                json!({ "sku": "WAX-L", "price": "3", "ean13": "4006381333931",
                         "barcode": "WX-LARGE" }),
            ),
            &w.api.owner,
        )
        .await;
    assert_eq!(variant.0, StatusCode::CREATED, "{}", variant.1);
    for (code, want) in [
        ("8712345678906", vec![w.honey.as_str()]),
        ("4006381333931", vec![w.wax.as_str()]),
        ("wx-large", vec![w.wax.as_str()]),
        ("0000000000000", vec![]),
    ] {
        let res = catalog(&w, &format!("?code={code}")).await;
        assert_eq!(ids(&res), want, "{code}");
    }
    let long = "9".repeat(101);
    for q in ["?updated_since=yesterday", &format!("?code={long}")] {
        let (status, _) = w
            .api
            .call(Method::GET, &format!("/catalog{q}"), None, &w.t)
            .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{q}");
    }
}

#[sqlx::test]
async fn updated_since_returns_changes_and_what_left(pool: PgPool) {
    let w = world(pool).await;
    let first = catalog(&w, "").await;
    let since = first["next_since"].as_str().unwrap().to_owned();
    // Rows written before `since` minus the overlap: push them into the past.
    for q in [
        "UPDATE products SET updated_at = now() - interval '1 hour'",
        "UPDATE product_variants SET updated_at = now() - interval '1 hour'",
        "UPDATE stocks SET updated_at = now() - interval '1 hour'",
        "UPDATE vat_groups SET updated_at = now() - interval '1 hour'",
    ] {
        sqlx::query(q).execute(&w.api.pool).await.unwrap();
    }
    assert_eq!(
        ids(&delta(&w, &since).await),
        Vec::<&str>::new(),
        "nothing changed"
    );

    // Stock moving in another warehouse is not this register's news.
    stock(&w.api, &w.honey, &w.back, 1).await;
    assert_eq!(ids(&delta(&w, &since).await), Vec::<&str>::new());
    // A price edit, then stock in the register's warehouse.
    edit(&w, &w.wax, json!({ "price": "3.50" })).await;
    assert_eq!(ids(&delta(&w, &since).await), [w.wax.as_str()]);
    stock(&w.api, &w.honey, &w.shop, 1).await;
    let res = delta(&w, &since).await;
    let mut want = [w.honey.as_str(), w.wax.as_str()];
    want.sort_unstable();
    assert_eq!(ids(&res), want);
    assert_eq!(row(&res, &w.wax)["unit_price"], 350);
    assert_eq!(row(&res, &w.honey)["stock"], "11.00");

    // Disabled: the delta says so; a full read leaves it out.
    edit(&w, &w.wax, json!({ "status": "disabled" })).await;
    let res = delta(&w, &since).await;
    assert_eq!(row(&res, &w.wax)["sellable"], false);
    assert_eq!(ids(&catalog(&w, "").await), [w.honey.as_str()]);
}

#[sqlx::test]
async fn only_a_till_session_of_this_workspace_reads_it(pool: PgPool) {
    let w = world(pool).await;
    let path = format!("/v1/workspace/{}/till/catalog", w.api.ws);
    for token in [w.api.owner.as_str(), w.shop_key.as_str()] {
        let (status, _) = send(&w.api.app, Method::GET, &path, None, Some(token)).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
    let eve = w.api.as_new_owner("eve@example.com", "evil").await;
    eve.with_base("/products")
        .create(json!({ "name": "Evil honey", "price": 1 }), "product_id")
        .await;
    let uri = format!("/v1/workspace/{}/till/catalog", eve.ws);
    let (status, _) = send(&w.api.app, Method::GET, &uri, None, Some(&w.t)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(
        catalog(&w, "").await["meta"]["total"],
        2,
        "eve's product never shows"
    );
}

#[sqlx::test]
async fn a_product_in_another_currency_is_left_out(pool: PgPool) {
    let w = world(pool).await;
    let usd = w
        .api
        .with_base("/currencies")
        .create(
            json!({ "name": "Dollar", "asset_type": "usd", "decimals": 2, "active": true,
                    "default_currency": false }),
            "currency_id",
        )
        .await;
    w.api
        .with_base("/products")
        .create(
            json!({ "name": "Imported", "price": "5", "currency_id": usd }),
            "product_id",
        )
        .await;
    assert_eq!(catalog(&w, "").await["meta"]["total"], 2);
}
