//! NetSuite adapter (C3b) against an in-process fake NetSuite. Verified against the documented
//! contract only: every fixture cites `docs/integrations/facts/netsuite-rest.md:<line>`.

use std::{
    collections::VecDeque,
    str::FromStr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, Response},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rust_decimal::Decimal;
use serde_json::{Value, json};
use sqlx::PgPool;
use verzo_360_api::{
    CredentialKey,
    integrations::{
        capability::{
            ConnectionProbe, CreateOutcome, ErrorClass, NewPo, NewPoLine, PurchaseOrderSink,
        },
        connections::repo::ConnectionRow,
        providers::{ConnCtx, Mode, SinkClient, netsuite::Client},
    },
    outbound::Outbound,
};

use crate::common;

const KEY: &str = include_str!("../fixtures/netsuite_es256_test_key.pem");
const TOKEN_PATH: &str = "/services/rest/auth/oauth2/v1/token";
const PO_PATH: &str = "/services/rest/record/v1/purchaseOrder";

/// One scripted record-service answer.
enum Step {
    Reply(u16, Option<String>, Value),
    /// Store the PO like a success, then hold the reply past the client timeout.
    CommitThenHang,
}

/// One request the fake saw (token calls excluded).
#[derive(Clone, Debug)]
struct Seen {
    method: String,
    uri: String,
    auth: String,
    name_validation: Option<String>,
    body: String,
}

#[derive(Clone, Default)]
struct Ns {
    seen: Arc<Mutex<Vec<Seen>>>,
    steps: Arc<Mutex<VecDeque<Step>>>,
    token_steps: Arc<Mutex<VecDeque<(u16, Value)>>>,
    token_forms: Arc<Mutex<Vec<String>>>,
    token_calls: Arc<AtomicUsize>,
    ttl: Arc<AtomicUsize>,
    stored: Arc<Mutex<Vec<Value>>>,
    /// Rate NetSuite reports on read for every line (it may differ from the posted one).
    read_rate: Arc<Mutex<Option<Value>>>,
}

