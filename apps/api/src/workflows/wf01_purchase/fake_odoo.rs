//! Test-only Odoo on 127.0.0.1: an in-memory RFQ store answering the exact JSON-2 calls the C3a
//! adapter sends (facts O1 cursor domain + order, O2 currencies, O5 company controls). Rows are
//! mutable between polls; `fail` makes the next calls on one path answer 500.

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use axum::{
    Router,
    extract::{Path, State},
    http::StatusCode,
    routing::post,
};
use serde_json::{Value, json};

use crate::{
    integrations::providers::{SourceClient, odoo},
    outbound::Outbound,
};

#[derive(Clone, Debug)]
pub struct Line {
    pub id: i64,
    pub product: i64,
    pub uom: i64,
    pub qty: &'static str,
    pub price: &'static str,
    pub subtotal: &'static str,
    pub taxes: Vec<i64>,
}

#[derive(Clone, Debug)]
pub struct Order {
    pub company: i64,
    pub state: &'static str,
    pub vendor: i64,
    pub currency: i64,
    pub buyer: Option<i64>,
    pub write_date: String,
    pub untaxed: &'static str,
    pub tax: &'static str,
    pub total: &'static str,
    pub lines: Vec<Line>,
}

impl Order {
    /// Draft RFQ of company 1, vendor 42, EUR, buyer 2: 2 × 150 = 300.00 + 63.00 tax.
    pub fn draft(write_date: &str, line_id: i64) -> Self {
        Self {
            company: 1,
            state: "draft",
            vendor: 42,
            currency: 1,
            buyer: Some(2),
            write_date: write_date.into(),
            untaxed: "300.00",
            tax: "63.00",
            total: "363.00",
            lines: vec![Line {
                id: line_id,
                product: 9,
                uom: 1,
                qty: "2.0",
                price: "150.0",
                subtotal: "300.00",
                taxes: vec![1],
            }],
        }
    }
}

#[derive(Default)]
pub struct Db {
    pub orders: BTreeMap<i64, Order>,
    /// company id → (po_double_validation, po_double_validation_amount)
    pub companies: BTreeMap<i64, (&'static str, &'static str)>,
    pub currencies: BTreeMap<i64, (&'static str, u8)>,
    /// `model/method` paths that answer 500 (Transient per C3a's classifier).
    pub fail: Vec<String>,
    pub calls: Vec<String>,
}

pub type Shared = Arc<Mutex<Db>>;

/// Exact JSON number from text (serde_json arbitrary_precision keeps the digits).
fn n(text: &str) -> Value {
    serde_json::from_str(text).unwrap()
}

fn ids_in(domain: &[Value], field: &str) -> Option<Vec<i64>> {
    domain
        .iter()
        .find(|d| d[0] == field && d[1] == "in")
        .map(|d| {
            d[2].as_array()
                .unwrap()
                .iter()
                .filter_map(Value::as_i64)
                .collect()
        })
}

fn term<'a>(domain: &'a [Value], field: &str, op: &str) -> Option<&'a Value> {
    domain
        .iter()
        .find(|d| d[0] == field && d[1] == op)
        .map(|d| &d[2])
}

fn eligible(o: &Order) -> bool {
    matches!(o.state, "draft" | "sent")
}

