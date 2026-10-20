//! Signup seeds Laravel WorkspaceSetup's reference rows (currencies, deal statuses, custom
//! types) in the signup transaction, with two fixes: hour types get the domain vimdesk-web
//! reads, and Supplier really is a supplier.

mod common;

use axum::{
    Router,
    http::{Method, StatusCode},
};
use common::{app, send, signup};
use serde_json::{Value, json};
use sqlx::PgPool;

async fn get(app: &Router, ws: &str, token: &str, path: &str) -> Value {
    let uri = format!("/v1/workspace/{ws}{path}");
    let (status, res) = send(app, Method::GET, &uri, None, Some(token)).await;
    assert_eq!(status, StatusCode::OK, "{path}: {res}");
    res
}

fn pick(res: &Value, keys: &[&str]) -> Vec<Value> {
    res["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| Value::Array(keys.iter().map(|k| row[*k].clone()).collect()))
        .collect()
}

#[sqlx::test]
async fn signup_seeds_currencies_with_euro_default(pool: PgPool) {
    let app = app(pool);
    let (token, ws) = signup(&app, "ada@example.com", "acme").await;

    let res = get(&app, &ws, &token, "/currencies").await;

    assert_eq!(
        pick(
            &res,
            &[
                "name",
                "asset_type",
                "symbol_prefix",
                "symbol_suffix",
                "decimals",
                "default_currency",
                "active"
            ]
        ),
        [
            json!(["Euro", "eur", "€", "EUR", 2, true, true]),
            json!(["UK Sterling", "gbp", "£", "GBP", 2, false, true]),
            json!(["Dollar", "usd", "$", "USD", 2, false, true]),
        ]
    );
}

#[sqlx::test]
async fn signup_seeds_deal_pipeline_statuses(pool: PgPool) {
    let app = app(pool);
    let (token, ws) = signup(&app, "ada@example.com", "acme").await;

    let res = get(
        &app,
        &ws,
        &token,
        "/custom-status?domain=deal_pipeline&limit=200",
    )
    .await;

    assert_eq!(
        pick(&res, &["name", "archived"]),
        [
            json!(["Won", false]),
            json!(["Lost", false]),
            json!(["In Progress", false]),
            json!(["To Do", false]),
            json!(["Completed", false]),
        ]
    );
    assert_eq!(res["meta"]["total"], 5);
}

#[sqlx::test]
async fn signup_seeds_custom_types(pool: PgPool) {
    let app = app(pool);
    let (token, ws) = signup(&app, "ada@example.com", "acme").await;

    let res = get(&app, &ws, &token, "/custom-types?page_size=100").await;

    assert_eq!(
        pick(&res, &["name", "domain", "properties"]),
        [
            json!(["Supplier", "relation", { "is_supplier": true }]),
            json!(["Client", "relation", { "is_supplier": false }]),
            json!(["Note", "activity", null]),
            json!(["Ticket", "activity", { "is_ticket_type": true }]),
            json!(["Website", "lead_source", null]),
            json!(["Social media", "lead_source", null]),
            json!(["Work", "hour_registration_type", null]),
            json!(["Leave", "hour_registration_type", { "is_leave_hours_type": true }]),
            json!(["Sick leave", "hour_registration_type", { "is_leave_hours_type": true }]),
        ]
    );
    // The hour-type filters vimdesk-web uses find the seeded rows.
    let leave = get(
        &app,
        &ws,
        &token,
        "/custom-types?domain=hour_registration_type&is_leave_hours_type=true",
    )
    .await;
    assert_eq!(leave["meta"]["total"], 2);
    let suppliers = get(
        &app,
        &ws,
        &token,
        "/custom-types?domain=relation&is_supplier=1",
    )
    .await;
    assert_eq!(pick(&suppliers, &["name"]), [json!(["Supplier"])]);
}

#[sqlx::test]
async fn every_workspace_gets_its_own_copy(pool: PgPool) {
    let app = app(pool);
    let (ada, acme) = signup(&app, "ada@example.com", "acme").await;
    let (eve, evil) = signup(&app, "eve@example.com", "evil").await;

    let mine = get(&app, &acme, &ada, "/currencies").await;
    let theirs = get(&app, &evil, &eve, "/currencies").await;

    assert_eq!(mine["meta"]["total"], 3);
    assert_eq!(theirs["meta"]["total"], 3);
    assert_ne!(mine["data"][0]["id"], theirs["data"][0]["id"]);
}
