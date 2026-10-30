//! Email verification: link mailed at signup, GET link verifies + redirects, resend with quota.

mod common;

use axum::http::{Method, StatusCode};
use common::{
    API_PUBLIC_URL, FRONTEND_URL, app_with_mail, get_redirect, send, signup, verification_mails,
};
use genesy_360_api::{build_app, mail::Mailer};
use serde_json::json;
use sqlx::PgPool;

const RESEND: &str = "/v1/email/verification-notification";

fn ok_location() -> Option<String> {
    Some(format!("{FRONTEND_URL}/login?email_verified=1"))
}

fn failed_location() -> Option<String> {
    Some(format!("{FRONTEND_URL}/login?email_verified=0"))
}

/// Token from the newest verification mail.
fn last_token(outbox: &genesy_360_api::mail::Outbox) -> String {
    let mails = verification_mails(outbox);
    let body = &mails.last().expect("no verification mail").body;
    let marker = format!("{API_PUBLIC_URL}/v1/verify-email/");
    let start = body.find(&marker).expect("no verify link") + marker.len();
    body[start..start + 64].to_owned()
}

async fn is_verified(pool: &PgPool, email: &str) -> bool {
    sqlx::query_scalar("SELECT email_verified_at IS NOT NULL FROM users WHERE email = $1")
        .bind(email)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Pretend every issued token is `minutes` older (quota / gap tests without sleeping).
async fn age_tokens(pool: &PgPool, minutes: i32) {
    sqlx::query(
        "UPDATE email_verification_tokens SET created_at = created_at - make_interval(mins => $1)",
    )
    .bind(minutes)
    .execute(pool)
    .await
    .unwrap();
}

async fn token_count(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM email_verification_tokens")
        .fetch_one(pool)
        .await
        .unwrap()
}

// ---- happy path

#[sqlx::test]
async fn signup_mails_link_and_link_verifies(pool: PgPool) {
    let (app, outbox) = app_with_mail(pool.clone());
    signup(&app, "ada@example.com", "acme").await;
    let mails = verification_mails(&outbox);
    assert_eq!(mails.len(), 1);
    assert_eq!(mails[0].to, "ada@example.com");
    assert!(
        mails[0].body.contains("If you didn't create"),
        "mail must tell non-owners not to click"
    );
    assert!(!is_verified(&pool, "ada@example.com").await);

    let (status, location) =
        get_redirect(&app, &format!("/v1/verify-email/{}", last_token(&outbox))).await;

    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(location, ok_location());
    assert!(is_verified(&pool, "ada@example.com").await);
}

#[sqlx::test]
async fn token_stored_hashed_only(pool: PgPool) {
    let (app, outbox) = app_with_mail(pool.clone());
    signup(&app, "ada@example.com", "acme").await;
    let token = last_token(&outbox);

    let stored: Vec<u8> = sqlx::query_scalar("SELECT token_hash FROM email_verification_tokens")
        .fetch_one(&pool)
        .await
        .unwrap();

    use sha2::Digest;
    assert_eq!(stored, sha2::Sha256::digest(token.as_bytes()).to_vec());
}

#[sqlx::test]
async fn second_click_or_scanner_prefetch_still_reports_success(pool: PgPool) {
    // Outlook SafeLinks & co. open links before the user does.
    let (app, outbox) = app_with_mail(pool);
    signup(&app, "ada@example.com", "acme").await;
    let uri = format!("/v1/verify-email/{}", last_token(&outbox));
    get_redirect(&app, &uri).await;

    assert_eq!(get_redirect(&app, &uri).await.1, ok_location());
}

#[sqlx::test]
async fn first_verification_logs_out_existing_sessions(pool: PgPool) {
    // Pre-hijack mitigation: a squatter who signed up with this address loses their sessions
    // the moment the real inbox owner clicks.
    let (app, outbox) = app_with_mail(pool);
    let (squatter_token, _) = signup(&app, "ada@example.com", "acme").await;

    get_redirect(&app, &format!("/v1/verify-email/{}", last_token(&outbox))).await;

    let (status, _) = send(
        &app,
        Method::POST,
        "/v1/logout",
        None,
        Some(&squatter_token),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn later_clicks_do_not_log_out_new_sessions(pool: PgPool) {
    let (app, outbox) = app_with_mail(pool);
    signup(&app, "ada@example.com", "acme").await;
    let uri = format!("/v1/verify-email/{}", last_token(&outbox));
    get_redirect(&app, &uri).await;
    let (_, login) = common::post(
        &app,
        "/v1/login",
        json!({ "email": "ada@example.com", "password": common::PASSWORD }),
    )
    .await;
    let fresh = login["data"]["access_token"].as_str().unwrap().to_owned();

    get_redirect(&app, &uri).await; // prefetch / double click after login

    assert_eq!(
        send(&app, Method::POST, "/v1/logout", None, Some(&fresh))
            .await
            .0,
        StatusCode::OK
    );
}

// ---- errors: every bad link redirects to the failure page, never a JSON error

#[sqlx::test]
async fn expired_link_fails_and_leaves_user_unverified(pool: PgPool) {
    let (app, outbox) = app_with_mail(pool.clone());
    signup(&app, "ada@example.com", "acme").await;
    sqlx::query("UPDATE email_verification_tokens SET expires_at = now() - interval '1 second'")
        .execute(&pool)
        .await
        .unwrap();

    let (_, location) =
        get_redirect(&app, &format!("/v1/verify-email/{}", last_token(&outbox))).await;

    assert_eq!(location, failed_location());
    assert!(!is_verified(&pool, "ada@example.com").await);
}

#[sqlx::test]
async fn deleted_user_link_fails(pool: PgPool) {
    let (app, outbox) = app_with_mail(pool.clone());
    signup(&app, "ada@example.com", "acme").await;
    sqlx::query("UPDATE users SET deleted_at = now()")
        .execute(&pool)
        .await
        .unwrap();

    let (_, location) =
        get_redirect(&app, &format!("/v1/verify-email/{}", last_token(&outbox))).await;

    assert_eq!(location, failed_location());
}

#[sqlx::test]
async fn garbage_or_unknown_token_fails(pool: PgPool) {
    let (app, _) = app_with_mail(pool);

    for token in ["nope".to_owned(), "a".repeat(64), "Z".repeat(64)] {
        let (status, location) = get_redirect(&app, &format!("/v1/verify-email/{token}")).await;
        assert_eq!(status, StatusCode::SEE_OTHER, "{token}");
        assert_eq!(location, failed_location(), "{token}");
    }
}

#[sqlx::test]
async fn signup_succeeds_even_if_verification_mail_fails(pool: PgPool) {
    let broken = Mailer::smtp("smtp://127.0.0.1:1", "Genesy <no-reply@genesy.test>").unwrap();
    let app = build_app(common::state(pool.clone(), broken));

    let (status, _) = common::post(
        &app,
        "/v1/create-workspace",
        json!({
            "first_name": "Ada", "last_name": "L", "email": "ada@example.com",
            "password": common::PASSWORD, "workspace_name": "acme",
        }),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::CREATED,
        "account exists; user can ask for a resend"
    );
}

// ---- resend

#[sqlx::test]
async fn resend_mails_new_link_and_old_link_keeps_working(pool: PgPool) {
    let (app, outbox) = app_with_mail(pool.clone());
    let (token, _) = signup(&app, "ada@example.com", "acme").await;
    let first = last_token(&outbox);
    age_tokens(&pool, 2).await;

    let (status, res) = send(&app, Method::POST, RESEND, None, Some(&token)).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(res, json!({ "message": "ok" }));
    assert_eq!(verification_mails(&outbox).len(), 2);
    assert_ne!(last_token(&outbox), first);
    assert_eq!(
        get_redirect(&app, &format!("/v1/verify-email/{first}"))
            .await
            .1,
        ok_location()
    );
}

#[sqlx::test]
async fn resend_within_a_minute_is_429(pool: PgPool) {
    let (app, outbox) = app_with_mail(pool);
    let (token, _) = signup(&app, "ada@example.com", "acme").await;

    let (status, res) = send(&app, Method::POST, RESEND, None, Some(&token)).await;

    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(res, json!({ "message": "Too Many Attempts." }));
    assert_eq!(verification_mails(&outbox).len(), 1);
}

#[sqlx::test]
async fn resend_when_already_verified_sends_nothing(pool: PgPool) {
    let (app, outbox) = app_with_mail(pool.clone());
    signup(&app, "ada@example.com", "acme").await;
    get_redirect(&app, &format!("/v1/verify-email/{}", last_token(&outbox))).await;
    let (_, login) = common::post(
        &app,
        "/v1/login",
        json!({ "email": "ada@example.com", "password": common::PASSWORD }),
    )
    .await;
    let token = login["data"]["access_token"].as_str().unwrap().to_owned();
    age_tokens(&pool, 2).await;

    let (status, _) = send(&app, Method::POST, RESEND, None, Some(&token)).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(verification_mails(&outbox).len(), 1);
}

#[sqlx::test]
async fn resend_quota_five_per_hour(pool: PgPool) {
    let (app, outbox) = app_with_mail(pool.clone());
    let (token, _) = signup(&app, "ada@example.com", "acme").await; // link 1
    for _ in 0..4 {
        age_tokens(&pool, 2).await;
        assert_eq!(
            send(&app, Method::POST, RESEND, None, Some(&token)).await.0,
            StatusCode::OK
        );
    }
    age_tokens(&pool, 2).await;

    assert_eq!(
        send(&app, Method::POST, RESEND, None, Some(&token)).await.0,
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(verification_mails(&outbox).len(), 5);
}

#[sqlx::test]
async fn resend_quota_ten_per_day(pool: PgPool) {
    let (app, _) = app_with_mail(pool.clone());
    let (token, _) = signup(&app, "ada@example.com", "acme").await; // link 1
    for _ in 0..9 {
        age_tokens(&pool, 61).await; // each hour fresh, day accumulates
        assert_eq!(
            send(&app, Method::POST, RESEND, None, Some(&token)).await.0,
            StatusCode::OK
        );
    }
    age_tokens(&pool, 61).await;

    assert_eq!(
        send(&app, Method::POST, RESEND, None, Some(&token)).await.0,
        StatusCode::TOO_MANY_REQUESTS
    );
}

#[sqlx::test]
async fn concurrent_resends_cannot_beat_the_quota(pool: PgPool) {
    let (app, outbox) = app_with_mail(pool.clone());
    let (token, _) = signup(&app, "ada@example.com", "acme").await;
    age_tokens(&pool, 2).await;

    let mut calls = tokio::task::JoinSet::new();
    for _ in 0..10 {
        let (app, token) = (app.clone(), token.clone());
        calls.spawn(async move { send(&app, Method::POST, RESEND, None, Some(&token)).await });
    }
    let results = calls.join_all().await;

    let sent = results.iter().filter(|(s, _)| *s == StatusCode::OK).count();
    assert_eq!(sent, 1, "60s gap allows exactly one: {results:?}");
    assert_eq!(token_count(&pool).await, 2);
    assert_eq!(verification_mails(&outbox).len(), 2);
}

#[sqlx::test]
async fn failed_resend_does_not_use_up_quota(pool: PgPool) {
    let (ok_app, _) = app_with_mail(pool.clone());
    let (token, _) = signup(&ok_app, "ada@example.com", "acme").await;
    age_tokens(&pool, 2).await;
    let broken = Mailer::smtp("smtp://127.0.0.1:1", "Genesy <no-reply@genesy.test>").unwrap();
    let broken_app = build_app(common::state(pool.clone(), broken));

    let (status, _) = send(&broken_app, Method::POST, RESEND, None, Some(&token)).await;

    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(
        token_count(&pool).await,
        1,
        "token rolled back with the failed send"
    );
    assert_eq!(
        send(&ok_app, Method::POST, RESEND, None, Some(&token))
            .await
            .0,
        StatusCode::OK
    );
}

#[sqlx::test]
async fn resend_requires_token(pool: PgPool) {
    let (app, _) = app_with_mail(pool);

    assert_eq!(
        send(&app, Method::POST, RESEND, None, None).await.0,
        StatusCode::UNAUTHORIZED
    );
}