fn answer(db: &Db, path: &str, body: &Value) -> Value {
    let empty = Vec::new();
    let domain = body["domain"].as_array().unwrap_or(&empty);
    let limit = body["limit"].as_u64().map_or(usize::MAX, |l| l as usize);
    match path {
        "purchase.order/search_read" if term(domain, "write_date", ">").is_some() => {
            // facts L57-79, L96: `>` / `=` on write_date then id, ordered write_date, id.
            let companies = ids_in(domain, "company_id").unwrap();
            let ts = term(domain, "write_date", ">").unwrap().as_str().unwrap();
            let last = term(domain, "id", ">").unwrap().as_i64().unwrap();
            let mut rows: Vec<(&String, i64)> = db
                .orders
                .iter()
                .filter(|(_, o)| companies.contains(&o.company) && eligible(o))
                .filter(|(id, o)| o.write_date.as_str() > ts || (o.write_date == ts && **id > last))
                .map(|(id, o)| (&o.write_date, *id))
                .collect();
            rows.sort();
            json!(
                rows.into_iter()
                    .take(limit)
                    .map(|(w, id)| json!({"id": id, "write_date": w}))
                    .collect::<Vec<_>>()
            )
        }
        "purchase.order/search_read" => {
            let ids = ids_in(domain, "id").unwrap();
            json!(ids
                .iter()
                .filter_map(|id| db.orders.get(id).map(|o| (id, o)))
                .map(|(id, o)| json!({
                    "id": id, "state": o.state,
                    "company_id": [o.company, format!("Company {}", o.company)],
                    "partner_id": [o.vendor, format!("Vendor {}", o.vendor)],
                    "currency_id": [o.currency, "EUR"],
                    "user_id": o.buyer.map_or(json!(false), |b| json!([b, "Ana Buyer"])),
                    "date_order": false,
                    "amount_untaxed": n(o.untaxed), "amount_tax": n(o.tax), "amount_total": n(o.total),
                }))
                .collect::<Vec<_>>())
        }
        "purchase.order/search" => {
            let company = term(domain, "company_id", "=").unwrap().as_i64().unwrap();
            let after = term(domain, "id", ">").unwrap().as_i64().unwrap();
            json!(
                db.orders
                    .iter()
                    .filter(|(id, o)| o.company == company && eligible(o) && **id > after)
                    .map(|(id, _)| *id)
                    .take(limit)
                    .collect::<Vec<_>>()
            )
        }
        "purchase.order.line/search_read" => {
            let orders = ids_in(domain, "order_id").unwrap();
            let mut out = Vec::new();
            for oid in orders {
                for l in db
                    .orders
                    .get(&oid)
                    .map(|o| o.lines.clone())
                    .unwrap_or_default()
                {
                    out.push(json!({
                        "id": l.id, "order_id": [oid, format!("P{oid:05}")], "display_type": false,
                        "product_id": [l.product, format!("Product {}", l.product)],
                        "product_uom_id": [l.uom, "Units"],
                        "product_qty": n(l.qty), "price_unit": n(l.price), "price_subtotal": n(l.subtotal),
                        "tax_ids": l.taxes, "date_planned": false,
                    }));
                }
            }
            json!(out)
        }
        "res.currency/search_read" => {
            let ids = ids_in(domain, "id").unwrap();
            json!(
                ids.iter()
                    .filter_map(|id| db
                        .currencies
                        .get(id)
                        .map(|(name, dp)| json!({"id": id, "name": name, "decimal_places": dp})))
                    .collect::<Vec<_>>()
            )
        }
        "res.company/search_read" => {
            let ids = ids_in(domain, "id").unwrap();
            json!(ids
                .iter()
                .filter_map(|id| db.companies.get(id).map(|(mode, amount)| json!({
                    "id": id, "po_double_validation": mode, "po_double_validation_amount": n(amount)})))
                .collect::<Vec<_>>())
        }
        other => panic!("fake odoo: unexpected call {other}"),
    }
}

async fn handle(
    State(db): State<Shared>,
    Path((model, method)): Path<(String, String)>,
    body: String,
) -> (StatusCode, String) {
    let body: Value = serde_json::from_str(&body).unwrap();
    let path = format!("{model}/{method}");
    let mut db = db.lock().unwrap();
    db.calls.push(path.clone());
    if db.fail.contains(&path) {
        // facts L38: anything not a UserError is 500.
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"name": "builtins.RuntimeError", "message": "boom", "arguments": [], "context": {}, "debug": ""})
                .to_string(),
        );
    }
    (StatusCode::OK, answer(&db, &path, &body).to_string())
}

/// Start the fake with company 1 controlled (two_step, 0) and EUR (2 decimals).
pub async fn start() -> (SourceClient, Shared) {
    let db: Shared = Arc::default();
    {
        let mut d = db.lock().unwrap();
        d.companies.insert(1, ("two_step", "0.0"));
        d.currencies.insert(1, ("EUR", 2));
    }
    let app = Router::new()
        .route("/json/2/{model}/{method}", post(handle))
        .with_state(db.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = odoo::Client::with_key(
        Outbound::new(&["127.0.0.1".to_owned()]).unwrap(),
        format!("http://{addr}"),
        None,
        "fake-key",
        Duration::from_secs(5),
    );
    (SourceClient::Odoo(client), db)
}
