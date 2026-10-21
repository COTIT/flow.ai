//! GET/POST /v1/workspace/{ws}/settings (Laravel WorkspaceSetting/Setting core).

mod common;

use axum::http::{Method, StatusCode};
use common::{Api, send, signup};
use serde_json::{Value, json};
use sqlx::PgPool;

const SECRET: &str = "Integration credentials can't be stored yet.";

async fn api(pool: PgPool) -> Api {
    Api::new(pool, "/settings").await
}

async fn get(api: &Api, query: &str) -> Value {
    let (status, res) = api.call(Method::GET, query, None, &api.owner).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    res["data"].clone()
}

async fn post(api: &Api, body: Value) -> (StatusCode, Value) {
    api.call(Method::POST, "", Some(body), &api.owner).await
}

async fn saved(api: &Api, body: Value) {
    let (status, res) = post(api, body).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res, json!({ "message": "ok" }));
}

/// First message for `field` in a 422 body.
async fn error(api: &Api, body: Value, field: &str) -> String {
    let (status, res) = post(api, body).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    res["errors"][field][0]
        .as_str()
        .unwrap_or_else(|| panic!("no error for {field}: {res}"))
        .to_owned()
}

async fn stored(api: &Api) -> Value {
    sqlx::query_scalar::<_, Value>("SELECT data FROM settings WHERE workspace_id = $1::uuid")
        .bind(&api.ws)
        .fetch_optional(&api.pool)
        .await
        .unwrap()
        .unwrap_or(Value::Null)
}

// --- happy ---------------------------------------------------------------------------------

#[sqlx::test]
async fn defaults_before_anything_is_saved(pool: PgPool) {
    let api = api(pool).await;
    let d = get(&api, "").await;
    assert_eq!(
        d["estimate"],
        json!({ "prefix": "EST-", "suffix": null, "start_number": 1,
                "default_validity_period": 14, "default_validity_period_company": 30,
                "default_footer_message": null, "default_note": null, "digits": 4 })
    );
    assert_eq!(
        d["invoice"]["customer_ref"],
        json!({ "ordered_by": true, "po_number": false, "reference": false })
    );
    assert_eq!(d["invoice"]["prefix"], Value::Null);
    assert_eq!(d["credit"]["prefix"], "CRE-");
    assert_eq!(d["collective_invoice"]["prefix"], "COL-INV-");
    assert_eq!(
        (&d["relation"]["prefix"], &d["relation"]["digits"]),
        (&json!("RE-"), &json!(4))
    );
    assert_eq!(
        (&d["product"]["prefix"], &d["product"]["digits"]),
        (&json!("PRO-"), &json!(5))
    );
    assert_eq!(
        (&d["order"]["prefix"], &d["purchase_order"]["digits"]),
        (&json!("ORD-"), &json!(5))
    );
    assert_eq!(d["work_order"]["prefix"], "WO-");
    assert_eq!(d["product"]["user_search_history_count"], 10);
    assert_eq!(d["general"]["language"], "en");
    assert_eq!(d["general"]["color"], "default");
    assert_eq!(d["incoming_invoice"]["required_approvals"], 1);
    assert_eq!(d["smtp"]["host"], Value::Null);
    assert_eq!(d["meta"], Value::Null);
    let cash = &d["cash_register"];
    assert_eq!(cash["receipt_line_length"], 34);
    assert_eq!(cash["cash_drawer_link_mode"], "flexible");
    assert_eq!(cash["allow_individual_lines"], true);
    assert_eq!(cash["logo"], Value::Null);
    assert_eq!(cash["accepted_cash"].as_array().unwrap().len(), 13);
    assert_eq!(cash["accepted_cash"][0], "0.05");
    let pay = &d["integrations"]["payment"]["payment_providers"]["pay"];
    assert_eq!(
        pay,
        &json!({ "api_code": null, "api_token": null, "service_id": null, "terminals": [] })
    );
    assert_eq!(d["integrations"]["mail"]["driver"], "imap");
    assert_eq!(stored(&api).await, Value::Null, "GET writes nothing");
}

