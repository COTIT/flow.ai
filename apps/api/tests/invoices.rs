//! /v1/workspace/{ws}/invoices (final batch F4): draft invoices with item lines. Totals are
//! stored in the transaction that writes the items, VAT once per rate.

mod common;

use axum::http::{Method, StatusCode};
use common::{Api, member_with};
use serde_json::{Value, json};
use sqlx::PgPool;

async fn api(pool: PgPool) -> Api {
    Api::new(pool, "/invoices").await
}

/// A business relation with one billing address and one contact; returns (relation, contact).
async fn relation(api: &Api, name: &str) -> (String, String) {
    let id = api
        .with_base("/relations")
        .create(
            json!({ "company_name": name, "type": "business",
                    "addresses": [{ "address": "Main", "house_number": "1", "zipcode": "1000AA",
                                    "city": "Amsterdam", "country": "NL", "type": "billing" }] }),
            "relation_id",
        )
        .await;
    let contact = api
        .with_base("/relations")
        .call(
            Method::POST,
            &format!("/{id}/contacts"),
            Some(json!({ "first_name": "Ada", "last_name": "L", "email": "ada@acme.test" })),
            &api.owner,
        )
        .await;
    assert_eq!(contact.0, StatusCode::CREATED, "{}", contact.1);
    (
        id,
        contact.1["data"]["contact_id"].as_str().unwrap().to_owned(),
    )
}

async fn product(api: &Api, name: &str) -> String {
    api.with_base("/products")
        .create(
            json!({ "name": name, "price": "10", "wholesale_price": "4" }),
            "product_id",
        )
        .await
}

fn line(title: &str, q: Value, p: Value, vat: Value) -> Value {
    json!({ "title": title, "quantity": q, "price": p, "vat": vat })
}

async fn post(api: &Api, path: &str, body: Value) -> (StatusCode, Value) {
    api.call(Method::POST, path, Some(body), &api.owner).await
}

async fn get(api: &Api, id: &str) -> Value {
    api.get(id).await
}

/// First message for `field` in a 422 body.
async fn error(api: &Api, path: &str, body: Value, field: &str) -> String {
    let (status, res) = post(api, path, body).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    res["errors"][field][0]
        .as_str()
        .unwrap_or_else(|| panic!("no error for {field}: {res}"))
        .to_owned()
}

async fn stored(api: &Api, id: &str) -> (i64, i64, Value) {
    sqlx::query_as("SELECT subtotal, total, summary FROM invoices WHERE id = $1::uuid")
        .bind(id)
        .fetch_one(&api.pool)
        .await
        .unwrap()
}

// --- happy ---------------------------------------------------------------------------------

