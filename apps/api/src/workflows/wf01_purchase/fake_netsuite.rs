//! Test-only NetSuite on 127.0.0.1 for WF-01 execute/reconcile: token endpoint, POST
//! purchaseOrder, `q=externalId IS "…"` lookup, GET by internal id. Every request (token calls
//! excluded) is logged with its method, so tests count POSTs and prove no PATCH/PUT. Scripts:
//! one-shot POST steps, held replies (gates), lookup misses, read-back overrides.

use std::{
    collections::VecDeque,
    str::FromStr,
    sync::{Arc, Mutex},
    time::Duration,
};

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, Response},
};
use rust_decimal::Decimal;
use serde_json::{Value, json};
use tokio::sync::Notify;

use crate::{
    integrations::providers::{SinkClient, netsuite::Client},
    outbound::Outbound,
};

const KEY: &str = include_str!("../../../tests/fixtures/netsuite_es256_test_key.pem");
const TOKEN_PATH: &str = "/services/rest/auth/oauth2/v1/token";
const PO_PATH: &str = "/services/rest/record/v1/purchaseOrder";

/// One scripted answer to the next POST.
pub enum Post {
    /// Answer as given, store nothing.
    Reply(u16, Value),
    /// Store the PO, then hang past the client timeout (facts l.244: the request may still land).
    CommitThenHang,
    /// Store the PO, wait for the gate, then answer 204 + Location.
    CommitThenHold(Arc<Notify>),
}

/// A request the fake saw.
#[derive(Clone, Debug)]
pub struct Seen {
    pub method: String,
    pub body: String,
}

#[derive(Default)]
pub struct State {
    pub seen: Vec<Seen>,
    pub posts: VecDeque<Post>,
    pub stored: Vec<Value>,
    /// Lookups by external id find nothing (facts l.202-206: a miss proves nothing).
    pub miss_lookup: bool,
    /// The next lookup waits for this gate.
    pub hold_lookup: Option<Arc<Notify>>,
    /// `taxTotal` the read-back reports (NetSuite computes tax, R3-§2b).
    pub tax_total: String,
    /// Edit a stored PO before it is read back.
    pub tamper: Option<fn(&mut Value)>,
}

pub type Shared = Arc<Mutex<State>>;

/// POSTs seen so far.
pub fn posts(ns: &Shared) -> Vec<Seen> {
    ns.lock()
        .unwrap()
        .seen
        .iter()
        .filter(|s| s.method == "POST")
        .cloned()
        .collect()
}

/// Methods other than GET/POST seen (must stay empty: never PATCH/PUT).
pub fn writes_other_than_post(ns: &Shared) -> usize {
    ns.lock()
        .unwrap()
        .seen
        .iter()
        .filter(|s| s.method != "GET" && s.method != "POST")
        .count()
}

fn reply(status: u16, location: Option<String>, body: &Value) -> Response<Body> {
    let mut b = Response::builder().status(status);
    if let Some(l) = location {
        b = b.header("Location", l);
    }
    let body = if body.is_null() {
        Body::empty()
    } else {
        Body::from(body.to_string())
    };
    b.body(body).unwrap()
}

/// nsError body (facts l.265).
pub fn ns_error(status: u16, code: &str, detail: &str) -> Value {
    json!({"type": "https://www.rfc-editor.org/rfc/rfc9110.html", "title": "Error", "status": status,
           "o:errorDetails": [{"detail": detail, "o:errorCode": code}]})
}

fn num(text: &str) -> Value {
    serde_json::from_str(text).unwrap()
}

fn store(ns: &Shared, body: &str) -> String {
    let mut po: Value = serde_json::from_str(body).unwrap();
    let mut s = ns.lock().unwrap();
    let id = (100 + s.stored.len()).to_string();
    // facts l.146-147: body fields `subtotal`, `taxTotal` on read.
    let subtotal: Decimal = po["item"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| {
            Decimal::from_str(&l["quantity"].to_string()).unwrap()
                * Decimal::from_str(&l["rate"].to_string()).unwrap()
        })
        .sum();
    po["id"] = json!(id);
    po["subtotal"] = num(&subtotal.to_string());
    po["taxTotal"] = num(&s.tax_total);
    s.stored.push(po);
    id
}

