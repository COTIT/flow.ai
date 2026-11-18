//! Adapter tests against an in-process fake Odoo. Every canned reply cites the
//! `docs/integrations/facts/odoo-19.md` line (as `facts L<n>`) whose documented shape it copies.

use std::{
    str::FromStr,
    sync::{Arc, Mutex},
    time::Duration,
};

use axum::{
    Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    routing::post,
};
use chrono::{TimeZone, Utc};
use rust_decimal::Decimal;
use serde_json::{Value, json};

use super::Client;
use crate::{
    integrations::capability::{
        ConnectionProbe, Cursor, ErrorClass, PurchaseRequestSource, SourceState,
    },
    outbound::Outbound,
};

const KEY: &str = "odoo-test-key-123";

/// One request the fake received.
#[derive(Clone, Debug)]
struct Seen {
    path: String,
    auth: Option<String>,
    database: Option<String>,
    content_type: Option<String>,
    body: Value,
}

type Reply = Box<dyn Fn(&str, &Value) -> (u16, String) + Send + Sync>;

struct Fake {
    seen: Mutex<Vec<Seen>>,
    reply: Reply,
    delay: Option<Duration>,
}

async fn handle(
    State(f): State<Arc<Fake>>,
    Path((model, method)): Path<(String, String)>,
    headers: HeaderMap,
    body: String,
) -> (StatusCode, String) {
    let body: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
    let header = |n: &str| {
        headers
            .get(n)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned)
    };
    f.seen.lock().unwrap().push(Seen {
        path: format!("{model}/{method}"),
        auth: header("authorization"),
        database: header("x-odoo-database"),
        content_type: header("content-type"),
        body: body.clone(),
    });
    if let Some(d) = f.delay {
        tokio::time::sleep(d).await;
    }
    let (status, text) = (f.reply)(&format!("{model}/{method}"), &body);
    (StatusCode::from_u16(status).unwrap(), text)
}