#[sqlx::test]
async fn post_replaces_only_the_sections_sent(pool: PgPool) {
    let api = api(pool).await;
    saved(
        &api,
        json!({ "invoice": { "prefix": "F-", "digits": 6, "start_number": 1000 },
                        "relation": { "prefix": "K-" } }),
    )
    .await;
    saved(&api, json!({ "invoice": { "default_note": "Thanks" } })).await;
    let d = get(&api, "").await;
    // The section is replaced as a whole: fields not sent fall back to defaults.
    assert_eq!(d["invoice"]["prefix"], Value::Null);
    assert_eq!(d["invoice"]["default_note"], "Thanks");
    // Sections not sent stay.
    assert_eq!(d["relation"]["prefix"], "K-");
    assert_eq!(d["relation"]["digits"], 4);
}

#[sqlx::test]
async fn type_reads_a_section_or_a_dotted_path(pool: PgPool) {
    let api = api(pool).await;
    saved(
        &api,
        json!({ "product": { "warning_below_preferred_marge": 15 },
                        "estimate": { "digits": 0 } }),
    )
    .await;
    assert_eq!(
        get(&api, "?type=product.warning_below_preferred_marge").await,
        json!([15])
    );
    assert_eq!(get(&api, "?type=invoice.default_note").await, json!([]));
    // A falsy stored value falls back to the default (Laravel FetchSettings).
    assert_eq!(get(&api, "?type=estimate.digits").await, json!([4]));
    assert_eq!(
        get(&api, "?type=cash_register.cash_drawer_link_mode").await,
        json!(["flexible"])
    );
    assert_eq!(get(&api, "?type=no_such_section").await, json!([]));
    let cash = get(&api, "?type=cash_register").await;
    assert_eq!(cash["receipt_line_length"], 34);
    assert_eq!(cash["cash_drawer_link_mode"], "flexible");
    assert_eq!(get(&api, "?type=estimate").await["prefix"], "EST-");
}

#[sqlx::test]
async fn cash_register_refs_and_shipping_options(pool: PgPool) {
    let api = api(pool).await;
    let wh = api
        .with_base("/warehouses")
        .create(json!({ "name": "Shop", "is_store": true }), "warehouse_id")
        .await;
    let rel = api
        .with_base("/relations")
        .create(
            json!({ "company_name": "Walk-in", "type": "business" }),
            "relation_id",
        )
        .await;
    saved(
        &api,
        json!({ "cash_register": {
            "prefix": "POS-", "receipt_line_length": 48, "accepted_cash": ["0.05", 1, "2.00"],
            "cash_drawer_link_mode": "strict", "default_relation_id": rel,
            "default_mutate_from_warehouse_id": wh,
            "shipping_options": [{ "type": "pickup", "name": "Pick up", "value": wh }],
        } }),
    )
    .await;
    let cash = get(&api, "?type=cash_register").await;
    assert_eq!(cash["default_relation_id"], json!(rel));
    assert_eq!(cash["accepted_cash"], json!(["0.05", 1, "2.00"]));
    assert_eq!(cash["cash_drawer_link_mode"], "strict");
    assert_eq!(cash["shipping_options"][0]["name"], "Pick up");
}

#[sqlx::test]
async fn integrations_keep_non_secret_settings(pool: PgPool) {
    let api = api(pool).await;
    saved(
        &api,
        json!({ "integrations": {
            "payment": { "active_provider": "pay", "payment_providers": {
                "pay": { "api_code": null, "api_token": "", "terminals": [{ "id": "TH-1", "name": "Till 1" }] },
            } },
            "syncProviders": { "picqer": { "base_url": "https://x.picqer.com", "order_export_tag_ids": [1, 2] } },
        } }),
    )
    .await;
    let d = get(&api, "?type=integrations").await;
    assert_eq!(d["payment"]["active_provider"], "pay");
    assert_eq!(
        d["payment"]["payment_providers"]["pay"]["terminals"][0]["id"],
        "TH-1"
    );
    assert_eq!(
        d["syncProviders"]["picqer"]["order_export_tag_ids"],
        json!([1, 2])
    );
    // Children not sent come back as defaults (Laravel IntegrationSetting::from fills them).
    assert_eq!(d["mail"]["driver"], "imap");
    assert_eq!(
        d["fulfillment"]["fulfillment_providers"]["sendcloud"]["countries"],
        json!([])
    );
}