impl Ns {
    fn step(&self, s: Step) {
        self.steps.lock().unwrap().push_back(s);
    }
    fn token_step(&self, status: u16, body: Value) {
        self.token_steps.lock().unwrap().push_back((status, body));
    }
    fn seen(&self) -> Vec<Seen> {
        self.seen.lock().unwrap().clone()
    }
    fn tokens(&self) -> usize {
        self.token_calls.load(Ordering::SeqCst)
    }
    fn store(&self, body: &str) -> String {
        let mut po: Value = serde_json::from_str(body).unwrap();
        let mut stored = self.stored.lock().unwrap();
        let id = (100 + stored.len()).to_string();
        po["id"] = json!(id);
        stored.push(po);
        id
    }
    /// GET-by-id reply in the documented read shape, built from the stored POST body (C6-F1).
    /// `refName` text and `taxTotal: 0` are fake values: they copy the documented shape only,
    /// not NetSuite's real behaviour (the fake does not model names or tax).
    fn read_shape(&self, po: &Value) -> Value {
        // facts/netsuite-rest.md:131 a reference is {id, refName}
        let reference = |r: &Value, kind: &str| json!({"id": r["id"], "refName": format!("Fake {kind} {}", r["id"].as_str().unwrap_or_default())});
        let num = |v: &Value| Decimal::from_str(&v.to_string()).unwrap();
        let read_rate = self.read_rate.lock().unwrap().clone();
        let mut subtotal = Decimal::ZERO;
        let items: Vec<Value> = po["item"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| {
                let mut line = l.clone();
                line["item"] = reference(&l["item"], "item");
                if let Some(rate) = &read_rate {
                    line["rate"] = rate.clone();
                }
                subtotal += num(&line["quantity"]) * num(&line["rate"]);
                line
            })
            .collect();
        let mut out = po.clone();
        for field in ["entity", "subsidiary", "currency", "location"] {
            if !po[field].is_null() {
                out[field] = reference(&po[field], field);
            }
        }
        let n = items.len();
        // facts/netsuite-rest.md:156 item collection: items + read-only count, hasMore, offset,
        // totalResults, links
        out["item"] = json!({"items": items, "count": n, "hasMore": false, "offset": 0,
                             "totalResults": n, "links": []});
        // facts/netsuite-rest.md:146-147 subtotal, taxTotal (numbers)
        out["subtotal"] =
            Value::Number(serde_json::Number::from_str(&subtotal.to_string()).unwrap());
        out["taxTotal"] = json!(0);
        out
    }
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

async fn handle(ns: Ns, base: String, req: Request<Body>) -> Response<Body> {
    let method = req.method().to_string();
    let uri = req.uri().to_string();
    let path = req.uri().path().to_owned();
    // Scoped: a `&Request<Body>` held across an await makes the future !Send.
    let (auth, name_validation) = {
        let header = |n: &str| {
            req.headers()
                .get(n)
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned)
        };
        (
            header("authorization").unwrap_or_default(),
            header("x-netsuite-propertynamevalidation"),
        )
    };
    let body =
        String::from_utf8(to_bytes(req.into_body(), 1 << 20).await.unwrap().to_vec()).unwrap();
    if path == TOKEN_PATH {
        let n = ns.token_calls.fetch_add(1, Ordering::SeqCst) + 1;
        ns.token_forms.lock().unwrap().push(body);
        let scripted = ns.token_steps.lock().unwrap().pop_front();
        let (status, json) = scripted.unwrap_or_else(|| {
            // facts/netsuite-rest.md:53 access_token, expires_in, token_type
            let ttl = match ns.ttl.load(Ordering::SeqCst) {
                0 => 3600,
                t => t,
            };
            (
                200,
                json!({"access_token": format!("tok-{n}"), "expires_in": ttl, "token_type": "bearer"}),
            )
        });
        return reply(status, None, &json);
    }
    ns.seen.lock().unwrap().push(Seen {
        method: method.clone(),
        uri: uri.clone(),
        auth,
        name_validation,
        body: body.clone(),
    });
    let step = ns.steps.lock().unwrap().pop_front();
    match step {
        Some(Step::Reply(status, location, json)) => return reply(status, location, &json),
        Some(Step::CommitThenHang) => {
            ns.store(&body);
            tokio::time::sleep(Duration::from_secs(5)).await;
            return reply(204, None, &Value::Null);
        }
        None => {}
    }
    match (method.as_str(), path.as_str()) {
        // facts/netsuite-rest.md:115,125 POST → 204 + Location of the new record
        ("POST", PO_PATH) => {
            let id = ns.store(&body);
            reply(204, Some(format!("{base}{PO_PATH}/{id}")), &Value::Null)
        }
        // facts/netsuite-rest.md:195-196 q= filter → {items:[{id, links}], totalResults}
        ("GET", PO_PATH) => {
            let wanted = uri.split("%22").nth(1).map(str::to_owned);
            let items: Vec<Value> = ns
                .stored
                .lock()
                .unwrap()
                .iter()
                .filter(|p| wanted.is_none() || p["externalId"].as_str() == wanted.as_deref())
                .map(|p| json!({"id": p["id"], "links": []}))
                .collect();
            reply(
                200,
                None,
                &json!({"items": items, "totalResults": items.len()}),
            )
        }
        // facts/netsuite-rest.md:116 GET by internal id, documented read shape (C6-F1)
        ("GET", p) if p.starts_with(PO_PATH) => {
            let id = p.rsplit('/').next().unwrap_or_default();
            let found = ns
                .stored
                .lock()
                .unwrap()
                .iter()
                .find(|s| s["id"] == json!(id))
                .cloned();
            match found {
                Some(po) => reply(200, None, &ns.read_shape(&po)),
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

/// nsError body (facts/netsuite-rest.md:265).
fn ns_error(status: u16, code: &str, detail: &str) -> Value {
    json!({
        "type": "https://www.rfc-editor.org/rfc/rfc9110.html",
        "title": "Error",
        "status": status,
        "o:errorDetails": [{"detail": detail, "o:errorCode": code}]
    })
}

async fn fake() -> (Ns, String) {
    let ns = Ns::default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let (state, b) = (ns.clone(), base.clone());
    let app =
        Router::new().fallback(move |req: Request<Body>| handle(state.clone(), b.clone(), req));
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (ns, base)
}

fn client(base: &str) -> Client {
    let outbound = Outbound::new(&["127.0.0.1".to_owned()]).unwrap();
    Client::with_pem(outbound, base, "client-1", "cert-1", KEY.to_owned())
        .with_timeout(Duration::from_millis(500))
}

fn dec(s: &str) -> Decimal {
    Decimal::from_str(s).unwrap()
}

fn po(eid: &str) -> NewPo {
    NewPo {
        external_id: eid.into(),
        vendor_id: "11".into(),
        subsidiary_id: "1".into(),
        currency_id: "2".into(),
        location_id: Some("7".into()),
        lines: vec![NewPoLine {
            item_id: "99".into(),
            unit_id: None,
            quantity: dec("3"),
            rate: dec("10.25"),
        }],
    }
}

fn rejected(o: CreateOutcome) -> verzo_360_api::integrations::capability::ProviderError {
    match o {
        CreateOutcome::Rejected(e) => e,
        CreateOutcome::Created { id, .. } => panic!("created {id}"),
        CreateOutcome::Ambiguous(e) => panic!("ambiguous {e}"),
        CreateOutcome::Conflict(e) => panic!("conflict {e}"),
    }
}

fn ambiguous(o: CreateOutcome) -> verzo_360_api::integrations::capability::ProviderError {
    match o {
        CreateOutcome::Ambiguous(e) => e,
        CreateOutcome::Created { id, .. } => panic!("created {id}"),
        CreateOutcome::Rejected(e) => panic!("rejected {e}"),
        CreateOutcome::Conflict(e) => panic!("conflict {e}"),
    }
}

fn posts(ns: &Ns) -> usize {
    ns.seen().iter().filter(|s| s.method == "POST").count()
}

#[tokio::test]
async fn create_posts_once_and_reads_location() {
    let (ns, base) = fake().await;
    let c = client(&base);
    let CreateOutcome::Created { id, location } = c.create(&po("vz-abc123")).await else {
        panic!("not created");
    };
    assert_eq!(id, "100");
    assert_eq!(location.unwrap(), format!("{base}{PO_PATH}/100"));
    let seen = ns.seen();
    assert_eq!(seen.len(), 1);
    let post = &seen[0];
    assert_eq!((post.method.as_str(), post.uri.as_str()), ("POST", PO_PATH));
    assert_eq!(post.auth, "Bearer tok-1");
    assert_eq!(post.name_validation.as_deref(), Some("error"));
    let body: Value = serde_json::from_str(&post.body).unwrap();
    // facts/netsuite-rest.md:105,133-168 field and sublist names
    assert_eq!(
        body,
        json!({
            "externalId": "vz-abc123",
            "entity": {"id": "11"},
            "subsidiary": {"id": "1"},
            "currency": {"id": "2"},
            "location": {"id": "7"},
            "item": {"items": [{"item": {"id": "99"}, "quantity": 3, "rate": 10.25}]}
        })
    );
    assert!(
        seen.iter()
            .all(|s| s.method != "PUT" && s.method != "PATCH")
    );
}

#[tokio::test]
async fn token_form_carries_netsuite_assertion() {
    let (ns, base) = fake().await;
    client(&base).read("100").await.err().expect("error");
    let form = ns.token_forms.lock().unwrap()[0].clone();
    // facts/netsuite-rest.md:49-52
    let prefix = "grant_type=client_credentials&client_assertion_type=urn%3Aietf%3Aparams%3Aoauth%3Aclient-assertion-type%3Ajwt-bearer&client_assertion=";
    let jwt = form.strip_prefix(prefix).expect("form fields");
    let parts: Vec<&str> = jwt.split('.').collect();
    let header: Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[0]).unwrap()).unwrap();
    let claims: Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[1]).unwrap()).unwrap();
    // facts/netsuite-rest.md:60-67
    assert_eq!(
        header,
        json!({"typ": "JWT", "alg": "ES256", "kid": "cert-1"})
    );
    assert_eq!(claims["iss"], "client-1");
    assert_eq!(claims["scope"], "rest_webservices");
    assert_eq!(claims["aud"], format!("{base}{TOKEN_PATH}"));
}