async fn handle(ns: Shared, base: String, req: Request<Body>) -> Response<Body> {
    let method = req.method().to_string();
    let uri = req.uri().to_string();
    let path = req.uri().path().to_owned();
    let body =
        String::from_utf8(to_bytes(req.into_body(), 1 << 20).await.unwrap().to_vec()).unwrap();
    if path == TOKEN_PATH {
        // facts l.53: access_token, expires_in, token_type
        return reply(
            200,
            None,
            &json!({"access_token": "tok", "expires_in": 3600, "token_type": "bearer"}),
        );
    }
    ns.lock().unwrap().seen.push(Seen {
        method: method.clone(),
        body: body.clone(),
    });
    match (method.as_str(), path.as_str()) {
        // facts l.115, l.125: POST → 204 + Location of the new record
        ("POST", PO_PATH) => {
            let step = ns.lock().unwrap().posts.pop_front();
            match step {
                Some(Post::Reply(status, json)) => reply(status, None, &json),
                Some(Post::CommitThenHang) => {
                    store(&ns, &body);
                    Notify::new().notified().await;
                    reply(204, None, &Value::Null)
                }
                Some(Post::CommitThenHold(gate)) => {
                    let id = store(&ns, &body);
                    gate.notified().await;
                    reply(204, Some(format!("{base}{PO_PATH}/{id}")), &Value::Null)
                }
                None => {
                    let id = store(&ns, &body);
                    reply(204, Some(format!("{base}{PO_PATH}/{id}")), &Value::Null)
                }
            }
        }
        // facts l.195-196: q= filter → {items:[{id, links}], totalResults}
        ("GET", PO_PATH) => {
            let gate = ns.lock().unwrap().hold_lookup.take();
            if let Some(g) = gate {
                g.notified().await;
            }
            let wanted = uri.split("%22").nth(1).map(str::to_owned);
            let s = ns.lock().unwrap();
            let items: Vec<Value> = if s.miss_lookup {
                vec![]
            } else {
                s.stored
                    .iter()
                    .filter(|p| p["externalId"].as_str() == wanted.as_deref())
                    .map(|p| json!({"id": p["id"], "links": []}))
                    .collect()
            };
            reply(
                200,
                None,
                &json!({"items": items, "totalResults": items.len()}),
            )
        }
        // facts l.116: GET by internal id
        ("GET", p) if p.starts_with(PO_PATH) => {
            let id = p.rsplit('/').next().unwrap_or_default().to_owned();
            let s = ns.lock().unwrap();
            match s.stored.iter().find(|po| po["id"] == json!(id)).cloned() {
                Some(mut po) => {
                    if let Some(f) = s.tamper {
                        f(&mut po);
                    }
                    reply(200, None, &po)
                }
                None => reply(
                    404,
                    None,
                    &ns_error(404, "NONEXISTENT_ID", "Record does not exist."),
                ),
            }
        }
        _ => reply(405, None, &Value::Null),
    }
}

/// Start the fake; the client times out after 500 ms. Default read-back tax: 63.00.
pub async fn start() -> (SinkClient, Shared) {
    let ns: Shared = Arc::new(Mutex::new(State {
        tax_total: "63.00".into(),
        ..State::default()
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let (state, b) = (ns.clone(), base.clone());
    let app =
        Router::new().fallback(move |req: Request<Body>| handle(state.clone(), b.clone(), req));
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (client(&base), ns)
}

/// Another client on the same fake (a second worker).
pub fn client(base: &str) -> SinkClient {
    let outbound = Outbound::new(&["127.0.0.1".to_owned()]).unwrap();
    SinkClient::NetSuite(
        Client::with_pem(outbound, base, "client-1", "cert-1", KEY.to_owned())
            .with_timeout(Duration::from_millis(500)),
    )
}