// --- errors --------------------------------------------------------------------------------

#[sqlx::test]
async fn guards(pool: PgPool) {
    let api = api(pool.clone()).await;
    let uri = format!("/v1/workspace/{}/settings", api.ws);
    let (status, _) = send(&api.app, Method::GET, &uri, None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (mallory, _) = signup(&api.app, "mallory@example.com", "mal").await;
    let (status, _) = send(&api.app, Method::GET, &uri, None, Some(&mallory)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let carl = common::member_with(&api.app, &pool, &api.ws, "carl@example.com", &[]).await;
    let (status, _) = api.call(Method::GET, "", None, &carl).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let body = json!({ "invoice": { "prefix": "X" } });
    let (status, _) = api.call(Method::POST, "", Some(body.clone()), &carl).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let dana = common::member_with(
        &api.app,
        &pool,
        &api.ws,
        "dana@example.com",
        &["setting.get"],
    )
    .await;
    let (status, _) = api.call(Method::GET, "", None, &dana).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = api.call(Method::POST, "", Some(body.clone()), &dana).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let erin = common::member_with(
        &api.app,
        &pool,
        &api.ws,
        "erin@example.com",
        &["setting.update"],
    )
    .await;
    let (status, _) = api.call(Method::POST, "", Some(body), &erin).await;
    assert_eq!(status, StatusCode::OK);
}

#[sqlx::test]
async fn field_rules(pool: PgPool) {
    let api = api(pool).await;
    let cases = [
        (
            json!({ "invoice": { "prefix": "x".repeat(11) } }),
            "invoice.prefix",
            "The invoice.prefix must not be greater than 10 characters.",
        ),
        (
            json!({ "invoice": { "digits": 11 } }),
            "invoice.digits",
            "The invoice.digits must not be greater than 10.",
        ),
        (
            json!({ "invoice": { "start_number": "abc" } }),
            "invoice.start_number",
            "The invoice.start number must be an integer.",
        ),
        (
            json!({ "invoice": "x" }),
            "invoice",
            "The invoice must be an array.",
        ),
        (
            json!({ "smtp": { "encryption": "starttls" } }),
            "smtp.encryption",
            "Invalid smtp.encryption encryption type.",
        ),
        (
            json!({ "smtp": { "port": "x" } }),
            "smtp.port",
            "The smtp.port must be an integer.",
        ),
        (
            json!({ "smtp": { "bcc_all_emails": "yes" } }),
            "smtp.bcc_all_emails",
            "The smtp.bcc all emails field must be true or false.",
        ),
        (
            json!({ "cash_register": { "receipt_line_length": 19 } }),
            "cash_register.receipt_line_length",
            "The cash register.receipt line length must be at least 20.",
        ),
        (
            json!({ "cash_register": { "cash_drawer_link_mode": "loose" } }),
            "cash_register.cash_drawer_link_mode",
            "The selected cash register.cash drawer link mode is invalid.",
        ),
        (
            json!({ "cash_register": { "accepted_cash": ["x"] } }),
            "cash_register.accepted_cash.0",
            "The cash register.accepted cash.0 must be a number.",
        ),
        (
            json!({ "cash_register": { "shipping_options": [{ "name": "x" }] } }),
            "cash_register.shipping_options.0.type",
            "The cash register.shipping options.0.type field is required.",
        ),
        (
            json!({ "cash_register": { "shipping_options": [{ "type": "drone", "name": "x" }] } }),
            "cash_register.shipping_options.0.type",
            "The selected cash register.shipping options.0.type is invalid.",
        ),
        (
            json!({ "cash_register": { "default_relation_id": "nope" } }),
            "cash_register.default_relation_id",
            "The cash register.default relation id must be a valid UUID.",
        ),
        (
            json!({ "product": { "warning_below_preferred_marge": 101 } }),
            "product.warning_below_preferred_marge",
            "The product.warning below preferred marge must not be greater than 100.",
        ),
        (
            json!({ "product": { "user_search_history_count": 0 } }),
            "product.user_search_history_count",
            "The product.user search history count must be at least 1.",
        ),
        (
            json!({ "incoming_invoice": { "ledgers": [{ "name": "Sales" }] } }),
            "incoming_invoice.ledgers.0.no",
            "The incoming invoice.ledgers.0.no field is required.",
        ),
        (
            json!({ "estimate": { "default_due_date_in_days": -1 } }),
            "estimate.default_due_date_in_days",
            "The estimate.default due date in days must be at least 0.",
        ),
        (
            json!({ "integrations": { "export": { "export_providers": { "prestashop": { "error_email": "nope" } } } } }),
            "integrations.export.export_providers.prestashop.error_email",
            "The integrations.export.export providers.prestashop.error email must be a valid email address.",
        ),
        (
            json!({ "integrations": { "syncProviders": { "picqer": { "order_export_start_date": "soon" } } } }),
            "integrations.syncProviders.picqer.order_export_start_date",
            "The integrations.syncProviders.picqer.order export start date is not a valid date.",
        ),
        (
            json!({ "integrations": { "syncProviders": { "picqer": { "order_export_tag_ids": ["a"] } } } }),
            "integrations.syncProviders.picqer.order_export_tag_ids.0",
            "The integrations.syncProviders.picqer.order export tag ids.0 must be an integer.",
        ),
        (
            json!({ "integrations": { "mail": { "driver": "graph" } } }),
            "integrations.mail.graph.tenant_id",
            "The integrations.mail.graph.tenant id field is required when integrations.mail.driver is graph.",
        ),
        (json!({ "meta": "x" }), "meta", "The meta must be an array."),
    ];
    for (body, field, msg) in cases {
        assert_eq!(error(&api, body, field).await, msg, "{field}");
    }
    assert_eq!(stored(&api).await, Value::Null, "a 422 writes nothing");
}

#[sqlx::test]
async fn refs_must_be_live_rows_of_this_workspace(pool: PgPool) {
    let api = api(pool).await;
    let (mallory, mal_ws) = signup(&api.app, "mallory@example.com", "mal").await;
    let (_, res) = send(
        &api.app,
        Method::POST,
        &format!("/v1/workspace/{mal_ws}/warehouses"),
        Some(json!({ "name": "Theirs" })),
        Some(&mallory),
    )
    .await;
    let theirs = res["data"]["warehouse_id"].as_str().unwrap().to_owned();
    for (section, key, label) in [
        (
            "cash_register",
            "default_mutate_from_warehouse_id",
            "cash register.default mutate from warehouse id",
        ),
        (
            "product",
            "second_chance_product_default_warehouse_id",
            "product.second chance product default warehouse id",
        ),
        (
            "cash_register",
            "default_relation_id",
            "cash register.default relation id",
        ),
        (
            "cash_register",
            "default_relation_type_id",
            "cash register.default relation type id",
        ),
        (
            "work_order",
            "fetch_users_from_team_id",
            "work order.fetch users from team id",
        ),
    ] {
        let body = json!({ section: { key: theirs } });
        assert_eq!(
            error(&api, body, &format!("{section}.{key}")).await,
            format!("The selected {label} is invalid.")
        );
    }
}

// --- secrets -------------------------------------------------------------------------------

/// Every key that holds a credential. F2 stores none of them (F2b adds encryption).
const SECRETS: &[&str] = &[
    "smtp.password",
    "integrations.payment.payment_providers.mollie.api_key",
    "integrations.payment.payment_providers.pay.api_code",
    "integrations.payment.payment_providers.pay.api_token",
    "integrations.payment.payment_providers.pay.service_id",
    "integrations.fulfillment.fulfillment_providers.sendcloud.public",
    "integrations.fulfillment.fulfillment_providers.sendcloud.secret",
    "integrations.accounting.accounting_providers.jortt.client_secret",
    "integrations.export.export_providers.modjo.api_key",
    "integrations.syncProviders.picqer.api_key",
    "integrations.mail.graph.client_secret",
];

/// `{a: {b: {c: v}}}` for "a.b.c".
fn nested(path: &str, v: Value) -> Value {
    path.rsplit('.').fold(v, |acc, k| json!({ k: acc }))
}

fn at<'a>(v: &'a Value, path: &str) -> &'a Value {
    path.split('.').fold(v, |acc, k| &acc[k])
}