#[tokio::test]
async fn token_is_cached_until_expiry_minus_60s() {
    let (ns, base) = fake().await;
    let c = client(&base);
    c.find_by_external_id("vz-1").await.unwrap();
    c.find_by_external_id("vz-2").await.unwrap();
    assert_eq!(ns.tokens(), 1);

    // expires_in 30 is inside the 60 s margin: every call fetches again.
    let (ns, base) = fake().await;
    ns.ttl.store(30, Ordering::SeqCst);
    let c = client(&base);
    c.find_by_external_id("vz-1").await.unwrap();
    c.find_by_external_id("vz-2").await.unwrap();
    assert_eq!(ns.tokens(), 2);
}

#[tokio::test]
async fn invalid_grant_refreshes_once_then_auth() {
    // facts/netsuite-rest.md:54 expired → invalid_grant, restart the flow
    let (ns, base) = fake().await;
    ns.token_step(400, json!({"error": "invalid_grant"}));
    client(&base).find_by_external_id("vz-1").await.unwrap();
    assert_eq!(ns.tokens(), 2);

    let (ns, base) = fake().await;
    ns.token_step(400, json!({"error": "invalid_grant"}));
    ns.token_step(400, json!({"error": "invalid_grant"}));
    let e = client(&base)
        .find_by_external_id("vz-1")
        .await
        .err()
        .expect("error");
    assert_eq!(e.class, ErrorClass::Auth);
    assert_eq!(e.code.as_deref(), Some("invalid_grant"));
    assert_eq!(ns.tokens(), 2);
    assert!(ns.seen().is_empty());
}

