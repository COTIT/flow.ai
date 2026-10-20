//! Invite flow: owner sends, invitee checks status, signs up with code or accepts while logged in.

mod common;

use axum::{
    Router,
    http::{Method, StatusCode},
};
use common::{FRONTEND_URL, PASSWORD, app_with_mail, post, send, signup};
use genesy_360_api::{build_app, mail::Mailer};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::PgPool;

const NOT_FOUND: &str = "The requested resource could not be found.";

struct World {
    app: Router,
    outbox: genesy_360_api::mail::Outbox,
    pool: PgPool,
    owner_token: String,
    ws: String,
}

/// Ada owns workspace `acme`.
async fn world(pool: PgPool) -> World {
    let (app, outbox) = app_with_mail(pool.clone());
    let (owner_token, ws) = signup(&app, "ada@example.com", "acme").await;
    common::verify_email(&pool, "ada@example.com").await; // inviting needs a verified owner
    World {
        app,
        outbox,
        pool,
        owner_token,
        ws,
    }
}

impl World {
    async fn invite_as(&self, token: &str, ws: &str, email: &str) -> (StatusCode, Value) {
        send(
            &self.app,
            Method::POST,
            &format!("/v1/workspace/{ws}/invites"),
            Some(json!({ "email": email, "team_id": null })),
            Some(token),
        )
        .await
    }

    async fn invite(&self, email: &str) -> (StatusCode, Value) {
        self.invite_as(&self.owner_token, &self.ws, email).await
    }

    /// Code from the newest captured mail.
    fn last_code(&self) -> String {
        let mails = common::invite_mails(&self.outbox);
        let body = &mails.last().expect("no invite mail sent").body;
        let start = body.find("code=").expect("no code in mail") + 5;
        body[start..start + 64].to_owned()
    }

    async fn status(&self, code: &str) -> (StatusCode, Value) {
        send(
            &self.app,
            Method::GET,
            &format!("/v1/invites/get-invite-status/{code}"),
            None,
            None,
        )
        .await
    }

    async fn register(&self, email: &str, code: &str) -> (StatusCode, Value) {
        post(
            &self.app,
            "/v1/register",
            json!({
                "first_name": "Ned", "last_name": "New", "email": email,
                "password": PASSWORD, "invite_code": code,
            }),
        )
        .await
    }

    async fn answer(&self, token: &str, code: &str, accept: bool) -> (StatusCode, Value) {
        send(
            &self.app,
            Method::POST,
            "/v1/invites",
            Some(json!({ "accept": accept, "invite_code": code })),
            Some(token),
        )
        .await
    }

    async fn workspaces_of(&self, email: &str) -> Value {
        let (_, res) = post(
            &self.app,
            "/v1/login",
            json!({ "email": email, "password": PASSWORD }),
        )
        .await;
        res["data"]["workspace_ids"].clone()
    }

    async fn expire_all(&self) {
        sqlx::query("UPDATE invites SET expires_at = now() - interval '1 second'")
            .execute(&self.pool)
            .await
            .unwrap();
    }
}

// ================= send

#[sqlx::test]
async fn owner_invite_stores_hash_and_mails_link(pool: PgPool) {
    let w = world(pool).await;

    let (status, res) = w.invite("new@example.com").await;

    assert_eq!(status, StatusCode::CREATED, "{res}");
    let invite_id: uuid::Uuid = res["data"]["invite_id"].as_str().unwrap().parse().unwrap();
    let code = w.last_code();
    {
        let mails = common::invite_mails(&w.outbox);
        assert_eq!(mails.len(), 1);
        assert_eq!(mails[0].to, "new@example.com");
        assert!(mails[0].subject.contains("acme"), "{}", mails[0].subject);
        assert!(
            mails[0]
                .body
                .contains(&format!("{FRONTEND_URL}/accept-invitation?code={code}")),
            "{}",
            mails[0].body
        );
    }
    let stored: Vec<u8> = sqlx::query_scalar("SELECT token_hash FROM invites WHERE id = $1")
        .bind(invite_id)
        .fetch_one(&w.pool)
        .await
        .unwrap();
    assert_eq!(
        stored,
        Sha256::digest(code.as_bytes()).to_vec(),
        "only sha256 stored"
    );
}