/// Fake on 127.0.0.1 answering every `POST /json/2/<model>/<method>` with `reply`.
async fn fake(
    reply: impl Fn(&str, &Value) -> (u16, String) + Send + Sync + 'static,
    delay: Option<Duration>,
) -> (String, Arc<Fake>) {
    let state = Arc::new(Fake {
        seen: Mutex::new(Vec::new()),
        reply: Box::new(reply),
        delay,
    });
    let app = Router::new()
        .route("/json/2/{model}/{method}", post(handle))
        .with_state(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (format!("http://{addr}"), state)
}

fn client(base: &str, database: Option<&str>) -> Client {
    let outbound = Outbound::new(&["127.0.0.1".to_owned()]).unwrap();
    Client::with_key(
        outbound,
        base.to_owned(),
        database.map(str::to_owned),
        KEY,
        Duration::from_millis(500),
    )
}

fn ok(v: Value) -> (u16, String) {
    (200, v.to_string())
}

/// `ok` with the `"PRICE"` placeholder swapped for a number f64 can't hold (facts L181:
/// `price_unit` is numeric with no fixed scale).
fn raw(v: Value) -> (u16, String) {
    (
        200,
        v.to_string().replace("\"PRICE\"", "0.123456789012345678"),
    )
}

/// facts L37: error body `{name, message, arguments, context, debug}`.
fn odoo_error(status: u16, name: &str, message: &str) -> (u16, String) {
    (
        status,
        json!({"name": name, "message": message, "arguments": [message], "context": {},
               "debug": "Traceback (most recent call last): SECRET-TRACE"})
        .to_string(),
    )
}

fn at(s: &str) -> chrono::DateTime<Utc> {
    Utc.from_utc_datetime(&chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").unwrap())
}

fn dec(s: &str) -> Decimal {
    Decimal::from_str(s).unwrap()
}

// ---- transport ----

#[tokio::test]
async fn list_changed_sends_named_args_bearer_and_database() {
    // facts L106: search_read body; L95: write_date whole seconds UTC.
    let (base, f) = fake(
        |_, _| {
            ok(json!([
                {"id": 7, "write_date": "2026-11-17 10:00:00"},
                {"id": 9, "write_date": "2026-11-17 10:00:05"}
            ]))
        },
        None,
    )
    .await;
    let c = client(&base, Some("acme-prod"));
    let cursor = Cursor {
        ts: at("2026-11-17 09:00:00"),
        id: 3,
    };
    let page = c
        .list_changed(&["1".into(), "2".into()], &cursor, 2)
        .await
        .unwrap();
    assert_eq!(page.items.len(), 2);
    assert_eq!(page.items[1].id, "9");
    assert_eq!(page.items[1].changed_at, at("2026-11-17 10:00:05"));
    let next = page.next.expect("full page has a next cursor");
    assert_eq!((next.ts, next.id), (at("2026-11-17 10:00:05"), 9));

    let seen = f.seen.lock().unwrap()[0].clone();
    assert_eq!(seen.path, "purchase.order/search_read");
    // facts L27-28: bearer key; X-Odoo-Database optional; JSON content type.
    assert_eq!(seen.auth.as_deref(), Some("bearer odoo-test-key-123"));
    assert_eq!(seen.database.as_deref(), Some("acme-prod"));
    assert!(seen.content_type.unwrap().starts_with("application/json"));
    // facts L30, L45-53: object body with named args only.
    assert_eq!(
        seen.body,
        json!({
            "domain": [
                ["company_id", "in", [1, 2]],
                ["state", "in", ["draft", "sent"]],
                "|", ["write_date", ">", "2026-11-17 09:00:00"],
                "&", ["write_date", "=", "2026-11-17 09:00:00"], ["id", ">", 3]
            ],
            "fields": ["write_date"],
            "order": "write_date asc, id asc",
            "limit": 2
        })
    );
}

#[tokio::test]
async fn short_page_has_no_next_and_no_database_header_when_unset() {
    let (base, f) = fake(
        |_, _| ok(json!([{"id": 1, "write_date": "2026-11-17 10:00:00"}])),
        None,
    )
    .await;
    let c = client(&base, None);
    let cursor = Cursor {
        ts: at("2026-11-17 09:00:00"),
        id: 0,
    };
    let page = c.list_changed(&["1".into()], &cursor, 100).await.unwrap();
    assert_eq!(page.items.len(), 1);
    assert!(page.next.is_none());
    assert!(f.seen.lock().unwrap()[0].database.is_none());
}

/// Apply the cursor domain of list_changed (facts L57-79: `>`/`=` on stored write_date, id).
fn cursor_eval(rows: &[(i64, &str)], body: &Value) -> Value {
    let domain = body["domain"].as_array().unwrap();
    let find = |field: &str, op: &str| {
        domain
            .iter()
            .find(|d| d[0] == field && d[1] == op)
            .map(|d| d[2].clone())
            .unwrap()
    };
    let ts = find("write_date", ">");
    let ts = ts.as_str().unwrap();
    let last = find("id", ">").as_i64().unwrap();
    let limit = body["limit"].as_u64().unwrap() as usize;
    let mut out: Vec<_> = rows
        .iter()
        .filter(|(id, w)| *w > ts || (*w == ts && *id > last))
        .collect();
    out.sort_by(|a, b| (a.1, a.0).cmp(&(b.1, b.0)));
    Value::Array(
        out.into_iter()
            .take(limit)
            .map(|(id, w)| json!({"id": id, "write_date": w}))
            .collect(),
    )
}

#[tokio::test]
async fn equal_write_dates_page_by_id_without_loss() {
    // facts L91-98: write_date is the transaction start, so one import gives 250 equal stamps.
    let rows: Vec<(i64, &'static str)> = (1..=250).map(|i| (i, "2026-11-17 10:00:00")).collect();
    let (base, f) = fake(move |_, body| ok(cursor_eval(&rows, body)), None).await;
    let c = client(&base, None);
    let mut cursor = Cursor {
        ts: at("2026-11-17 09:59:59"),
        id: 0,
    };
    let mut got = Vec::new();
    loop {
        let page = c.list_changed(&["1".into()], &cursor, 100).await.unwrap();
        got.extend(page.items.iter().map(|r| r.id.clone()));
        match page.next {
            Some(n) => cursor = n,
            None => break,
        }
    }
    assert_eq!(f.seen.lock().unwrap().len(), 3);
    let want: Vec<String> = (1..=250).map(|i| i.to_string()).collect();
    assert_eq!(got, want);
}

#[tokio::test]
async fn eligible_ids_page_by_id_per_company() {
    // facts L63: search(domain, offset, limit, order) returns ids; L35: recordset → ids list.
    let (base, f) = fake(|_, _| ok(json!([11, 12, 15])), None).await;
    let c = client(&base, None);
    let ids = c.list_eligible_ids("4", 10, 3).await.unwrap();
    assert_eq!(ids, ["11", "12", "15"]);
    let seen = f.seen.lock().unwrap()[0].clone();
    assert_eq!(seen.path, "purchase.order/search");
    assert_eq!(
        seen.body,
        json!({
            "domain": [["company_id", "=", 4], ["state", "in", ["draft", "sent"]], ["id", ">", 10]],
            "order": "id asc",
            "limit": 3
        })
    );
}

// ---- read ----

/// Canned replies for a read of RFQ 21 (present) and 22 (absent).
fn read_reply(model: &str, _: &Value) -> (u16, String) {
    match model {
        // facts L107: many2one as [id, display_name] (shape UNVERIFIED item 4, parsed both ways).
        "purchase.order/search_read" => ok(json!([{
            "id": 21, "state": "sent",
            "company_id": [1, "Acme EU"], "partner_id": [8, "Wood Co"],
            "currency_id": [3, "EUR"], "user_id": false,
            "date_order": "2026-11-17 08:30:00",
            "amount_untaxed": 293.79, "amount_tax": 0.0, "amount_total": 293.79
        }])),
        // facts L229/L237: display_type lines (section/note) are not order lines.
        // Raw text: a json! literal would pass through f64 and lose digits.
        "purchase.order.line/search_read" => raw(json!([
            {"id": 101, "order_id": [21, "P00021"], "display_type": "line_section",
             "product_id": false, "product_qty": 0.0, "product_uom_id": false, "price_unit": 0.0,
             "price_subtotal": 0.0, "tax_ids": [], "date_planned": false},
            {"id": 102, "order_id": [21, "P00021"], "display_type": false,
             "product_id": [55, "Oak plank"], "product_qty": 12.0, "product_uom_id": [1, "Units"],
             "price_unit": "PRICE", "price_subtotal": 1.48,
             "tax_ids": [9, 4], "date_planned": "2026-11-20 00:00:00"},
            {"id": 103, "order_id": [21, "P00021"], "display_type": "line_note",
             "product_id": false, "product_qty": 0.0, "product_uom_id": false, "price_unit": 0.0,
             "price_subtotal": 0.0, "tax_ids": [], "date_planned": false}
        ])),
        // facts L120-121: decimal_places stored, from rounding.
        "res.currency/search_read" => ok(json!([{"id": 3, "name": "EUR", "decimal_places": 2}])),
        other => panic!("unexpected call {other}"),
    }
}

#[tokio::test]
async fn read_normalises_and_reports_absent_ids_as_gone() {
    let (base, f) = fake(read_reply, None).await;
    let c = client(&base, None);
    let (found, gone) = c.read(&["21".into(), "22".into()]).await.unwrap();
    assert_eq!(gone, ["22"]);
    assert_eq!(found.len(), 1);
    let r = &found[0];
    assert_eq!(r.id, "21");
    assert!(matches!(r.state, SourceState::Sent));
    assert_eq!((r.company_id.as_str(), r.vendor_id.as_str()), ("1", "8"));
    assert_eq!(
        (
            r.currency_id.as_str(),
            r.currency_ref.as_str(),
            r.currency_decimals
        ),
        ("3", "EUR", 2)
    );
    assert_eq!(r.buyer_id, None);
    // O4: picking_type_id never requested in WF-01; location comes from the pair config.
    assert_eq!(r.location_id, None);
    assert_eq!(r.date_order.as_deref(), Some("2026-11-17 08:30:00"));
    assert_eq!(r.amount_untaxed, dec("293.79"));
    assert_eq!(r.amount_total, dec("293.79"));
    assert_eq!(
        r.labels.get("vendor:8").map(String::as_str),
        Some("Wood Co")
    );
    assert_eq!(r.lines.len(), 1, "section and note lines dropped");
    let l = &r.lines[0];
    assert_eq!(l.id, "102");
    assert_eq!((l.product_id.as_str(), l.uom_id.as_str()), ("55", "1"));
    assert_eq!(l.price_unit, dec("0.123456789012345678"), "no f64 rounding");
    assert_eq!(l.quantity, dec("12.0"));
    assert_eq!(l.subtotal, dec("1.48"));
    assert_eq!(l.tax_ids, ["9", "4"]);
    assert_eq!(l.date_planned.as_deref(), Some("2026-11-20 00:00:00"));

    let seen = f.seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 3, "one call per model, no N+1");
    assert_eq!(seen[0].body["domain"], json!([["id", "in", [21, 22]]]));
    let fields = seen[0].body["fields"].as_array().unwrap();
    assert!(!fields.contains(&json!("picking_type_id")));
    assert_eq!(seen[1].body["domain"], json!([["order_id", "in", [21]]]));
    assert_eq!(seen[2].body["domain"], json!([["id", "in", [3]]]));
    // Archived currencies stay readable.
    assert_eq!(seen[2].body["context"], json!({"active_test": false}));
}

#[tokio::test]
async fn read_of_only_absent_ids_makes_one_call() {
    let (base, f) = fake(|_, _| ok(json!([])), None).await;
    let c = client(&base, None);
    let (found, gone) = c.read(&["5".into(), "x".into()]).await.unwrap();
    assert!(found.is_empty());
    assert_eq!(gone, ["5", "x"]);
    assert_eq!(f.seen.lock().unwrap().len(), 1);
    let (found, gone) = c.read(&[]).await.unwrap();
    assert!(found.is_empty() && gone.is_empty());
    assert_eq!(f.seen.lock().unwrap().len(), 1, "empty input calls nothing");
}

#[tokio::test]
async fn decimals_stay_exact_for_any_currency_precision() {
    // facts L121: decimal_places 0 (JPY-like) and 3 (KWD-like) come from rounding.
    let (base, _) = fake(
        |model, _| match model {
            "purchase.order/search_read" => ok(json!([
                {"id": 1, "state": "draft", "company_id": 1, "partner_id": 2, "currency_id": 4,
                 "user_id": [6, "Bo"], "date_order": false,
                 "amount_untaxed": 1000, "amount_tax": 100, "amount_total": 1100},
                {"id": 2, "state": "to approve", "company_id": 1, "partner_id": 2, "currency_id": 5,
                 "user_id": false, "date_order": false,
                 "amount_untaxed": 12.345, "amount_tax": 1.2e-2, "amount_total": 12.357}
            ])),
            "purchase.order.line/search_read" => ok(json!([])),
            "res.currency/search_read" => ok(json!([
                {"id": 4, "name": "JPY", "decimal_places": 0},
                {"id": 5, "name": "KWD", "decimal_places": 3}
            ])),
            other => panic!("unexpected {other}"),
        },
        None,
    )
    .await;
    let c = client(&base, None);
    let (found, _) = c.read(&["1".into(), "2".into()]).await.unwrap();
    let jpy = found.iter().find(|r| r.id == "1").unwrap();
    assert_eq!(
        (jpy.currency_ref.as_str(), jpy.currency_decimals),
        ("JPY", 0)
    );
    assert_eq!(jpy.amount_total, dec("1100"));
    assert_eq!(jpy.vendor_id, "2", "plain id many2one accepted");
    assert_eq!(jpy.buyer_id.as_deref(), Some("6"));
    let kwd = found.iter().find(|r| r.id == "2").unwrap();
    assert_eq!(kwd.currency_decimals, 3);
    assert!(matches!(kwd.state, SourceState::ToApprove));
    assert_eq!(kwd.amount_untaxed, dec("12.345"));
    assert_eq!(kwd.amount_tax, dec("0.012"), "exponent form parsed exactly");
}

#[tokio::test]
async fn read_controls_reports_two_step_and_zero_threshold() {
    // facts L163-165: both O5 fields, no groups=, readable by base.group_user.
    let (base, f) = fake(
        |_, _| {
            ok(json!([
                {"id": 1, "po_double_validation": "two_step", "po_double_validation_amount": 0.0},
                {"id": 2, "po_double_validation": "one_step", "po_double_validation_amount": 5000.0}
            ]))
        },
        None,
    )
    .await;
    let c = client(&base, None);
    let controls = c.read_controls(&["1".into(), "2".into()]).await.unwrap();
    let one = controls.iter().find(|c| c.company_id == "1").unwrap();
    assert!(one.two_step && one.threshold_zero);
    let two = controls.iter().find(|c| c.company_id == "2").unwrap();
    assert!(!two.two_step && !two.threshold_zero);
    let seen = f.seen.lock().unwrap()[0].clone();
    assert_eq!(seen.path, "res.company/search_read");
    assert_eq!(
        seen.body["fields"],
        json!(["po_double_validation", "po_double_validation_amount"])
    );
}

// ---- errors ----

async fn list_error(reply: (u16, String)) -> crate::integrations::capability::ProviderError {
    let (base, _) = fake(move |_, _| reply.clone(), None).await;
    let c = client(&base, None);
    let cursor = Cursor {
        ts: at("2026-11-17 09:00:00"),
        id: 0,
    };
    match c.list_changed(&["1".into()], &cursor, 10).await {
        Ok(_) => panic!("expected an error"),
        Err(e) => e,
    }
}

#[tokio::test]
async fn http_errors_map_to_classes_without_leaking_debug() {
    let cases = [
        // facts L28: invalid key → Unauthorized("Invalid apikey") 401.
        (
            odoo_error(401, "werkzeug.exceptions.Unauthorized", "Invalid apikey"),
            ErrorClass::Auth,
        ),
        // facts L39: AccessError 403.
        (
            odoo_error(403, "odoo.exceptions.AccessError", "You are not allowed"),
            ErrorClass::Permission,
        ),
        // facts L33: unknown model/method 404.
        (
            odoo_error(404, "werkzeug.exceptions.NotFound", "Not Found"),
            ErrorClass::NotFound,
        ),
        // facts L39: LockError 409.
        (
            odoo_error(409, "odoo.exceptions.LockError", "locked"),
            ErrorClass::Conflict,
        ),
        // facts L84: bad order syntax → UserError 422.
        (
            odoo_error(
                422,
                "odoo.exceptions.UserError",
                "Invalid \"order\" specified",
            ),
            ErrorClass::Validation,
        ),
        // facts L36: unparseable body → 400.
        (
            odoo_error(
                400,
                "werkzeug.exceptions.BadRequest",
                "could not parse the body as json",
            ),
            ErrorClass::Validation,
        ),
        // facts L86-87: unknown field / malformed domain → 500 builtins.ValueError.
        (
            odoo_error(
                500,
                "builtins.ValueError",
                "Invalid field 'nope' on model 'purchase.order'",
            ),
            ErrorClass::Validation,
        ),
        // facts L38: anything else → 500; a read is safe to retry.
        (
            odoo_error(
                500,
                "psycopg2.OperationalError",
                "server closed the connection",
            ),
            ErrorClass::Transient,
        ),
        ((503, "Service Unavailable".into()), ErrorClass::Transient),
        ((429, "".into()), ErrorClass::Transient),
        ((418, "teapot".into()), ErrorClass::Unknown),
    ];
    for (reply, class) in cases {
        let status = reply.0;
        let e = list_error(reply).await;
        assert_eq!(e.class, class, "status {status}: {}", e.detail);
        assert_eq!(e.http_status, Some(status));
        assert!(!e.detail.contains("SECRET-TRACE"), "debug never copied");
        assert!(!e.detail.contains(KEY));
    }
    let e = list_error(odoo_error(403, "odoo.exceptions.AccessError", "No access")).await;
    assert_eq!(e.code.as_deref(), Some("odoo.exceptions.AccessError"));
    assert!(e.detail.contains("No access"));
}

#[tokio::test]
async fn malformed_success_body_is_unknown() {
    let e = list_error((200, "{not json".into())).await;
    assert_eq!(e.class, ErrorClass::Unknown);
    let e = list_error(ok(json!({"id": 1}))).await;
    assert_eq!(e.class, ErrorClass::Unknown, "object where a list belongs");
    let e = list_error(ok(json!([{"id": 1, "write_date": "yesterday"}]))).await;
    assert_eq!(e.class, ErrorClass::Unknown);
}

#[tokio::test]
async fn timeout_and_refused_connection_are_transient() {
    let (base, _) = fake(|_, _| ok(json!([])), Some(Duration::from_secs(3))).await;
    let c = client(&base, None);
    let cursor = Cursor {
        ts: at("2026-11-17 09:00:00"),
        id: 0,
    };
    let e = c
        .list_changed(&["1".into()], &cursor, 1)
        .await
        .err()
        .unwrap();
    assert_eq!(e.class, ErrorClass::Transient);
    assert_eq!(e.http_status, None);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    let c = client(&format!("http://{addr}"), None);
    let e = c
        .list_changed(&["1".into()], &cursor, 1)
        .await
        .err()
        .unwrap();
    assert_eq!(e.class, ErrorClass::Transient);
}

// ---- probe ----

#[tokio::test]
async fn probe_passes_every_odoo_capability() {
    let (base, f) = fake(|_, _| ok(json!([])), None).await;
    let checks = client(&base, None).test().await;
    let keys: Vec<_> = checks.iter().map(|c| c.key.as_str()).collect();
    assert_eq!(
        keys,
        [
            "purchase_request.list",
            "purchase_request.read",
            "source_controls.read"
        ]
    );
    assert!(checks.iter().all(|c| c.ok && c.error_class.is_none()));
    let paths: Vec<_> = f
        .seen
        .lock()
        .unwrap()
        .iter()
        .map(|s| s.path.clone())
        .collect();
    assert_eq!(
        paths,
        [
            "purchase.order/search_read",
            "purchase.order/search_read",
            "purchase.order.line/search_read",
            "res.currency/search_read",
            "res.company/search_read"
        ]
    );
}

#[tokio::test]
async fn probe_fails_only_the_refused_capability() {
    let (base, _) = fake(
        |model, _| match model {
            // facts L165-166: company record rules can hide res.company.
            "res.company/search_read" => {
                odoo_error(403, "odoo.exceptions.AccessError", "res.company denied")
            }
            _ => ok(json!([])),
        },
        None,
    )
    .await;
    let checks = client(&base, None).test().await;
    assert!(checks[0].ok && checks[1].ok);
    assert!(!checks[2].ok);
    assert_eq!(checks[2].error_class, Some(ErrorClass::Permission));
    assert!(
        checks[2]
            .detail
            .as_deref()
            .unwrap()
            .contains("res.company denied")
    );
}

#[tokio::test]
async fn probe_with_bad_key_fails_every_check_as_auth() {
    let (base, _) = fake(
        |_, _| odoo_error(401, "werkzeug.exceptions.Unauthorized", "Invalid apikey"),
        None,
    )
    .await;
    let checks = client(&base, None).test().await;
    assert_eq!(checks.len(), 3);
    assert!(
        checks
            .iter()
            .all(|c| !c.ok && c.error_class == Some(ErrorClass::Auth))
    );
}
