//! Assistant reviews of stock, order and invoice writes list old → new rows the card leads with.

use crate::assistant_preconditions::world;
use crate::common::{Api, invoicing, send};
use axum::http::{Method, StatusCode};
use serde_json::{Value, json};
use sqlx::PgPool;

/// Propose one action as the owner; the reply's status and body.
async fn propose(
    api: &Api,
    op: &str,
    method: Method,
    path: &str,
    body: Value,
) -> (StatusCode, Value) {
    let base = format!("/v1/workspace/{}/assistant", api.ws);
    let (status, session) = send(
        &api.app,
        Method::POST,
        &format!("{base}/sessions"),
        Some(json!({"language":"en"})),
        Some(&api.owner),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{session}");
    send(
        &api.app,
        Method::POST,
        &format!("{base}/actions"),
        Some(json!({"session_id":session["data"]["id"],"operation_id":op,"method":method.as_str(),"path":path,"body":body})),
        Some(&api.owner),
    )
    .await
}

/// The rows of an accepted proposal.
async fn rows(api: &Api, op: &str, method: Method, path: &str, body: Value) -> Value {
    let (status, res) = propose(api, op, method, path, body).await;
    assert_eq!(status, StatusCode::CREATED, "{op}: {res}");
    res["data"]["review_facts"]["facts"]["changes"].clone()
}

fn money(field: &str, currency: &Value, before: Option<i64>, after: i64) -> Value {
    let mut row = json!({"field":field,"currency":{"code":currency["code"],"decimals":currency["decimals"]},"after_minor":after});
    if let Some(before) = before {
        row["before_minor"] = before.into();
    }
    row
}

async fn bins(api: &Api, warehouse: &str) -> (String, String) {
    let wh = api.with_base("/warehouses");
    let section = json!({"section_name":"Hall","section_code":"H","rows":1,"shelves_per_row":1,"bins_per_shelf":2});
    let (status, res) = wh
        .call(
            Method::POST,
            &format!("/{warehouse}/sections"),
            Some(section),
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let bins = wh.get(&format!("{warehouse}/bins")).await;
    let id = |i: usize| bins[i]["id"].as_str().unwrap().to_owned();
    (id(0), id(1))
}

#[sqlx::test]
async fn stock_writes_list_each_warehouse_old_and_new(pool: PgPool) {
    let (api, warehouse, product) = world(pool).await;
    let products = format!("/v1/workspace/{}/products/{product}", api.ws);
    let (status, res) = send(
        &api.app,
        Method::POST,
        &format!("{products}/set-stock-levels"),
        Some(json!({"warehouses":[{"warehouse_id":warehouse,"stock_lvl":"14","reason":"Count"}]})),
        Some(&api.owner),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{res}");

    // P3-3: the number being set is on the card, beside what it replaces.
    let levels = format!("{products}/set-stock-levels");
    let body = json!({"warehouses":[{"warehouse_id":warehouse,"stock_lvl":"130","minimal_stock_level":5,"reason":"Count"}]});
    assert_eq!(
        rows(&api, "stock_set_levels", Method::POST, &levels, body).await,
        json!([
            {"field":"stock","label":"Main","before":"14","after":"130"},
            {"field":"minimal","label":"Main","before":"0","after":"5"},
        ])
    );
    // Same level, nothing sent for min/max: nothing changes, nothing listed.
    let same =
        json!({"warehouses":[{"warehouse_id":warehouse,"stock_lvl":"14.00","reason":"Count"}]});
    assert_eq!(
        rows(&api, "stock_set_levels", Method::POST, &levels, same).await,
        json!([])
    );

    let add = json!({"warehouse_id":warehouse,"mutation_value":"6.5","reason":"Delivery"});
    assert_eq!(
        rows(
            &api,
            "stock_mutate",
            Method::POST,
            &format!("{products}/stocks"),
            add
        )
        .await,
        json!([{"field":"stock","label":"Main","before":"14","after":"20.5"}])
    );
    let take = json!({"warehouse_id":warehouse,"mutation_value":-4,"reason":"Broken"});
    assert_eq!(
        rows(
            &api,
            "stock_mutate",
            Method::POST,
            &format!("{products}/stocks"),
            take
        )
        .await,
        json!([{"field":"stock","label":"Main","before":"14","after":"10"}])
    );

    let requirements = json!({"warehouses":[{"warehouse_id":warehouse,"minimal_stock_level":3,"maximum_stock_level":50}]});
    assert_eq!(
        rows(
            &api,
            "stock_set_requirements",
            Method::POST,
            &format!("{products}/set-stock-requirements"),
            requirements
        )
        .await,
        json!([
            {"field":"minimal","label":"Main","before":"0","after":"3"},
            {"field":"maximum","label":"Main","before":"0","after":"50"},
        ])
    );

    let (first, second) = bins(&api, &warehouse).await;
    let (status, res) = send(
        &api.app,
        Method::POST,
        &format!("{products}/stocks"),
        Some(json!({"warehouse_id":warehouse,"bin_id":first,"mutation_value":5})),
        Some(&api.owner),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    let moved = json!({"warehouse_id":warehouse,"bin_id_from":first,"bin_id_to":second,"quantity":"2","reason":"Tidy"});
    assert_eq!(
        rows(
            &api,
            "stock_move_between_bins",
            Method::POST,
            &format!("{products}/move-stock-between-bins"),
            moved
        )
        .await,
        json!([
            {"field":"stock_from","label":"Main · Bin A101","before":"5","after":"3"},
            {"field":"stock_to","label":"Main · Bin A102","before":"0","after":"2"},
        ])
    );

    let lot = crate::attention::lot(&api, &product, "LOT-7", &warehouse, 8).await;
    let lot_path = format!("/v1/workspace/{}/lots/{lot}/stock-mutations", api.ws);
    let out = json!({"warehouse_id":warehouse,"quantity":-3,"reason":"Sold"});
    assert_eq!(
        rows(&api, "lots_stock_mutate", Method::POST, &lot_path, out).await,
        json!([{"field":"stock","label":"Main · LOT-7","before":"8","after":"5"}])
    );
}

#[sqlx::test]
async fn order_writes_list_status_totals_quantities_and_money(pool: PgPool) {
    let (api, warehouse, product) = world(pool.clone()).await;
    let products = format!("/v1/workspace/{}/products/{product}", api.ws);
    send(
        &api.app,
        Method::POST,
        &format!("{products}/set-stock-levels"),
        Some(json!({"warehouses":[{"warehouse_id":warehouse,"stock_lvl":"10","reason":"Count"}]})),
        Some(&api.owner),
    )
    .await;
    let orders = format!("/v1/workspace/{}/orders", api.ws);

    let (status, res) = propose(
        &api,
        "orders_create",
        Method::POST,
        &orders,
        json!({"lines":[{"product_id":product,"quantity":"2"}]}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let facts = &res["data"]["review_facts"]["facts"];
    let eur = facts["priced"]["currency"].clone();
    assert_eq!(facts["changes"], json!([money("total", &eur, None, 2500)]));

    let relation = api.with_base("/relations").create(json!({"company_name":"Reviewed buyer","type":"business","addresses":[{"address":"Main","house_number":"1","zipcode":"1000AA","city":"Reviewed town","country":"NL","type":"billing"}]}), "relation_id").await;
    let id = api
        .create(
            json!({"relation_id":relation,"lines":[{"product_id":product,"quantity":"1"}]}),
            "order_id",
        )
        .await;
    let order = format!("{orders}/{id}");
    assert_eq!(
        rows(
            &api,
            "orders_update",
            Method::PATCH,
            &order,
            json!({"lines":[{"product_id":product,"quantity":"3"}]})
        )
        .await,
        json!([money("total", &eur, Some(1250), 3750)])
    );
    assert_eq!(
        rows(
            &api,
            "orders_confirm",
            Method::POST,
            &format!("{order}/confirm"),
            json!({})
        )
        .await,
        json!([{"field":"status","before":"draft","after":"open"}])
    );
    let (status, res) = api
        .call(Method::POST, &format!("/{id}/confirm"), None, &api.owner)
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(
        rows(
            &api,
            "orders_cancel",
            Method::POST,
            &format!("{order}/cancel"),
            json!({})
        )
        .await,
        json!([{"field":"status","before":"open","after":"cancelled"}])
    );
    let line = api.get(&id).await["lines"][0]["id"].clone();
    let pick = json!({"lines":[{"line_id":line,"quantity":"1"}]});
    assert_eq!(
        rows(
            &api,
            "orders_pick",
            Method::POST,
            &format!("{order}/pick"),
            pick.clone()
        )
        .await,
        json!([{"field":"picked","label":"Reviewed honey","before":"0","after":"1"}])
    );
    let (status, res) = api
        .call(Method::POST, &format!("/{id}/pick"), Some(pick), &api.owner)
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(
        rows(
            &api,
            "orders_complete",
            Method::POST,
            &format!("{order}/complete"),
            json!({})
        )
        .await,
        json!([{"field":"status","before":"open","after":"completed"}])
    );
    let (status, res) = api
        .call(Method::POST, &format!("/{id}/complete"), None, &api.owner)
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(
        rows(
            &api,
            "orders_returns",
            Method::POST,
            &format!("{order}/returns"),
            json!({"lines":[{"line_id":line,"quantity":"1"}]})
        )
        .await,
        json!([{"field":"returned","label":"Reviewed honey","before":"0","after":"1"}])
    );
    assert_eq!(
        rows(
            &api,
            "orders_invoice",
            Method::POST,
            &format!("{order}/invoice"),
            json!({})
        )
        .await,
        json!([money("total", &eur, None, 1250)])
    );
    sqlx::query(
        "WITH o AS (UPDATE orders SET paid_at = now() WHERE id = $1::uuid RETURNING workspace_id, id, total)
         INSERT INTO order_payments (workspace_id, order_id, kind, provider, provider_id, amount, currency, status)
         SELECT workspace_id, id, 'payment', 'fake', 'fk_1', total, 'EUR', 'paid' FROM o",
    )
    .bind(&id)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        rows(
            &api,
            "orders_refund",
            Method::POST,
            &format!("{order}/refunds"),
            json!({"amount":"5.00"})
        )
        .await,
        json!([
            money("refund", &eur, None, 500),
            money("paid", &eur, Some(1250), 750)
        ])
    );
}

#[sqlx::test]
async fn invoice_writes_list_totals_and_payments(pool: PgPool) {
    let f = invoicing::setup(pool).await;
    let invoices = format!("/v1/workspace/{}/invoices", f.api.ws);
    let (status, res) = propose(
        &f.api,
        "invoices_create",
        Method::POST,
        &invoices,
        json!({"relation_id":f.rel,"items":invoicing::chairs(&f, 1, None)}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let facts = &res["data"]["review_facts"]["facts"];
    let eur = facts["priced"]["currency"].clone();
    assert_eq!(facts["changes"], json!([money("total", &eur, None, 1210)]));

    let id = invoicing::draft(&f, invoicing::chairs(&f, 1, None), false).await;
    let invoice = format!("{invoices}/{id}");
    assert_eq!(
        rows(
            &f.api,
            "invoices_update",
            Method::POST,
            &invoice,
            json!({"relation_id":f.rel,"items":invoicing::chairs(&f, 1, None)})
        )
        .await,
        json!([]),
        "the update quote carries line changes, not a new total (follow-up)"
    );
    assert_eq!(
        rows(
            &f.api,
            "invoices_duplicate",
            Method::POST,
            &format!("{invoice}/duplicate"),
            json!({})
        )
        .await,
        json!([money("total", &eur, None, 1210)])
    );

    assert_eq!(
        invoicing::send(&f, &id, invoicing::message(&f, "manual"))
            .await
            .0,
        StatusCode::OK
    );
    let pay = json!({"date":"2026-10-23","amount":"4","payment_method":"cash"});
    assert_eq!(
        rows(
            &f.api,
            "invoice_payments_create",
            Method::POST,
            &format!("{invoice}/payments"),
            pay.clone()
        )
        .await,
        json!([
            money("payment", &eur, None, 400),
            money("paid", &eur, Some(0), 400)
        ])
    );
    let (status, res) = f
        .api
        .call(
            Method::POST,
            &format!("/{id}/payments"),
            Some(pay),
            &f.api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let payment = res["data"]["invoice_payment_id"].as_str().unwrap();
    assert_eq!(
        rows(
            &f.api,
            "invoice_payments_void",
            Method::DELETE,
            &format!("{invoice}/payments/{payment}"),
            json!({})
        )
        .await,
        json!([money("paid", &eur, Some(400), 0)])
    );
}

#[sqlx::test]
async fn review_rows_never_read_another_workspace(pool: PgPool) {
    let (api, warehouse, product) = world(pool.clone()).await;
    let foreign = api.as_new_owner("eve@example.com", "foreign").await;
    let their_warehouse = foreign
        .with_base("/warehouses")
        .create(
            json!({"name":"Secret depot","default_warehouse":true}),
            "warehouse_id",
        )
        .await;
    let their_product = foreign
        .with_base("/products")
        .create(
            json!({"name":"Private foreign honey","price":"9","has_stock":true}),
            "product_id",
        )
        .await;
    let their_order = foreign
        .create(
            json!({"lines":[{"product_id":their_product,"quantity":"1"}]}),
            "order_id",
        )
        .await;
    let their_lot =
        crate::attention::lot(&foreign, &their_product, "SECRET-LOT", &their_warehouse, 2).await;
    let ws = &api.ws;
    let cases = [
        (
            "stock_set_levels",
            format!("/v1/workspace/{ws}/products/{their_product}/set-stock-levels"),
            json!({"warehouses":[{"warehouse_id":warehouse,"stock_lvl":"1","reason":"x"}]}),
        ),
        (
            "stock_set_levels",
            format!("/v1/workspace/{ws}/products/{product}/set-stock-levels"),
            json!({"warehouses":[{"warehouse_id":their_warehouse,"stock_lvl":"1","reason":"x"}]}),
        ),
        (
            "stock_mutate",
            format!("/v1/workspace/{ws}/products/{their_product}/stocks"),
            json!({"warehouse_id":warehouse,"mutation_value":1}),
        ),
        (
            "stock_mutate",
            format!("/v1/workspace/{ws}/products/{product}/stocks"),
            json!({"warehouse_id":their_warehouse,"mutation_value":1}),
        ),
        (
            "stock_set_requirements",
            format!("/v1/workspace/{ws}/products/{product}/set-stock-requirements"),
            json!({"warehouses":[{"warehouse_id":their_warehouse,"minimal_stock_level":1}]}),
        ),
        (
            "stock_move_between_bins",
            format!("/v1/workspace/{ws}/products/{their_product}/move-stock-between-bins"),
            json!({"warehouse_id":warehouse,"quantity":"1","reason":"x","bin_id_to":uuid::Uuid::now_v7()}),
        ),
        (
            "lots_stock_mutate",
            format!("/v1/workspace/{ws}/lots/{their_lot}/stock-mutations"),
            json!({"warehouse_id":warehouse,"quantity":1}),
        ),
        (
            "orders_confirm",
            format!("/v1/workspace/{ws}/orders/{their_order}/confirm"),
            json!({}),
        ),
        (
            "orders_pick",
            format!("/v1/workspace/{ws}/orders/{their_order}/pick"),
            json!({"lines":[{"line_id":uuid::Uuid::now_v7(),"quantity":"1"}]}),
        ),
        (
            "orders_refund",
            format!("/v1/workspace/{ws}/orders/{their_order}/refunds"),
            json!({"amount":"1"}),
        ),
    ];
    for (op, path, body) in cases {
        let (status, res) = propose(&api, op, Method::POST, &path, body).await;
        assert!(
            matches!(
                status,
                StatusCode::NOT_FOUND | StatusCode::UNPROCESSABLE_ENTITY
            ),
            "{op} {path}: {status} {res}"
        );
        let text = res.to_string();
        for secret in ["Secret depot", "Private foreign honey", "SECRET-LOT"] {
            assert!(!text.contains(secret), "{op} leaked {secret}: {text}");
        }
    }
}
