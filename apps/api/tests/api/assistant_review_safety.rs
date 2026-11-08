//! Voice reviews show current values, exact money changes and large-change warnings.

use super::assistant_preconditions::{approved, approved_method, dispatch, world};
use crate::common::{Api, send};
use axum::http::{Method, StatusCode};
use serde_json::{Value, json};
use sqlx::PgPool;

/// Proposal only: status and body, no approval.
async fn propose(
    api: &Api,
    operation: &str,
    method: Method,
    path: &str,
    body: Value,
) -> (StatusCode, Value) {
    let base = format!("/v1/workspace/{}/assistant", api.ws);
    let (_, session) = send(
        &api.app,
        Method::POST,
        &format!("{base}/sessions"),
        Some(json!({"language":"en"})),
        Some(&api.owner),
    )
    .await;
    send(
        &api.app,
        Method::POST,
        &format!("{base}/actions"),
        Some(json!({"session_id":session["data"]["id"],"operation_id":operation,"method":method.as_str(),"path":path,"body":body})),
        Some(&api.owner),
    )
    .await
}

/// Approved DELETE dispatch with no body, so the request hash matches the reviewed proposal.
async fn delete(api: &Api, path: &str, session: &str, action: &str) -> (StatusCode, Value) {
    let (status, body) = crate::common::send_raw(
        &api.app,
        Method::DELETE,
        path,
        None,
        Some(&api.owner),
        &[
            ("Assistant-Action-Id", action),
            ("Assistant-Session-Id", session),
        ],
    )
    .await;
    (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
}

fn change<'a>(facts: &'a Value, field: &str) -> &'a Value {
    facts["changes"]
        .as_array()
        .unwrap_or_else(|| panic!("no changes in {facts}"))
        .iter()
        .find(|c| c["field"] == field)
        .unwrap_or_else(|| panic!("no {field} change in {facts}"))
}