#[sqlx::test]
async fn reinvite_keeps_id_and_kills_old_link(pool: PgPool) {
    let w = world(pool).await;
    let (_, first) = w.invite("new@example.com").await;
    let old = w.last_code();

    let (_, second) = w.invite("NEW@example.com").await;
    let new = w.last_code();

    assert_eq!(first["data"]["invite_id"], second["data"]["invite_id"]);
    assert_eq!(w.status(&old).await.0, StatusCode::NOT_FOUND);
    assert_eq!(w.status(&new).await.0, StatusCode::OK);
}

#[sqlx::test]
async fn non_member_gets_404_member_non_owner_gets_403(pool: PgPool) {
    let w = world(pool).await;
    let (bob_token, _) = signup(&w.app, "bob@example.com", "bobs").await;

    let (status, res) = w.invite_as(&bob_token, &w.ws, "x@example.com").await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "outsider must not learn workspace exists"
    );
    assert_eq!(res["message"], NOT_FOUND);

    w.invite("bob@example.com").await;
    w.answer(&bob_token, &w.last_code(), true).await;
    let (status, res) = w.invite_as(&bob_token, &w.ws, "x@example.com").await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(res["message"], "This action is unauthorized.");
}

#[sqlx::test]
async fn unknown_or_malformed_workspace_is_404(pool: PgPool) {
    let w = world(pool).await;

    for ws in ["00000000-0000-7000-8000-000000000000", "not-a-uuid"] {
        let (status, _) = w.invite_as(&w.owner_token, ws, "x@example.com").await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{ws}");
    }
}

