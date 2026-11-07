//! Order conversion approvals bind the native invoice defaults before any invoice write.

use super::assistant_preconditions::{approved, dispatch, world};
use crate::common::{Api, send};
use axum::http::{Method, StatusCode};
use serde_json::{Value, json};
use sqlx::PgPool;

async fn conversion_world(pool: PgPool) -> (Api, String, String, String) {
    let (api, _, _) = world(pool).await;
    let product = api
        .with_base("/products")
        .create(
            json!({"name":"Conversion honey","price":"12.50","has_stock":false}),
            "product_id",
        )
        .await;
    let relation = api.with_base("/relations").create(json!({"company_name":"Conversion buyer","type":"business","addresses":[{"address":"Main","house_number":"1","zipcode":"1000AA","city":"Reviewed town","country":"NL","type":"billing"}]}), "relation_id").await;
    let order = api
        .create(
            json!({"relation_id":relation,"lines":[{"product_id":product,"quantity":"2"}]}),
            "order_id",
        )
        .await;
    let (status, body) = send(
        &api.app,
        Method::POST,
        &format!("/v1/workspace/{}/orders/{order}/confirm", api.ws),
        Some(json!({})),
        Some(&api.owner),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    (api, order, relation, product)
}

async fn no_invoice(pool: &PgPool, api: &Api, order: &str) {
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM invoices WHERE workspace_id=$1::uuid")
            .bind(&api.ws)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(count, 0, "stale conversion must insert no invoice");
    let linked: Option<uuid::Uuid> = sqlx::query_scalar(
        "SELECT invoice_id FROM orders WHERE workspace_id=$1::uuid AND id=$2::uuid",
    )
    .bind(&api.ws)
    .bind(order)
    .fetch_one(pool)
    .await
    .unwrap();
    assert!(linked.is_none());
    let events:i64=sqlx::query_scalar("SELECT count(*) FROM order_events WHERE workspace_id=$1::uuid AND order_id=$2::uuid AND kind='invoiced'").bind(&api.ws).bind(order).fetch_one(pool).await.unwrap();
    assert_eq!(events, 0);
}

#[sqlx::test]
async fn order_invoice_billing_drift_refuses_then_fresh_review_succeeds(pool: PgPool) {
    let (api, order, relation, _) = conversion_world(pool.clone()).await;
    let path = format!("/v1/workspace/{}/orders/{order}/invoice", api.ws);
    let (session, action, _) = approved(&api, "orders_invoice", &path, json!({})).await;
    sqlx::query("UPDATE relations SET addresses=$3 WHERE workspace_id=$1::uuid AND id=$2::uuid").bind(&api.ws).bind(&relation).bind(json!([{"address":"Main","house_number":"1","zipcode":"1000AA","city":"Changed town","country":"NL","type":"billing"}])).execute(&pool).await.unwrap();
    let (status, body) = dispatch(&api.app, &path, json!({}), &api.owner, &session, &action).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    no_invoice(&pool, &api, &order).await;
    let (session, action, review) = approved(&api, "orders_invoice", &path, json!({})).await;
    assert_eq!(
        review["review_facts"]["facts"]["priced"]["billing_address"]["city"],
        "Changed town"
    );
    let (status, body) = dispatch(&api.app, &path, json!({}), &api.owner, &session, &action).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let invoice = body["data"]["invoice_id"].as_str().unwrap();
    let stored:Value=sqlx::query_scalar("SELECT jsonb_build_object('billing',billing_address,'total',total) FROM invoices WHERE workspace_id=$1::uuid AND id=$2::uuid").bind(&api.ws).bind(invoice).fetch_one(&pool).await.unwrap();
    assert_eq!(stored["billing"]["city"], "Changed town");
    assert_eq!(stored["total"], 2500);
}

#[sqlx::test]
async fn order_invoice_default_variant_drift_refuses_and_explicit_no_warehouse_stays_unset(
    pool: PgPool,
) {
    let (api, order, _, product) = conversion_world(pool.clone()).await;
    let path = format!("/v1/workspace/{}/orders/{order}/invoice", api.ws);
    let (session, action, _) = approved(&api, "orders_invoice", &path, json!({})).await;
    let (status, created) = send(
        &api.app,
        Method::POST,
        &format!("/v1/workspace/{}/products/{product}/variants", api.ws),
        Some(json!({"price":"99","is_default":true,"sku":"New conversion default"})),
        Some(&api.owner),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let variant = created["data"]["variant_id"].as_str().unwrap();
    let (status, body) = dispatch(&api.app, &path, json!({}), &api.owner, &session, &action).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    no_invoice(&pool, &api, &order).await;
    let (session, action, review) = approved(&api, "orders_invoice", &path, json!({})).await;
    assert_eq!(
        review["review_facts"]["facts"]["priced"]["lines_preview"][0]["variant_id"],
        variant
    );
    let warehouse = api
        .with_base("/warehouses")
        .create(
            json!({"name":"Unconsumed conversion default"}),
            "warehouse_id",
        )
        .await;
    sqlx::query("UPDATE products SET default_warehouse_id=$3::uuid WHERE workspace_id=$1::uuid AND id=$2::uuid").bind(&api.ws).bind(&product).bind(&warehouse).execute(&pool).await.unwrap();
    let (status, body) = dispatch(&api.app, &path, json!({}), &api.owner, &session, &action).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let invoice = body["data"]["invoice_id"].as_str().unwrap();
    let stored:Value=sqlx::query_scalar("SELECT jsonb_build_object('variant',product_variant_id,'warehouse',mutate_from_warehouse_id,'price',trim_scale(price)::text) FROM items WHERE workspace_id=$1::uuid AND invoice_id=$2::uuid").bind(&api.ws).bind(invoice).fetch_one(&pool).await.unwrap();
    assert_eq!(stored["variant"], variant);
    assert!(stored["warehouse"].is_null());
    assert_eq!(
        stored["price"], "12.5",
        "conversion keeps reviewed order price"
    );
}

#[sqlx::test]
async fn order_invoice_legacy_locked_only_review_refuses_before_insert(pool: PgPool) {
    let (api, order, _, _) = conversion_world(pool.clone()).await;
    let path = format!("/v1/workspace/{}/orders/{order}/invoice", api.ws);
    let (session, action, _) = approved(&api, "orders_invoice", &path, json!({})).await;
    sqlx::query("UPDATE assistant_actions SET review_facts=review_facts #- '{facts,priced}' WHERE workspace_id=$1::uuid AND id=$2::uuid").bind(&api.ws).bind(&action).execute(&pool).await.unwrap();
    let (status, body) = dispatch(&api.app, &path, json!({}), &api.owner, &session, &action).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    no_invoice(&pool, &api, &order).await;
}

#[sqlx::test]
async fn order_invoice_writer_without_invoice_view_keeps_derived_facts_private(pool: PgPool) {
    let (api, order, _, _) = conversion_world(pool.clone()).await;
    let token = crate::common::member_with(
        &api.app,
        &pool,
        &api.ws,
        "order-conversion-writer@example.com",
        &["order.update", "order.view"],
    )
    .await;
    let mut writer = api.with_base("/orders");
    writer.owner = token;
    let path = format!("/v1/workspace/{}/orders/{order}/invoice", api.ws);
    let (session, action, review) = approved(&writer, "orders_invoice", &path, json!({})).await;
    assert_eq!(
        review["review_facts"],
        json!({"factsHiddenByPermission":true})
    );
    let private: Value = sqlx::query_scalar(
        "SELECT review_facts FROM assistant_actions WHERE workspace_id=$1::uuid AND id=$2::uuid",
    )
    .bind(&api.ws)
    .bind(&action)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        private["facts"]["priced"]["billing_address"]["city"],
        "Reviewed town"
    );
    let (status, history) = send(
        &writer.app,
        Method::GET,
        &format!(
            "/v1/workspace/{}/assistant/sessions/{session}/actions",
            api.ws
        ),
        None,
        Some(&writer.owner),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{history}");
    assert_eq!(
        history["data"][0]["review_facts"],
        json!({"factsHiddenByPermission":true})
    );
    let (status, body) = dispatch(
        &writer.app,
        &path,
        json!({}),
        &writer.owner,
        &session,
        &action,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["data"]["invoice_id"].is_string());
}