#[sqlx::test]
async fn every_secret_is_refused_and_never_returned(pool: PgPool) {
    let api = api(pool).await;
    for path in SECRETS {
        let mut body = nested(path, json!("s3cret"));
        if path.starts_with("integrations.mail.graph") {
            // Graph needs its other fields; the secret check must still fire.
            body["integrations"]["mail"]["graph"]["tenant_id"] = json!("t");
        }
        assert_eq!(error(&api, body, path).await, SECRET, "{path}");
        // Null and blank are fine: the web app sends them back on every integrations save.
        saved(&api, nested(path, Value::Null)).await;
        saved(&api, nested(path, json!(""))).await;
    }
    // A secret that reached the row some other way (old data, hand edit) never leaves.
    let mut row = json!({});
    for path in SECRETS {
        let (section, rest) = path.split_once('.').unwrap();
        let mut sec = row.get(section).cloned().unwrap_or(json!({}));
        let leaf = nested(rest, json!("leaked"));
        merge(&mut sec, leaf);
        row[section] = sec;
    }
    sqlx::query(
        "INSERT INTO settings (workspace_id, data) VALUES ($1::uuid, $2)
         ON CONFLICT (workspace_id) DO UPDATE SET data = EXCLUDED.data",
    )
    .bind(&api.ws)
    .bind(&row)
    .execute(&api.pool)
    .await
    .unwrap();
    let all = get(&api, "").await;
    for path in SECRETS {
        assert_eq!(at(&all, path), &Value::Null, "GET leaks {path}");
        assert_eq!(
            get(&api, &format!("?type={path}")).await,
            json!([]),
            "?type leaks {path}"
        );
        let (section, _) = path.split_once('.').unwrap();
        let sec = get(&api, &format!("?type={section}")).await;
        assert!(!sec.to_string().contains("leaked"), "?type={section} leaks");
    }
    assert!(!all.to_string().contains("leaked"));
}