#[tokio::test]
async fn read_401_refreshes_token_and_retries_once() {
    let (ns, base) = fake().await;
    // facts/netsuite-rest.md:273
    ns.step(Step::Reply(
        401,
        None,
        ns_error(401, "INVALID_LOGIN", "Invalid login attempt."),
    ));
    assert!(
        client(&base)
            .find_by_external_id("vz-1")
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(ns.tokens(), 2);
    let seen = ns.seen();
    assert_eq!(
        (seen[0].auth.as_str(), seen[1].auth.as_str()),
        ("Bearer tok-1", "Bearer tok-2")
    );
}

#[tokio::test]
async fn create_401_is_rejected_without_retry() {
    let (ns, base) = fake().await;
    ns.step(Step::Reply(
        401,
        None,
        ns_error(401, "INVALID_LOGIN", "Invalid login attempt."),
    ));
    let e = rejected(client(&base).create(&po("vz-1")).await);
    assert_eq!(
        (e.class, e.code.as_deref()),
        (ErrorClass::Auth, Some("INVALID_LOGIN"))
    );
    assert_eq!(posts(&ns), 1);
}

/// C6-N2: a refused token is dropped, so the next call fetches a new one.
#[tokio::test]
async fn create_401_forgets_token() {
    let (ns, base) = fake().await;
    ns.step(Step::Reply(
        401,
        None,
        ns_error(401, "INVALID_LOGIN", "Invalid login attempt."),
    ));
    let c = client(&base);
    rejected(c.create(&po("vz-1")).await);
    c.find_by_external_id("vz-1").await.unwrap();
    assert_eq!(ns.tokens(), 2);
    let seen = ns.seen();
    assert_eq!(
        (seen[0].auth.as_str(), seen[1].auth.as_str()),
        ("Bearer tok-1", "Bearer tok-2")
    );
    assert_eq!(posts(&ns), 1);
}

#[tokio::test]
async fn create_that_commits_then_times_out_is_ambiguous_and_found_later() {
    let (ns, base) = fake().await;
    ns.step(Step::CommitThenHang);
    let c = client(&base);
    let e = ambiguous(c.create(&po("vz-late")).await);
    assert_eq!(e.class, ErrorClass::Ambiguous);
    assert_eq!(posts(&ns), 1);
    assert_eq!(ns.stored.lock().unwrap().len(), 1);
    let found = c
        .find_by_external_id("vz-late")
        .await
        .unwrap()
        .expect("found");
    assert_eq!(found.external_id.as_deref(), Some("vz-late"));
    assert_eq!(posts(&ns), 1);
}

#[tokio::test]
async fn create_429_is_rejected_transient() {
    let (ns, base) = fake().await;
    // facts/netsuite-rest.md:256
    ns.step(Step::Reply(
        429,
        None,
        ns_error(
            429,
            "CONCURRENCY_LIMIT_EXCEEDED",
            "Concurrent request limit exceeded. Request blocked.",
        ),
    ));
    let e = rejected(client(&base).create(&po("vz-1")).await);
    assert_eq!(e.class, ErrorClass::Transient);
    assert_eq!(e.code.as_deref(), Some("CONCURRENCY_LIMIT_EXCEEDED"));
    assert_eq!(e.http_status, Some(429));
}

#[tokio::test]
async fn create_5xx_after_send_is_ambiguous() {
    let (ns, base) = fake().await;
    // facts/netsuite-rest.md:278
    ns.step(Step::Reply(
        500,
        None,
        ns_error(
            500,
            "UNEXPECTED_ERROR",
            "An unexpected error occurred. Error ID: x1",
        ),
    ));
    let e = ambiguous(client(&base).create(&po("vz-1")).await);
    assert_eq!(e.code.as_deref(), Some("UNEXPECTED_ERROR"));
    assert_eq!(posts(&ns), 1);
}

#[tokio::test]
async fn create_with_malformed_or_missing_location_is_ambiguous() {
    let (ns, base) = fake().await;
    ns.step(Step::Reply(
        204,
        Some(format!("{base}/services/rest/record/v1/customer/647")),
        Value::Null,
    ));
    ambiguous(client(&base).create(&po("vz-1")).await);
    ns.step(Step::Reply(204, None, Value::Null));
    ambiguous(client(&base).create(&po("vz-2")).await);
    // facts/netsuite-rest.md:115 202 = async accepted; not requested, so not trusted
    ns.step(Step::Reply(202, None, Value::Null));
    ambiguous(client(&base).create(&po("vz-3")).await);
}

#[tokio::test]
async fn unreachable_host_sends_nothing() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let e = rejected(client(&base).create(&po("vz-1")).await);
    assert_eq!(e.class, ErrorClass::Transient);
}