#[sqlx::test]
async fn invite_requires_token(pool: PgPool) {
    let w = world(pool).await;

    let (status, _) = send(
        &w.app,
        Method::POST,
        &format!("/v1/workspace/{}/invites", w.ws),
        Some(json!({ "email": "x@example.com" })),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn existing_member_rejected_ignoring_case(pool: PgPool) {
    let w = world(pool).await;

    let (status, res) = w.invite("ADA@example.com").await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"]["email"],
        json!(["user already exists in the workspace"])
    );
    assert!(
        common::invite_mails(&w.outbox).is_empty(),
        "no mail for rejected invite"
    );
}

#[sqlx::test]
async fn invite_email_validated(pool: PgPool) {
    let w = world(pool).await;

    assert_eq!(
        w.invite("not-an-email").await.1["errors"]["email"],
        json!(["The email must be a valid email address."])
    );
    let (_, res) = send(
        &w.app,
        Method::POST,
        &format!("/v1/workspace/{}/invites", w.ws),
        Some(json!({})),
        Some(&w.owner_token),
    )
    .await;
    assert_eq!(
        res["errors"]["email"],
        json!(["The email field is required."])
    );
}

#[sqlx::test]
async fn smtp_failure_is_500_without_internals(pool: PgPool) {
    let (ok_app, _) = app_with_mail(pool.clone());
    let (token, ws) = signup(&ok_app, "ada@example.com", "acme").await;
    common::verify_email(&pool, "ada@example.com").await;
    // Port 1: nothing listens, connect fails fast.
    let broken = Mailer::smtp("smtp://127.0.0.1:1", "Genesy <no-reply@genesy.test>").unwrap();
    let app = build_app(common::state(pool, broken));

    let (status, res) = send(
        &app,
        Method::POST,
        &format!("/v1/workspace/{ws}/invites"),
        Some(json!({ "email": "new@example.com" })),
        Some(&token),
    )
    .await;

    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(res, json!({ "message": "Server error." }));
}

// ================= status

#[sqlx::test]
async fn status_for_new_user(pool: PgPool) {
    let w = world(pool).await;
    let (_, sent) = w.invite("new@example.com").await;

    let (status, res) = w.status(&w.last_code()).await;

    assert_eq!(status, StatusCode::OK);
    let data = &res["data"];
    assert_eq!(data["id"], sent["data"]["invite_id"]);
    assert_eq!(data["status"], "pending");
    assert_eq!(data["new_user"], true);
    assert_eq!(data["workspace_name"], "acme");
    assert_eq!(data["email"], "new@example.com");
    assert!(
        data["invited_on"].as_str().unwrap().starts_with("20"),
        "{data}"
    );
}

#[sqlx::test]
async fn status_for_existing_user_and_expired(pool: PgPool) {
    let w = world(pool).await;
    signup(&w.app, "bob@example.com", "bobs").await;
    w.invite("bob@example.com").await;
    let code = w.last_code();

    assert_eq!(w.status(&code).await.1["data"]["new_user"], false);

    w.expire_all().await;
    assert_eq!(w.status(&code).await.1["data"]["status"], "expired");
}

#[sqlx::test]
async fn status_unknown_or_malformed_code_is_404(pool: PgPool) {
    let w = world(pool).await;

    for code in ["a".repeat(64), "short".into(), "z".repeat(64)] {
        let (status, res) = w.status(&code).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{code}");
        assert_eq!(res["message"], NOT_FOUND);
    }
}

// ================= register with invite

#[sqlx::test]
async fn register_creates_user_in_invited_workspace(pool: PgPool) {
    let w = world(pool).await;
    w.invite("new@example.com").await;
    let code = w.last_code();

    let (status, res) = w.register("new@example.com", &code).await;

    assert_eq!(status, StatusCode::CREATED, "{res}");
    assert_eq!(res["data"]["token_type"], "Bearer");
    assert_eq!(res["data"]["workspace_ids"], json!([w.ws]));
    assert_eq!(w.workspaces_of("new@example.com").await, json!([w.ws]));
    let after = w.status(&code).await.1;
    assert_eq!(after["data"]["status"], "accepted");
    assert_eq!(after["data"]["new_user"], false);
}

#[sqlx::test]
async fn register_email_must_match_invite(pool: PgPool) {
    let w = world(pool).await;
    w.invite("new@example.com").await;

    let (status, res) = w.register("other@example.com", &w.last_code()).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(res["message"], NOT_FOUND);
}

#[sqlx::test]
async fn register_email_match_ignores_case(pool: PgPool) {
    let w = world(pool).await;
    w.invite("New@Example.com").await;

    assert_eq!(
        w.register("new@example.com", &w.last_code()).await.0,
        StatusCode::CREATED
    );
}

#[sqlx::test]
async fn register_unknown_code_is_404(pool: PgPool) {
    let w = world(pool).await;

    assert_eq!(
        w.register("new@example.com", &"b".repeat(64)).await.0,
        StatusCode::NOT_FOUND
    );
}

#[sqlx::test]
async fn register_expired_invite_is_422(pool: PgPool) {
    let w = world(pool).await;
    w.invite("new@example.com").await;
    w.expire_all().await;

    let (status, res) = w.register("new@example.com", &w.last_code()).await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"]["invite_code"],
        json!(["Workspace invite has expired"])
    );
}

#[sqlx::test]
async fn register_twice_with_same_code_is_422_used(pool: PgPool) {
    let w = world(pool).await;
    w.invite("new@example.com").await;
    let code = w.last_code();
    w.register("new@example.com", &code).await;

    let (status, res) = w.register("new@example.com", &code).await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"]["invite_code"],
        json!(["This invitation has already been used."])
    );
}

#[sqlx::test]
async fn register_existing_email_must_log_in_instead(pool: PgPool) {
    let w = world(pool).await;
    signup(&w.app, "bob@example.com", "bobs").await;
    w.invite("bob@example.com").await;

    let (status, res) = w.register("bob@example.com", &w.last_code()).await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"]["email"],
        json!(["The email has already been taken."])
    );
}