fn merge(into: &mut Value, from: Value) {
    match (into, from) {
        (Value::Object(a), Value::Object(b)) => {
            for (k, v) in b {
                merge(a.entry(k).or_insert(Value::Null), v);
            }
        }
        (slot, v) => *slot = v,
    }
}

#[sqlx::test]
async fn unknown_sections_and_keys_are_never_stored(pool: PgPool) {
    let api = api(pool).await;
    saved(
        &api,
        json!({ "secrets": { "api_key": "x" }, "order": { "prefix": "Z-" },
                "smtp": { "host": "mail.example.com", "token": "x" },
                "invoice": { "prefix": "F-", "customer_ref": { "api_key": "x" } } }),
    )
    .await;
    let row = stored(&api).await;
    assert!(row.get("secrets").is_none());
    assert!(
        row.get("order").is_none(),
        "order is read-only, as in Laravel"
    );
    assert!(row["smtp"].get("token").is_none());
    assert_eq!(row["smtp"]["host"], "mail.example.com");
    assert!(!row.to_string().contains("api_key\":\"x"));
}

// --- edge ----------------------------------------------------------------------------------

#[sqlx::test]
async fn null_section_falls_back_to_defaults(pool: PgPool) {
    let api = api(pool).await;
    saved(&api, json!({ "estimate": { "prefix": "Q-" } })).await;
    saved(&api, json!({ "estimate": null })).await;
    assert_eq!(get(&api, "?type=estimate").await["prefix"], "EST-");
}

