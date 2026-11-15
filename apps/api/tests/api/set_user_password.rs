//! Operator CLI `set-user-password`: password from stdin only, `afm` only, never printed;
//! creates or updates the login, ensures active membership, `--admin` joins the admin team.

use crate::common;

use axum::{
    Router,
    http::{Method, StatusCode},
};
use common::send;
use genesy_360_api::{AppState, bootstrap, build_app, mail::Mailer};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

const NEW: &str = "N3w!Passw0rd-xyz";

struct Setup {
    state: AppState,
    app: Router,
    afm: Uuid,
    acme: Uuid,
    owner_password: String,
}

async fn setup(pool: PgPool) -> Setup {
    let state = common::state(pool, Mailer::capture().0);
    let afm = bootstrap::create_owner(&state, "owner@afm.test", "afm", "Ro", "Owner")
        .await
        .unwrap();
    let acme = bootstrap::create_owner(&state, "boss@acme.test", "acme", "Bo", "Boss")
        .await
        .unwrap();
    Setup {
        app: build_app(state.clone()),
        state,
        afm: afm.workspace_id,
        acme: acme.workspace_id,
        owner_password: afm.password,
    }
}

/// Runs the CLI as main.rs does; returns exit code and everything it printed.
async fn cli(state: &AppState, args: &[&str], stdin: &str) -> (i32, String) {
    let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    let mut out = Vec::new();
    let code = bootstrap::set_user_password_cli(state, &args, stdin.as_bytes(), &mut out).await;
    (code, String::from_utf8(out).unwrap())
}

async fn login(app: &Router, email: &str, password: &str) -> (StatusCode, Value) {
    send(
        app,
        Method::POST,
        "/v1/login",
        Some(json!({ "email": email, "password": password })),
        None,
    )
    .await
}

async fn user_id(pool: &PgPool, email: &str) -> Option<Uuid> {
    sqlx::query_scalar("SELECT id FROM users WHERE email = $1")
        .bind(email)
        .fetch_optional(pool)
        .await
        .unwrap()
}

/// (workspace, active) membership rows of a user.
async fn memberships(pool: &PgPool, user: Uuid) -> Vec<(Uuid, bool)> {
    sqlx::query_as("SELECT workspace_id, active FROM workspace_members WHERE user_id = $1")
        .bind(user)
        .fetch_all(pool)
        .await
        .unwrap()
}

/// Names of the teams a user belongs to.
async fn teams_of(pool: &PgPool, user: Uuid) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT t.name FROM team_members m JOIN teams t ON t.id = m.team_id
         WHERE m.user_id = $1 ORDER BY t.name",
    )
    .bind(user)
    .fetch_all(pool)
    .await
    .unwrap()
}

/// Rows of every table the command may write, for one workspace.
async fn snapshot(pool: &PgPool, ws: Uuid) -> (i64, i64, i64, i64) {
    sqlx::query_as(
        "SELECT (SELECT count(*) FROM workspace_members WHERE workspace_id = $1 AND active),
                (SELECT count(*) FROM team_members m JOIN teams t ON t.id = m.team_id
                  WHERE t.workspace_id = $1),
                (SELECT count(*) FROM access_tokens a JOIN workspaces w ON w.created_by_user_id = a.user_id
                  WHERE w.id = $1),
                (SELECT count(*) FROM users)",
    )
    .bind(ws)
    .fetch_one(pool)
    .await
    .unwrap()
}

fn admin_args(email: &str) -> Vec<&str> {
    vec!["--workspace", "afm", "--email", email, "--admin"]
}

// ---- happy path

