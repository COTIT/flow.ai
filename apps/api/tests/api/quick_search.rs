//! GET /v1/workspace/{ws}/quick-search?q=: the office top bar. A few rows per entity kind,
//! each group only for callers who may list that kind; another workspace's rows never show.

use crate::common;

use axum::http::{Method, StatusCode};
use common::{
    invoicing::{self, F},
    member_with, send,
};
use serde_json::{Value, json};
use sqlx::PgPool;

/// The invoicing fixture (Chair, Client with contact Cleo, INV-1000 sent) plus an order of
/// chairs and a lot `CHAIR-26-A`.
async fn world(pool: PgPool) -> F {
    let f = invoicing::setup(pool).await;
    let id = invoicing::draft(&f, invoicing::chairs(&f, 1, None), false).await;
    let (status, res) = invoicing::send(&f, &id, invoicing::message(&f, "manual")).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    let (status, res) = f
        .api
        .with_base("/orders")
        .call(
            Method::POST,
            "",
            Some(json!({ "reference": "PO chairs 77", "lines": [{ "product_id": f.product, "quantity": "1" }] })),
            &f.api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    f.api
        .with_base("/lots")
        .create(
            json!({ "product_id": f.product, "code": "CHAIR-26-A" }),
            "id",
        )
        .await;
    f
}

async fn search(f: &F, ws: &str, token: &str, query: &str) -> (StatusCode, Value) {
    send(
        &f.api.app,
        Method::GET,
        &format!("/v1/workspace/{ws}/quick-search?{query}"),
        None,
        Some(token),
    )
    .await
}

fn len(v: &Value) -> usize {
    v.as_array().map_or(usize::MAX, Vec::len)
}

#[sqlx::test]
async fn one_query_finds_each_kind(pool: PgPool) {
    let f = world(pool).await;
    let (ws, owner) = (f.api.ws.clone(), f.api.owner.clone());

    let (status, res) = search(&f, &ws, &owner, "q=chair").await;
    assert_eq!(status, StatusCode::OK, "{res}");
    let d = &res["data"];
    assert_eq!(d["products"][0]["name"], "Chair");
    assert!(d["products"][0]["product_nr"].is_string(), "{d}");
    assert_eq!(
        d["orders"][0]["reference"], "PO chairs 77",
        "matched on reference"
    );
    assert!(d["orders"][0]["number"].is_string());
    assert!(d["orders"][0]["total"].is_i64());
    assert_eq!(d["lots"][0]["code"], "CHAIR-26-A");
    assert_eq!(d["lots"][0]["product_name"], "Chair");
    assert_eq!(d["invoices"], json!([]));
    assert_eq!(d["relations"], json!([]));

    let (_, res) = search(&f, &ws, &owner, "q=INV-1000").await;
    assert_eq!(res["data"]["invoices"][0]["number"], "INV-1000", "{res}");
    assert_eq!(res["data"]["invoices"][0]["customer_name"], "Client");
    let (_, res) = search(&f, &ws, &owner, "q=1000").await;
    assert_eq!(len(&res["data"]["invoices"]), 1, "digits find the number");

    let (_, res) = search(&f, &ws, &owner, "q=client").await;
    assert_eq!(res["data"]["relations"][0]["name"], "Client");
    let (_, res) = search(&f, &ws, &owner, "q=CLEO").await;
    assert_eq!(
        res["data"]["relations"][0]["name"], "Client",
        "by contact, any case"
    );

    let (_, res) = search(&f, &ws, &owner, "q=zzz-nothing").await;
    for g in ["products", "orders", "invoices", "lots", "relations"] {
        assert_eq!(res["data"][g], json!([]), "{g}");
    }
}

#[sqlx::test]
async fn groups_are_capped_and_wildcards_are_literal(pool: PgPool) {
    let f = world(pool).await;
    let products = f.api.with_base("/products");
    for n in 0..12 {
        products
            .create(
                json!({ "name": format!("Chair {n}"), "price": "1" }),
                "product_id",
            )
            .await;
    }
    let (ws, owner) = (f.api.ws.clone(), f.api.owner.clone());
    let (_, res) = search(&f, &ws, &owner, "q=chair").await;
    assert_eq!(len(&res["data"]["products"]), 5, "default 5 per group");
    let (_, res) = search(&f, &ws, &owner, "q=chair&limit=10").await;
    assert_eq!(len(&res["data"]["products"]), 10);
    let (_, res) = search(&f, &ws, &owner, "q=chair&limit=500").await;
    assert_eq!(len(&res["data"]["products"]), 10, "capped at 10");
    let (_, res) = search(&f, &ws, &owner, "q=ch%25r").await;
    assert_eq!(len(&res["data"]["products"]), 0, "% is literal");
    let (_, res) = search(&f, &ws, &owner, "q=c_air").await;
    assert_eq!(len(&res["data"]["products"]), 0, "_ is literal");

    for q in ["", "q=", "q=ab", "q=%20%20ab%20"] {
        let (status, res) = search(&f, &ws, &owner, q).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{q}: {res}");
        assert!(res["errors"]["q"].is_array(), "{res}");
    }
    let long = format!("q={}", "a".repeat(101));
    let (status, _) = search(&f, &ws, &owner, &long).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test]
async fn each_group_needs_its_list_permission(pool: PgPool) {
    let f = world(pool).await;
    let ws = f.api.ws.clone();
    let (app, db) = (&f.api.app, &f.api.pool);
    let none = member_with(app, db, &ws, "none@example.com", &[]).await;
    let (status, _) = search(&f, &ws, &none, "q=chair").await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "needs workspace.global-search"
    );

    let some = member_with(
        app,
        db,
        &ws,
        "some@example.com",
        &["workspace.global-search", "product.list"],
    )
    .await;
    let (status, res) = search(&f, &ws, &some, "q=chair").await;
    assert_eq!(status, StatusCode::OK, "{res}");
    let d = &res["data"];
    assert_eq!(len(&d["products"]), 1);
    assert_eq!(d["lots"][0]["code"], "CHAIR-26-A", "lots: any member");
    for g in ["orders", "invoices", "relations"] {
        assert_eq!(d[g], Value::Null, "{g} not allowed");
    }
}

#[sqlx::test]
async fn another_workspace_finds_nothing_of_acme(pool: PgPool) {
    let f = world(pool).await;
    let eve = f.api.as_new_owner("eve@example.com", "evil").await;
    for q in ["q=chair", "q=client", "q=1000", "q=INV-1000", "q=CHAIR-26"] {
        let (status, res) = search(&f, &eve.ws, &eve.owner, q).await;
        assert_eq!(status, StatusCode::OK, "{res}");
        for g in ["products", "orders", "invoices", "lots", "relations"] {
            assert_eq!(res["data"][g], json!([]), "{q} {g}");
        }
    }
    let (status, _) = search(&f, &f.api.ws, &eve.owner, "q=chair").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "eve under acme's path");
}