#[sqlx::test]
async fn meta_and_general_are_kept(pool: PgPool) {
    let api = api(pool).await;
    saved(
        &api,
        json!({ "meta": { "onboarding": { "done": true } },
                        "general": { "language": "nl", "color": "orange", "extra": 1 } }),
    )
    .await;
    let d = get(&api, "").await;
    assert_eq!(d["meta"], json!({ "onboarding": { "done": true } }));
    assert_eq!(d["general"]["language"], "nl");
    assert_eq!(d["general"]["color"], "orange");
    assert_eq!(d["general"]["timezone"], "UTC");
    assert!(d["general"].get("extra").is_none());
}

#[sqlx::test]
async fn other_workspaces_settings_stay_apart(pool: PgPool) {
    let api = api(pool).await;
    let (mallory, mal_ws) = signup(&api.app, "mallory@example.com", "mal").await;
    let (status, _) = send(
        &api.app,
        Method::POST,
        &format!("/v1/workspace/{mal_ws}/settings"),
        Some(json!({ "invoice": { "prefix": "MAL-" } })),
        Some(&mallory),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(get(&api, "?type=invoice.prefix").await, json!([]));
}

// --- boundary ------------------------------------------------------------------------------

#[sqlx::test]
async fn limits_pass_at_the_edge(pool: PgPool) {
    let api = api(pool).await;
    saved(
        &api,
        json!({ "invoice": { "prefix": "x".repeat(10), "digits": 10 },
                "cash_register": { "receipt_line_length": 80 },
                "product": { "user_search_history_count": 40, "warning_below_preferred_marge": 0 } }),
    )
    .await;
    assert_eq!(
        error(
            &api,
            json!({ "cash_register": { "receipt_line_length": 81 } }),
            "cash_register.receipt_line_length"
        )
        .await,
        "The cash register.receipt line length must not be greater than 80."
    );
}

/// Ten first POSTs at once on a workspace without a row: one row, every section kept.
#[sqlx::test]
async fn parallel_first_posts_lose_nothing(pool: PgPool) {
    let api = api(pool).await;
    let sections = [
        "invoice",
        "estimate",
        "credit",
        "relation",
        "product",
        "work_order",
        "collective_invoice",
        "cash_register",
        "smtp",
        "incoming_invoice",
    ];
    let mut tasks = Vec::new();
    for s in sections {
        let body = if s == "smtp" || s == "incoming_invoice" {
            json!({ s: {} })
        } else {
            json!({ s: { "prefix": format!("{}-", &s[..3]) } })
        };
        let app = api.app.clone();
        let uri = format!("/v1/workspace/{}/settings", api.ws);
        let token = api.owner.clone();
        tasks.push(tokio::spawn(async move {
            send(&app, Method::POST, &uri, Some(body), Some(&token))
                .await
                .0
        }));
    }
    for t in tasks {
        assert_eq!(t.await.unwrap(), StatusCode::OK);
    }
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM settings WHERE workspace_id = $1::uuid")
        .bind(&api.ws)
        .fetch_one(&api.pool)
        .await
        .unwrap();
    assert_eq!(n, 1);
    let row = stored(&api).await;
    for s in sections {
        assert!(row.get(s).is_some(), "{s} lost");
    }
    assert_eq!(row["invoice"]["prefix"], "inv-");
}

// --- performance ---------------------------------------------------------------------------

/// GET is one primary-key read whatever the row holds; a big integrations tree stays fast.
#[sqlx::test]
async fn big_settings_read_quickly(pool: PgPool) {
    let api = api(pool).await;
    let terminals: Vec<Value> = (0..200)
        .map(|i| json!({ "id": format!("TH-{i}") }))
        .collect();
    saved(&api, json!({ "integrations": { "payment": { "payment_providers": { "pay": { "terminals": terminals } } } } })).await;
    let started = std::time::Instant::now();
    for _ in 0..20 {
        get(&api, "").await;
    }
    assert!(
        started.elapsed().as_millis() < 1_000,
        "{:?}",
        started.elapsed()
    );
}
