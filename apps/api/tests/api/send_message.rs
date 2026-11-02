//! Workspace mail (final batch F5b2): `POST /send-message` through a profile's or the
//! workspace's SMTP server (a local fake) or Microsoft Graph (a local fake), the recipient
//! allow-list on every transport, the send logs and their sweep.

use crate::common;

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use axum::{
    Json, Router,
    body::Bytes,
    extract::Path,
    http::{HeaderMap, Method, StatusCode},
    routing::{get, post},
};
use common::{Api, fake_smtp, member_with};
use genesy_360_api::{
    build_app, cleanup,
    mail::{Mailer, graph},
    outbound::Outbound,
};
use serde_json::{Value, json};
use sqlx::PgPool;

const PDF_B64: &str = "JVBERi0xLjQKJcOkw7zDtsOfCg==";

/// Calls the fake Graph saw: (path, authorization, body).
type GraphLog = Arc<Mutex<Vec<(String, String, String)>>>;

/// A Graph fake on 127.0.0.1: tokens for any tenant (but `bad`), 202 for sendMail, one inbox
/// message.
async fn fake_graph() -> (String, GraphLog) {
    let log = GraphLog::default();
    let (l1, l2, l3) = (log.clone(), log.clone(), log.clone());
    let app = Router::new()
        .route(
            "/{tenant}/oauth2/v2.0/token",
            post(move |Path(tenant): Path<String>, body: Bytes| async move {
                l1.lock().unwrap().push((
                    format!("/{tenant}/token"),
                    String::new(),
                    String::from_utf8_lossy(&body).into(),
                ));
                if tenant == "bad" {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(json!({ "error": "invalid_client" })),
                    );
                }
                (
                    StatusCode::OK,
                    Json(json!({ "access_token": "tok-1", "expires_in": 3600 })),
                )
            }),
        )
        .route(
            "/v1.0/users/{user}/sendMail",
            post(
                move |Path(user): Path<String>, h: HeaderMap, body: Bytes| async move {
                    let auth = h
                        .get("authorization")
                        .map(|v| v.to_str().unwrap().to_owned())
                        .unwrap_or_default();
                    l2.lock().unwrap().push((
                        format!("/sendMail/{user}"),
                        auth,
                        String::from_utf8_lossy(&body).into(),
                    ));
                    StatusCode::ACCEPTED
                },
            ),
        )
        .route(
            "/v1.0/users/{user}/mailFolders/inbox/messages",
            get(move |Path(user): Path<String>| async move {
                l3.lock()
                    .unwrap()
                    .push((format!("/inbox/{user}"), String::new(), String::new()));
                Json(json!({ "value": [{ "id": "m1" }] }))
            }),
        )
        .fallback(|| async { StatusCode::NOT_FOUND });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (url, log)
}

struct Fakes {
    api: Api,
    smtp_port: u16,
    inbox: fake_smtp::Inbox,
    graph: GraphLog,
    outbox: genesy_360_api::mail::Outbox,
}

/// An app whose mailer trusts `localhost` (the SMTP fake), talks to the Graph fake, and
/// allows `allowed` recipient domains (empty = all).
async fn setup(pool: PgPool, allowed: &str, trust_localhost: bool) -> Fakes {
    let (smtp_port, inbox) = fake_smtp::start().await;
    let (graph_url, graph) = fake_graph().await;
    let (mailer, outbox) = Mailer::capture();
    let trusted = if trust_localhost {
        vec!["localhost".to_owned()]
    } else {
        vec![]
    };
    let mailer = mailer
        .with_allowed_domains(allowed)
        .with_trusted_smtp_hosts(&trusted)
        .with_timeout(Duration::from_secs(2))
        .with_graph(graph::Client::new(
            &graph_url,
            &graph_url,
            Outbound::new(&["127.0.0.1".to_owned()]).unwrap(),
        ));
    let state = common::state(pool.clone(), mailer);
    let mut api = Api::new(pool, "/send-message").await;
    api.app = build_app(state);
    Fakes {
        api,
        smtp_port,
        inbox,
        graph,
        outbox,
    }
}