#[sqlx::test]
async fn register_validates_every_field(pool: PgPool) {
    let w = world(pool).await;

    let (status, res) = post(&w.app, "/v1/register", json!({ "password": "weak" })).await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let mut fields: Vec<_> = res["errors"].as_object().unwrap().keys().cloned().collect();
    fields.sort();
    assert_eq!(
        fields,
        [
            "email",
            "first_name",
            "invite_code",
            "last_name",
            "password"
        ]
    );
}

#[sqlx::test]
async fn concurrent_register_same_code_one_wins(pool: PgPool) {
    let w = world(pool).await;
    w.invite("new@example.com").await;
    let code = w.last_code();

    let (a, b) = tokio::join!(
        w.register("new@example.com", &code),
        w.register("new@example.com", &code)
    );

    let mut statuses = [a.0, b.0];
    statuses.sort();
    assert_eq!(
        statuses,
        [StatusCode::CREATED, StatusCode::UNPROCESSABLE_ENTITY],
        "{a:?} {b:?}"
    );
}

// ================= answer while logged in

#[sqlx::test]
async fn existing_user_accepts_and_joins(pool: PgPool) {
    let w = world(pool).await;
    let (bob_token, bobs) = signup(&w.app, "bob@example.com", "bobs").await;
    w.invite("Bob@Example.com").await;
    let code = w.last_code();

    let (status, res) = w.answer(&bob_token, &code, true).await;

    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res, json!({ "message": "ok" }));
    assert_eq!(
        w.workspaces_of("bob@example.com").await,
        json!([bobs, w.ws])
    );
    assert_eq!(w.status(&code).await.1["data"]["status"], "accepted");
}

#[sqlx::test]
async fn accept_twice_is_idempotent(pool: PgPool) {
    let w = world(pool).await;
    let (bob_token, _) = signup(&w.app, "bob@example.com", "bobs").await;
    w.invite("bob@example.com").await;
    let code = w.last_code();
    w.answer(&bob_token, &code, true).await;

    assert_eq!(w.answer(&bob_token, &code, true).await.0, StatusCode::OK);
}

#[sqlx::test]
async fn decline_then_accept_is_422_used(pool: PgPool) {
    let w = world(pool).await;
    let (bob_token, bobs) = signup(&w.app, "bob@example.com", "bobs").await;
    w.invite("bob@example.com").await;
    let code = w.last_code();

    assert_eq!(w.answer(&bob_token, &code, false).await.0, StatusCode::OK);
    assert_eq!(w.status(&code).await.1["data"]["status"], "denied");
    assert_eq!(
        w.workspaces_of("bob@example.com").await,
        json!([bobs]),
        "decline must not join"
    );

    let (status, res) = w.answer(&bob_token, &code, true).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"]["invite_code"],
        json!(["This invitation has already been used."])
    );
}

#[sqlx::test]
async fn other_user_cannot_answer(pool: PgPool) {
    let w = world(pool).await;
    let (eve_token, _) = signup(&w.app, "eve@example.com", "eves").await;
    w.invite("bob@example.com").await;
    let code = w.last_code();

    let (status, _) = w.answer(&eve_token, &code, true).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(
        w.status(&code).await.1["data"]["status"],
        "pending",
        "invite untouched"
    );
}

#[sqlx::test]
async fn answer_expired_is_422(pool: PgPool) {
    let w = world(pool).await;
    let (bob_token, _) = signup(&w.app, "bob@example.com", "bobs").await;
    w.invite("bob@example.com").await;
    w.expire_all().await;

    let (status, res) = w.answer(&bob_token, &w.last_code(), true).await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"]["invite_code"],
        json!(["Workspace invite has expired"])
    );
}

