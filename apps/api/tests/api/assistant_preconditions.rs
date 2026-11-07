//! Assistant reviews bind material order prices and absolute stock before-values.

use crate::common::{self, Api, send, send_raw};
use axum::{
    Router,
    http::{Method, StatusCode},
};
use hmac::{Hmac, KeyInit, Mac};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::PgPool;

pub(super) const BRIDGE: &str = "assistant-preconditions-test-bridge-at-least-thirty-two-bytes";

pub(super) async fn world(pool: PgPool) -> (Api, String, String) {
    let mut api = Api::new(pool.clone(), "/orders").await;
    let mut state = common::state(pool, genesy_360_api::mail::Mailer::capture().0);
    state.assistant_bridge_secret = Some(std::sync::Arc::new(
        genesy_360_api::AssistantSecret::new(BRIDGE.into()).unwrap(),
    ));
    api.app = genesy_360_api::build_app(state);
    let warehouse = api
        .with_base("/warehouses")
        .create(
            json!({"name":"Main","default_warehouse":true}),
            "warehouse_id",
        )
        .await;
    let product = api
        .with_base("/products")
        .create(
            json!({"name":"Reviewed honey","price":"12.50","has_stock":true}),
            "product_id",
        )
        .await;
    (api, warehouse, product)
}

pub(super) async fn approved(
    api: &Api,
    operation: &str,
    path: &str,
    body: Value,
) -> (String, String, Value) {
    approved_method(api, operation, path, body, Method::POST).await
}