async fn call(
    api: &Api,
    base: &'static str,
    method: Method,
    path: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    api.with_base(base)
        .call(method, path, body, &api.owner)
        .await
}

/// A relation with one contact (email `contact@acme.test`); the contact's id.
async fn contact(api: &Api) -> String {
    let (_, rel) = call(
        api,
        "/relations",
        Method::POST,
        "",
        Some(json!({ "company_name": "Acme", "type": "business" })),
    )
    .await;
    let rel = rel["data"]["relation_id"].as_str().unwrap().to_owned();
    let (status, res) = call(
        api,
        "/relations",
        Method::POST,
        &format!("/{rel}/contacts"),
        Some(json!({ "first_name": "Cleo", "last_name": "C", "email": "Contact@Acme.test" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    res["data"]["contact_id"].as_str().unwrap().to_owned()
}

/// A profile on the SMTP fake.
async fn profile(api: &Api, port: u16) -> String {
    let body = json!({ "name": "Sales", "sender_name": "Acme Sales", "email": "sales@acme.test", "default": true,
                       "html": "<p>Kind regards, Sales</p>",
                       "smtp": { "host": "localhost", "port": port } });
    let (status, res) = call(api, "/email-profiles", Method::POST, "", Some(body)).await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    res["data"]["email_profile_id"].as_str().unwrap().to_owned()
}

fn message(contact: &str) -> Value {
    json!({
        "addressed_contact_id": contact,
        "address": { "address": "Main", "house_number": "1", "zipcode": "1000AA", "city": "Amsterdam", "country": "NL" },
        "to": ["Client@Acme.test"], "cc": ["boss@acme.test"], "bcc": ["archive@acme.test"],
        "subject": "Your offer", "body": "<p>Hello there</p>", "send_method": "email",
    })
}

async fn send(f: &Fakes, body: Value) -> (StatusCode, Value) {
    f.api.call(Method::POST, "", Some(body), &f.api.owner).await
}

/// (status, provider, from, to, error) of the only send log.
async fn the_log(api: &Api) -> (String, String, String, Value, Option<String>) {
    sqlx::query_as(
        "SELECT status, provider, from_email, to_recipients, error_message FROM email_send_logs",
    )
    .fetch_one(&api.pool)
    .await
    .unwrap()
}

// --- SMTP --------------------------------------------------------------------------------

#[sqlx::test]
async fn a_profile_sends_through_its_own_server(pool: PgPool) {
    let f = setup(pool, "", true).await;
    let contact = contact(&f.api).await;
    let profile = profile(&f.api, f.smtp_port).await;
    let (_, file) = call(
        &f.api,
        "/files",
        Method::POST,
        "",
        Some(json!({ "upload": PDF_B64, "document_name": "Offer.pdf", "public": 1 })),
    )
    .await;
    let mut body = message(&contact);
    body["email_profile_id"] = json!(profile);
    body["attachments"] = json!([file["data"]["file_id"]]);
    let (status, res) = send(&f, body).await;
    assert_eq!((status, res), (StatusCode::OK, json!({ "message": "ok" })));

    let inbox = f.inbox.lock().unwrap().clone();
    assert_eq!(inbox.len(), 1);
    let m = &inbox[0];
    assert_eq!(m.from, "<sales@acme.test>");
    assert_eq!(
        m.rcpt,
        [
            "<client@acme.test>",
            "<boss@acme.test>",
            "<archive@acme.test>"
        ]
    );
    for part in [
        "Subject: Your offer",
        "Acme Sales",
        "<sales@acme.test>",
        "Hello there",
        "Kind regards, Sales",
        "X-Genesy-Email-Log:",
        "Offer.pdf",
    ] {
        assert!(m.data.contains(part), "{part} in {}", m.data);
    }
    assert!(!m.data.contains("Bcc:"), "bcc stays off the headers");

    let (status, provider, from, to, error) = the_log(&f.api).await;
    assert_eq!(
        (status.as_str(), provider.as_str(), from.as_str()),
        ("accepted", "smtp", "sales@acme.test")
    );
    assert_eq!((to, error), (json!(["client@acme.test"]), None));
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM email_send_log_recipients")
        .fetch_one(&f.api.pool)
        .await
        .unwrap();
    assert_eq!(n, 3);
}

/// Line breaks in a sender name or subject can't add headers (lettre refused them with a
/// panic): they are dropped.
#[sqlx::test]
async fn line_breaks_never_reach_the_headers(pool: PgPool) {
    let f = setup(pool, "", true).await;
    let contact = contact(&f.api).await;
    let profile = profile(&f.api, f.smtp_port).await;
    sqlx::query("UPDATE email_profiles SET sender_name = E'Acme\r\nBcc: spy@evil.test'")
        .execute(&f.api.pool)
        .await
        .unwrap();
    let mut body = message(&contact);
    body["email_profile_id"] = json!(profile);
    let (status, res) = send(&f, body).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    let m = f.inbox.lock().unwrap()[0].clone();
    assert!(
        !m.data.contains("Bcc: spy") && !m.rcpt.iter().any(|r| r.contains("spy")),
        "{}",
        m.data
    );
}

/// Without `to` (vimdesk-web's mail dialog sends none) the addressed contact gets it.
#[sqlx::test]
async fn to_defaults_to_the_addressed_contact(pool: PgPool) {
    let f = setup(pool, "", true).await;
    let contact = contact(&f.api).await;
    let profile = profile(&f.api, f.smtp_port).await;
    let mut body = message(&contact);
    body["email_profile_id"] = json!(profile);
    body.as_object_mut().unwrap().remove("to");
    let (status, res) = send(&f, body).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(f.inbox.lock().unwrap()[0].rcpt[0], "<contact@acme.test>");
}

#[sqlx::test]
async fn the_workspace_server_without_a_profile(pool: PgPool) {
    let f = setup(pool, "", true).await;
    let contact = contact(&f.api).await;
    let (status, res) = send(&f, message(&contact)).await;
    assert_eq!(
        (status, res),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            json!({ "message": "No smtp host specified in the settings" })
        )
    );
    let smtp = json!({ "smtp": { "host": "localhost", "port": f.smtp_port, "from_address": "office@acme.test" } });
    let (status, res) = call(&f.api, "/settings", Method::POST, "", Some(smtp)).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    let (status, res) = send(&f, message(&contact)).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(f.inbox.lock().unwrap()[0].from, "<office@acme.test>");
}

/// A server at a private address is refused unless the operator trusts its host; the attempt
/// is logged as failed.
#[sqlx::test]
async fn private_servers_are_refused(pool: PgPool) {
    let f = setup(pool, "", false).await;
    let contact = contact(&f.api).await;
    let profile = profile(&f.api, 587).await;
    let mut body = message(&contact);
    body["email_profile_id"] = json!(profile);
    let (status, res) = send(&f, body).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res,
        json!({ "message": "The SMTP server isn't at a public address." })
    );
    assert_eq!(the_log(&f.api).await.0, "failed");
}

/// A server that never answers is cut off by the send limit: 503, logged as failed.
#[sqlx::test]
async fn a_silent_server_times_out(pool: PgPool) {
    let f = setup(pool, "", true).await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        let mut held = Vec::new();
        while let Ok((s, _)) = listener.accept().await {
            held.push(s);
        }
    });
    let contact = contact(&f.api).await;
    let mut body = message(&contact);
    body["email_profile_id"] = json!(profile(&f.api, port).await);
    let started = std::time::Instant::now();
    let (status, res) = send(&f, body).await;
    assert_eq!(
        (status, res),
        (
            StatusCode::SERVICE_UNAVAILABLE,
            json!({ "message": "The mail server didn't answer in time: check the send log before sending again." })
        )
    );
    assert!(started.elapsed() < Duration::from_secs(5));
    // Whether it left can't be known: `unknown`, never retried.
    let (status, .., error) = the_log(&f.api).await;
    assert_eq!(
        (status.as_str(), error.as_deref()),
        ("unknown", Some("timed out"))
    );
}

