//! POST /v1/create-workspace: new user + new workspace + token, all or nothing.

use crate::common;

use axum::http::StatusCode;
use common::{app, post};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

const URI: &str = "/v1/create-workspace";

fn body() -> Value {
    json!({
        "first_name": "Ada",
        "last_name": "Lovelace",
        "email": "ada@example.com",
        "password": "Str0ng!pass",
        "workspace_name": "acme",
    })
}

fn with(mut base: Value, key: &str, value: Value) -> Value {
    base[key] = value;
    base
}

async fn count(pool: &PgPool, sql: &'static str) -> i64 {
    sqlx::query_scalar(sql).fetch_one(pool).await.unwrap()
}

// ---- happy path

#[sqlx::test]
async fn creates_user_workspace_membership_and_token(pool: PgPool) {
    let (status, res) = post(&app(pool.clone()), URI, body()).await;

    assert_eq!(status, StatusCode::CREATED, "{res}");
    let data = &res["data"];
    assert_eq!(data["token_type"], "Bearer");
    let token = data["access_token"].as_str().unwrap();
    assert_eq!(token.len(), 64);
    let ws: Uuid = data["workspace_ids"][0].as_str().unwrap().parse().unwrap();
    assert_eq!(data["workspace_ids"].as_array().unwrap().len(), 1);

    let (owner, ws_type): (Uuid, String) = sqlx::query_as(
        "SELECT created_by_user_id, workspace_type FROM workspaces WHERE id = $1 AND name = 'acme'",
    )
    .bind(ws)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(ws_type, "retail", "default type");
    let (email, hash): (String, String) =
        sqlx::query_as("SELECT email, password_hash FROM users WHERE id = $1")
            .bind(owner)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(email, "ada@example.com");
    assert!(hash.starts_with("$argon2id$"), "password stored hashed");
    let active: bool = sqlx::query_scalar(
        "SELECT active FROM workspace_members WHERE workspace_id = $1 AND user_id = $2",
    )
    .bind(ws)
    .bind(owner)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(active);
    let stored: Vec<u8> =
        sqlx::query_scalar("SELECT token_hash FROM access_tokens WHERE user_id = $1")
            .bind(owner)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        stored,
        Sha256::digest(token.as_bytes()).to_vec(),
        "only sha256 stored"
    );
}

#[sqlx::test]
async fn explicit_manufacturing_type_and_middle_name_stored(pool: PgPool) {
    let req = with(
        with(body(), "workspace_type", json!("manufacturing")),
        "middle_name",
        json!("King"),
    );

    let (status, _) = post(&app(pool.clone()), URI, req).await;

    assert_eq!(status, StatusCode::CREATED);
    let ty: String = sqlx::query_scalar("SELECT workspace_type FROM workspaces")
        .fetch_one(&pool)
        .await
        .unwrap();
    let middle: Option<String> = sqlx::query_scalar("SELECT middle_name FROM users")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(ty, "manufacturing");
    assert_eq!(middle.as_deref(), Some("King"));
}

#[sqlx::test]
async fn frontend_nulls_and_extra_fields_accepted(pool: PgPool) {
    // vimdesk-web sends unset form fields as null and may send `meta`.
    let req = with(
        with(
            with(body(), "workspace_type", Value::Null),
            "middle_name",
            Value::Null,
        ),
        "meta",
        json!({ "source": "web" }),
    );

    assert_eq!(post(&app(pool), URI, req).await.0, StatusCode::CREATED);
}

#[sqlx::test]
async fn text_fields_trimmed(pool: PgPool) {
    let req = with(
        with(body(), "email", json!("  ada@example.com ")),
        "first_name",
        json!(" Ada "),
    );

    assert_eq!(
        post(&app(pool.clone()), URI, req).await.0,
        StatusCode::CREATED
    );
    let (email, first): (String, String) = sqlx::query_as("SELECT email, first_name FROM users")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!((email.as_str(), first.as_str()), ("ada@example.com", "Ada"));
}

// ---- errors

#[sqlx::test]
async fn empty_body_lists_every_required_field(pool: PgPool) {
    let (status, res) = post(&app(pool), URI, json!({})).await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let mut fields: Vec<_> = res["errors"].as_object().unwrap().keys().cloned().collect();
    fields.sort();
    assert_eq!(
        fields,
        [
            "email",
            "first_name",
            "last_name",
            "password",
            "workspace_name"
        ]
    );
    assert_eq!(
        res["errors"]["first_name"],
        json!(["The first name field is required."])
    );
}

#[sqlx::test]
async fn weak_password_lists_every_failed_rule(pool: PgPool) {
    let (status, res) = post(&app(pool), URI, with(body(), "password", json!("weak"))).await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(res["errors"]["password"].as_array().unwrap().len(), 4);
    assert_eq!(
        res["errors"]["password"][0],
        "The password must be at least 8 characters."
    );
}

#[sqlx::test]
async fn email_taken_ignoring_case(pool: PgPool) {
    let app = app(pool);
    post(&app, URI, body()).await;

    let req = with(
        with(body(), "email", json!("ADA@Example.com")),
        "workspace_name",
        json!("other"),
    );
    let (status, res) = post(&app, URI, req).await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"],
        json!({ "email": ["The email has already been taken."] })
    );
}

#[sqlx::test]
async fn workspace_name_taken(pool: PgPool) {
    let app = app(pool);
    post(&app, URI, body()).await;

    let (status, res) = post(&app, URI, with(body(), "email", json!("bob@example.com"))).await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"],
        json!({ "workspace_name": ["The workspace name has already been taken."] })
    );
}

