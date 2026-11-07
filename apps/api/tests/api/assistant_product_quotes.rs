//! Native product, variant and invoice defaults stay bound to independent assistant approval.

use super::assistant_preconditions::{approved, approved_method, dispatch, dispatch_method, world};
use crate::common::{self, send};
use axum::http::{Method, StatusCode};
use serde_json::{Value, json};
use sqlx::PgPool;

#[sqlx::test]
async fn duplicate_and_variant_generation_reject_inherited_price_drift(pool: PgPool) {
    let (api, _, product) = world(pool.clone()).await;
    let product_path = format!("/v1/workspace/{}/products/{product}", api.ws);
    let path = format!("{product_path}/duplicate");
    let (session, action, _) = approved(&api, "products_duplicate", &path, json!({})).await;
    sqlx::query("UPDATE products SET price=25 WHERE workspace_id=$1::uuid AND id=$2::uuid")
        .bind(&api.ws)
        .bind(&product)
        .execute(&pool)
        .await
        .unwrap();
    let (status, body) = dispatch(&api.app, &path, json!({}), &api.owner, &session, &action).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM products WHERE workspace_id=$1::uuid")
            .bind(&api.ws)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 1);
    let path = format!("{product_path}/variants/generate");
    let body = json!({"option_types":[{"name":"Size","values":[{"value":"Small"}]}],"combinations":[{"option_value_ids":["Small"]}]});
    let (session, action, _) =
        approved(&api, "product_variants_generate", &path, body.clone()).await;
    sqlx::query("UPDATE products SET price=30 WHERE workspace_id=$1::uuid AND id=$2::uuid")
        .bind(&api.ws)
        .bind(&product)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        dispatch(&api.app, &path, body.clone(), &api.owner, &session, &action)
            .await
            .0,
        StatusCode::CONFLICT
    );
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM product_option_types WHERE workspace_id=$1::uuid")
            .bind(&api.ws)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 0, "no option write precedes stale review rejection");
    let (session, action, _) =
        approved(&api, "product_variants_generate", &path, body.clone()).await;
    let (status, created) = dispatch(&api.app, &path, body, &api.owner, &session, &action).await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    assert_eq!(created["data"].as_array().unwrap().len(), 1);
}