#[tokio::test]
async fn untrusted_private_host_is_refused() {
    let c = Client::with_pem(
        Outbound::new(&[]).unwrap(),
        "http://127.0.0.1:9",
        "c",
        "k",
        KEY.into(),
    );
    let e = c.read("1").await.err().expect("error");
    assert_eq!(e.class, ErrorClass::Validation);
}

#[tokio::test]
async fn find_by_external_id_found_missing_duplicate_and_bad() {
    let (ns, base) = fake().await;
    let c = client(&base);
    assert!(c.find_by_external_id("vz-none").await.unwrap().is_none());
    let CreateOutcome::Created { id, .. } = c.create(&po("vz-x")).await else {
        panic!()
    };
    let found = c.find_by_external_id("vz-x").await.unwrap().unwrap();
    assert_eq!(found.id, id);
    let q = ns
        .seen()
        .into_iter()
        .find(|s| s.uri.contains("q="))
        .unwrap()
        .uri;
    // facts/netsuite-rest.md:195
    assert_eq!(q, format!("{PO_PATH}?q=externalId%20IS%20%22vz-none%22"));

    ns.step(Step::Reply(
        200,
        None,
        json!({"items": [{"id": "1"}, {"id": "2"}], "totalResults": 2}),
    ));
    assert_eq!(
        c.find_by_external_id("vz-x")
            .await
            .err()
            .expect("error")
            .class,
        ErrorClass::Conflict
    );

    // facts/netsuite-rest.md:203 a 404 miss counts as not found
    ns.step(Step::Reply(
        404,
        None,
        ns_error(404, "NONEXISTENT_ID", "Record does not exist."),
    ));
    assert!(c.find_by_external_id("vz-x").await.unwrap().is_none());

    let before = ns.seen().len();
    let e = c
        .find_by_external_id("vz\" OR \"1")
        .await
        .err()
        .expect("error");
    assert_eq!(e.class, ErrorClass::Validation);
    assert_eq!(ns.seen().len(), before);
}

