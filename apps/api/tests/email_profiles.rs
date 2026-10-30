//! /v1/workspace/{ws}/email-profiles (final batch F5b): CRUD; the SMTP password is sealed
//! like the workspace credentials (F2b) and printed masked.

mod common;

use axum::http::{Method, StatusCode};
use common::Api;
use genesy_360_api::{build_app, mail::Mailer};
use serde_json::{Value, json};
use sqlx::PgPool;

const MASK: &str = "********";

async fn api(pool: PgPool) -> Api {
    Api::new(pool, "/email-profiles").await
}

fn profile(name: &str, password: Value) -> Value {
    json!({
        "name": name, "sender_name": "Acme Sales", "email": "sales@acme.test", "default": true,
        "html": "<p>Kind regards</p>",
        "smtp": { "host": "smtp.acme.test", "username": "sales", "password": password, "port": 587,
                  "encryption": "tls", "bcc_all_emails": true, "from_name": "dropped", "from_address": "x@y.z" },
        "conditions": { "language": "nl" },
    })
}

async fn post(api: &Api, path: &str, body: Value) -> (StatusCode, Value) {
    api.call(Method::POST, path, Some(body), &api.owner).await
}

/// (sealed bytes, key id) of a profile's stored password.
async fn sealed(api: &Api, id: &str) -> (Option<Vec<u8>>, Option<String>) {
    sqlx::query_as("SELECT smtp_password, smtp_key_id FROM email_profiles WHERE id = $1::uuid")
        .bind(id)
        .fetch_one(&api.pool)
        .await
        .unwrap()
}

#[sqlx::test]
async fn create_get_list_delete(pool: PgPool) {
    let api = api(pool).await;
    let id = api
        .create(profile("Sales", json!("s3cret-pass")), "email_profile_id")
        .await;
    let p = api.get(&id).await;
    assert_eq!(p["id"], id);
    assert_eq!(
        (
            p["name"].as_str(),
            p["sender_name"].as_str(),
            p["email"].as_str()
        ),
        (Some("Sales"), Some("Acme Sales"), Some("sales@acme.test"))
    );
    assert_eq!(p["default"], true);
    assert_eq!(p["html"], "<p>Kind regards</p>");
    assert_eq!(
        p["smtp"],
        json!({ "host": "smtp.acme.test", "username": "sales", "password": MASK, "port": 587, "encryption": "tls",
                "from_name": null, "from_address": null, "bcc_all_emails": true }),
        "from_* are dropped as in Laravel; the password is masked"
    );
    assert_eq!(p["conditions"]["language"], "nl");
    assert_eq!(p["meta"], Value::Null);

    // Sealed at rest: no plaintext in the row, bound to this key.
    let (bytes, key_id) = sealed(&api, &id).await;
    let bytes = bytes.unwrap();
    assert!(!bytes.windows(10).any(|w| w == b"s3cret-pas"));
    assert_eq!(key_id.unwrap().len(), 16);

    let list = api.list("").await;
    let row = &list["data"][0];
    assert_eq!(row["html"], "<p>Kind regards</p>");
    assert!(row.get("smtp").is_none(), "the list has no smtp");

    // Without SMTP every key is null.
    let mut bare = profile("Bare", Value::Null);
    bare["smtp"] = Value::Null;
    let bare = api.create(bare, "email_profile_id").await;
    assert_eq!(api.get(&bare).await["smtp"]["host"], Value::Null);
    assert_eq!(sealed(&api, &bare).await, (None, None));

    api.delete(&id).await;
    let (status, _) = api
        .call(Method::GET, &format!("/{id}"), None, &api.owner)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

/// The mask keeps the stored password while host, port, username and encryption stay; a new
/// value replaces it, null clears it, and the mask can't follow a changed server.
#[sqlx::test]
async fn the_password_mask(pool: PgPool) {
    let api = api(pool).await;
    let id = api
        .create(profile("Sales", json!("first")), "email_profile_id")
        .await;
    let first = sealed(&api, &id).await.0;

    let (status, res) = post(&api, &format!("/{id}"), profile("Sales 2", json!(MASK))).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(sealed(&api, &id).await.0, first, "kept byte for byte");
    assert_eq!(api.get(&id).await["name"], "Sales 2");

    for (field, value) in [
        ("host", json!("evil.test")),
        ("port", json!(25)),
        ("username", json!("other")),
        ("encryption", json!("ssl")),
    ] {
        let mut body = profile("Sales", json!(MASK));
        body["smtp"][field] = value;
        let (status, res) = post(&api, &format!("/{id}"), body).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{field}: {res}");
        assert_eq!(
            res["errors"]["smtp.password"][0],
            "Enter this credential again."
        );
    }
    assert_eq!(sealed(&api, &id).await.0, first);

    let (status, _) = post(&api, &format!("/{id}"), profile("Sales", json!("second"))).await;
    assert_eq!(status, StatusCode::OK);
    let second = sealed(&api, &id).await.0;
    assert!(second.is_some() && second != first);

    let (status, _) = post(&api, &format!("/{id}"), profile("Sales", Value::Null)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(sealed(&api, &id).await, (None, None));
    assert_eq!(api.get(&id).await["smtp"]["password"], Value::Null);

    // The mask with nothing stored (and on create) can't keep anything.
    let (status, res) = post(&api, &format!("/{id}"), profile("Sales", json!(MASK))).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    let (status, res) = post(&api, "", profile("New", json!(MASK))).await;
    assert_eq!(
        (status, res["errors"]["smtp.password"][0].as_str()),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Some("Enter this credential again.")
        )
    );
}

#[sqlx::test]
async fn without_a_key_passwords_cant_be_stored(pool: PgPool) {
    let mut api = api(pool.clone()).await;
    let mut state = common::state(pool, Mailer::capture().0);
    state.credentials = None;
    api.app = build_app(state);
    let (status, res) = post(&api, "", profile("Sales", json!("pw"))).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"]["smtp.password"][0],
        "Credential storage isn't configured."
    );
    let (status, res) = post(&api, "", profile("Sales", Value::Null)).await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
}