// --- the allow-list ------------------------------------------------------------------------

/// One recipient outside the allowed domains blocks the whole mail on every transport: the
/// caller gets ok (as Laravel), nothing leaves, the log says blocked.
#[sqlx::test]
async fn the_allow_list_covers_every_transport(pool: PgPool) {
    let f = setup(pool, "acme.test", true).await;
    let contact = contact(&f.api).await;
    let profile = profile(&f.api, f.smtp_port).await;
    let mut body = message(&contact);
    body["email_profile_id"] = json!(profile);
    body["bcc"] = json!(["spy@outside.test"]);
    let (status, res) = send(&f, body.clone()).await;
    assert_eq!((status, res), (StatusCode::OK, json!({ "message": "ok" })));
    assert!(f.inbox.lock().unwrap().is_empty(), "SMTP: nothing sent");
    assert_eq!(the_log(&f.api).await.0, "blocked");

    // Graph.
    sqlx::query("DELETE FROM email_send_logs")
        .execute(&f.api.pool)
        .await
        .unwrap();
    graph_settings(&f.api, "tenant-1").await;
    body.as_object_mut().unwrap().remove("email_profile_id");
    let (status, _) = send(&f, body.clone()).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        f.graph
            .lock()
            .unwrap()
            .iter()
            .all(|(p, ..)| !p.starts_with("/sendMail")),
        "Graph: nothing sent"
    );
    assert_eq!(the_log(&f.api).await.0, "blocked");

    // Platform mail (a signup's verification mail to an outside domain).
    common::signup(&f.api.app, "new@outside.test", "outside").await;
    assert!(
        f.outbox
            .lock()
            .unwrap()
            .iter()
            .all(|m| m.to != "new@outside.test"),
        "platform: nothing sent"
    );

    // Allowed recipients pass.
    body["bcc"] = json!([]);
    let (status, _) = send(&f, body).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        f.graph
            .lock()
            .unwrap()
            .iter()
            .any(|(p, ..)| p.starts_with("/sendMail"))
    );
}