#[tokio::test]
async fn read_normalises_mismatching_values_exactly() {
    let (ns, base) = fake().await;
    // facts/netsuite-rest.md:133-181 body fields + item line fields
    ns.step(Step::Reply(
        200,
        None,
        json!({
            "id": "100", "externalId": "vz-1",
            "entity": {"id": "12", "refName": "Other vendor"},
            "subsidiary": {"id": "3"}, "currency": {"id": "4"},
            "subtotal": 30.74, "taxTotal": 4.61,
            "item": {"items": [
                {"item": {"id": "99"}, "quantity": 3, "rate": 9.99, "units": "5"},
                {"item": {"id": 98}, "quantity": 1.5, "rate": 0.5}
            ], "count": 2, "hasMore": false}
        }),
    ));
    let t = client(&base).read("100").await.unwrap();
    assert_eq!(
        ns.seen()[0].uri,
        format!("{PO_PATH}/100?expandSubResources=true")
    );
    assert_eq!(
        (
            t.vendor_id.as_str(),
            t.subsidiary_id.as_str(),
            t.currency_id.as_str()
        ),
        ("12", "3", "4")
    );
    assert_eq!(t.untaxed, Some(dec("30.74")));
    assert_eq!(t.tax, Some(dec("4.61")));
    assert_eq!(t.lines.len(), 2);
    assert_eq!(t.lines[0].rate, dec("9.99"));
    assert_eq!(t.lines[0].unit_id.as_deref(), Some("5"));
    assert_eq!(
        (t.lines[1].item_id.as_str(), t.lines[1].quantity),
        ("98", dec("1.5"))
    );

    ns.step(Step::Reply(
        200,
        None,
        json!({"id": "100", "subsidiary": {"id": "3"}, "currency": {"id": "4"}}),
    ));
    assert_eq!(
        client(&base).read("100").await.err().expect("error").class,
        ErrorClass::Unknown
    );
    assert_eq!(
        client(&base)
            .read("1 OR 1")
            .await
            .err()
            .expect("error")
            .class,
        ErrorClass::Validation
    );
}

/// C6-F1: the default read path reports NetSuite's stored values, not the posted ones.
#[tokio::test]
async fn read_reports_netsuite_rate_not_posted() {
    let (ns, base) = fake().await;
    let c = client(&base);
    let CreateOutcome::Created { id, .. } = c.create(&po("vz-1")).await else {
        panic!("not created");
    };
    *ns.read_rate.lock().unwrap() = Some(json!(10.3));
    let t = c.read(&id).await.unwrap();
    assert_eq!(t.lines[0].rate, dec("10.3"));
    assert_eq!(t.untaxed, Some(dec("30.9")));
    assert_eq!(t.tax, Some(dec("0")));
}