#[sqlx::test]
async fn variant_update_rejects_changed_parent_currency_before_price_write(pool: PgPool) {
    let (api, _, product) = world(pool.clone()).await;
    let path = format!("/v1/workspace/{}/products/{product}/variants", api.ws);
    let (status, created) = send(
        &api.app,
        Method::POST,
        &path,
        Some(json!({"price":"33.25","sku":"Reviewed size"})),
        Some(&api.owner),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let variant = created["data"]["variant_id"].as_str().unwrap();
    let path = format!("{path}/{variant}");
    let body = json!({"price":"44.25"});
    let (session, action, _) = approved_method(
        &api,
        "product_variants_update",
        &path,
        body.clone(),
        Method::PUT,
    )
    .await;
    sqlx::query("UPDATE currencies SET decimals=3 WHERE workspace_id=$1::uuid AND id=(SELECT currency_id FROM products WHERE workspace_id=$1::uuid AND id=$2::uuid)").bind(&api.ws).bind(&product).execute(&pool).await.unwrap();
    assert_eq!(
        dispatch_method(
            &api.app,
            &path,
            body,
            &api.owner,
            &session,
            &action,
            Method::PUT
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let price: String = sqlx::query_scalar(
        "SELECT price::text FROM product_variants WHERE workspace_id=$1::uuid AND id=$2::uuid",
    )
    .bind(&api.ws)
    .bind(variant)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        price.parse::<rust_decimal::Decimal>().unwrap(),
        rust_decimal::Decimal::new(3325, 2)
    );
}

#[sqlx::test]
async fn write_only_product_creator_keeps_native_execution_without_public_price_disclosure(
    pool: PgPool,
) {
    let (api, _, _) = world(pool.clone()).await;
    let member = common::member_with(
        &api.app,
        &pool,
        &api.ws,
        "product-writer@example.com",
        &["product.create"],
    )
    .await;
    let mut writer = api.with_base("/products");
    writer.owner = member;
    let path = format!("/v1/workspace/{}/products", api.ws);
    let body = json!({"name":"Private fractional quote","price":"743.0578","gross_price":"0","wholesale_price":"800.12345678"});
    let (session, action, review) = approved(&writer, "products_create", &path, body.clone()).await;
    assert_eq!(review["review_facts"]["factsHiddenByPermission"], true);
    let stored: Value = sqlx::query_scalar(
        "SELECT review_facts FROM assistant_actions WHERE workspace_id=$1::uuid AND id=$2::uuid",
    )
    .bind(&api.ws)
    .bind(&action)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(stored["facts"]["priced"]["price"], "743.0578");
    assert_eq!(stored["facts"]["priced"]["wholesale_price"], "800.12345678");
    let (status, history) = send(
        &api.app,
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
    for row in history["data"].as_array().unwrap() {
        assert_eq!(row["review_facts"], json!({"factsHiddenByPermission":true}));
        assert!(!row["review_facts"].to_string().contains("743.0578"));
    }
    assert_eq!(
        dispatch(&writer.app, &path, body, &writer.owner, &session, &action)
            .await
            .0,
        StatusCode::CREATED
    );
}

#[sqlx::test]
async fn product_quote_checks_write_permission_before_foreign_target_lookup(pool: PgPool) {
    let (api, _, _) = world(pool.clone()).await;
    let other = api
        .as_new_owner("other-quote@example.com", "other-quote")
        .await;
    let foreign = other
        .with_base("/products")
        .create(
            json!({"name":"Foreign private quote","price":"999.23"}),
            "product_id",
        )
        .await;
    for (index, permissions, expected) in [
        (0, Vec::new(), StatusCode::FORBIDDEN),
        (1, vec!["product.create"], StatusCode::NOT_FOUND),
    ] {
        let member = common::member_with(
            &api.app,
            &pool,
            &api.ws,
            &format!("quote-member-{index}@example.com"),
            &permissions,
        )
        .await;
        let base = format!("/v1/workspace/{}/assistant", api.ws);
        let (status, session) = send(
            &api.app,
            Method::POST,
            &format!("{base}/sessions"),
            Some(json!({"language":"en"})),
            Some(&member),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{session}");
        let (status,response)=send(&api.app,Method::POST,&format!("{base}/actions"),Some(json!({"session_id":session["data"]["id"],"operation_id":"product_variants_create","method":"POST","path":format!("/v1/workspace/{}/products/{foreign}/variants",api.ws),"body":{"price":"1"}})),Some(&member)).await;
        assert_eq!(status, expected, "{response}");
        assert!(!response.to_string().contains("Foreign private quote"));
        assert!(!response.to_string().contains("999.23"));
    }
}

#[sqlx::test]
async fn invoice_create_binds_default_billing_and_selected_contact(pool: PgPool) {
    let (api, _, product) = world(pool.clone()).await;
    let relations = api.with_base("/relations");
    let relation=relations.create(json!({"company_name":"Reviewed invoice buyer","type":"business","addresses":[{"address":"Main","house_number":"1","zipcode":"1000AA","city":"Amsterdam","country":"NL","type":"billing"}]}),"relation_id").await;
    let (status, contact) = relations
        .call(
            Method::POST,
            &format!("/{relation}/contacts"),
            Some(
                json!({"first_name":"Reviewed","last_name":"Buyer","email":"reviewed@example.com"}),
            ),
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{contact}");
    let contact = contact["data"]["contact_id"].as_str().unwrap();
    let path = format!("/v1/workspace/{}/invoices", api.ws);
    let body = json!({"relation_id":relation,"addressed_contact_id":contact,"items":[{"product_id":product,"title":"Line","quantity":"1","price":"1.25","vat":"0"}]});
    let (session, action, _) = approved(&api, "invoices_create", &path, body.clone()).await;
    sqlx::query("UPDATE contacts SET email='changed@example.com' WHERE workspace_id=$1::uuid AND id=$2::uuid").bind(&api.ws).bind(contact).execute(&pool).await.unwrap();
    assert_eq!(
        dispatch(&api.app, &path, body.clone(), &api.owner, &session, &action)
            .await
            .0,
        StatusCode::CONFLICT
    );
    let (session, action, _) = approved(&api, "invoices_create", &path, body.clone()).await;
    sqlx::query("UPDATE relations SET addresses=$3 WHERE workspace_id=$1::uuid AND id=$2::uuid")
        .bind(&api.ws)
        .bind(&relation)
        .bind(json!([{"city":"Utrecht","country":"NL","type":"billing"}]))
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        dispatch(&api.app, &path, body.clone(), &api.owner, &session, &action)
            .await
            .0,
        StatusCode::CONFLICT
    );
    let (session, action, review) = approved(&api, "invoices_create", &path, body.clone()).await;
    assert_eq!(
        review["review_facts"]["facts"]["priced"]["billing_address"]["city"],
        "Utrecht"
    );
    assert_eq!(
        review["review_facts"]["facts"]["priced"]["currency"]["code"],
        "EUR"
    );
    assert!(
        review["review_facts"]["facts"]["priced"]
            .get("material_sha256")
            .is_none()
    );
    assert_eq!(
        dispatch(&api.app, &path, body, &api.owner, &session, &action)
            .await
            .0,
        StatusCode::CREATED
    );
}

async fn invoice_patch_world(pool: PgPool) -> (common::Api, String, String, String) {
    let (api, _, product) = world(pool).await;
    let relation = api
        .with_base("/relations")
        .create(
            json!({"company_name":"Patch buyer","type":"business"}),
            "relation_id",
        )
        .await;
    let invoice = api.with_base("/invoices").create(json!({"relation_id":relation,"items":[{"title":"Existing reviewed line","quantity":"1","price":"1.25","vat":"0"}]}),"invoice_id").await;
    (api, invoice, relation, product)
}

#[sqlx::test]
async fn invoice_patch_binds_newly_selected_contact_before_any_header_write(pool: PgPool) {
    let (api, invoice, relation, _) = invoice_patch_world(pool.clone()).await;
    let (status,contact)=api.with_base("/relations").call(Method::POST,&format!("/{relation}/contacts"),Some(json!({"first_name":"Selected","last_name":"Buyer","email":"approved-patch@example.com"})),&api.owner).await;
    assert_eq!(status, StatusCode::CREATED, "{contact}");
    let contact = contact["data"]["contact_id"].as_str().unwrap();
    let path = format!("/v1/workspace/{}/invoices/{invoice}", api.ws);
    let body = json!({"relation_id":relation,"addressed_contact_id":contact,"reference":"Reviewed contact patch"});
    let (session, action, _) = approved(&api, "invoices_update", &path, body.clone()).await;
    sqlx::query("UPDATE contacts SET email='changed-patch@example.com' WHERE workspace_id=$1::uuid AND id=$2::uuid").bind(&api.ws).bind(contact).execute(&pool).await.unwrap();
    let (status, response) =
        dispatch(&api.app, &path, body.clone(), &api.owner, &session, &action).await;
    assert_eq!(status, StatusCode::CONFLICT, "{response}");
    let stored:Value=sqlx::query_scalar("SELECT jsonb_build_object('reference',reference,'contact',addressed_contact) FROM invoices WHERE workspace_id=$1::uuid AND id=$2::uuid").bind(&api.ws).bind(&invoice).fetch_one(&pool).await.unwrap();
    assert!(stored["reference"].is_null());
    assert!(stored["contact"].is_null());
    let (session, action, review) = approved(&api, "invoices_update", &path, body.clone()).await;
    assert_eq!(
        review["review_facts"]["facts"]["priced"]["addressed_contact"]["email"],
        "changed-patch@example.com"
    );
    assert_eq!(
        dispatch(&api.app, &path, body, &api.owner, &session, &action)
            .await
            .0,
        StatusCode::OK
    );
    let stored: Value = sqlx::query_scalar(
        "SELECT addressed_contact FROM invoices WHERE workspace_id=$1::uuid AND id=$2::uuid",
    )
    .bind(&api.ws)
    .bind(&invoice)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(stored["email"], "changed-patch@example.com");
}

#[sqlx::test]
async fn invoice_patch_binds_replacement_relation_billing_without_changing_currency_contract(
    pool: PgPool,
) {
    let (api, invoice, _, _) = invoice_patch_world(pool.clone()).await;
    let replacement=api.with_base("/relations").create(json!({"company_name":"Replacement buyer","type":"business","addresses":[{"address":"Main","house_number":"1","zipcode":"1000AA","city":"Reviewed town","country":"NL","type":"billing"}]}),"relation_id").await;
    let ignored_currency=api.with_base("/currencies").create(json!({"name":"Ignored USD patch","asset_type":"usd","decimals":2,"active":true,"default_currency":false}),"currency_id").await;
    let path = format!("/v1/workspace/{}/invoices/{invoice}", api.ws);
    let body = json!({"relation_id":replacement,"currency_id":ignored_currency,"reference":"Reviewed billing patch"});
    let (session, action, _) = approved(&api, "invoices_update", &path, body.clone()).await;
    sqlx::query("UPDATE relations SET addresses=$3 WHERE workspace_id=$1::uuid AND id=$2::uuid")
        .bind(&api.ws)
        .bind(&replacement)
        .bind(json!([{"address":"Main","house_number":"1","zipcode":"1000AA","city":"Changed town","country":"NL","type":"billing"}]))
        .execute(&pool)
        .await
        .unwrap();
    let (status, response) =
        dispatch(&api.app, &path, body.clone(), &api.owner, &session, &action).await;
    assert_eq!(status, StatusCode::CONFLICT, "{response}");
    let reference: Option<String> = sqlx::query_scalar(
        "SELECT reference FROM invoices WHERE workspace_id=$1::uuid AND id=$2::uuid",
    )
    .bind(&api.ws)
    .bind(&invoice)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(reference.is_none());
    let (session, action, review) = approved(&api, "invoices_update", &path, body.clone()).await;
    assert_eq!(
        review["review_facts"]["facts"]["priced"]["billing_address"]["city"],
        "Changed town"
    );
    assert_eq!(
        dispatch(&api.app, &path, body, &api.owner, &session, &action)
            .await
            .0,
        StatusCode::OK
    );
    let stored:Value=sqlx::query_scalar("SELECT jsonb_build_object('currency',currency_id,'billing',billing_address) FROM invoices WHERE workspace_id=$1::uuid AND id=$2::uuid").bind(&api.ws).bind(&invoice).fetch_one(&pool).await.unwrap();
    assert_ne!(stored["currency"], ignored_currency);
    assert_eq!(stored["billing"]["city"], "Changed town");
}

#[sqlx::test]
async fn invoice_patch_binds_new_line_default_variant_and_warehouse_before_insert(pool: PgPool) {
    let (api, invoice, relation, product) = invoice_patch_world(pool.clone()).await;
    let path = format!("/v1/workspace/{}/invoices/{invoice}", api.ws);
    let body = json!({"relation_id":relation,"reference":"Reviewed new line","items":[{"product_id":product,"title":"New reviewed line","quantity":"1","price":"2.25","vat":"0"}]});
    let (session, action, _) = approved(&api, "invoices_update", &path, body.clone()).await;
    let warehouse = api
        .with_base("/warehouses")
        .create(json!({"name":"Changed patch warehouse"}), "warehouse_id")
        .await;
    let variants = format!("/v1/workspace/{}/products/{product}/variants", api.ws);
    let (status, created) = send(
        &api.app,
        Method::POST,
        &variants,
        Some(json!({"price":"12.50","is_default":true,"sku":"Changed patch default"})),
        Some(&api.owner),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let variant = created["data"]["variant_id"].as_str().unwrap();
    sqlx::query("UPDATE products SET default_warehouse_id=$3::uuid WHERE workspace_id=$1::uuid AND id=$2::uuid").bind(&api.ws).bind(&product).bind(&warehouse).execute(&pool).await.unwrap();
    let (status, response) =
        dispatch(&api.app, &path, body.clone(), &api.owner, &session, &action).await;
    assert_eq!(status, StatusCode::CONFLICT, "{response}");
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM items WHERE workspace_id=$1::uuid AND invoice_id=$2::uuid",
    )
    .bind(&api.ws)
    .bind(&invoice)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1);
    let (session, action, review) = approved(&api, "invoices_update", &path, body.clone()).await;
    let preview = review["review_facts"]["facts"]["priced"]["lines_preview"]
        .as_array()
        .unwrap();
    assert!(
        preview
            .iter()
            .any(|line| line["variant_id"] == variant && line["warehouse_id"] == warehouse)
    );
    assert_eq!(
        dispatch(&api.app, &path, body, &api.owner, &session, &action)
            .await
            .0,
        StatusCode::OK
    );
    let stored:Value=sqlx::query_scalar("SELECT jsonb_build_object('variant',product_variant_id,'warehouse',mutate_from_warehouse_id) FROM items WHERE workspace_id=$1::uuid AND invoice_id=$2::uuid AND title='New reviewed line'").bind(&api.ws).bind(&invoice).fetch_one(&pool).await.unwrap();
    assert_eq!(stored["variant"], variant);
    assert_eq!(stored["warehouse"], warehouse);
}

#[sqlx::test]
async fn invoice_patch_old_pending_review_without_priced_stage_is_refused_before_write(
    pool: PgPool,
) {
    let (api, invoice, relation, _) = invoice_patch_world(pool.clone()).await;
    let path = format!("/v1/workspace/{}/invoices/{invoice}", api.ws);
    let body = json!({"relation_id":relation,"reference":"Missing staged quote"});
    let (session, action, _) = approved(&api, "invoices_update", &path, body.clone()).await;
    sqlx::query("UPDATE assistant_actions SET review_facts=review_facts #- '{facts,priced}' WHERE workspace_id=$1::uuid AND id=$2::uuid").bind(&api.ws).bind(&action).execute(&pool).await.unwrap();
    let (status, response) = dispatch(&api.app, &path, body, &api.owner, &session, &action).await;
    assert_eq!(status, StatusCode::CONFLICT, "{response}");
    let reference: Option<String> = sqlx::query_scalar(
        "SELECT reference FROM invoices WHERE workspace_id=$1::uuid AND id=$2::uuid",
    )
    .bind(&api.ws)
    .bind(&invoice)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(reference.is_none());
}