#[sqlx::test]
async fn create_stores_a_draft_with_items_and_totals(pool: PgPool) {
    let api = api(pool).await;
    let (rel, contact) = relation(&api, "Acme").await;
    let prod = product(&api, "Widget").await;
    let mut discounted = line("Widget", json!(1), json!("10"), json!(21));
    discounted["product_id"] = json!(prod);
    discounted["productDiscount"] = json!(10);
    let body = json!({
        "relation_id": rel, "addressed_contact_id": contact, "reference": "PO-1",
        "note": "<p>Thanks</p>",
        "items": [line("Tiny", json!(3), json!(0.07), json!(21)), discounted],
    });
    let (status, res) = post(&api, "", body).await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let id = res["data"]["invoice_id"].as_str().unwrap().to_owned();

    let inv = get(&api, &id).await;
    assert_eq!(inv["id"], id);
    assert_eq!(inv["status"], "draft");
    assert_eq!(inv["invoice_nr"], Value::Null);
    assert_eq!(inv["payment_method"], "bank_transfer");
    assert_eq!(inv["reference"], "PO-1");
    assert_eq!(inv["is_credit"], false);
    assert_eq!(inv["send_date"], Value::Null);
    assert_eq!(inv["due_date"], Value::Null);
    // 0.21 + (10 − 10 %) = 9.21 at 21 % → VAT 1.93 once on the summed lines.
    assert_eq!(inv["summary"]["total_excl"], json!(9.21));
    assert_eq!(inv["summary"]["total_vat"], json!(1.93));
    assert_eq!(inv["summary"]["total"], json!(11.14));
    assert_eq!(inv["summary"]["total_discount"], json!(1));
    assert_eq!(
        inv["summary"]["total_vats"]["21"],
        json!({ "percentage": 21, "total_amount": 1.93 })
    );
    assert_eq!(
        inv["balance"],
        json!({ "open": 11.14, "paid": 0, "total": 11.14 })
    );
    assert_eq!(inv["relationships"]["relation"]["id"], rel);
    assert_eq!(inv["relationships"]["relation"]["company_name"], "Acme");
    assert_eq!(inv["relationships"]["currency"]["default_currency"], true);
    assert_eq!(inv["relationships"]["payments"], json!([]));
    assert_eq!(inv["addressed_contact"]["first_name"], "Ada");
    assert_eq!(inv["addressed_contact"]["uuid"], contact);
    assert_eq!(inv["billing_address"]["city"], "Amsterdam");

    let items = inv["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["title"], "Tiny");
    assert_eq!(items[0]["quantity"], "3");
    assert_eq!(items[0]["price"], "0.07");
    assert_eq!(items[0]["vat"], "21");
    assert_eq!(items[0]["total"], "0.21");
    assert_eq!(items[0]["position"], 0);
    assert_eq!(items[1]["discount_percentage"], "10");
    assert_eq!(items[1]["discount_amount"], "1");
    assert_eq!(items[1]["total"], "9");
    assert_eq!(items[1]["total_incl"], "10.89");
    assert_eq!(items[1]["price_incl"], "12.1");
    // Purchase price falls back to the product's (wholesale 4).
    assert_eq!(items[1]["purchase_price"], "4");
    assert_eq!(items[1]["marge"], "5");
    assert_eq!(items[1]["relationships"]["product"]["id"], prod);
    assert_eq!(items[1]["position"], 1);

    let (subtotal, total, summary) = stored(&api, &id).await;
    assert_eq!((subtotal, total), (921, 1114));
    assert_eq!(
        summary["vats"],
        json!([{ "rate": "21", "base": "9.21", "vat": "1.93" }])
    );
}

#[sqlx::test]
async fn create_without_items_and_defaults(pool: PgPool) {
    let api = api(pool).await;
    let (rel, _) = relation(&api, "Acme").await;
    for items in [json!([]), Value::Null] {
        let (status, res) = post(&api, "", json!({ "relation_id": rel, "items": items })).await;
        assert_eq!(status, StatusCode::CREATED, "{res}");
        let inv = get(&api, res["data"]["invoice_id"].as_str().unwrap()).await;
        assert_eq!(inv["items"], json!([]));
        assert_eq!(inv["summary"]["total"], json!(0));
    }
    // `items` left out entirely (Laravel: 500).
    let (status, _) = post(&api, "", json!({ "relation_id": rel })).await;
    assert_eq!(status, StatusCode::CREATED);
}

#[sqlx::test]
async fn update_diffs_items_and_recomputes_totals(pool: PgPool) {
    let api = api(pool).await;
    let (rel, _) = relation(&api, "Acme").await;
    let body = json!({ "relation_id": rel, "items": [
        line("A", json!(1), json!(10), json!(21)),
        line("B", json!(2), json!(5), json!(9)),
        line("C", json!(1), json!(1), json!(0)),
    ]});
    let id = api.create(body, "invoice_id").await;
    let items = get(&api, &id).await["items"].clone();
    let (a, b, c) = (&items[0]["id"], &items[1]["id"], &items[2]["id"]);

    let update = json!({ "relation_id": rel, "reference": "R2", "items": [
        { "id": a, "title": "A2", "quantity": 3, "price": 10, "vat": 21 },
        { "id": b, "deleted": true },
        line("D", json!(1), json!(100), json!(21)),
    ]});
    let (status, res) = post(&api, &format!("/{id}"), update).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res, json!({ "message": "ok" }));

    let inv = get(&api, &id).await;
    assert_eq!(inv["reference"], "R2");
    let titles: Vec<&str> = inv["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["title"].as_str().unwrap())
        .collect();
    // B deleted, C left as it was (not sent), D added.
    assert_eq!(titles, ["A2", "C", "D"]);
    assert_eq!(inv["items"][1]["id"], *c);
    // 30 + 100 at 21 % (VAT 27.30) + 1 at 0 %.
    assert_eq!(inv["summary"]["total"], json!(158.3));
    let (subtotal, total, _) = stored(&api, &id).await;
    assert_eq!((subtotal, total), (13100, 15830));
}

#[sqlx::test]
async fn delete_cancels_and_only_drafts_change(pool: PgPool) {
    let api = api(pool).await;
    let (rel, _) = relation(&api, "Acme").await;
    let id = api
        .create(json!({ "relation_id": rel, "items": [] }), "invoice_id")
        .await;
    let (status, res) = api
        .call(Method::DELETE, &format!("/{id}"), None, &api.owner)
        .await;
    assert_eq!((status, res), (StatusCode::OK, json!({ "message": "ok" })));
    let inv = get(&api, &id).await;
    assert_eq!(inv["status"], "cancelled");
    assert_eq!(inv["invoice_nr"], "N/A");
    let msg = json!({ "message": "This Invoice does not have the correct status to be updated." });
    let (status, res) = post(&api, &format!("/{id}"), json!({ "relation_id": rel })).await;
    assert_eq!(
        (status, res),
        (StatusCode::UNPROCESSABLE_ENTITY, msg.clone())
    );
    let (status, res) = api
        .call(Method::DELETE, &format!("/{id}"), None, &api.owner)
        .await;
    assert_eq!((status, res), (StatusCode::UNPROCESSABLE_ENTITY, msg));
}

#[sqlx::test]
async fn duplicate_copies_header_and_lines(pool: PgPool) {
    let api = api(pool).await;
    let (rel, contact) = relation(&api, "Acme").await;
    let prod = product(&api, "Widget").await;
    let mut l = line("W", json!(2), json!(10), json!(21));
    l["product_id"] = json!(prod);
    l["productDiscount"] = json!(5);
    l["purchase_price"] = json!(3);
    let id = api
        .create(
            json!({ "relation_id": rel, "addressed_contact_id": contact, "reference": "R",
                    "note": "n", "meta": { "on_account": true }, "items": [l] }),
            "invoice_id",
        )
        .await;
    api.call(Method::DELETE, &format!("/{id}"), None, &api.owner)
        .await;
    let (status, res) = post(&api, &format!("/{id}/duplicate"), json!({})).await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let copy = get(&api, res["data"]["invoice_id"].as_str().unwrap()).await;
    assert_ne!(copy["id"], id);
    assert_eq!(copy["status"], "draft");
    assert_eq!(copy["note"], "n");
    assert_eq!(copy["meta"], json!({ "on_account": true }));
    assert_eq!(copy["reference"], Value::Null);
    assert_eq!(copy["addressed_contact"], Value::Null);
    let item = &copy["items"][0];
    assert_eq!(item["title"], "W");
    assert_eq!(item["discount_percentage"], "5");
    assert_eq!(item["purchase_price"], "3");
    assert_eq!(item["relationships"]["product"]["id"], prod);
    assert_eq!(
        copy["summary"]["total"],
        get(&api, &id).await["summary"]["total"]
    );
}

#[sqlx::test]
async fn list_filters_and_pages(pool: PgPool) {
    let api = api(pool).await;
    let (acme, _) = relation(&api, "Acme").await;
    let (beta, _) = relation(&api, "Beta").await;
    let a = api
        .create(
            json!({ "relation_id": acme, "reference": "first",
                        "items": [line("x", json!(1), json!(10), json!(21))] }),
            "invoice_id",
        )
        .await;
    let b = api
        .create(
            json!({ "relation_id": beta, "reference": "second", "is_credit": true }),
            "invoice_id",
        )
        .await;
    let c = api
        .create(
            json!({ "relation_id": acme, "reference": "third", "meta": { "on_account": true } }),
            "invoice_id",
        )
        .await;
    api.call(Method::DELETE, &format!("/{c}"), None, &api.owner)
        .await;

    let all = api.list("").await;
    let ids: Vec<&str> = all["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, [c.as_str(), b.as_str(), a.as_str()], "newest first");
    assert_eq!(all["meta"]["total"], 3);
    assert_eq!(all["meta"]["per_page"], 50);
    let row = all["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == a)
        .unwrap();
    assert_eq!(row["total_amount"], "12.10");
    assert_eq!(row["summary"]["total"], json!(12.1));
    assert_eq!(row["balance"], json!({ "paid": 0, "open": 12.1 }));
    assert_eq!(row["relationships"]["relation"]["company_name"], "Acme");
    assert_eq!(row["relationships"]["currency"]["default_currency"], true);
    assert_eq!(row["is_on_account"], Value::Null);

    let count = |v: &Value| v["data"].as_array().unwrap().len();
    assert_eq!(count(&api.list(&format!("?relation_id={acme}")).await), 2);
    assert_eq!(count(&api.list("?status=cancelled").await), 1);
    assert_eq!(count(&api.list("?in_status=draft,cancelled").await), 3);
    assert_eq!(count(&api.list("?is_credit=1").await), 1);
    assert_eq!(count(&api.list("?q=seco").await), 1);
    assert_eq!(count(&api.list("?q=beta").await), 1);
    assert_eq!(count(&api.list("?page_size=2&page=2").await), 1);
    let limited = api.list("?limit=1").await;
    assert_eq!(count(&limited), 1);
    assert!(limited.get("meta").is_none(), "limit answers a plain list");
}

// --- errors --------------------------------------------------------------------------------

/// List rows name the customer: company name, else alias, else the first contact, else null.
#[sqlx::test]
async fn list_names_the_customer(pool: PgPool) {
    let api = api(pool).await;
    let rels = api.with_base("/relations");
    let (acme, _) = relation(&api, "Acme").await;
    let alias = rels
        .create(
            json!({ "type": "personal", "alias": "Bobby" }),
            "relation_id",
        )
        .await;
    let person = rels
        .create(json!({ "type": "personal" }), "relation_id")
        .await;
    let (s, b) = rels
        .call(
            Method::POST,
            &format!("/{person}/contacts"),
            Some(json!({ "first_name": "Grace", "last_name": "Hopper" })),
            &api.owner,
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    let nobody = rels
        .create(json!({ "type": "personal" }), "relation_id")
        .await;
    let mut want = Vec::new();
    for (rel, name) in [
        (&acme, json!("Acme")),
        (&alias, json!("Bobby")),
        (&person, json!("Grace Hopper")),
        (&nobody, Value::Null),
    ] {
        let id = api
            .create(json!({ "relation_id": rel }), "invoice_id")
            .await;
        want.push((id, name));
    }
    let list = api.list("").await;
    for (id, name) in want {
        let row = list["data"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == id.as_str())
            .unwrap();
        assert_eq!(row["customer_name"], name, "{id}");
    }
}

#[sqlx::test]
async fn header_rules(pool: PgPool) {
    let api = api(pool).await;
    let (rel, _) = relation(&api, "Acme").await;
    let (other, other_contact) = relation(&api, "Other").await;
    let _ = other;
    let cases = [
        (
            json!({}),
            "relation_id",
            "The relation id field is required.",
        ),
        (
            json!({ "relation_id": "01900000-0000-7000-8000-000000000001" }),
            "relation_id",
            "The relation id is an invalid id for the relation domain.",
        ),
        (
            json!({ "relation_id": rel, "currency_id": "01900000-0000-7000-8000-000000000001" }),
            "currency_id",
            "This currency id does not exist",
        ),
        (
            json!({ "relation_id": rel, "addressed_contact_id": other_contact }),
            "addressed_contact_id",
            "The selected addressed contact id is invalid.",
        ),
        (
            json!({ "relation_id": rel, "payment_method": "cash" }),
            "payment_method",
            "The selected payment method is invalid.",
        ),
        (
            json!({ "relation_id": rel, "reference": "x".repeat(256) }),
            "reference",
            "The reference must not be greater than 255 characters.",
        ),
        (
            json!({ "relation_id": rel, "note": "x".repeat(1001) }),
            "note",
            "The note must not be greater than 1000 characters.",
        ),
        (
            json!({ "relation_id": rel, "is_credit": "maybe" }),
            "is_credit",
            "The is credit field must be true or false.",
        ),
        (
            json!({ "relation_id": rel, "billing_address": { "city": "X" } }),
            "address",
            "The address field is required.",
        ),
    ];
    for (body, field, msg) in cases {
        assert_eq!(error(&api, "", body.clone(), field).await, msg, "{body}");
    }
    let (status, _) = post(
        &api,
        "",
        json!({ "relation_id": rel, "payment_method": "MOLLIE" }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Laravel EnumExist lowercases");
}

#[sqlx::test]
async fn item_rules(pool: PgPool) {
    let api = api(pool).await;
    let (rel, _) = relation(&api, "Acme").await;
    let with = |item: Value| json!({ "relation_id": rel, "items": [item] });
    let cases = [
        (
            json!({ "quantity": 1, "price": 1, "vat": 21 }),
            "items.0.title",
            "The items.0.title field is required.",
        ),
        (
            json!({ "title": "t", "price": 1, "vat": 21 }),
            "items.0.quantity",
            "The items.0.quantity field is required.",
        ),
        (
            json!({ "title": "t", "quantity": 1, "vat": 21 }),
            "items.0.price",
            "The items.0.price field is required when items.0.product id is not present.",
        ),
        (
            json!({ "title": "t", "quantity": 1, "price": 1 }),
            "items.0.vat",
            "The items.0.vat field is required.",
        ),
        (
            json!({ "title": "t", "quantity": 1, "price": 1, "vat": -1 }),
            "items.0.vat",
            "The items.0.vat must be at least 0.",
        ),
        (
            json!({ "title": "t", "quantity": "x", "price": 1, "vat": 21 }),
            "items.0.quantity",
            "The items.0.quantity must be a number.",
        ),
        (
            json!({ "title": "t", "quantity": 1, "price": 1, "vat": 21, "productDiscount": 101 }),
            "items.0.productDiscount",
            "The items.0.productDiscount must not be greater than 100.",
        ),
        (
            json!({ "title": "x".repeat(256), "quantity": 1, "price": 1, "vat": 21 }),
            "items.0.title",
            "The items.0.title must not be greater than 255 characters.",
        ),
        (
            json!({ "title": "t", "quantity": 1, "price": 1, "vat": 21, "product_id": "01900000-0000-7000-8000-000000000001" }),
            "items.0.product_id",
            "The selected items.0.product id is invalid.",
        ),
        (
            json!({ "title": "t", "quantity": 1, "price": 1, "vat": 21, "position": "first" }),
            "items.0.position",
            "The items.0.position must be an integer.",
        ),
        (
            json!({ "title": "t", "quantity": 1, "price": 1, "vat": 21, "mutate_from_warehouse_id": "01900000-0000-7000-8000-000000000001" }),
            "items.0.mutate_from_warehouse_id",
            "The selected items.0.mutate from warehouse id is invalid.",
        ),
    ];
    for (item, field, msg) in cases {
        assert_eq!(
            error(&api, "", with(item.clone()), field).await,
            msg,
            "{item}"
        );
    }
    // A product line may leave the price out: it stores 0 (Laravel: no price lookup).
    let prod = product(&api, "Widget").await;
    let id = api
        .create(
            with(json!({ "title": "W", "quantity": 1, "vat": 21, "product_id": prod })),
            "invoice_id",
        )
        .await;
    assert_eq!(get(&api, &id).await["items"][0]["price"], "0.00");
    // `items` not a list.
    assert_eq!(
        error(&api, "", json!({ "relation_id": rel, "items": 5 }), "items").await,
        "The items must be an array."
    );
}

/// Item ids are scoped to their invoice: changing another invoice's line is a 422 and
/// deleting one is a no-op (Laravel changed it). A deleted row whose id is gone is skipped.
#[sqlx::test]
async fn item_ids_belong_to_their_invoice(pool: PgPool) {
    let api = api(pool).await;
    let (rel, _) = relation(&api, "Acme").await;
    let one = api
        .create(
            json!({ "relation_id": rel, "items": [line("A", json!(1), json!(1), json!(21))] }),
            "invoice_id",
        )
        .await;
    let two = api
        .create(
            json!({ "relation_id": rel, "items": [line("B", json!(1), json!(1), json!(21))] }),
            "invoice_id",
        )
        .await;
    let foreign = get(&api, &two).await["items"][0]["id"].clone();
    let change = json!({ "relation_id": rel, "items": [
        { "id": foreign, "title": "X", "quantity": 1, "price": 1, "vat": 21 }] });
    assert_eq!(
        error(&api, &format!("/{one}"), change, "items.0.id").await,
        "The selected items.0.id is invalid."
    );
    // Deleting a line that isn't on this invoice is a no-op.
    let delete = json!({ "relation_id": rel, "items": [{ "id": foreign, "deleted": true }] });
    assert_eq!(
        post(&api, &format!("/{one}"), delete).await.0,
        StatusCode::OK
    );
    assert_eq!(get(&api, &two).await["items"][0]["title"], "B");
    let gone = json!({ "relation_id": rel, "items": [
        { "id": "01900000-0000-7000-8000-000000000001", "deleted": true }] });
    let (status, _) = post(&api, &format!("/{one}"), gone).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(get(&api, &one).await["items"].as_array().unwrap().len(), 1);
}

#[sqlx::test]
async fn at_most_1000_lines(pool: PgPool) {
    let api = api(pool).await;
    let (rel, _) = relation(&api, "Acme").await;
    let lines: Vec<Value> = (0..1000)
        .map(|n| line(&format!("L{n}"), json!(1), json!(0.01), json!(21)))
        .collect();
    let id = api
        .create(json!({ "relation_id": rel, "items": lines }), "invoice_id")
        .await;
    let inv = get(&api, &id).await;
    assert_eq!(inv["items"].as_array().unwrap().len(), 1000);
    assert_eq!(inv["summary"]["total"], json!(12.1));
    let more =
        json!({ "relation_id": rel, "items": [line("one more", json!(1), json!(1), json!(21))] });
    assert_eq!(
        error(&api, &format!("/{id}"), more, "items").await,
        "An invoice holds at most 1000 items."
    );
}

#[sqlx::test]
async fn oversized_objects_are_refused(pool: PgPool) {
    let api = api(pool).await;
    let (rel, _) = relation(&api, "Acme").await;
    let big: Vec<u32> = (0..5000).collect();
    assert_eq!(
        error(
            &api,
            "",
            json!({ "relation_id": rel, "meta": { "x": big } }),
            "meta"
        )
        .await,
        "The meta is too large."
    );
}

// --- access --------------------------------------------------------------------------------

#[sqlx::test]
async fn access_rules(pool: PgPool) {
    let api = api(pool).await;
    let (rel, _) = relation(&api, "Acme").await;
    let body = json!({ "relation_id": rel, "items": [] });
    let id = api.create(body.clone(), "invoice_id").await;
    api.assert_member_forbidden(&id).await;
    api.assert_granted(
        &[
            "invoice.list",
            "invoice.view",
            "invoice.create",
            "invoice.update",
            "invoice.delete",
        ],
        body.clone(),
        "invoice_id",
    )
    .await;
    api.assert_foreign_404(&id, body.clone()).await;
    // Invoices are never soft-deleted yet: an unknown id stands in for a deleted one.
    api.assert_bad_ids_404("01900000-0000-7000-8000-000000000002", body)
        .await;
}

#[sqlx::test]
async fn another_workspaces_rows_are_unknown(pool: PgPool) {
    let api = api(pool).await;
    let (eve, eve_ws) = common::signup(&api.app, "eve@example.com", "evil").await;
    let mut eve_api = api.with_base("/invoices");
    eve_api.owner = eve;
    eve_api.ws = eve_ws;
    let (eve_rel, eve_contact) = relation(&eve_api, "Eve").await;
    let eve_prod = product(&eve_api, "Eve's").await;
    let (rel, _) = relation(&api, "Acme").await;
    let cases = [
        (json!({ "relation_id": eve_rel }), "relation_id"),
        (
            json!({ "relation_id": rel, "addressed_contact_id": eve_contact }),
            "addressed_contact_id",
        ),
        (
            json!({ "relation_id": rel, "items": [{ "title": "t", "quantity": 1, "price": 1, "vat": 21, "product_id": eve_prod }] }),
            "items.0.product_id",
        ),
    ];
    for (body, field) in cases {
        error(&api, "", body, field).await;
    }
}

// --- guards --------------------------------------------------------------------------------

#[sqlx::test]
async fn a_product_on_an_item_cant_be_deleted(pool: PgPool) {
    let api = api(pool).await;
    let (rel, _) = relation(&api, "Acme").await;
    let prod = product(&api, "Widget").await;
    let mut l = line("W", json!(1), json!(1), json!(21));
    l["product_id"] = json!(prod);
    let id = api
        .create(json!({ "relation_id": rel, "items": [l] }), "invoice_id")
        .await;
    let products = api.with_base("/products");
    let (status, res) = products
        .call(Method::DELETE, &format!("/{prod}"), None, &api.owner)
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(
        res,
        json!({ "message": "Product can not be deleted because it is used in items." })
    );
    let item = get(&api, &id).await["items"][0]["id"].clone();
    post(
        &api,
        &format!("/{id}"),
        json!({ "relation_id": rel, "items": [{ "id": item, "deleted": true }] }),
    )
    .await;
    let (status, _) = products
        .call(Method::DELETE, &format!("/{prod}"), None, &api.owner)
        .await;
    assert_eq!(status, StatusCode::OK);
}

#[sqlx::test]
async fn a_relation_with_invoices_cant_be_deleted(pool: PgPool) {
    let api = api(pool).await;
    let (rel, _) = relation(&api, "Acme").await;
    let id = api
        .create(json!({ "relation_id": rel }), "invoice_id")
        .await;
    // A cancelled invoice still counts (Laravel).
    api.call(Method::DELETE, &format!("/{id}"), None, &api.owner)
        .await;
    let relations = api.with_base("/relations");
    let (status, res) = relations
        .call(Method::DELETE, &format!("/{rel}"), None, &api.owner)
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(
        res["errors"]["relation_id"][0],
        "This relation cannot be deleted because it has invoices, orders, estimates, or work orders."
    );
    let (free, _) = relation(&api, "Free").await;
    let (status, _) = relations
        .call(Method::DELETE, &format!("/{free}"), None, &api.owner)
        .await;
    assert_eq!(status, StatusCode::OK);
}

/// A deleted product or relation can't be put on an invoice.
#[sqlx::test]
async fn deleted_references_are_refused(pool: PgPool) {
    let api = api(pool).await;
    let (rel, _) = relation(&api, "Acme").await;
    let prod = product(&api, "Widget").await;
    api.with_base("/products")
        .call(Method::DELETE, &format!("/{prod}"), None, &api.owner)
        .await;
    let mut l = line("W", json!(1), json!(1), json!(21));
    l["product_id"] = json!(prod);
    error(
        &api,
        "",
        json!({ "relation_id": rel, "items": [l] }),
        "items.0.product_id",
    )
    .await;
    let (gone, _) = relation(&api, "Gone").await;
    api.with_base("/relations")
        .call(Method::DELETE, &format!("/{gone}"), None, &api.owner)
        .await;
    error(&api, "", json!({ "relation_id": gone }), "relation_id").await;
}

// --- concurrency ---------------------------------------------------------------------------

/// 20 parallel updates of one draft, each adding a line: every line lands and the stored
/// totals equal a fresh recompute (the row lock serializes the item writes and totals).
#[sqlx::test]
async fn parallel_updates_lose_no_line(pool: PgPool) {
    let api = api(pool).await;
    let (rel, _) = relation(&api, "Acme").await;
    let id = api
        .create(json!({ "relation_id": rel, "items": [] }), "invoice_id")
        .await;
    let mut tasks = Vec::new();
    for n in 0..20 {
        let (app, uri, owner, rel) = (
            api.app.clone(),
            format!("/v1/workspace/{}/invoices/{id}", api.ws),
            api.owner.clone(),
            rel.clone(),
        );
        tasks.push(tokio::spawn(async move {
            let body = json!({ "relation_id": rel,
                               "items": [line(&format!("L{n}"), json!(1), json!(0.07), json!(21))] });
            common::send(&app, Method::POST, &uri, Some(body), Some(&owner)).await.0
        }));
    }
    for t in tasks {
        assert_eq!(t.await.unwrap(), StatusCode::OK);
    }
    let inv = get(&api, &id).await;
    assert_eq!(inv["items"].as_array().unwrap().len(), 20);
    // 20 × 0.07 = 1.40, VAT 0.29 once.
    assert_eq!(inv["summary"]["total"], json!(1.69));
    let (subtotal, total, _) = stored(&api, &id).await;
    assert_eq!((subtotal, total), (140, 169));
}

#[sqlx::test]
async fn a_member_with_only_list_sees_rows(pool: PgPool) {
    let api = api(pool).await;
    let (rel, _) = relation(&api, "Acme").await;
    api.create(json!({ "relation_id": rel }), "invoice_id")
        .await;
    let lister = member_with(
        &api.app,
        &api.pool,
        &api.ws,
        "l@example.com",
        &["invoice.list"],
    )
    .await;
    let (status, res) = api.call(Method::GET, "", None, &lister).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res["data"].as_array().unwrap().len(), 1);
}

/// A product delete that starts while an invoice write holds the product waits for it, then
/// sees the new line and refuses (the write key-share locks the product; the delete takes
/// FOR UPDATE).
#[sqlx::test]
async fn a_product_delete_waits_for_an_invoice_write(pool: PgPool) {
    let api = api(pool).await;
    let (rel, _) = relation(&api, "Acme").await;
    let prod = product(&api, "Widget").await;
    let id = api
        .create(json!({ "relation_id": rel, "items": [] }), "invoice_id")
        .await;
    // The invoice write in flight: product locked as the service locks it, line not yet in.
    let mut writer = api.pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM products WHERE id = $1::uuid FOR KEY SHARE")
        .bind(&prod)
        .execute(&mut *writer)
        .await
        .unwrap();
    let (app, uri, owner) = (
        api.app.clone(),
        format!("/v1/workspace/{}/products/{prod}", api.ws),
        api.owner.clone(),
    );
    let delete =
        tokio::spawn(
            async move { common::send(&app, Method::DELETE, &uri, None, Some(&owner)).await },
        );
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    assert!(!delete.is_finished(), "the delete waits for the writer");
    sqlx::query(
        "INSERT INTO items (workspace_id, invoice_id, product_id, title, quantity, vat)
         VALUES ($1::uuid, $2::uuid, $3::uuid, 'W', 1, 21)",
    )
    .bind(&api.ws)
    .bind(&id)
    .bind(&prod)
    .execute(&mut *writer)
    .await
    .unwrap();
    writer.commit().await.unwrap();
    let (status, res) = delete.await.unwrap();
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
}

// --- review fixes --------------------------------------------------------------------------

/// `meta` and `customer_ref` are capped in bytes too (list rows print `meta`): one 2 MiB
/// string is one node.
#[sqlx::test]
async fn header_objects_are_capped_in_bytes(pool: PgPool) {
    let api = api(pool).await;
    let (rel, _) = relation(&api, "Acme").await;
    let big = "x".repeat(17_000);
    assert_eq!(
        error(
            &api,
            "",
            json!({ "relation_id": rel, "meta": { "x": big } }),
            "meta"
        )
        .await,
        "The meta is too large."
    );
    let id = api
        .create(
            json!({ "relation_id": rel, "meta": { "x": "y".repeat(8_000) } }),
            "invoice_id",
        )
        .await;
    let long = json!({ "relation_id": rel, "customer_ref": { "po_number": "p".repeat(256) } });
    assert_eq!(
        error(&api, &format!("/{id}"), long, "customer_ref.po_number").await,
        "The customer ref.po number must not be greater than 255 characters."
    );
}

/// Moving a draft to another relation without new snapshots drops the old relation's
/// contact and takes the new relation's billing address.
#[sqlx::test]
async fn a_new_relation_resets_the_snapshots(pool: PgPool) {
    let api = api(pool).await;
    let (acme, contact) = relation(&api, "Acme").await;
    let beta = api
        .with_base("/relations")
        .create(
            json!({ "company_name": "Beta", "type": "business",
                    "addresses": [{ "address": "Side", "house_number": "2", "zipcode": "2000BB",
                                    "city": "Utrecht", "country": "NL", "type": "billing" }] }),
            "relation_id",
        )
        .await;
    let id = api
        .create(
            json!({ "relation_id": acme, "addressed_contact_id": contact }),
            "invoice_id",
        )
        .await;
    post(
        &api,
        &format!("/{id}"),
        json!({ "relation_id": acme, "reference": "same" }),
    )
    .await;
    let inv = get(&api, &id).await;
    assert_eq!(
        inv["addressed_contact"]["uuid"], contact,
        "same relation keeps it"
    );
    post(&api, &format!("/{id}"), json!({ "relation_id": beta })).await;
    let inv = get(&api, &id).await;
    assert_eq!(inv["addressed_contact"], Value::Null);
    assert_eq!(inv["billing_address"]["city"], "Utrecht");
}

/// A variant must belong to the line's product; another product's variant is dropped and the
/// product's default variant is taken (Laravel dropped unknown variants).
#[sqlx::test]
async fn a_variant_belongs_to_its_product(pool: PgPool) {
    let api = api(pool).await;
    let (rel, _) = relation(&api, "Acme").await;
    let (one, two) = (product(&api, "One").await, product(&api, "Two").await);
    let variant = |p: String| {
        let pool = api.pool.clone();
        async move {
            sqlx::query_scalar::<_, String>(
                "SELECT id::text FROM product_variants WHERE product_id = $1::uuid AND is_default",
            )
            .bind(p)
            .fetch_one(&pool)
            .await
            .unwrap()
        }
    };
    let (v1, v2) = (variant(one.clone()).await, variant(two.clone()).await);
    let mut l = line("W", json!(1), json!(1), json!(21));
    l["product_id"] = json!(one);
    l["product_variant_id"] = json!(v2);
    let id = api
        .create(json!({ "relation_id": rel, "items": [l] }), "invoice_id")
        .await;
    assert_eq!(get(&api, &id).await["items"][0]["product_variant_id"], v1);
}

/// An update row without `description` keeps it; null clears it (Laravel updated sent keys).
#[sqlx::test]
async fn an_update_row_patches_description(pool: PgPool) {
    let api = api(pool).await;
    let (rel, _) = relation(&api, "Acme").await;
    let mut l = line("A", json!(1), json!(1), json!(21));
    l["description"] = json!("kept");
    let id = api
        .create(json!({ "relation_id": rel, "items": [l] }), "invoice_id")
        .await;
    let item = get(&api, &id).await["items"][0]["id"].clone();
    let row = |extra: Value| {
        let mut r = json!({ "id": item, "title": "A", "quantity": 2, "price": 1, "vat": 21 });
        if let Value::Object(m) = extra {
            r.as_object_mut().unwrap().extend(m);
        }
        json!({ "relation_id": rel, "items": [r] })
    };
    post(&api, &format!("/{id}"), row(json!({}))).await;
    assert_eq!(get(&api, &id).await["items"][0]["description"], "kept");
    post(&api, &format!("/{id}"), row(json!({ "description": null }))).await;
    assert_eq!(get(&api, &id).await["items"][0]["description"], Value::Null);
}

/// Totals keep the decimals they were computed with: editing the currency afterwards doesn't
/// change a stored invoice's detail or list row.
#[sqlx::test]
async fn totals_keep_their_decimals(pool: PgPool) {
    let api = api(pool).await;
    let (rel, _) = relation(&api, "Acme").await;
    let id = api
        .create(
            json!({ "relation_id": rel, "items": [line("T", json!(3), json!(0.07), json!(21))] }),
            "invoice_id",
        )
        .await;
    sqlx::query("UPDATE currencies SET decimals = 0 WHERE workspace_id = $1::uuid")
        .bind(&api.ws)
        .execute(&api.pool)
        .await
        .unwrap();
    assert_eq!(get(&api, &id).await["summary"]["total"], json!(0.25));
    assert_eq!(
        api.list("").await["data"][0]["summary"]["total"],
        json!(0.25)
    );
}

/// A line whose margin leaves Decimal's range is refused on save, so the detail can always
/// price what is stored.
#[sqlx::test]
async fn an_unpriceable_line_is_refused_on_save(pool: PgPool) {
    let api = api(pool).await;
    let (rel, _) = relation(&api, "Acme").await;
    let mut l = line("Big", json!("9999999999999999"), json!(1), json!(0));
    l["purchase_price"] = json!("9999999999999999");
    assert_eq!(
        error(
            &api,
            "",
            json!({ "relation_id": rel, "items": [l] }),
            "items"
        )
        .await,
        "The item totals are out of range."
    );
}

/// Several rows with one id: the last one wins (Laravel applied them in order).
#[sqlx::test]
async fn the_last_row_of_an_id_wins(pool: PgPool) {
    let api = api(pool).await;
    let (rel, _) = relation(&api, "Acme").await;
    let id = api
        .create(
            json!({ "relation_id": rel, "items": [line("A", json!(1), json!(1), json!(21))] }),
            "invoice_id",
        )
        .await;
    let item = get(&api, &id).await["items"][0]["id"].clone();
    let row = |t: &str| json!({ "id": item, "title": t, "quantity": 1, "price": 1, "vat": 21 });
    post(
        &api,
        &format!("/{id}"),
        json!({ "relation_id": rel, "items": [row("first"), row("last")] }),
    )
    .await;
    assert_eq!(get(&api, &id).await["items"][0]["title"], "last");
    post(
        &api,
        &format!("/{id}"),
        json!({ "relation_id": rel,
        "items": [row("kept"), { "id": item, "deleted": true }] }),
    )
    .await;
    assert_eq!(get(&api, &id).await["items"], json!([]));
}

#[sqlx::test]
async fn header_patch_rules(pool: PgPool) {
    let api = api(pool).await;
    let (rel, contact) = relation(&api, "Acme").await;
    let id = api
        .create(
            json!({ "relation_id": rel, "reference": "  R1  ", "addressed_contact_id": contact,
                        "meta": { "a": 1 } }),
            "invoice_id",
        )
        .await;
    let inv = get(&api, &id).await;
    assert_eq!(inv["reference"], "R1", "trimmed (Laravel TrimStrings)");
    let today = chrono::Utc::now().date_naive().to_string();
    let (status, res) = post(
        &api,
        &format!("/{id}"),
        json!({
        "relation_id": rel, "reference": "   ", "meta": null, "addressed_contact_id": null,
        "due_date": today, "customer_ref": { "po_no": " PO-7 ", "ordered_by": "Ada" } }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    let inv = get(&api, &id).await;
    assert_eq!(inv["reference"], Value::Null, "blank clears");
    assert_eq!(inv["meta"], Value::Null);
    assert_eq!(inv["addressed_contact"], Value::Null);
    assert!(inv["due_date"].as_str().unwrap().starts_with(&today));
    assert_eq!(
        inv["customer_ref"],
        json!({ "ordered_by": "Ada", "po_number": "PO-7", "reference": null })
    );
    let yesterday = (chrono::Utc::now() - chrono::Duration::days(1))
        .date_naive()
        .to_string();
    assert_eq!(
        error(
            &api,
            &format!("/{id}"),
            json!({ "relation_id": rel, "due_date": yesterday }),
            "due_date"
        )
        .await,
        "The due date must be a date after yesterday."
    );
    // Left out: kept.
    post(&api, &format!("/{id}"), json!({ "relation_id": rel })).await;
    assert_eq!(get(&api, &id).await["customer_ref"]["po_number"], "PO-7");
    let huge = json!({ "relation_id": rel, "billing_address": { "address": "x".repeat(17_000) } });
    assert_eq!(
        error(&api, &format!("/{id}"), huge, "billing_address").await,
        "The billing address is too large."
    );
}

/// Set children and rows whose set parent was deleted are skipped (no product sets yet).
#[sqlx::test]
async fn set_rows_are_skipped(pool: PgPool) {
    let api = api(pool).await;
    let (rel, _) = relation(&api, "Acme").await;
    let mut child = line("child", json!(1), json!(0), json!(0));
    child["multi_set_product_id"] = json!("01900000-0000-7000-8000-000000000001");
    let id = api
        .create(json!({ "relation_id": rel, "items": [line("A", json!(1), json!(1), json!(21)), child] }), "invoice_id")
        .await;
    let items = get(&api, &id).await["items"].clone();
    assert_eq!(items.as_array().unwrap().len(), 1);
    let orphan = json!({ "id": items[0]["id"], "title": "changed", "quantity": 1, "price": 1,
                         "vat": 21, "parent_deleted": true });
    post(
        &api,
        &format!("/{id}"),
        json!({ "relation_id": rel, "items": [orphan] }),
    )
    .await;
    assert_eq!(get(&api, &id).await["items"][0]["title"], "A");
}

/// A relation delete that starts while an invoice write holds the relation waits for it, then
/// refuses.
#[sqlx::test]
async fn a_relation_delete_waits_for_an_invoice_write(pool: PgPool) {
    let api = api(pool).await;
    let (rel, _) = relation(&api, "Acme").await;
    let currency: String = sqlx::query_scalar(
        "SELECT id::text FROM currencies WHERE workspace_id = $1::uuid AND default_currency",
    )
    .bind(&api.ws)
    .fetch_one(&api.pool)
    .await
    .unwrap();
    let mut writer = api.pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM relations WHERE id = $1::uuid FOR KEY SHARE")
        .bind(&rel)
        .execute(&mut *writer)
        .await
        .unwrap();
    let (app, uri, owner) = (
        api.app.clone(),
        format!("/v1/workspace/{}/relations/{rel}", api.ws),
        api.owner.clone(),
    );
    let delete =
        tokio::spawn(
            async move { common::send(&app, Method::DELETE, &uri, None, Some(&owner)).await },
        );
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    assert!(!delete.is_finished(), "the delete waits for the writer");
    sqlx::query(
        "INSERT INTO invoices (workspace_id, relation_id, currency_id)
         VALUES ($1::uuid, $2::uuid, $3::uuid)",
    )
    .bind(&api.ws)
    .bind(&rel)
    .bind(&currency)
    .execute(&mut *writer)
    .await
    .unwrap();
    writer.commit().await.unwrap();
    let (status, res) = delete.await.unwrap();
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
}