#[sqlx::test]
async fn both_taken_reported_together(pool: PgPool) {
    let app = app(pool);
    post(&app, URI, body()).await;

    let (_, res) = post(&app, URI, body()).await;

    assert_eq!(
        res["message"],
        "The email has already been taken. (and 1 more error)"
    );
}

#[sqlx::test]
async fn bad_email_and_workspace_type_rejected(pool: PgPool) {
    let req = with(
        with(body(), "email", json!("not-an-email")),
        "workspace_type",
        json!("bakery"),
    );

    let (_, res) = post(&app(pool), URI, req).await;

    assert_eq!(
        res["errors"]["email"],
        json!(["The email must be a valid email address."])
    );
    assert_eq!(
        res["errors"]["workspace_type"],
        json!(["The selected workspace type is invalid."])
    );
}

#[sqlx::test]
async fn workspace_name_must_be_lowercase_slug(pool: PgPool) {
    let app = app(pool);
    for bad in [
        "Acme",
        "acme shop",
        "-acme",
        "acme-",
        "acme_shop",
        "acme--shop",
    ] {
        let (status, res) = post(&app, URI, with(body(), "workspace_name", json!(bad))).await;

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "accepted {bad:?}");
        assert_eq!(
            res["errors"]["workspace_name"],
            json!([
                "The workspace name field must be lowercase letters, numbers and single dashes."
            ]),
            "{bad:?}"
        );
    }
}

#[sqlx::test]
async fn seeds_admin_team_with_owner(pool: PgPool) {
    let (_, res) = post(&app(pool.clone()), URI, body()).await;
    let ws: Uuid = res["data"]["workspace_ids"][0]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();

    // Laravel WorkspaceSetup::teamAttributes, owner added right after.
    let (name, description, perms, members): (String, String, Vec<String>, Vec<String>) =
        sqlx::query_as(
            "SELECT t.name, t.description,
                    ARRAY(SELECT permission FROM team_permissions WHERE team_id = t.id),
                    ARRAY(SELECT u.email FROM team_members m JOIN users u ON u.id = m.user_id
                          WHERE m.team_id = t.id)
             FROM teams t WHERE t.workspace_id = $1",
        )
        .bind(ws)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(name, "Admin Team");
    assert_eq!(description, "This team consist of admins of the workspace");
    assert_eq!(perms, ["workspace_admin"]);
    assert_eq!(members, ["ada@example.com"]);
}

#[sqlx::test]
async fn failed_signup_writes_nothing(pool: PgPool) {
    let app = app(pool.clone());
    post(&app, URI, body()).await;

    post(&app, URI, with(body(), "email", json!("bob@example.com"))).await; // name taken

    assert_eq!(count(&pool, "SELECT count(*) FROM users").await, 1);
    assert_eq!(count(&pool, "SELECT count(*) FROM access_tokens").await, 1);
    assert_eq!(count(&pool, "SELECT count(*) FROM teams").await, 1);
}

// ---- edge / boundary

#[sqlx::test]
async fn name_lengths_boundary_55_chars(pool: PgPool) {
    let app = app(pool);
    let at = "é".repeat(55);
    let over = "é".repeat(56);
    let slug_at = "a".repeat(55);
    let slug_over = "a".repeat(56);

    let ok = with(
        with(
            with(body(), "first_name", json!(at)),
            "last_name",
            json!(at),
        ),
        "workspace_name",
        json!(slug_at),
    );
    assert_eq!(post(&app, URI, ok).await.0, StatusCode::CREATED);

    let bad = with(
        with(
            with(body(), "first_name", json!(over)),
            "middle_name",
            json!(over),
        ),
        "workspace_name",
        json!(slug_over),
    );
    let (status, res) = post(&app, URI, with(bad, "email", json!("bob@example.com"))).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    for field in ["first_name", "middle_name", "workspace_name"] {
        assert!(
            res["errors"][field].is_array(),
            "{field} not rejected: {res}"
        );
    }
}

#[sqlx::test]
async fn blank_optional_fields_treated_as_absent(pool: PgPool) {
    let req = with(
        with(body(), "middle_name", json!("  ")),
        "workspace_type",
        json!(""),
    );

    assert_eq!(
        post(&app(pool.clone()), URI, req).await.0,
        StatusCode::CREATED
    );
    let middle: Option<String> = sqlx::query_scalar("SELECT middle_name FROM users")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(middle, None);
}

// ---- rare: concurrent signups racing for the same email

#[sqlx::test]
async fn concurrent_same_email_one_wins_other_gets_422(pool: PgPool) {
    let app = app(pool.clone());
    let a = post(&app, URI, body());
    let b = post(&app, URI, with(body(), "workspace_name", json!("other")));

    let (ra, rb) = tokio::join!(a, b);

    let mut statuses = [ra.0, rb.0];
    statuses.sort();
    assert_eq!(
        statuses,
        [StatusCode::CREATED, StatusCode::UNPROCESSABLE_ENTITY],
        "{ra:?} {rb:?}"
    );
    let loser = if ra.0 == StatusCode::CREATED {
        rb.1
    } else {
        ra.1
    };
    assert_eq!(
        loser["errors"]["email"],
        json!(["The email has already been taken."])
    );
    assert_eq!(count(&pool, "SELECT count(*) FROM users").await, 1);
}
