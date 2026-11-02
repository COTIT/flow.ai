//! Farm bug reports (RUAHA `bug_report`): anyone with a farm role files a report with a
//! screenshot or photo under their own name; only farm admin reads; nothing edits or deletes
//! one. RUAHA ids in docs/ruaha-rls-mapping.md.

use crate::common;

use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
};
use common::{farm::Farm, send};
use genesy_360_api::{build_app, mail::Mailer, storage::Store};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;

const PNG: &[u8] = b"\x89PNG\r\n\x1a\n0000IHDR";
const PDF: &[u8] = b"%PDF-1.7\n1 0 obj\n<<>>\nendobj\ntrailer\n%%EOF\n";

async fn setup(pool: PgPool) -> (Farm, Store) {
    let mut f = Farm::new(pool.clone()).await;
    let store = Store::memory();
    let mut state = common::state(pool, Mailer::capture().0);
    state.files = Some(store.clone());
    f.app = build_app(state);
    (f, store)
}

/// One multipart part: name, file name (a file part) and bytes.
type Part<'a> = (&'a str, Option<&'a str>, &'a [u8]);

async fn file(f: &Farm, token: &str, parts: &[Part<'_>]) -> (StatusCode, Value) {
    let boundary = "bugBoundary";
    let mut body = Vec::new();
    for (name, file, data) in parts {
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        let head = match file {
            Some(n) => format!(
                "Content-Disposition: form-data; name=\"{name}\"; filename=\"{n}\"\r\nContent-Type: image/png\r\n\r\n"
            ),
            None => format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n"),
        };
        body.extend_from_slice(head.as_bytes());
        body.extend_from_slice(data);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    let req = Request::post(f.url("/bug-reports"))
        .header("authorization", format!("Bearer {token}"))
        .header(
            "content-type",
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(Body::from(body))
        .unwrap();
    let res = f.app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    common::contract::check(&Method::POST, &f.url("/bug-reports"), status, &value);
    (status, value)
}

const ACTIONS: &str = r#"[{"at":"2026-10-25T08:00:00Z","what":"tap","target":"Save"}]"#;

fn report<'a>(extra: &'a [Part<'a>]) -> Vec<Part<'a>> {
    let mut parts: Vec<Part<'a>> = vec![
        ("route", None, b"/farmer/pue/new"),
        ("user_agent", None, b"Mozilla/5.0 (Linux; Android 14)"),
        ("viewport", None, b"375x812"),
        ("description", None, b"The save button does nothing"),
        ("actions", None, ACTIONS.as_bytes()),
    ];
    parts.extend_from_slice(extra);
    parts
}

#[sqlx::test]
async fn a_farmer_files_a_report_under_their_own_name(pool: PgPool) {
    let (f, store) = setup(pool).await;
    let neema = f.farmer("neema@ruaha.test", &f.ilundo).await;
    let (status, res) = file(&f, &neema, &report(&[("screenshot", Some("s.png"), PNG)])).await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let id = res["data"]["id"].as_str().unwrap().to_owned();
    assert!(res["data"]["ref"].as_i64().unwrap() >= 1, "{res}");
    let (reporter, status, key): (String, String, String) = sqlx::query_as(
        "SELECT reporter_id::text, status, screenshot_key FROM farm_bug_reports WHERE id = $1::uuid",
    )
    .bind(&id)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(reporter, f.user_id("neema@ruaha.test").await);
    assert_eq!(status, "open");
    // The image sits in the reporter's own folder, whatever the client sends.
    assert!(key.contains(&format!("/bug-reports/{reporter}/")), "{key}");
    assert_eq!(store.read(&key).unwrap().0, PNG);
}

#[sqlx::test]
async fn a_report_cannot_be_filed_in_someone_elses_name(pool: PgPool) {
    let (f, _) = setup(pool).await;
    let ops = f.user_id("ops@ruaha.test").await;
    let (status, res) = file(
        &f,
        &f.officer_ilu,
        &report(&[
            ("screenshot", Some("s.png"), PNG),
            ("reporter_id", None, ops.as_bytes()),
        ]),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM farm_bug_reports")
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(n, 0);
}

#[sqlx::test]
async fn a_report_needs_an_image_a_description_and_at_most_30_actions(pool: PgPool) {
    let (f, _) = setup(pool).await;
    let (status, res) = file(&f, &f.ops, &report(&[])).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert!(res["errors"]["screenshot"].is_array(), "{res}");
    let blank: Vec<Part> = vec![
        ("route", None, b"/x"),
        ("user_agent", None, b"ua"),
        ("viewport", None, b"1x1"),
        ("description", None, b"   "),
        ("actions", None, b"[]"),
        ("photo", Some("p.png"), PNG),
    ];
    let (status, res) = file(&f, &f.ops, &blank).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert!(res["errors"]["description"].is_array(), "{res}");
    let many = format!("[{}]", vec!["{}"; 31].join(","));
    let too_many: Vec<Part> = vec![
        ("route", None, b"/x"),
        ("user_agent", None, b"ua"),
        ("viewport", None, b"1x1"),
        ("description", None, b"broken"),
        ("actions", None, many.as_bytes()),
        ("photo", Some("p.png"), PNG),
    ];
    let (status, res) = file(&f, &f.ops, &too_many).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert!(res["errors"]["actions"].is_array(), "{res}");
    // Images only: a PDF is refused.
    let (status, res) = file(&f, &f.ops, &report(&[("photo", Some("p.pdf"), PDF)])).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert!(res["errors"]["photo"].is_array(), "{res}");
}

#[sqlx::test]
async fn only_farm_admin_reads_reports_and_their_images(pool: PgPool) {
    let (f, _) = setup(pool).await;
    let neema = f.farmer("neema@ruaha.test", &f.ilundo).await;
    let (_, res) = file(&f, &neema, &report(&[("photo", Some("p.png"), PNG)])).await;
    let id = res["data"]["id"].as_str().unwrap().to_owned();
    for token in [&neema, &f.officer_ilu, &f.ops] {
        let (status, _) = f.call(Method::GET, "/bug-reports", None, token).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        let (status, _) = f
            .call(Method::GET, &format!("/bug-reports/{id}"), None, token)
            .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }
    let list = f.list("/bug-reports", &f.owner).await;
    assert_eq!(list["meta"]["total"], 1, "{list}");
    let (status, res) = f
        .call(Method::GET, &format!("/bug-reports/{id}"), None, &f.owner)
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    let r = &res["data"];
    assert_eq!(r["description"], "The save button does nothing");
    assert_eq!(r["status"], "open");
    assert_eq!(r["reporter"]["id"], f.user_id("neema@ruaha.test").await);
    assert_eq!(
        r["actions"],
        serde_json::from_str::<Value>(ACTIONS).unwrap()
    );
    assert_eq!(r["screenshot_url"], Value::Null);
    assert!(
        r["photo_url"].as_str().unwrap().contains("/bug-reports/"),
        "{res}"
    );
    let (status, _) = send(&f.app, Method::GET, &f.url("/bug-reports"), None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn no_route_changes_or_deletes_a_report(pool: PgPool) {
    let (f, _) = setup(pool).await;
    let (_, res) = file(&f, &f.ops, &report(&[("photo", Some("p.png"), PNG)])).await;
    let id = res["data"]["id"].as_str().unwrap().to_owned();
    for method in [Method::PATCH, Method::PUT, Method::DELETE] {
        let (status, _) = send(
            &f.app,
            method.clone(),
            &f.url(&format!("/bug-reports/{id}")),
            Some(json!({ "status": "fixed" })),
            Some(&f.owner),
        )
        .await;
        assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED, "{method}");
    }
    let (_, res) = f
        .call(Method::GET, &format!("/bug-reports/{id}"), None, &f.owner)
        .await;
    assert_eq!(res["data"]["status"], "open");
}

#[sqlx::test]
async fn a_retried_report_is_filed_once(pool: PgPool) {
    let (f, _) = setup(pool).await;
    let id = "019a0000-0000-7000-8000-0000000000bb";
    let extra = [("id", None, id.as_bytes()), ("photo", Some("p.png"), PNG)];
    let parts = report(&extra);
    let (status, _) = file(&f, &f.ops, &parts).await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, res) = file(&f, &f.ops, &parts).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res["data"]["id"], id);
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM farm_bug_reports")
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(n, 1);
}