#[tokio::test]
async fn error_classes_per_documented_code() {
    // facts/netsuite-rest.md:271-278
    let cases = [
        (400, "INVALID_CONTENT", ErrorClass::Validation),
        (400, "INVALID_REQUEST", ErrorClass::Validation),
        (403, "FORBIDDEN", ErrorClass::Permission),
        (404, "NONEXISTENT_ID", ErrorClass::NotFound),
        (429, "CONCURRENCY_LIMIT_EXCEEDED", ErrorClass::Transient),
        (500, "UNEXPECTED_ERROR", ErrorClass::Transient),
    ];
    for (status, code, class) in cases {
        let (ns, base) = fake().await;
        ns.step(Step::Reply(status, None, ns_error(status, code, "x")));
        let e = client(&base).read("100").await.err().expect("error");
        assert_eq!(
            (e.class, e.code.as_deref(), e.http_status),
            (class, Some(code), Some(status)),
            "{code}"
        );
    }
    // 401 twice: refreshed once, then Auth.
    let (ns, base) = fake().await;
    ns.step(Step::Reply(401, None, ns_error(401, "INVALID_LOGIN", "x")));
    ns.step(Step::Reply(401, None, ns_error(401, "INVALID_LOGIN", "x")));
    assert_eq!(
        client(&base).read("100").await.err().expect("error").class,
        ErrorClass::Auth
    );

    // Create: 400 is a definite reject with the field path kept.
    let (ns, base) = fake().await;
    let mut body = ns_error(400, "INVALID_CONTENT", "Invalid value.");
    body["o:errorDetails"][0]["o:errorPath"] = json!("item.items[0].item");
    ns.step(Step::Reply(400, None, body));
    let e = rejected(client(&base).create(&po("vz-1")).await);
    assert_eq!(e.class, ErrorClass::Validation);
    assert!(e.detail.contains("item.items[0].item"));
    // 409 → Conflict (PLAN §8 N3 conflict-shaped error)
    ns.step(Step::Reply(409, None, ns_error(409, "CONFLICT", "x")));
    assert!(matches!(
        client(&base).create(&po("vz-1")).await,
        CreateOutcome::Conflict(_)
    ));
}

#[tokio::test]
async fn probe_reads_once_and_never_writes() {
    let (ns, base) = fake().await;
    let checks = client(&base).test().await;
    assert!(checks.iter().all(|c| c.ok), "{checks:?}");
    assert_eq!(checks.len(), 4);
    let create = checks
        .iter()
        .find(|c| c.key == "purchase_order.create")
        .unwrap();
    assert!(create.detail.as_deref().unwrap().contains("never writes"));
    let seen = ns.seen();
    assert_eq!(seen.len(), 1);
    assert_eq!(
        (seen[0].method.as_str(), seen[0].uri.as_str()),
        ("GET", &*format!("{PO_PATH}?limit=1"))
    );

    let (ns, base) = fake().await;
    ns.token_step(401, json!({"error": "invalid_client"}));
    let checks = client(&base).test().await;
    assert!(
        checks
            .iter()
            .all(|c| !c.ok && c.error_class == Some(ErrorClass::Auth))
    );
    assert!(ns.seen().is_empty());

    let (ns, base) = fake().await;
    ns.step(Step::Reply(403, None, ns_error(403, "x", "no")));
    let checks = client(&base).test().await;
    assert!(checks[0].ok);
    assert!(
        checks[1..]
            .iter()
            .all(|c| c.error_class == Some(ErrorClass::Permission))
    );
}

#[sqlx::test]
async fn live_connection_builds_netsuite_client_with_sealed_key(pool: PgPool) {
    let row = ConnectionRow {
        id: uuid::Uuid::now_v7(),
        workspace_id: uuid::Uuid::now_v7(),
        provider: "netsuite".into(),
        label: "NS".into(),
        config: json!({"rest_domain": "123456.suitetalk.api.netsuite.com", "environment": "sandbox",
                       "client_id": "c", "certificate_id": "k", "alg": "ES256"}),
        status: "draft".into(),
        status_reason: None,
        credential_expires_at: None,
        last_probe: None,
        last_probe_at: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    let ctx = ConnCtx {
        outbound: Outbound::new(&[]).unwrap(),
        credentials: Some(Arc::new(
            CredentialKey::from_hex(common::CREDENTIALS_KEY).unwrap(),
        )),
        db: pool,
        mode: Mode::Live,
    };
    let client = SinkClient::for_connection(&ctx, &row).unwrap();
    assert!(matches!(client, SinkClient::NetSuite(_)));
    // No sealed key: every check fails Auth before any request leaves.
    let checks = client.test().await;
    assert!(
        checks
            .iter()
            .all(|c| c.error_class == Some(ErrorClass::Auth)),
        "{checks:?}"
    );
    assert!(
        checks[0]
            .detail
            .as_deref()
            .unwrap()
            .contains("No private key")
    );
}