// --- Graph --------------------------------------------------------------------------------

async fn graph_settings(api: &Api, tenant: &str) {
    let body = json!({ "integrations": { "mail": { "driver": "graph",
        "graph": { "tenant_id": tenant, "client_id": "client-1", "client_secret": "s3cret", "mailbox": "box@acme.test" },
        "outbound": { "send_enabled": true, "default_sender": { "email": "sales@acme.test", "sender_name": "Acme Sales" } } } } });
    let (status, res) = call(api, "/settings", Method::POST, "", Some(body)).await;
    assert_eq!(status, StatusCode::OK, "{res}");
}

#[sqlx::test]
async fn graph_sends_with_a_cached_token(pool: PgPool) {
    let f = setup(pool, "", true).await;
    graph_settings(&f.api, "tenant-1").await;
    let contact = contact(&f.api).await;
    let (_, file) = call(
        &f.api,
        "/files",
        Method::POST,
        "",
        Some(json!({ "upload": PDF_B64, "document_name": "Offer.pdf", "public": 1 })),
    )
    .await;
    let mut body = message(&contact);
    body["attachments"] = json!([file["data"]["file_id"]]);
    for _ in 0..2 {
        let (status, res) = send(&f, body.clone()).await;
        assert_eq!(status, StatusCode::OK, "{res}");
    }
    let calls = f.graph.lock().unwrap().clone();
    let tokens: Vec<_> = calls
        .iter()
        .filter(|(p, ..)| p.ends_with("/token"))
        .collect();
    assert_eq!(tokens.len(), 1, "the token is cached");
    assert!(
        tokens[0].2.contains("client_secret=s3cret")
            && tokens[0].2.contains("grant_type=client_credentials")
    );
    let sends: Vec<_> = calls
        .iter()
        .filter(|(p, ..)| p.starts_with("/sendMail"))
        .collect();
    assert_eq!(sends.len(), 2);
    let (path, auth, payload) = sends[0];
    assert_eq!(
        (path.as_str(), auth.as_str()),
        ("/sendMail/sales@acme.test", "Bearer tok-1")
    );
    let v: Value = serde_json::from_str(payload).unwrap();
    assert_eq!(v["saveToSentItems"], true);
    assert_eq!(v["message"]["subject"], "Your offer");
    assert_eq!(
        v["message"]["toRecipients"][0]["emailAddress"]["address"],
        "client@acme.test"
    );
    assert_eq!(
        v["message"]["bccRecipients"][0]["emailAddress"]["address"],
        "archive@acme.test"
    );
    assert_eq!(v["message"]["attachments"][0]["contentBytes"], PDF_B64);
    assert_eq!(
        v["message"]["attachments"][0]["contentType"],
        "application/pdf"
    );
    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT status, provider FROM email_send_logs")
            .fetch_all(&f.api.pool)
            .await
            .unwrap();
    assert!(
        rows.iter()
            .all(|(s, p)| s == "accepted" && p == "microsoft-graph")
    );
}