#[sqlx::test]
async fn creates_an_admin_who_logs_in_with_the_typed_password(pool: PgPool) {
    let s = setup(pool.clone()).await;
    let acme_before = snapshot(&pool, s.acme).await;
    let mut args = admin_args("anthony@afm.test");
    args.extend(["--first-name", "Anthony", "--last-name", "Tester"]);

    let (code, out) = cli(&s.state, &args, &format!("{NEW}\n")).await;

    assert_eq!(code, 0, "{out}");
    assert!(out.contains("anthony@afm.test created"), "{out}");
    assert!(!out.contains(NEW), "password printed: {out}");
    let (status, res) = login(&s.app, "anthony@afm.test", NEW).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res["data"]["workspace_ids"], json!([s.afm.to_string()]));
    // Same rows an accepted invite into the Admin Team gives: active member + that team only.
    let anthony = user_id(&pool, "anthony@afm.test").await.unwrap();
    assert_eq!(memberships(&pool, anthony).await, vec![(s.afm, true)]);
    assert_eq!(teams_of(&pool, anthony).await, vec!["Admin Team"]);
    let (first, last): (String, String) =
        sqlx::query_as("SELECT first_name, last_name FROM users WHERE id = $1")
            .bind(anthony)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((first.as_str(), last.as_str()), ("Anthony", "Tester"));
    // Admin powers: owner-or-admin-only route answers for him.
    let token = res["data"]["access_token"].as_str().unwrap();
    let (status, res) = send(
        &s.app,
        Method::GET,
        &format!("/v1/workspace/{}/users", s.afm),
        None,
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    let (status, me) = send(&s.app, Method::GET, "/v1/me", None, Some(token)).await;
    assert_eq!(status, StatusCode::OK, "{me}");
    assert!(me.to_string().contains("workspace_admin"), "{me}");
    // Another workspace untouched, apart from the one new users row.
    let mut expect = acme_before;
    expect.3 += 1;
    assert_eq!(snapshot(&pool, s.acme).await, expect);
}

