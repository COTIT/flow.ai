//! GET /sales-returns/candidates over HTTP: per recognised sale of one order, what a new
//! credit may take (same capacity as the create check), workspace isolation, right gate.

use axum::http::{Method, StatusCode};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{sales_returns::draft, sales_returns_schema::shipped, shipments::S};

async fn candidates(s: &S, order: &str) -> (StatusCode, Value) {
    s.req(
        Method::GET,
        &format!("/sales-returns/candidates?order_id={order}"),
        None,
        None,
    )
    .await
}

#[sqlx::test]
async fn lists_creditable_lines_and_follows_drafts(pool: PgPool) {
    let (s, o, order_line, rec, line) = shipped(pool).await;
    let (st, b) = candidates(&s, &o).await;
    assert_eq!(st, StatusCode::OK, "{b}");
    let d = &b["data"];
    assert_eq!(d["order_id"], o);
    assert!(d["order_number"].is_string(), "{b}");
    let r = &d["recognitions"];
    assert_eq!(r.as_array().unwrap().len(), 1, "{b}");
    assert_eq!(r[0]["recognition_id"], rec.to_string());
    assert_eq!(r[0]["source_kind"], "shipment");
    assert_eq!(r[0]["kind"], "receipt_credit");
    let l = &r[0]["lines"][0];
    assert_eq!(l["recognition_line_id"], line.to_string());
    assert_eq!(l["order_line_id"], order_line);
    assert!(l["product_name"].is_string(), "{b}");
    assert_eq!(l["recognised"], "2");
    assert_eq!(l["creditable"], "2");

    // A partial draft holds capacity: 1 left, and creating exactly that passes, one more fails.
    let (st, a) = draft(&s, rec, &[(line, "1")]).await;
    assert_eq!(st, StatusCode::CREATED, "{a}");
    let (_, b) = candidates(&s, &o).await;
    assert_eq!(b["data"]["recognitions"][0]["lines"][0]["creditable"], "1");
    let (st, x) = draft(&s, rec, &[(line, "1.01")]).await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY, "{x}");

    // Cancelling the draft frees it again.
    let id = a["data"]["id"].as_str().unwrap();
    let (st, c) = s
        .post(
            &format!("/sales-returns/{id}/cancel"),
            json!({ "client_ref": Uuid::now_v7(), "expected_version": a["data"]["version"],
                    "payload": { "reason": "typo" } }),
        )
        .await;
    assert_eq!(st, StatusCode::OK, "{c}");
    let (_, b) = candidates(&s, &o).await;
    assert_eq!(b["data"]["recognitions"][0]["lines"][0]["creditable"], "2");
}

#[sqlx::test]
async fn an_order_without_recognitions_lists_none(pool: PgPool) {
    let (s, _, _, _, _) = shipped(pool).await;
    let (o2, _) = s.open_order("1").await;
    let (st, b) = candidates(&s, &o2).await;
    assert_eq!(st, StatusCode::OK, "{b}");
    assert_eq!(b["data"]["recognitions"], json!([]));
}

#[sqlx::test]
async fn refuses_missing_order_other_workspaces_and_missing_right(pool: PgPool) {
    let (s, o, _, _, _) = shipped(pool).await;
    let (st, b) = s
        .req(Method::GET, "/sales-returns/candidates", None, None)
        .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    assert!(b["errors"]["order_id"].is_array(), "{b}");
    let (st, _) = candidates(&s, &Uuid::now_v7().to_string()).await;
    assert_eq!(st, StatusCode::NOT_FOUND);

    let other = s.api.as_new_owner("eve@example.com", "evil").await;
    crate::sales_recognition::activate_sales(s.pool(), &other.ws).await;
    let path = format!("/sales-returns/candidates?order_id={o}");
    let (st, _) = s
        .req_as(Method::GET, &path, None, None, &other.owner, &other.ws)
        .await;
    assert_eq!(st, StatusCode::NOT_FOUND);

    let reader = crate::common::member_with(
        &s.api.app,
        s.pool(),
        s.ws(),
        "r@example.com",
        &["salesreturn.get", "salesreturn.list"],
    )
    .await;
    let ws = s.ws().to_owned();
    let (st, _) = s.req_as(Method::GET, &path, None, None, &reader, &ws).await;
    assert_eq!(st, StatusCode::FORBIDDEN);
}