#[sqlx::test]
async fn answer_requires_token_and_valid_body(pool: PgPool) {
    let w = world(pool).await;
    w.invite("bob@example.com").await;
    let code = w.last_code();

    let (status, _) = send(
        &w.app,
        Method::POST,
        "/v1/invites",
        Some(json!({ "accept": true, "invite_code": code })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (_, res) = send(
        &w.app,
        Method::POST,
        "/v1/invites",
        Some(json!({})),
        Some(&w.owner_token),
    )
    .await;
    assert_eq!(
        res["errors"]["accept"],
        json!(["The accept field is required."])
    );
    assert_eq!(
        res["errors"]["invite_code"],
        json!(["The invite code field is required."])
    );
}

// ================= review fixes

#[sqlx::test]
async fn invite_send_quota_per_user_per_hour(pool: PgPool) {
    let (ok_app, outbox) = app_with_mail(pool.clone());
    let (token, ws) = signup(&ok_app, "ada@example.com", "acme").await;
    common::verify_email(&pool, "ada@example.com").await;
    let mut state = common::state(pool, Mailer::Capture(outbox.clone()));
    state.invites_per_hour = 2;
    let app = build_app(state);
    let invite = |email: &'static str| {
        let app = app.clone();
        let (token, ws) = (token.clone(), ws.clone());
        async move {
            send(
                &app,
                Method::POST,
                &format!("/v1/workspace/{ws}/invites"),
                Some(json!({ "email": email })),
                Some(&token),
            )
            .await
        }
    };

    assert_eq!(invite("a@example.com").await.0, StatusCode::CREATED);
    assert_eq!(
        invite("a@example.com").await.0,
        StatusCode::CREATED,
        "re-send counts too"
    );
    let (status, res) = invite("b@example.com").await;

    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(res, json!({ "message": "Too Many Attempts." }));
    assert_eq!(common::invite_mails(&outbox).len(), 2, "no mail past quota");
}

#[sqlx::test]
async fn accepted_code_cannot_rejoin_after_removal(pool: PgPool) {
    let w = world(pool).await;
    let (bob_token, _) = signup(&w.app, "bob@example.com", "bobs").await;
    w.invite("bob@example.com").await;
    let code = w.last_code();
    w.answer(&bob_token, &code, true).await;
    sqlx::query(
        "DELETE FROM workspace_members WHERE workspace_id = $1::uuid
         AND user_id = (SELECT id FROM users WHERE email = 'bob@example.com')",
    )
    .bind(w.ws.parse::<uuid::Uuid>().unwrap())
    .execute(&w.pool)
    .await
    .unwrap();

    let (status, res) = w.answer(&bob_token, &code, true).await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"]["invite_code"],
        json!(["This invitation has already been used."])
    );
}

#[sqlx::test]
async fn concurrent_accept_and_decline_one_wins(pool: PgPool) {
    let w = world(pool).await;
    let (bob_token, _) = signup(&w.app, "bob@example.com", "bobs").await;
    w.invite("bob@example.com").await;
    let code = w.last_code();

    let (a, b) = tokio::join!(
        w.answer(&bob_token, &code, true),
        w.answer(&bob_token, &code, false)
    );

    let mut statuses = [a.0, b.0];
    statuses.sort();
    assert_eq!(
        statuses,
        [StatusCode::OK, StatusCode::UNPROCESSABLE_ENTITY],
        "{a:?} {b:?}"
    );
}

#[sqlx::test]
async fn invite_rejects_address_mail_parser_would_reject(pool: PgPool) {
    let w = world(pool).await;

    let (status, _) = w.invite("a\"b@example.com").await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(common::invite_mails(&w.outbox).is_empty());
}

// ================= email verification side effects

async fn verified(pool: &PgPool, email: &str) -> bool {
    sqlx::query_scalar("SELECT email_verified_at IS NOT NULL FROM users WHERE email = $1")
        .bind(email)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test]