#[sqlx::test]
async fn updates_an_existing_password_and_ends_its_sessions(pool: PgPool) {
    let s = setup(pool.clone()).await;
    let (status, res) = login(&s.app, "owner@afm.test", &s.owner_password).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    let old_token = res["data"]["access_token"].as_str().unwrap().to_owned();
    let owner = user_id(&pool, "owner@afm.test").await.unwrap();

    // Mixed case email, CRLF line end (pasted on Windows): both fine.
    let (code, out) = cli(
        &s.state,
        &admin_args("Owner@AFM.test"),
        &format!("{NEW}\r\n"),
    )
    .await;

    assert_eq!(code, 0, "{out}");
    assert!(out.contains("updated"), "{out}");
    assert!(out.contains("2 sessions revoked"), "{out}");
    assert!(!out.contains(NEW), "{out}");
    assert_eq!(
        login(&s.app, "owner@afm.test", &s.owner_password).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(login(&s.app, "owner@afm.test", NEW).await.0, StatusCode::OK);
    let (status, _) = send(&s.app, Method::GET, "/v1/me", None, Some(&old_token)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "old session must end");
    // Already admin: nothing doubled.
    assert_eq!(teams_of(&pool, owner).await, vec!["Admin Team"]);
    assert_eq!(memberships(&pool, owner).await, vec![(s.afm, true)]);
}

#[sqlx::test]
async fn without_admin_joins_no_team(pool: PgPool) {
    let s = setup(pool.clone()).await;

    let (code, out) = cli(
        &s.state,
        &[
            "--workspace",
            "afm",
            "--email",
            "clerk@afm.test",
            "--first-name",
            "Cl",
            "--last-name",
            "Erk",
        ],
        &format!("{NEW}\n"),
    )
    .await;

    assert_eq!(code, 0, "{out}");
    let clerk = user_id(&pool, "clerk@afm.test").await.unwrap();
    assert_eq!(memberships(&pool, clerk).await, vec![(s.afm, true)]);
    assert!(teams_of(&pool, clerk).await.is_empty());
}

// ---- edge cases

#[sqlx::test]
async fn keeps_spaces_inside_the_password_and_reactivates_a_removed_member(pool: PgPool) {
    let s = setup(pool.clone()).await;
    let owner = user_id(&pool, "owner@afm.test").await.unwrap();
    sqlx::query("UPDATE workspace_members SET active = false WHERE user_id = $1")
        .bind(owner)
        .execute(&pool)
        .await
        .unwrap();
    let spaced = " Sp4ce! pass ";

    let (code, out) = cli(
        &s.state,
        &admin_args("owner@afm.test"),
        &format!("{spaced}\n"),
    )
    .await;

    assert_eq!(code, 0, "{out}");
    assert_eq!(
        login(&s.app, "owner@afm.test", spaced).await.0,
        StatusCode::OK
    );
    assert_eq!(memberships(&pool, owner).await, vec![(s.afm, true)]);
}

#[sqlx::test]
async fn clears_a_login_lock(pool: PgPool) {
    let s = setup(pool.clone()).await;
    for _ in 0..12 {
        login(&s.app, "owner@afm.test", "Wr0ng!pass").await;
    }
    assert_eq!(
        login(&s.app, "owner@afm.test", NEW).await.0,
        StatusCode::TOO_MANY_REQUESTS
    );

    let (code, out) = cli(&s.state, &admin_args("owner@afm.test"), &format!("{NEW}\n")).await;

    assert_eq!(code, 0, "{out}");
    assert_eq!(login(&s.app, "owner@afm.test", NEW).await.0, StatusCode::OK);
}

// ---- refusals: nothing written, nothing secret printed

#[sqlx::test]
async fn weak_password_is_refused_with_the_policy_messages(pool: PgPool) {
    let s = setup(pool.clone()).await;
    let before = snapshot(&pool, s.afm).await;
    let weak = "qq7zz";
    let mut args = admin_args("anthony@afm.test");
    args.extend(["--first-name", "A", "--last-name", "T"]);

    let (code, out) = cli(&s.state, &args, &format!("{weak}\n")).await;

    assert_eq!(code, 1, "{out}");
    assert!(
        out.contains("The password must be at least 8 characters."),
        "{out}"
    );
    assert!(out.contains("at least one symbol"), "{out}");
    assert!(!out.contains(weak), "{out}");
    assert_eq!(user_id(&pool, "anthony@afm.test").await, None);
    assert_eq!(snapshot(&pool, s.afm).await, before);
}

#[sqlx::test]
async fn only_the_afm_workspace(pool: PgPool) {
    let s = setup(pool.clone()).await;
    let before = snapshot(&pool, s.acme).await;
    let owner_hash: String =
        sqlx::query_scalar("SELECT password_hash FROM users WHERE email = 'boss@acme.test'")
            .fetch_one(&pool)
            .await
            .unwrap();

    for ws in ["acme", "afm-demo", "AFM"] {
        let (code, out) = cli(
            &s.state,
            &["--workspace", ws, "--email", "boss@acme.test", "--admin"],
            &format!("{NEW}\n"),
        )
        .await;
        assert_eq!(code, 1, "{ws}: {out}");
        assert!(out.contains("only afm"), "{out}");
    }

    assert_eq!(snapshot(&pool, s.acme).await, before);
    let after: String =
        sqlx::query_scalar("SELECT password_hash FROM users WHERE email = 'boss@acme.test'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(after, owner_hash);
}

#[sqlx::test]
async fn new_user_needs_both_names(pool: PgPool) {
    let s = setup(pool.clone()).await;
    let mut args = admin_args("anthony@afm.test");
    args.extend(["--first-name", "Anthony"]);

    let (code, out) = cli(&s.state, &args, &format!("{NEW}\n")).await;

    assert_eq!(code, 1, "{out}");
    assert!(out.contains("last name"), "{out}");
    assert_eq!(user_id(&pool, "anthony@afm.test").await, None);
}

#[sqlx::test]
async fn bad_input_is_refused(pool: PgPool) {
    let s = setup(pool.clone()).await;

    // No password on stdin.
    let (code, out) = cli(&s.state, &admin_args("owner@afm.test"), "").await;
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("password field is required"), "{out}");
    // Bad email shape.
    let (code, out) = cli(&s.state, &admin_args("not-an-email"), &format!("{NEW}\n")).await;
    assert_eq!(code, 1, "{out}");
    assert!(!out.contains(NEW), "{out}");
    // Missing flags: usage, exit 2.
    let (code, out) = cli(&s.state, &["--workspace", "afm"], &format!("{NEW}\n")).await;
    assert_eq!(code, 2, "{out}");
    assert!(out.contains("usage: set-user-password"), "{out}");
    // A password on the command line is never read as one.
    let (code, _) = cli(
        &s.state,
        &[
            "--workspace",
            "afm",
            "--email",
            "owner@afm.test",
            "--password",
            NEW,
        ],
        "",
    )
    .await;
    assert_ne!(code, 0);
    assert_eq!(
        login(&s.app, "owner@afm.test", &s.owner_password).await.0,
        StatusCode::OK
    );
}

#[sqlx::test]
async fn admin_flag_without_an_admin_team_is_refused(pool: PgPool) {
    let s = setup(pool.clone()).await;
    sqlx::query("UPDATE teams SET deleted_at = now() WHERE workspace_id = $1")
        .bind(s.afm)
        .execute(&pool)
        .await
        .unwrap();
    let mut args = admin_args("anthony@afm.test");
    args.extend(["--first-name", "A", "--last-name", "T"]);

    let (code, out) = cli(&s.state, &args, &format!("{NEW}\n")).await;

    assert_eq!(code, 1, "{out}");
    assert!(out.contains("no admin team"), "{out}");
    assert_eq!(user_id(&pool, "anthony@afm.test").await, None);
}