#[sqlx::test]
async fn price_update_review_has_before_exact_change_and_tenfold_warning(pool: PgPool) {
    let (api, _, product) = world(pool).await;
    let path = format!("/v1/workspace/{}/products/{product}", api.ws);
    let (session, action, proposed) =
        approved(&api, "products_update", &path, json!({"price":"125"})).await;
    let facts = &proposed["review_facts"]["facts"];
    assert_eq!(facts["before"]["price"], "12.5", "{facts}");
    assert_eq!(facts["priced"]["price"], "125");
    let price = change(facts, "price");
    let decimals = price["currency"]["decimals"].as_u64().unwrap() as u32;
    assert_eq!(price["before_minor"], 1250 * 10_i64.pow(decimals) / 100);
    assert_eq!(price["after_minor"], 12500 * 10_i64.pow(decimals) / 100);
    assert_eq!(price["change_bp"], 90000);
    assert_eq!(price["warning"], "large_change");
    assert!(price["currency"]["code"].is_string());
    for entry in facts["changes"].as_array().unwrap() {
        assert_ne!(
            entry.get("before_minor").or(entry.get("before")),
            entry.get("after_minor").or(entry.get("after")),
            "{entry}"
        );
        assert_ne!(entry["field"], "name");
    }
    // Extra review keys leave the priced stale check unchanged: approval still executes.
    let (status, body) = dispatch(
        &api.app,
        &path,
        json!({"price":"125"}),
        &api.owner,
        &session,
        &action,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

#[sqlx::test]
async fn warning_thresholds_are_tenfold_up_and_ninety_percent_down(pool: PgPool) {
    let (api, _, product) = world(pool).await;
    let path = format!("/v1/workspace/{}/products/{product}", api.ws);
    for (price, bp, warned) in [
        ("124.99", 89992, false),
        ("1.25", -9000, true),
        ("1.26", -8992, false),
    ] {
        let (status, body) = propose(
            &api,
            "products_update",
            Method::POST,
            &path,
            json!({"price":price}),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        let entry = change(&body["data"]["review_facts"]["facts"], "price");
        assert_eq!(entry["change_bp"], bp, "{price}");
        assert_eq!(entry.get("warning").is_some(), warned, "{price}: {entry}");
    }
}

#[sqlx::test]
async fn sub_cent_price_keeps_exact_decimal_strings(pool: PgPool) {
    let (api, _, product) = world(pool).await;
    let path = format!("/v1/workspace/{}/products/{product}", api.ws);
    let (status, body) = propose(
        &api,
        "products_update",
        Method::POST,
        &path,
        json!({"price":"743.0578"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let entry = change(&body["data"]["review_facts"]["facts"], "price");
    assert_eq!(entry["before"], "12.5", "{entry}");
    assert_eq!(entry["after"], "743.0578");
    assert!(entry.get("before_minor").is_none() && entry.get("after_minor").is_none());
}

#[sqlx::test]
async fn zero_before_price_has_no_ratio_or_warning(pool: PgPool) {
    let (api, _, _) = world(pool).await;
    let product = api
        .with_base("/products")
        .create(json!({"name":"Free sample","price":"0"}), "product_id")
        .await;
    let path = format!("/v1/workspace/{}/products/{product}", api.ws);
    let (_, body) = propose(
        &api,
        "products_update",
        Method::POST,
        &path,
        json!({"price":"500"}),
    )
    .await;
    let entry = change(&body["data"]["review_facts"]["facts"], "price");
    assert!(
        entry.get("change_bp").is_none() && entry.get("warning").is_none(),
        "{entry}"
    );
}

#[sqlx::test]
async fn variant_update_review_has_before_and_warning(pool: PgPool) {
    let (api, _, product) = world(pool).await;
    let path = format!("/v1/workspace/{}/products/{product}/variants", api.ws);
    let (_, created) = send(
        &api.app,
        Method::POST,
        &path,
        Some(json!({"price":"2","sku":"Small"})),
        Some(&api.owner),
    )
    .await;
    let path = format!("{path}/{}", created["data"]["variant_id"].as_str().unwrap());
    let (_, _, proposed) = approved_method(
        &api,
        "product_variants_update",
        &path,
        json!({"price":"200"}),
        Method::PUT,
    )
    .await;
    let facts = &proposed["review_facts"]["facts"];
    assert_eq!(facts["before"]["variant"]["price"], "2", "{facts}");
    let entry = change(facts, "price");
    assert_eq!(entry["change_bp"], 990000);
    assert_eq!(entry["warning"], "large_change");
}

#[sqlx::test]
async fn another_workspace_product_is_not_reviewed(pool: PgPool) {
    let (api, _, _) = world(pool).await;
    let foreign = api.as_new_owner("eve@example.com", "foreign").await;
    let product = foreign
        .with_base("/products")
        .create(
            json!({"name":"Private foreign honey","price":"88"}),
            "product_id",
        )
        .await;
    let path = format!("/v1/workspace/{}/products/{product}", api.ws);
    let (status, body) = propose(
        &api,
        "products_update",
        Method::POST,
        &path,
        json!({"price":"880"}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert!(!body.to_string().contains("Private foreign honey"));
}

async fn customer(api: &Api, name: &str) -> String {
    let id = api
        .with_base("/relations")
        .create(
            if name.is_empty() {
                json!({"type":"personal"})
            } else {
                json!({"type":"personal","alias":name})
            },
            "relation_id",
        )
        .await;
    let (status, body) = api
        .with_base("/relations")
        .call(
            Method::POST,
            &format!("/{id}/contacts"),
            Some(json!({"first_name":"Kendall","last_name":"Moreau"})),
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    id
}

#[sqlx::test]
async fn customer_with_invoices_is_refused_before_any_review(pool: PgPool) {
    let (api, _, _) = world(pool.clone()).await;
    let rel = customer(&api, "Kendall Moreau").await;
    api.with_base("/invoices")
        .create(json!({"relation_id":rel}), "invoice_id")
        .await;
    let path = format!("/v1/workspace/{}/relations/{rel}", api.ws);
    let (status, body) =
        propose(&api, "relations_delete", Method::DELETE, &path, Value::Null).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(
        body["errors"]["relation_id"][0],
        "This relation cannot be deleted because it has invoices, orders, estimates, or work orders."
    );
    let actions: i64 = sqlx::query_scalar("SELECT count(*) FROM assistant_actions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(actions, 0, "no review row for a refused delete");
}

#[sqlx::test]
async fn customer_delete_review_names_target_and_counts_impact_bound_at_approval(pool: PgPool) {
    let (api, _, product) = world(pool.clone()).await;
    let rel = customer(&api, "").await;
    let orders = format!("/v1/workspace/{}/orders", api.ws);
    let order = json!({"relation_id":rel,"lines":[{"product_id":product,"quantity":"1"}]});
    let (status, body) = send(
        &api.app,
        Method::POST,
        &orders,
        Some(order.clone()),
        Some(&api.owner),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let path = format!("/v1/workspace/{}/relations/{rel}", api.ws);
    let (session, action, proposed) = super::assistant_preconditions::approved_method(
        &api,
        "relations_delete",
        &path,
        Value::Null,
        Method::DELETE,
    )
    .await;
    let facts = &proposed["review_facts"]["facts"];
    assert_eq!(facts["kind"], "relation", "{facts}");
    assert_eq!(
        facts["locked"]["label"], "Kendall Moreau",
        "first contact names an alias-less customer"
    );
    assert_eq!(
        facts["impact"],
        json!({"effect":"archive","orders":1,"contacts":1})
    );
    // A new order after review changes the impact: approval is stale and the customer stays.
    send(
        &api.app,
        Method::POST,
        &orders,
        Some(order),
        Some(&api.owner),
    )
    .await;
    let (status, body) = delete(&api, &path, &session, &action).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(
        body.to_string().contains("Assistant review is stale"),
        "{body}"
    );
    let live: bool =
        sqlx::query_scalar("SELECT deleted_at IS NULL FROM relations WHERE id=$1::uuid")
            .bind(&rel)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(live);
    let (session, action, _) = super::assistant_preconditions::approved_method(
        &api,
        "relations_delete",
        &path,
        Value::Null,
        Method::DELETE,
    )
    .await;
    let (status, body) = delete(&api, &path, &session, &action).await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

#[sqlx::test]
async fn product_delete_review_counts_sizes_and_used_product_is_refused(pool: PgPool) {
    let (api, _, product) = world(pool).await;
    let base = format!("/v1/workspace/{}/products/{product}", api.ws);
    let (status, body) = send(
        &api.app,
        Method::POST,
        &format!("{base}/variants"),
        Some(json!({"price":"1","sku":"Small"})),
        Some(&api.owner),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let (status, body) = propose(&api, "products_delete", Method::DELETE, &base, Value::Null).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let facts = &body["data"]["review_facts"]["facts"];
    assert_eq!(facts["locked"]["label"], "Reviewed honey", "{facts}");
    assert_eq!(facts["impact"]["effect"], "archive");
    assert_eq!(facts["impact"]["variants"], 2, "default size plus Small");
    let rel = customer(&api, "Buyer").await;
    api.with_base("/invoices")
        .create(json!({"relation_id":rel,"items":[{"product_id":product,"title":"Honey","quantity":1,"price":12.5,"vat":21}]}), "invoice_id")
        .await;
    let (status, body) = propose(&api, "products_delete", Method::DELETE, &base, Value::Null).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(
        body.to_string()
            .contains("Product can not be deleted because it is used in items.")
    );
}

#[sqlx::test]
async fn delete_reviews_refuse_other_workspaces_and_missing_permission(pool: PgPool) {
    let (api, _, _) = world(pool.clone()).await;
    let foreign = api.as_new_owner("eve@example.com", "foreign").await;
    let rel = foreign
        .with_base("/relations")
        .create(
            json!({"type":"business","company_name":"Private foreign buyer"}),
            "relation_id",
        )
        .await;
    let path = format!("/v1/workspace/{}/relations/{rel}", api.ws);
    let (status, body) =
        propose(&api, "relations_delete", Method::DELETE, &path, Value::Null).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert!(!body.to_string().contains("Private foreign buyer"));
    let mine = customer(&api, "Mine").await;
    let member =
        crate::common::member_with(&api.app, &pool, &api.ws, "member@example.com", &[]).await;
    let base = format!("/v1/workspace/{}/assistant", api.ws);
    let (_, session) = send(
        &api.app,
        Method::POST,
        &format!("{base}/sessions"),
        Some(json!({"language":"en"})),
        Some(&member),
    )
    .await;
    let path = format!("/v1/workspace/{}/relations/{mine}", api.ws);
    let (status, body) = send(&api.app, Method::POST, &format!("{base}/actions"), Some(json!({"session_id":session["data"]["id"],"operation_id":"relations_delete","method":"DELETE","path":path})), Some(&member)).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert!(!body.to_string().contains("Kendall"));
}