pub(super) async fn approved_method(
    api: &Api,
    operation: &str,
    path: &str,
    body: Value,
    method: Method,
) -> (String, String, Value) {
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
    let session = session["data"]["id"].as_str().unwrap().to_owned();
    let (status, proposed) = send(&api.app, Method::POST, &format!("{base}/actions"), Some(json!({"session_id":session,"operation_id":operation,"method":method.as_str(),"path":path,"body":body})), Some(&api.owner)).await;
    assert_eq!(status, StatusCode::CREATED, "{proposed}");
    let action = proposed["data"]["id"].as_str().unwrap().to_owned();
    let uri = format!("{base}/actions/{action}/approve");
    let approval = json!({"session_id":session});
    let timestamp = chrono::Utc::now().timestamp().to_string();
    let text = format!(
        "{timestamp}\nPOST\n{uri}\n{}",
        hex::encode(Sha256::digest(approval.to_string().as_bytes()))
    );
    let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(BRIDGE.as_bytes()).unwrap();
    mac.update(text.as_bytes());
    let signature = hex::encode(mac.finalize().into_bytes());
    let (status, response) = send_raw(
        &api.app,
        Method::POST,
        &uri,
        Some(approval),
        Some(&api.owner),
        &[
            ("Assistant-Bridge-Timestamp", &timestamp),
            ("Assistant-Bridge-Signature", &signature),
        ],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let response: Value = serde_json::from_slice(&response).unwrap();
    assert_eq!(
        response["data"]["review_facts"],
        proposed["data"]["review_facts"]
    );
    (session, action, proposed["data"].clone())
}

pub(super) async fn dispatch(
    app: &Router,
    path: &str,
    body: Value,
    token: &str,
    session: &str,
    action: &str,
) -> (StatusCode, Value) {
    dispatch_method(app, path, body, token, session, action, Method::POST).await
}

pub(super) async fn dispatch_method(
    app: &Router,
    path: &str,
    body: Value,
    token: &str,
    session: &str,
    action: &str,
    method: Method,
) -> (StatusCode, Value) {
    let (status, body) = send_raw(
        app,
        method,
        path,
        Some(body),
        Some(token),
        &[
            ("Assistant-Action-Id", action),
            ("Assistant-Session-Id", session),
        ],
    )
    .await;
    (status, serde_json::from_slice(&body).unwrap())
}

#[sqlx::test]
async fn order_catalog_price_changed_after_review_is_conflict_without_order(pool: PgPool) {
    let (api, warehouse, product) = world(pool.clone()).await;
    let path = format!("/v1/workspace/{}/orders", api.ws);
    let input = json!({"lines":[{"product_id":product,"quantity":"2"}]});
    let (session, action, review) = approved(&api, "orders_create", &path, input.clone()).await;
    sqlx::query("UPDATE products SET price=20 WHERE workspace_id=$1::uuid AND id=$2::uuid")
        .bind(&api.ws)
        .bind(&product)
        .execute(&pool)
        .await
        .unwrap();
    let (status, body) = dispatch(&api.app, &path, input, &api.owner, &session, &action).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(
        review["review_facts"]["facts"]["priced"]["warehouse"]["id"],
        warehouse
    );
    assert_eq!(
        review["review_facts"]["facts"]["priced"]["total_minor"],
        2500
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM orders WHERE workspace_id=$1::uuid")
        .bind(&api.ws)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    let fresh = json!({"lines":[{"product_id":product,"quantity":"2"}]});
    let (session, action, review) = approved(&api, "orders_create", &path, fresh.clone()).await;
    assert_eq!(
        review["review_facts"]["facts"]["priced"]["total_minor"],
        4000
    );
    let (status, body) = dispatch(&api.app, &path, fresh, &api.owner, &session, &action).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
}

#[sqlx::test]
async fn stock_set_changed_after_review_is_conflict_without_second_movement(pool: PgPool) {
    let (api, warehouse, product) = world(pool.clone()).await;
    let path = format!(
        "/v1/workspace/{}/products/{product}/set-stock-levels",
        api.ws
    );
    let input =
        json!({"warehouses":[{"warehouse_id":warehouse,"stock_lvl":"10","reason":"Counted"}]});
    let (session, action, _) = approved(&api, "stock_set_levels", &path, input.clone()).await;
    let (status, body) = send(&api.app,Method::POST,&path,Some(json!({"warehouses":[{"warehouse_id":warehouse,"stock_lvl":"4","reason":"Other count"}]})),Some(&api.owner)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = dispatch(&api.app, &path, input, &api.owner, &session, &action).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM stock_movements WHERE workspace_id=$1::uuid AND product_id=$2::uuid",
    )
    .bind(&api.ws)
    .bind(&product)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1);
    let level: String = sqlx::query_scalar("SELECT trim_scale(stock_lvl)::text FROM stocks WHERE workspace_id=$1::uuid AND product_id=$2::uuid AND warehouse_id=$3::uuid AND bin_id IS NULL AND lot_id IS NULL AND deleted_at IS NULL").bind(&api.ws).bind(&product).bind(&warehouse).fetch_one(&pool).await.unwrap();
    assert_eq!(level, "4");
    let fresh =
        json!({"warehouses":[{"warehouse_id":warehouse,"stock_lvl":"10","reason":"Recounted"}]});
    let (session, action, review) = approved(&api, "stock_set_levels", &path, fresh.clone()).await;
    assert_eq!(
        review["review_facts"]["facts"]["locked"]["slots"][0]["before"],
        "4"
    );
    let (status, body) = dispatch(&api.app, &path, fresh, &api.owner, &session, &action).await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

#[sqlx::test]
async fn order_changed_after_review_is_conflict_under_order_lock(pool: PgPool) {
    let (api, _, product) = world(pool.clone()).await;
    let id = api
        .create(
            json!({"lines":[{"product_id":product,"quantity":"1"}]}),
            "order_id",
        )
        .await;
    let path = format!("/v1/workspace/{}/orders/{id}/cancel", api.ws);
    let (session, action, review) = approved(&api, "orders_cancel", &path, json!({})).await;
    assert_eq!(review["review_facts"]["facts"]["locked"]["id"], id);
    let (status, body) = api
        .call(
            Method::PATCH,
            &format!("/{id}"),
            Some(json!({"note":"Changed after review"})),
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = dispatch(&api.app, &path, json!({}), &api.owner, &session, &action).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(api.get(&id).await["status"], "draft");
    let (session, action, _) = approved(&api, "orders_cancel", &path, json!({})).await;
    let (status, body) = dispatch(&api.app, &path, json!({}), &api.owner, &session, &action).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(api.get(&id).await["status"], "cancelled");
}

#[sqlx::test]
async fn material_facts_do_not_reveal_another_workspace_or_ungranted_business_data(pool: PgPool) {
    let (api, _, product) = world(pool.clone()).await;
    let foreign = api.as_new_owner("eve@example.com", "foreign").await;
    let foreign_product = foreign
        .with_base("/products")
        .create(
            json!({"name":"Private foreign honey","price":"88"}),
            "product_id",
        )
        .await;
    let path = format!("/v1/workspace/{}/orders", api.ws);
    let propose = format!("/v1/workspace/{}/assistant/actions", api.ws);
    let (_, session) = send(
        &api.app,
        Method::POST,
        &format!("/v1/workspace/{}/assistant/sessions", api.ws),
        Some(json!({"language":"en"})),
        Some(&api.owner),
    )
    .await;
    let (status,body) = send(&api.app,Method::POST,&propose,Some(json!({"session_id":session["data"]["id"],"operation_id":"orders_create","method":"POST","path":path,"body":{"lines":[{"product_id":foreign_product,"quantity":"1"}]}})),Some(&api.owner)).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(!body.to_string().contains("Private foreign honey"));
    let member = common::member_with(&api.app, &pool, &api.ws, "member@example.com", &[]).await;
    let (_, session) = send(
        &api.app,
        Method::POST,
        &format!("/v1/workspace/{}/assistant/sessions", api.ws),
        Some(json!({"language":"en"})),
        Some(&member),
    )
    .await;
    let (status,body) = send(&api.app,Method::POST,&propose,Some(json!({"session_id":session["data"]["id"],"operation_id":"orders_create","method":"POST","path":path,"body":{"lines":[{"product_id":product,"quantity":"1"}]}})),Some(&member)).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert!(!body.to_string().contains("Reviewed honey"));
}

#[sqlx::test]
async fn write_only_role_keeps_native_creation_but_all_public_reviews_hide_derived_facts(
    pool: PgPool,
) {
    let (api, warehouse, product) = world(pool.clone()).await;
    let member = common::member_with(
        &api.app,
        &pool,
        &api.ws,
        "writer@example.com",
        &["order.create", "stock.set_stock_levels"],
    )
    .await;
    let mut writer = api.with_base("/orders");
    writer.owner = member;
    let path = format!("/v1/workspace/{}/orders", api.ws);
    let input = json!({"lines":[{"product_id":product,"quantity":"2"}]});
    let (session, action, proposed) =
        approved(&writer, "orders_create", &path, input.clone()).await;
    assert_eq!(proposed["review_facts"]["factsHiddenByPermission"], true);
    let stored: Value = sqlx::query_scalar(
        "SELECT review_facts FROM assistant_actions WHERE workspace_id=$1::uuid AND id=$2::uuid",
    )
    .bind(&api.ws)
    .bind(&action)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(stored["facts"]["priced"]["total_minor"], 2500);
    let base = format!("/v1/workspace/{}/assistant", api.ws);
    for uri in [
        format!("{base}/actions/{action}"),
        format!("{base}/sessions/{session}/actions"),
    ] {
        let (status, body) = send(&api.app, Method::GET, &uri, None, Some(&writer.owner)).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert!(!body.to_string().contains("total_minor"), "{body}");
        assert!(!body.to_string().contains("Reviewed honey"), "{body}");
    }
    let (status, body) =
        dispatch(&writer.app, &path, input, &writer.owner, &session, &action).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let path = format!(
        "/v1/workspace/{}/products/{product}/set-stock-levels",
        api.ws
    );
    let input =
        json!({"warehouses":[{"warehouse_id":warehouse,"stock_lvl":"10","reason":"Counted"}]});
    let (session, action, proposed) =
        approved(&writer, "stock_set_levels", &path, input.clone()).await;
    assert_eq!(proposed["review_facts"]["factsHiddenByPermission"], true);
    for uri in [
        format!("{base}/actions/{action}"),
        format!("{base}/sessions/{session}/actions"),
    ] {
        let (status, body) = send(&api.app, Method::GET, &uri, None, Some(&writer.owner)).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert!(!body.to_string().contains("\"before\""), "{body}");
        assert!(!body.to_string().contains("\"reserved\""), "{body}");
    }
    let (status, body) =
        dispatch(&writer.app, &path, input, &writer.owner, &session, &action).await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

#[sqlx::test]
async fn product_default_currency_and_vat_drift_rejects_before_insert(pool: PgPool) {
    let (api, _, _) = world(pool.clone()).await;
    let path = format!("/v1/workspace/{}/products", api.ws);
    let body = json!({"name":"Exact fractional product","price":"743.0578","gross_price":"0","wholesale_price":"800.12345678"});
    let (session, action, _) = approved(&api, "products_create", &path, body.clone()).await;
    api.with_base("/currencies").create(json!({"name":"Reviewed USD","asset_type":"usd","decimals":2,"active":true,"default_currency":true}),"currency_id").await;
    let (status, response) =
        dispatch(&api.app, &path, body.clone(), &api.owner, &session, &action).await;
    assert_eq!(status, StatusCode::CONFLICT, "{response}");
    let count:i64=sqlx::query_scalar("SELECT count(*) FROM products WHERE workspace_id=$1::uuid AND name='Exact fractional product'").bind(&api.ws).fetch_one(&pool).await.unwrap();
    assert_eq!(count, 0);
    let (session, action, _) = approved(&api, "products_create", &path, body.clone()).await;
    api.with_base("/settings/vat-groups")
        .create(
            json!({"name":"Reviewed VAT","vat_amount":"19.5","default_group":true}),
            "vat_group_id",
        )
        .await;
    assert_eq!(
        dispatch(&api.app, &path, body.clone(), &api.owner, &session, &action)
            .await
            .0,
        StatusCode::CONFLICT
    );
    let (session, action, review) = approved(&api, "products_create", &path, body.clone()).await;
    assert_eq!(
        review["review_facts"]["facts"]["priced"]["price"],
        "743.0578"
    );
    assert_eq!(
        dispatch(&api.app, &path, body, &api.owner, &session, &action)
            .await
            .0,
        StatusCode::CREATED
    );
}

#[sqlx::test]
async fn product_price_patch_and_variant_parent_currency_drift_are_stale(pool: PgPool) {
    let (api, _, product) = world(pool.clone()).await;
    let other=api.with_base("/currencies").create(json!({"name":"Other USD","asset_type":"usd","decimals":2,"active":true,"default_currency":false}),"currency_id").await;
    let path = format!("/v1/workspace/{}/products/{product}", api.ws);
    let body = json!({"price":"743.0578"});
    let (session, action, _) = approved(&api, "products_update", &path, body.clone()).await;
    sqlx::query(
        "UPDATE products SET currency_id=$3::uuid WHERE workspace_id=$1::uuid AND id=$2::uuid",
    )
    .bind(&api.ws)
    .bind(&product)
    .bind(&other)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        dispatch(&api.app, &path, body, &api.owner, &session, &action)
            .await
            .0,
        StatusCode::CONFLICT
    );
    let price: String = sqlx::query_scalar(
        "SELECT price::text FROM products WHERE workspace_id=$1::uuid AND id=$2::uuid",
    )
    .bind(&api.ws)
    .bind(&product)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        price.parse::<rust_decimal::Decimal>().unwrap(),
        rust_decimal::Decimal::new(1250, 2)
    );
    let variantpath = format!("{path}/variants");
    let body = json!({"price":"33.2578","sku":"Reviewed variant"});
    let (session, action, _) =
        approved(&api, "product_variants_create", &variantpath, body.clone()).await;
    sqlx::query("UPDATE currencies SET decimals=3 WHERE workspace_id=$1::uuid AND id=$2::uuid")
        .bind(&api.ws)
        .bind(&other)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        dispatch(&api.app, &variantpath, body, &api.owner, &session, &action)
            .await
            .0,
        StatusCode::CONFLICT
    );
    let count:i64=sqlx::query_scalar("SELECT count(*) FROM product_variants WHERE workspace_id=$1::uuid AND product_id=$2::uuid AND sku='Reviewed variant'").bind(&api.ws).bind(&product).fetch_one(&pool).await.unwrap();
    assert_eq!(count, 0);
}

#[sqlx::test]
async fn invoice_create_default_currency_drift_is_stale_before_insert(pool: PgPool) {
    let (api, _, _) = world(pool.clone()).await;
    let relation = api
        .with_base("/relations")
        .create(
            json!({"company_name":"Reviewed buyer","type":"business"}),
            "relation_id",
        )
        .await;
    let path = format!("/v1/workspace/{}/invoices", api.ws);
    let body = json!({"relation_id":relation,"items":[{"title":"Exact line","quantity":"1","price":"1.25","vat":"0"}]});
    let (session, action, _) = approved(&api, "invoices_create", &path, body.clone()).await;
    api.with_base("/currencies").create(json!({"name":"Later USD","asset_type":"usd","decimals":2,"active":true,"default_currency":true}),"currency_id").await;
    let (status, response) = dispatch(&api.app, &path, body, &api.owner, &session, &action).await;
    assert_eq!(status, StatusCode::CONFLICT, "{response}");
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM invoices WHERE workspace_id=$1::uuid")
            .bind(&api.ws)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 0);
}
