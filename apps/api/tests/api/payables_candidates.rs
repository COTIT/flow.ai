//! Payables pickers for Office (Flow ERP M7-A): bill match candidates, purchase return
//! candidates, supplier payment targets and the cost-adjustment bill filter. Read-only; each
//! candidate quantity or residual is exactly what the matching write then accepts.

use crate::{
    common,
    payables::{bank_location, book, goods},
    purchasing::{uuid, world},
};

use axum::http::{Method, StatusCode};
use common::{member_with, send};
use serde_json::json;
use sqlx::PgPool;

impl crate::purchasing::W {
    /// A paged list body (data + meta).
    async fn page(&self, path: &str) -> serde_json::Value {
        let (s, b) = self.req(Method::GET, path, None).await;
        assert_eq!(s, StatusCode::OK, "{b}");
        b
    }
}

#[sqlx::test]
async fn bill_match_candidates_are_exactly_what_match_accepts(pool: PgPool) {
    let w = world(pool).await;
    book(&w).await;
    let r1 = w.booked(&w.eur, &w.wax, "10", "10.00", 0).await;
    let r2 = w.booked(&w.eur, &w.wax, "5", "10.00", 0).await;
    let (l1, l2) = (r1["lines"][0]["id"].clone(), r2["lines"][0]["id"].clone());
    w.returned(&r1, "4", None, false).await;
    // Another draft reserves 2 of r2.
    let other = w
        .bill(&w.eur, "B", json!([goods(&w.wax, "2", "10.00")]))
        .await;
    let (s, res) = w.matched(&other, 0, &[(&l2, "2")]).await;
    assert_eq!(s, StatusCode::OK, "{res}");

    let bill = w
        .bill(&w.eur, "A", json!([goods(&w.wax, "11", "10.00")]))
        .await;
    assert_eq!(bill["lines"][0]["product_name"], "Wax", "{bill}");
    let id = bill["id"].as_str().unwrap();
    let path = format!("/supplier-bills/{id}/match-candidates");
    let c = w.get(&path).await;
    let rows = c["candidates"].as_array().unwrap().clone();
    assert_eq!(rows.len(), 2, "{c}");
    let line = bill["lines"][0]["id"].clone();
    let of = |rl: &serde_json::Value| {
        rows.iter()
            .find(|x| x["receipt_line_id"] == *rl)
            .cloned()
            .unwrap()
    };
    assert_eq!(of(&l1)["bill_line_id"], line);
    assert_eq!(of(&l1)["matchable"], "6");
    assert_eq!(of(&l2)["matchable"], "3");
    assert_eq!(of(&l1)["receipt_id"], r1["id"]);
    assert!(of(&l1)["receipt_number"].as_i64().is_some(), "{c}");
    assert!(of(&l1)["purchase_order_number"].as_i64().is_some(), "{c}");

    // One more than a candidate is refused; the candidate itself is accepted.
    let (s, res) = w.matched(&bill, 0, &[(&l1, "6.01")]).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    let (s, res) = w.matched(&bill, 0, &[(&l1, "6"), (&l2, "3")]).await;
    assert_eq!(s, StatusCode::OK, "{res}");
    let m = &res["data"]["lines"][0]["matches"];
    assert_eq!(m[0]["receipt_id"], r1["id"], "{res}");
    assert_eq!(m[0]["purchase_order_id"], r1["purchase_order_id"], "{res}");
    // The bill's own reservations stay in its candidates (match replaces them).
    let c = w.get(&path).await;
    assert_eq!(c["candidates"].as_array().unwrap().len(), 2, "{c}");

    // Permission and isolation.
    let get_only = member_with(
        &w.api.app,
        w.pool(),
        &w.api.ws,
        "bob@example.com",
        &["supplierbill.get"],
    )
    .await;
    let (s, _) = w.req_as(&get_only, Method::GET, &path, None).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = w
        .req(
            Method::GET,
            &format!("/supplier-bills/{}/match-candidates", uuid()),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let eve = w.api.as_new_owner("eve@example.com", "evil").await;
    let (s, _) = send(
        &w.api.app,
        Method::GET,
        &format!("/v1/workspace/{}{path}", eve.ws),
        None,
        Some(&eve.owner),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn return_candidates_are_exactly_what_returns_accept(pool: PgPool) {
    let w = world(pool).await;
    book(&w).await;
    let r = w.booked(&w.eur, &w.wax, "10", "10.00", 0).await;
    let rl = r["lines"][0]["id"].clone();
    let bill = w
        .bill(&w.eur, "R", json!([goods(&w.wax, "6", "10.00")]))
        .await;
    let (_, bill) = w.matched(&bill, 0, &[(&rl, "6")]).await;
    let bill = w.posted(&bill["data"]).await;
    let m = bill["lines"][0]["matches"][0]["id"].clone();
    let ret = w.returned(&r, "2", Some(&m), false).await;
    assert_eq!(ret["lines"][0]["product_name"], "Wax", "{ret}");
    assert_eq!(ret["lines"][0]["warehouse_name"], "Shop", "{ret}");

    let path = format!(
        "/purchase-returns/candidates?receipt_id={}",
        r["id"].as_str().unwrap()
    );
    let c = w.get(&path).await;
    let d = &c;
    assert_eq!(d["receipt_id"], r["id"], "{c}");
    assert_eq!(d["supplier"]["id"], w.supplier.as_str());
    let line = &d["lines"][0];
    assert_eq!(line["receipt_line_id"], rl);
    assert_eq!(line["product_name"], "Wax");
    assert_eq!(line["warehouse_name"], "Shop");
    assert_eq!(line["booked"], "10");
    assert_eq!(line["unbilled_returnable"], "4", "{c}");
    assert_eq!(line["billed"][0]["bill_allocation_id"], m);
    assert_eq!(line["billed"][0]["bill_id"], bill["id"]);
    assert_eq!(line["billed"][0]["supplier_invoice_number"], "R");
    assert_eq!(line["billed"][0]["returnable"], "4", "{c}");

    // One more than a candidate is refused; the candidate itself is accepted.
    for (q, billed, ok) in [
        ("4.01", None, false),
        ("4", None, true),
        ("4.01", Some(&m), false),
        ("4", Some(&m), true),
    ] {
        let (s, res) = w.return_draft(&r, q, billed, false).await;
        let want = if ok {
            StatusCode::CREATED
        } else {
            StatusCode::UNPROCESSABLE_ENTITY
        };
        assert_eq!(s, want, "{q} billed={}: {res}", billed.is_some());
    }

    let list_only = member_with(
        &w.api.app,
        w.pool(),
        &w.api.ws,
        "bob@example.com",
        &["purchasereturn.list", "purchasereturn.get"],
    )
    .await;
    let (s, _) = w.req_as(&list_only, Method::GET, &path, None).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = w
        .req(
            Method::GET,
            &format!("/purchase-returns/candidates?receipt_id={}", uuid()),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, res) = w
        .req(Method::GET, "/purchase-returns/candidates", None)
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    let eve = w.api.as_new_owner("eve@example.com", "evil").await;
    let (s, _) = send(
        &w.api.app,
        Method::GET,
        &format!("/v1/workspace/{}{path}", eve.ws),
        None,
        Some(&eve.owner),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn payment_targets_are_exactly_what_allocate_accepts(pool: PgPool) {
    let w = world(pool).await;
    let rent = book(&w).await;
    let mut bills = Vec::new();
    for (n, price) in [("X-1", "30"), ("X-2", "20")] {
        let b = w
            .bill(
                &w.eur,
                n,
                json!([{ "expense_account_id": rent, "quantity": "1", "unit_price": price }]),
            )
            .await;
        bills.push(w.posted(&b).await);
    }
    let bank = w.bank(&w.eur, "1120").await;
    let pay = w
        .paid(
            w.money(&w.eur, "payment", "40.00", bank_location(&bank))
                .await,
        )
        .await;
    let id = pay["id"].as_str().unwrap();
    let path = format!("/supplier-payments/{id}/targets");
    let t = w.page(&path).await;
    assert_eq!(t["meta"]["total"], 2, "{t}");
    let row = |t: &serde_json::Value, bill: &serde_json::Value| {
        t["data"]
            .as_array()
            .unwrap()
            .iter()
            .find(|x| x["open_item_id"] == bill["ap_item_id"])
            .cloned()
            .unwrap()
    };
    let x1 = row(&t, &bills[0]);
    assert_eq!(x1["residual"], "30.00", "{t}");
    assert_eq!(x1["source_kind"], "supplier_bill");
    assert_eq!(x1["source_id"], bills[0]["id"]);
    assert_eq!(x1["source_number"], bills[0]["number"]);
    assert_eq!(x1["supplier_invoice_number"], "X-1");

    let (s, res) = w.allocate(&pay, &bills[0]["ap_item_id"], "30.01").await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    let (s, res) = w.allocate(&pay, &bills[0]["ap_item_id"], "30.00").await;
    assert_eq!(s, StatusCode::OK, "{res}");
    // Settled item drops out; the rest shows what is still open.
    let t = w.page(&path).await;
    assert_eq!(t["meta"]["total"], 1, "{t}");
    assert_eq!(row(&t, &bills[1])["residual"], "20.00");

    let get_only = member_with(
        &w.api.app,
        w.pool(),
        &w.api.ws,
        "bob@example.com",
        &["supplierpayment.get"],
    )
    .await;
    let (s, _) = w.req_as(&get_only, Method::GET, &path, None).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = w
        .req(
            Method::GET,
            &format!("/supplier-payments/{}/targets", uuid()),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let eve = w.api.as_new_owner("eve@example.com", "evil").await;
    let (s, _) = send(
        &w.api.app,
        Method::GET,
        &format!("/v1/workspace/{}{path}", eve.ws),
        None,
        Some(&eve.owner),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn cost_adjustments_filter_by_bill(pool: PgPool) {
    let w = world(pool).await;
    book(&w).await;
    let r = w.booked(&w.eur, &w.wax, "3", "10.00", 0).await;
    let rl = r["lines"][0]["id"].clone();
    let bill = w
        .bill(&w.eur, "LATE", json!([goods(&w.wax, "3", "11.00")]))
        .await;
    let (_, bill) = w.matched(&bill, 0, &[(&rl, "3")]).await;
    let bill = w.posted(&bill["data"]).await;
    let (s, adj) = w.adjust(&bill, &rl, "2.00", "1.00").await;
    assert_eq!(s, StatusCode::OK, "{adj}");
    let id = bill["id"].as_str().unwrap();
    let list = w
        .page(&format!("/purchase-cost-adjustments?bill_id={id}"))
        .await;
    assert_eq!(list["meta"]["total"], 1, "{list}");
    assert_eq!(list["data"][0]["bill_id"], bill["id"]);
    let list = w
        .page(&format!("/purchase-cost-adjustments?bill_id={}", uuid()))
        .await;
    assert_eq!(list["meta"]["total"], 0, "{list}");
}