/// Graph attachments are capped at 3 MB (its request limit is 4 MB).
#[sqlx::test]
async fn graph_attachments_are_capped(pool: PgPool) {
    use base64::Engine;
    let f = setup(pool, "", true).await;
    graph_settings(&f.api, "tenant-1").await;
    let contact = contact(&f.api).await;
    let big = base64::engine::general_purpose::STANDARD
        .encode([b"%PDF-".as_slice(), &vec![0u8; 2 * 1024 * 1024]].concat());
    let (_, file) = call(
        &f.api,
        "/files",
        Method::POST,
        "",
        Some(json!({ "upload": big, "document_name": "Big.pdf", "public": 1 })),
    )
    .await;
    let mut body = message(&contact);
    body["attachments"] = json!([file["data"]["file_id"]]);
    let (status, res) = send(&f, body).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"]["attachments"][0],
        "The attachments must not be greater than 2048 kilobytes in total."
    );
}

// --- rules and access ----------------------------------------------------------------------

#[sqlx::test]
async fn rules(pool: PgPool) {
    let f = setup(pool, "", true).await;
    let contact = contact(&f.api).await;
    let with = |k: &str, v: Value| {
        let mut b = message(&contact);
        b[k] = v;
        b
    };
    let unknown = json!("01900000-0000-7000-8000-000000000001");
    let cases = [
        (
            with("addressed_contact_id", Value::Null),
            "addressed_contact_id",
            "The addressed contact id field is required.",
        ),
        (
            with("addressed_contact_id", unknown.clone()),
            "addressed_contact_id",
            "The selected addressed contact id is invalid.",
        ),
        (
            with("address", Value::Null),
            "address",
            "The address field is required.",
        ),
        (
            with("address", json!({ "address": "Main" })),
            "city",
            "The city field is required.",
        ),
        (
            with("send_method", json!("post")),
            "send_method",
            "The selected send method: post is invalid.",
        ),
        (
            with("subject", Value::Null),
            "subject",
            "The subject field is required when send method is email.",
        ),
        (
            with("subject", json!("x".repeat(101))),
            "subject",
            "The subject must not be greater than 100 characters.",
        ),
        (
            with("body", Value::Null),
            "body",
            "The body field is required when send method is email.",
        ),
        (
            with("to", json!(["nope"])),
            "to.0",
            "The to.0 must be a valid email address.",
        ),
        (
            with("cc", json!(["a@b.co", "nope"])),
            "cc.1",
            "The cc.1 must be a valid email address.",
        ),
        (
            with("attachments", json!([unknown])),
            "attachments.0",
            "The selected attachments.0 is invalid.",
        ),
        (
            with("email_profile_id", unknown),
            "email_profile_id",
            "The selected email profile id is invalid.",
        ),
    ];
    for (body, field, msg) in cases {
        let (status, res) = send(&f, body.clone()).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{field}: {res}");
        assert_eq!(res["errors"][field][0], msg, "{res}");
    }
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM email_send_logs")
        .fetch_one(&f.api.pool)
        .await
        .unwrap();
    assert_eq!(n, 0);
}