#[sqlx::test]
async fn rules(pool: PgPool) {
    let api = api(pool).await;
    let with = |path: &[&str], v: Value| {
        let mut b = profile("A", json!("pw"));
        match path {
            [k] => b[*k] = v,
            [k, sub] => b[*k][*sub] = v,
            _ => unreachable!(),
        }
        b
    };
    let cases = [
        (
            with(&["name"], Value::Null),
            "name",
            "The name field is required.",
        ),
        (
            with(&["name"], json!("x".repeat(81))),
            "name",
            "The name must not be greater than 80 characters.",
        ),
        (
            with(&["sender_name"], Value::Null),
            "sender_name",
            "The sender name field is required.",
        ),
        (
            with(&["email"], json!("nope")),
            "email",
            "The email must be a valid email address.",
        ),
        (
            with(&["default"], Value::Null),
            "default",
            "The default field is required.",
        ),
        (
            with(&["html"], json!("x".repeat(25_001))),
            "html",
            "The html must not be greater than 25000 characters.",
        ),
        (
            with(&["smtp"], json!("x")),
            "smtp",
            "The smtp must be an array.",
        ),
        (
            with(&["smtp", "port"], json!("abc")),
            "smtp.port",
            "The smtp.port must be an integer.",
        ),
        (
            with(&["smtp", "encryption"], json!("starttls")),
            "smtp.encryption",
            "Invalid smtp.encryption encryption type.",
        ),
        (
            with(&["smtp", "host"], json!("x".repeat(256))),
            "smtp.host",
            "The smtp.host must not be greater than 255 characters.",
        ),
        (
            with(&["smtp", "host"], json!("smtp acme.test")),
            "smtp.host",
            "The smtp.host must be a valid host name.",
        ),
        (
            with(&["smtp", "host"], json!("-bad-.test")),
            "smtp.host",
            "The smtp.host must be a valid host name.",
        ),
        (
            with(&["smtp", "host"], json!("169.254.169.254")),
            "smtp.host",
            "The smtp.host must be a public address.",
        ),
        (
            with(&["smtp", "host"], json!("[::1]")),
            "smtp.host",
            "The smtp.host must be a public address.",
        ),
        (
            with(&["smtp", "host"], Value::Null),
            "smtp.host",
            "The smtp.host field is required when smtp.password is present.",
        ),
        (
            with(&["smtp", "bcc_all_emails"], json!("maybe")),
            "smtp.bcc_all_emails",
            "The smtp.bcc all emails field must be true or false.",
        ),
        (
            with(&["conditions"], json!({ "relation_type": "alien" })),
            "conditions.relation_type",
            "The selected conditions.relation type is invalid.",
        ),
    ];
    for (body, field, msg) in cases {
        let (status, res) = post(&api, "", body.clone()).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}: {res}");
        assert_eq!(res["errors"][field][0], msg, "{res}");
    }
}

/// Hosts are stored lowercase without a trailing dot, so the mask compares them as names.
#[sqlx::test]
async fn hosts_are_normalised(pool: PgPool) {
    let api = api(pool).await;
    let mut body = profile("Sales", json!("pw"));
    body["smtp"]["host"] = json!("SMTP.Acme.TEST.");
    let id = api.create(body.clone(), "email_profile_id").await;
    assert_eq!(api.get(&id).await["smtp"]["host"], "smtp.acme.test");
    body["smtp"]["password"] = json!(MASK);
    body["smtp"]["host"] = json!("smtp.acme.test");
    let (status, res) = post(&api, &format!("/{id}"), body).await;
    assert_eq!(status, StatusCode::OK, "{res}");
}

#[sqlx::test]
async fn list_filters(pool: PgPool) {
    let api = api(pool).await;
    api.create(profile("NL", json!("pw")), "email_profile_id")
        .await;
    let mut any = profile("Any", Value::Null);
    any["conditions"] = Value::Null;
    api.create(any, "email_profile_id").await;
    let names = |v: &Value| -> Vec<String> {
        v["data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["name"].as_str().unwrap().to_owned())
            .collect()
    };
    assert_eq!(
        names(&api.list("").await),
        ["NL", "Any"],
        "oldest first, as Laravel's unordered list"
    );
    assert_eq!(names(&api.list("?language=en").await), ["Any"]);
    assert_eq!(names(&api.list("?language=nl").await), ["NL", "Any"]);
}

#[sqlx::test]
async fn access_rules(pool: PgPool) {
    let api = api(pool).await;
    let body = profile("A", Value::Null);
    let id = api.create(body.clone(), "email_profile_id").await;
    api.assert_member_forbidden(&id).await;
    let perms = [
        "emailprofile.list",
        "emailprofile.get",
        "emailprofile.create",
        "emailprofile.update",
        "emailprofile.delete",
    ];
    api.assert_granted(&perms, body.clone(), "email_profile_id")
        .await;
    api.assert_foreign_404(&id, body.clone()).await;
    api.delete(&id).await;
    api.assert_bad_ids_404(&id, body).await;
}