async fn register_marks_email_verified(pool: PgPool) {
    let w = world(pool).await;
    w.invite("new@example.com").await;

    w.register("new@example.com", &w.last_code()).await;

    assert!(
        verified(&w.pool, "new@example.com").await,
        "emailed code proves inbox"
    );
}

#[sqlx::test]
async fn answering_with_emailed_code_marks_email_verified(pool: PgPool) {
    let w = world(pool).await;
    let (bob_token, _) = signup(&w.app, "bob@example.com", "bobs").await;
    assert!(!verified(&w.pool, "bob@example.com").await);
    w.invite("bob@example.com").await;

    w.answer(&bob_token, &w.last_code(), false).await;

    assert!(verified(&w.pool, "bob@example.com").await);
}

// ================= answer by invite id (from /v1/me pending_invites)

impl World {
    async fn invite_id(&self, email: &str) -> String {
        sqlx::query_scalar("SELECT id::text FROM invites WHERE lower(email) = lower($1)")
            .bind(email)
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }

    async fn verify(&self, email: &str) {
        sqlx::query("UPDATE users SET email_verified_at = now() WHERE lower(email) = lower($1)")
            .bind(email)
            .execute(&self.pool)
            .await
            .unwrap();
    }
}

#[sqlx::test]
async fn verified_user_accepts_by_invite_id(pool: PgPool) {
    let w = world(pool).await;
    let (bob_token, bobs) = signup(&w.app, "bob@example.com", "bobs").await;
    w.verify("bob@example.com").await;
    w.invite("bob@example.com").await;
    let id = w.invite_id("bob@example.com").await;

    let (status, res) = w.answer(&bob_token, &id, true).await;

    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(
        w.workspaces_of("bob@example.com").await,
        json!([bobs, w.ws])
    );
}

#[sqlx::test]
async fn unverified_user_cannot_use_invite_id(pool: PgPool) {
    let w = world(pool).await;
    let (bob_token, _) = signup(&w.app, "bob@example.com", "bobs").await;
    w.invite("bob@example.com").await;
    let id = w.invite_id("bob@example.com").await;

    let (status, res) = w.answer(&bob_token, &id, true).await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(
        res,
        json!({ "message": "Verify your email address first." })
    );
    assert_eq!(
        w.status(&w.last_code()).await.1["data"]["status"],
        "pending"
    );
}

#[sqlx::test]
async fn invite_id_of_someone_else_is_404(pool: PgPool) {
    let w = world(pool).await;
    let (eve_token, _) = signup(&w.app, "eve@example.com", "eves").await;
    w.verify("eve@example.com").await;
    w.invite("bob@example.com").await;
    let id = w.invite_id("bob@example.com").await;

    assert_eq!(
        w.answer(&eve_token, &id, true).await.0,
        StatusCode::NOT_FOUND
    );
}