/// `manual` records the sending elsewhere: nothing goes out.
#[sqlx::test]
async fn manual_sends_nothing(pool: PgPool) {
    let f = setup(pool, "", true).await;
    let contact = contact(&f.api).await;
    let mut body = message(&contact);
    body["send_method"] = json!("manual");
    body["subject"] = Value::Null;
    let (status, res) = send(&f, body).await;
    assert_eq!((status, res), (StatusCode::OK, json!({ "message": "ok" })));
    assert!(f.inbox.lock().unwrap().is_empty());
}

#[sqlx::test]
async fn access_rules(pool: PgPool) {
    let f = setup(pool, "", true).await;
    let contact = contact(&f.api).await;
    let bob = member_with(&f.api.app, &f.api.pool, &f.api.ws, "bob@example.com", &[]).await;
    let (status, _) = f
        .api
        .call(Method::POST, "", Some(message(&contact)), &bob)
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    // Another workspace's contact and profile are unknown here.
    let (eve, eve_ws) = common::signup(&f.api.app, "eve@example.com", "evil").await;
    let uri = format!("/v1/workspace/{eve_ws}/send-message");
    let (status, res) = common::send(
        &f.api.app,
        Method::POST,
        &uri,
        Some(message(&contact)),
        Some(&eve),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"]["addressed_contact_id"][0],
        "The selected addressed contact id is invalid."
    );
}

// --- logs ----------------------------------------------------------------------------------

#[sqlx::test]
async fn the_send_logs(pool: PgPool) {
    let f = setup(pool, "", true).await;
    let contact = contact(&f.api).await;
    let mut body = message(&contact);
    body["email_profile_id"] = json!(profile(&f.api, f.smtp_port).await);
    send(&f, body).await;
    let (status, res) = call(&f.api, "/settings", Method::GET, "/email-logs", None).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    let row = &res["data"][0];
    assert_eq!(row["subject"], "Your offer");
    assert_eq!(row["status"], "accepted");
    assert_eq!(row["provider"], "smtp");
    assert_eq!(row["mailer"], f.api.ws);
    assert_eq!(row["recipient_count"], 3);
    assert_eq!(row["opened_count"], 0);
    assert_eq!(row["to_recipients"], json!(["client@acme.test"]));
    assert_eq!(
        row["recipients_summary"],
        "client@acme.test, boss@acme.test, archive@acme.test"
    );
    assert!(row["sent_at"].is_string() && row["delivered_at"].is_null());
    assert_eq!(res["meta"]["total"], 1);
    // Another workspace sees none.
    let (eve, eve_ws) = common::signup(&f.api.app, "eve@example.com", "evil").await;
    let uri = format!("/v1/workspace/{eve_ws}/settings/email-logs");
    let (_, res) = common::send(&f.api.app, Method::GET, &uri, None, Some(&eve)).await;
    assert_eq!(res["meta"]["total"], 0);
}

