//! Invoicing fixture shared by the send and credit tests: Acme with SMTP on the local fake,
//! a PDF sidecar fake, invoice numbering `INV-` from 1000, a main warehouse (10 of a product)
//! and a store, and a business relation with a contact.

use std::time::Duration;

use axum::http::{Method, StatusCode};
use genesy_360_api::{build_app, mail::Mailer, outbound::Outbound, pdf::Renderer};
use serde_json::{Value, json};
use sqlx::PgPool;

use super::{Api, fake_pdf, fake_smtp};

pub struct F {
    pub api: Api,
    pub inbox: fake_smtp::Inbox,
    pub smtp_port: u16,
    pub rel: String,
    pub contact: String,
    pub product: String,
    pub main: String,
    pub shop: String,
}

pub async fn setup(pool: PgPool) -> F {
    setup_with(pool, true).await
}

/// `setup`, or without the PDF sidecar and the file store (`renders: false`, as staging).
pub async fn setup_with(pool: PgPool, renders: bool) -> F {
    let (smtp_port, inbox) = fake_smtp::start().await;
    let (pdf_url, _) = fake_pdf::start(b"%PDF-1.4 invoice".to_vec()).await;
    let mailer = Mailer::capture()
        .0
        .with_trusted_smtp_hosts(&["localhost".to_owned()])
        .with_timeout(Duration::from_secs(5));
    let mut state = super::state(pool.clone(), mailer);
    state.pdf = Some(Renderer::new(
        &pdf_url,
        Outbound::new(&["127.0.0.1".to_owned()]).unwrap(),
    ));
    if !renders {
        state.pdf = None;
        state.files = None;
    }
    let mut api = Api::new(pool, "/invoices").await;
    api.app = build_app(state);
    settings(
        &api,
        json!({
            "smtp": { "host": "localhost", "port": smtp_port, "from_address": "office@acme.test" },
            "invoice": { "prefix": "INV-", "digits": 4, "start_number": 1000 },
        }),
    )
    .await;
    let wh = api.with_base("/warehouses");
    let main = wh
        .create(
            json!({ "name": "Main", "default_warehouse": true }),
            "warehouse_id",
        )
        .await;
    let shop = wh
        .create(
            json!({ "name": "Shop", "is_store": true, "pickup_enabled": true }),
            "warehouse_id",
        )
        .await;
    let product = api
        .with_base("/products")
        .create(
            json!({ "name": "Chair", "price": "10", "wholesale_price": "4" }),
            "product_id",
        )
        .await;
    stock(&api, &product, &main, 10).await;
    let rel = api
        .with_base("/relations")
        .create(
            json!({ "company_name": "Client", "type": "business",
                    "addresses": [{ "address": "Main", "house_number": "1", "zipcode": "1000AA",
                                    "city": "Amsterdam", "country": "NL", "type": "billing" }] }),
            "relation_id",
        )
        .await;
    let (status, res) = api
        .with_base("/relations")
        .call(
            Method::POST,
            &format!("/{rel}/contacts"),
            Some(json!({ "first_name": "Cleo", "last_name": "C", "email": "cleo@client.test" })),
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let contact = res["data"]["contact_id"].as_str().unwrap().to_owned();
    F {
        api,
        inbox,
        smtp_port,
        rel,
        contact,
        product,
        main,
        shop,
    }
}

pub async fn settings(api: &Api, body: Value) {
    let (status, res) = api
        .with_base("/settings")
        .call(Method::POST, "", Some(body), &api.owner)
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
}

pub async fn stock(api: &Api, product: &str, warehouse: &str, change: i64) {
    let (status, res) = api
        .with_base("/products")
        .call(
            Method::POST,
            &format!("/{product}/stocks"),
            Some(json!({ "warehouse_id": warehouse, "mutation_value": change, "reason": "count" })),
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
}

pub async fn level(f: &F, warehouse: &str) -> String {
    sqlx::query_scalar(
        "SELECT stock_lvl::text FROM stocks
         WHERE product_id = $1::uuid AND warehouse_id = $2::uuid AND bin_id IS NULL",
    )
    .bind(&f.product)
    .bind(warehouse)
    .fetch_one(&f.api.pool)
    .await
    .unwrap()
}

/// A draft with `lines`; `credit` makes it a credit note.
pub async fn draft(f: &F, lines: Value, credit: bool) -> String {
    f.api
        .create(
            json!({ "relation_id": f.rel, "is_credit": credit, "items": lines }),
            "invoice_id",
        )
        .await
}

/// Two chairs at 10 (21% VAT) from `warehouse`.
pub fn chairs(f: &F, quantity: i64, warehouse: Option<&str>) -> Value {
    json!([{ "title": "Chair", "product_id": f.product, "quantity": quantity, "price": "10",
             "vat": "21", "mutate_from_warehouse_id": warehouse }])
}

pub fn message(f: &F, method: &str) -> Value {
    json!({
        "addressed_contact_id": f.contact,
        "address": { "address": "Dam", "house_number": "1", "zipcode": "1012JS",
                     "city": "Amsterdam", "country": "NL" },
        "to": ["Client@Client.test"],
        "subject": "Invoice {invoice.invoice_nr}",
        "body": "<p>Total {invoice.total}, due {invoice.due_date}, {unknown}</p>",
        "send_method": method,
    })
}

pub async fn send(f: &F, id: &str, body: Value) -> (StatusCode, Value) {
    f.api
        .call(
            Method::POST,
            &format!("/{id}/send"),
            Some(body),
            &f.api.owner,
        )
        .await
}

pub async fn resend(f: &F, id: &str, body: Value) -> (StatusCode, Value) {
    f.api
        .call(
            Method::POST,
            &format!("/{id}/resend"),
            Some(body),
            &f.api.owner,
        )
        .await
}

/// Stored number and status.
pub async fn numbered(f: &F, id: &str) -> (Option<i64>, String) {
    sqlx::query_as("SELECT invoice_number, status FROM invoices WHERE id = $1::uuid")
        .bind(id)
        .fetch_one(&f.api.pool)
        .await
        .unwrap()
}

/// A mail body as sent, quoted-printable soft breaks and `=3D` undone.
pub fn plain(data: &str) -> String {
    data.replace("=\r\n", "")
        .replace("=\n", "")
        .replace("=3D", "=")
}

pub fn ok() -> (StatusCode, Value) {
    (StatusCode::OK, json!({ "message": "ok" }))
}