#[sqlx::test]
async fn unknown_invite_id_is_404(pool: PgPool) {
    let w = world(pool).await;
    let (bob_token, _) = signup(&w.app, "bob@example.com", "bobs").await;
    w.verify("bob@example.com").await;

    let (status, _) = w
        .answer(&bob_token, "00000000-0000-7000-8000-000000000000", true)
        .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn deny_by_invite_id(pool: PgPool) {
    let w = world(pool).await;
    let (bob_token, bobs) = signup(&w.app, "bob@example.com", "bobs").await;
    w.verify("bob@example.com").await;
    w.invite("bob@example.com").await;
    let id = w.invite_id("bob@example.com").await;

    assert_eq!(w.answer(&bob_token, &id, false).await.0, StatusCode::OK);
    assert_eq!(w.status(&w.last_code()).await.1["data"]["status"], "denied");
    assert_eq!(w.workspaces_of("bob@example.com").await, json!([bobs]));
}

#[sqlx::test]
async fn invite_id_still_works_after_reinvite_rotates_code(pool: PgPool) {
    let w = world(pool).await;
    let (bob_token, _) = signup(&w.app, "bob@example.com", "bobs").await;
    w.verify("bob@example.com").await;
    w.invite("bob@example.com").await;
    let id = w.invite_id("bob@example.com").await;
    w.invite("bob@example.com").await; // rotates code, same row

    assert_eq!(w.answer(&bob_token, &id, true).await.0, StatusCode::OK);
}

#[sqlx::test]
async fn unverified_owner_cannot_invite(pool: PgPool) {
    // Throwaway signups must not be able to mail strangers from our domain.
    let (app, outbox) = app_with_mail(pool);
    let (token, ws) = signup(&app, "ada@example.com", "acme").await;

    let (status, res) = send(
        &app,
        Method::POST,
        &format!("/v1/workspace/{ws}/invites"),
        Some(json!({ "email": "new@example.com" })),
        Some(&token),
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(
        res,
        json!({ "message": "Verify your email address first." })
    );
    assert!(common::invite_mails(&outbox).is_empty());
}

#[sqlx::test]
async fn deny_by_invite_id_also_needs_verified_email(pool: PgPool) {
    let w = world(pool).await;
    let (bob_token, _) = signup(&w.app, "bob@example.com", "bobs").await;
    w.invite("bob@example.com").await;
    let id = w.invite_id("bob@example.com").await;

    assert_eq!(
        w.answer(&bob_token, &id, false).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        w.status(&w.last_code()).await.1["data"]["status"],
        "pending"
    );
}

#[sqlx::test]
async fn deactivated_owner_cannot_invite(pool: PgPool) {
    let w = world(pool).await;
    sqlx::query("UPDATE workspace_members SET active = false WHERE workspace_id = $1::uuid")
        .bind(w.ws.parse::<uuid::Uuid>().unwrap())
        .execute(&w.pool)
        .await
        .unwrap();

    let (status, res) = w.invite("new@example.com").await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(res["message"], "This action is unauthorized.");
}

// ================= team_id: accepting joins the team (Laravel AnswerInviteController)

const BAD_TEAM: &str = "The team id is an invalid id for the team domain.";

impl World {
    async fn invite_to_team(&self, email: &str, team: Value) -> (StatusCode, Value) {
        send(
            &self.app,
            Method::POST,
            &format!("/v1/workspace/{}/invites", self.ws),
            Some(json!({ "email": email, "team_id": team })),
            Some(&self.owner_token),
        )
        .await
    }

    async fn team_members(&self, team: &str) -> Vec<String> {
        sqlx::query_scalar(
            "SELECT u.email FROM team_members m JOIN users u ON u.id = m.user_id
             WHERE m.team_id = $1::uuid ORDER BY u.email",
        )
        .bind(team)
        .fetch_all(&self.pool)
        .await
        .unwrap()
    }
}

#[sqlx::test]
async fn register_through_team_invite_joins_team(pool: PgPool) {
    let w = world(pool).await;
    let team = common::add_team(&w.pool, &w.ws, "Sales", &["relation.list"], false).await;
    let (status, res) = w.invite_to_team("new@example.com", json!(team)).await;
    assert_eq!(status, StatusCode::CREATED, "{res}");

    let (status, res) = w.register("new@example.com", &w.last_code()).await;

    assert_eq!(status, StatusCode::CREATED, "{res}");
    assert_eq!(w.team_members(&team).await, ["new@example.com"]);
}

#[sqlx::test]
async fn accepting_team_invite_joins_team(pool: PgPool) {
    let w = world(pool).await;
    let (bob, _) = signup(&w.app, "bob@example.com", "bobs").await;
    let team = common::add_team(&w.pool, &w.ws, "Sales", &[], false).await;
    w.invite_to_team("bob@example.com", json!(team)).await;

    assert_eq!(w.answer(&bob, &w.last_code(), true).await.0, StatusCode::OK);
    assert_eq!(w.team_members(&team).await, ["bob@example.com"]);
}

#[sqlx::test]
async fn declining_team_invite_joins_nothing(pool: PgPool) {
    let w = world(pool).await;
    let (bob, _) = signup(&w.app, "bob@example.com", "bobs").await;
    let team = common::add_team(&w.pool, &w.ws, "Sales", &[], false).await;
    w.invite_to_team("bob@example.com", json!(team)).await;

    w.answer(&bob, &w.last_code(), false).await;

    assert!(w.team_members(&team).await.is_empty());
}

#[sqlx::test]
async fn team_deleted_after_invite_is_skipped(pool: PgPool) {
    let w = world(pool).await;
    let (bob, _) = signup(&w.app, "bob@example.com", "bobs").await;
    let team = common::add_team(&w.pool, &w.ws, "Sales", &[], false).await;
    w.invite_to_team("bob@example.com", json!(team)).await;
    sqlx::query("UPDATE teams SET deleted_at = now()")
        .execute(&w.pool)
        .await
        .unwrap();

    assert_eq!(w.answer(&bob, &w.last_code(), true).await.0, StatusCode::OK);
    assert!(w.team_members(&team).await.is_empty());
    assert_eq!(
        w.workspaces_of("bob@example.com")
            .await
            .as_array()
            .unwrap()
            .len(),
        2,
        "still joins the workspace"
    );
}

#[sqlx::test]
async fn foreign_deleted_or_garbage_team_is_422(pool: PgPool) {
    let w = world(pool).await;
    let (_, eves) = signup(&w.app, "eve@example.com", "eves").await;
    let foreign = common::add_team(&w.pool, &eves, "Theirs", &[], false).await;
    let deleted = common::add_team(&w.pool, &w.ws, "Gone", &[], true).await;

    for team in [json!(foreign), json!(deleted), json!("nope"), json!(42)] {
        let (status, res) = w.invite_to_team("new@example.com", team.clone()).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{team}: {res}");
        assert_eq!(res["errors"]["team_id"], json!([BAD_TEAM]), "{team}");
    }
    assert!(common::invite_mails(&w.outbox).is_empty());
}

#[sqlx::test]
async fn blank_team_id_means_no_team(pool: PgPool) {
    let w = world(pool).await;

    // Laravel ConvertEmptyStringsToNull.
    let (status, res) = w.invite_to_team("new@example.com", json!("  ")).await;

    assert_eq!(status, StatusCode::CREATED, "{res}");
}

#[sqlx::test]
async fn only_admins_invite_into_a_team_holding_workspace_admin(pool: PgPool) {
    let w = world(pool).await;
    let admins: String = sqlx::query_scalar("SELECT id::text FROM teams WHERE name = 'Admin Team'")
        .fetch_one(&w.pool)
        .await
        .unwrap();
    let bob = common::member_with(
        &w.app,
        &w.pool,
        &w.ws,
        "bob@example.com",
        &["invite.create"],
    )
    .await;
    common::verify_email(&w.pool, "bob@example.com").await;

    let (status, res) = send(
        &w.app,
        Method::POST,
        &format!("/v1/workspace/{}/invites", w.ws),
        Some(json!({ "email": "alt@example.com", "team_id": admins })),
        Some(&bob),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "second-account escalation: {res}"
    );
    assert!(common::invite_mails(&w.outbox).is_empty());

    assert_eq!(
        w.invite_to_team("alt@example.com", json!(admins)).await.0,
        StatusCode::CREATED
    );
}

#[sqlx::test]
async fn reinvite_takes_newest_team(pool: PgPool) {
    let w = world(pool).await;
    let first = common::add_team(&w.pool, &w.ws, "First", &[], false).await;
    let second = common::add_team(&w.pool, &w.ws, "Second", &[], false).await;
    w.invite_to_team("new@example.com", json!(first)).await;
    w.invite_to_team("new@example.com", json!(second)).await;

    w.register("new@example.com", &w.last_code()).await;

    assert!(w.team_members(&first).await.is_empty());
    assert_eq!(w.team_members(&second).await, ["new@example.com"]);
}