/// A send a dead request left `pending` or `sending` for 10 minutes becomes `unknown`.
#[sqlx::test]
async fn the_sweep_closes_stuck_sends(pool: PgPool) {
    let api = Api::new(pool, "/send-message").await;
    for (status, age) in [
        ("pending", 11),
        ("sending", 11),
        ("pending", 1),
        ("accepted", 60),
    ] {
        sqlx::query(
            "INSERT INTO email_send_logs (id, workspace_id, provider, from_email, subject, status, updated_at)
             VALUES (gen_random_uuid(), $1::uuid, 'smtp', 'a@b.co', 's', $2, now() - make_interval(mins => $3))",
        )
        .bind(&api.ws)
        .bind(status)
        .bind(age)
        .execute(&api.pool)
        .await
        .unwrap();
    }
    cleanup::prune(&api.pool).await.unwrap();
    let left: Vec<String> =
        sqlx::query_scalar("SELECT status FROM email_send_logs ORDER BY status")
            .fetch_all(&api.pool)
            .await
            .unwrap();
    assert_eq!(left, ["accepted", "pending", "unknown", "unknown"]);
}

// --- connection tests ----------------------------------------------------------------------

#[sqlx::test]
async fn a_profile_test_mails_the_caller(pool: PgPool) {
    let f = setup(pool, "", true).await;
    let id = profile(&f.api, f.smtp_port).await;
    let (status, res) = call(
        &f.api,
        "/email-profiles",
        Method::GET,
        &format!("/{id}/test-connection"),
        None,
    )
    .await;
    assert_eq!(
        (status, res),
        (
            StatusCode::OK,
            json!({ "data": { "success_connection": true } })
        )
    );
    let inbox = f.inbox.lock().unwrap().clone();
    assert_eq!(inbox[0].rcpt, ["<ada@example.com>"]);
    assert!(
        inbox[0]
            .data
            .contains("Subject: Test email profile connection")
    );
    // It needs emailprofile.update (Laravel checked ftpserver.view).
    let viewer = member_with(
        &f.api.app,
        &f.api.pool,
        &f.api.ws,
        "vic@example.com",
        &["emailprofile.get"],
    )
    .await;
    let uri = format!(
        "/v1/workspace/{}/email-profiles/{id}/test-connection",
        f.api.ws
    );
    let (status, _) = common::send(&f.api.app, Method::GET, &uri, None, Some(&viewer)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[sqlx::test]
async fn office365_connection_tests(pool: PgPool) {
    let f = setup(pool, "", true).await;
    let graph = |tenant: &str, secret: &str| json!({ "graph": { "tenant_id": tenant, "client_id": "client-1", "client_secret": secret, "mailbox": "box@acme.test" } });
    let (status, res) = call(
        &f.api,
        "/settings",
        Method::POST,
        "/office365/test-connection",
        Some(graph("tenant-1", "s3cret")),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(
        res["data"],
        json!({ "ok": true, "stage": "mailbox", "message": "Connection successful. Mailbox is reachable and the Mail.ReadWrite permission is granted.", "messages_with_attachments": 1 })
    );
    let (status, res) = call(
        &f.api,
        "/settings",
        Method::POST,
        "/office365/test-connection",
        Some(graph("bad", "s3cret")),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["data"],
        json!({ "ok": false, "stage": "token", "message": "Could not authenticate. Check the tenant, client id and client secret." })
    );

    // The mask uses the stored secret.
    graph_settings(&f.api, "tenant-2").await;
    let mut body = graph("tenant-2", "********");
    body["to"] = json!("it@acme.test");
    let (status, res) = call(
        &f.api,
        "/settings",
        Method::POST,
        "/office365/test-send",
        Some(body),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(
        res["data"]["message"],
        "Test e-mail sent to it@acme.test from box@acme.test. Check the inbox and the sender mailbox Sent Items."
    );
    let calls = f.graph.lock().unwrap().clone();
    assert!(
        calls
            .iter()
            .any(|(p, _, b)| p == "/tenant-2/token" && b.contains("client_secret=s3cret"))
    );
    assert!(calls.iter().any(|(p, ..)| p == "/sendMail/box@acme.test"));
}
