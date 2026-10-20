//! GET /v1/workspace/{ws}/permissions + the WorkspaceAccess guard every workspace route uses.

mod common;

use axum::{
    Router,
    http::{Method, StatusCode},
};
use common::{add_team, app_with_mail, join_team, member_with, send, signup};
use serde_json::{Value, json};
use sqlx::PgPool;

const NOT_FOUND: &str = "The requested resource could not be found.";
const UNAUTHORIZED: &str = "This action is unauthorized.";

async fn catalog(app: &Router, token: Option<&str>, ws: &str) -> (StatusCode, Value) {
    send(
        app,
        Method::GET,
        &format!("/v1/workspace/{ws}/permissions"),
        None,
        token,
    )
    .await
}

async fn invite(app: &Router, token: &str, ws: &str, email: &str) -> (StatusCode, Value) {
    send(
        app,
        Method::POST,
        &format!("/v1/workspace/{ws}/invites"),
        Some(json!({ "email": email })),
        Some(token),
    )
    .await
}

// ================= catalog

#[sqlx::test]
async fn member_gets_laravel_catalog_shape(pool: PgPool) {
    let (app, _) = app_with_mail(pool.clone());
    let (_, ws) = signup(&app, "ada@example.com", "acme").await;
    let bob = member_with(&app, &pool, &ws, "bob@example.com", &[]).await;

    let (status, res) = catalog(&app, Some(&bob), &ws).await;

    assert_eq!(status, StatusCode::OK, "{res}");
    let data = res["data"].as_object().unwrap();
    assert_eq!(data.len(), 44, "43 groups + __classification");
    assert_eq!(
        data["team"],
        json!([
            "create",
            "delete",
            "get",
            "invite",
            "list",
            "permissions",
            "update"
        ])
    );
    // vimdesk-web drops this key; kept for Laravel parity.
    let class = &data["__classification"];
    assert_eq!(
        class["frontend_only"],
        json!([
            "product.view_price",
            "product.view_purchase_price",
            "product.view_margin"
        ])
    );
    let enforced = class["api_enforced"].as_array().unwrap();
    assert_eq!(enforced.len(), 222);
    assert_eq!(enforced[0], "association.create");
    assert!(data.get("workspace_admin").is_none());
}

#[sqlx::test]
async fn owner_gets_catalog_too(pool: PgPool) {
    let (app, _) = app_with_mail(pool);
    let (ada, ws) = signup(&app, "ada@example.com", "acme").await;

    assert_eq!(catalog(&app, Some(&ada), &ws).await.0, StatusCode::OK);
}

// ================= guard

#[sqlx::test]
async fn outsider_unknown_or_malformed_workspace_is_404(pool: PgPool) {
    let (app, _) = app_with_mail(pool);
    let (_, ws) = signup(&app, "ada@example.com", "acme").await;
    let (eve, _) = signup(&app, "eve@example.com", "eves").await;

    for target in [
        ws.as_str(),
        "00000000-0000-7000-8000-000000000000",
        "not-a-uuid",
    ] {
        let (status, res) = catalog(&app, Some(&eve), target).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{target}");
        assert_eq!(res["message"], NOT_FOUND);
    }
}

#[sqlx::test]
async fn deleted_workspace_is_404_even_for_owner(pool: PgPool) {
    let (app, _) = app_with_mail(pool.clone());
    let (ada, ws) = signup(&app, "ada@example.com", "acme").await;
    sqlx::query("UPDATE workspaces SET deleted_at = now()")
        .execute(&pool)
        .await
        .unwrap();

    assert_eq!(
        catalog(&app, Some(&ada), &ws).await.0,
        StatusCode::NOT_FOUND
    );
}

#[sqlx::test]
async fn inactive_member_is_403(pool: PgPool) {
    let (app, _) = app_with_mail(pool.clone());
    let (_, ws) = signup(&app, "ada@example.com", "acme").await;
    let bob = member_with(&app, &pool, &ws, "bob@example.com", &["invite.create"]).await;
    sqlx::query(
        "UPDATE workspace_members SET active = false
         WHERE workspace_id = $1::uuid AND user_id = (SELECT id FROM users WHERE email = 'bob@example.com')",
    )
    .bind(&ws)
    .execute(&pool)
    .await
    .unwrap();

    let (status, res) = catalog(&app, Some(&bob), &ws).await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(res["message"], UNAUTHORIZED);
}

#[sqlx::test]
async fn no_token_is_401(pool: PgPool) {
    let (app, _) = app_with_mail(pool);
    let (_, ws) = signup(&app, "ada@example.com", "acme").await;

    assert_eq!(catalog(&app, None, &ws).await.0, StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn member_with_invite_create_may_invite(pool: PgPool) {
    let (app, _) = app_with_mail(pool.clone());
    let (_, ws) = signup(&app, "ada@example.com", "acme").await;
    let bob = member_with(&app, &pool, &ws, "bob@example.com", &["invite.create"]).await;
    common::verify_email(&pool, "bob@example.com").await;

    let (status, res) = invite(&app, &bob, &ws, "new@example.com").await;

    assert_eq!(status, StatusCode::CREATED, "{res}");
}

#[sqlx::test]
async fn member_without_invite_create_is_403(pool: PgPool) {
    let (app, _) = app_with_mail(pool.clone());
    let (_, ws) = signup(&app, "ada@example.com", "acme").await;
    let bob = member_with(&app, &pool, &ws, "bob@example.com", &["invite.list"]).await;
    common::verify_email(&pool, "bob@example.com").await;

    let (status, res) = invite(&app, &bob, &ws, "new@example.com").await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(res["message"], UNAUTHORIZED);
}

#[sqlx::test]
async fn workspace_admin_passes_every_check(pool: PgPool) {
    let (app, _) = app_with_mail(pool.clone());
    let (_, ws) = signup(&app, "ada@example.com", "acme").await;
    let bob = member_with(&app, &pool, &ws, "bob@example.com", &["workspace_admin"]).await;
    common::verify_email(&pool, "bob@example.com").await;

    assert_eq!(
        invite(&app, &bob, &ws, "new@example.com").await.0,
        StatusCode::CREATED
    );
}

#[sqlx::test]
async fn grant_from_deleted_team_or_other_workspace_does_not_count(pool: PgPool) {
    let (app, _) = app_with_mail(pool.clone());
    let (_, ws) = signup(&app, "ada@example.com", "acme").await;
    let bob = member_with(&app, &pool, &ws, "bob@example.com", &[]).await;
    common::verify_email(&pool, "bob@example.com").await;
    let gone = add_team(&pool, &ws, "Gone", &["invite.create"], true).await;
    join_team(&pool, &gone, "bob@example.com").await;
    let (_, eves) = signup(&app, "eve@example.com", "eves").await;
    let elsewhere = add_team(&pool, &eves, "Elsewhere", &["invite.create"], false).await;
    join_team(&pool, &elsewhere, "bob@example.com").await;

    assert_eq!(
        invite(&app, &bob, &ws, "new@example.com").await.0,
        StatusCode::FORBIDDEN
    );
}

#[sqlx::test]
async fn permitted_member_still_needs_verified_email(pool: PgPool) {
    let (app, _) = app_with_mail(pool.clone());
    let (_, ws) = signup(&app, "ada@example.com", "acme").await;
    let bob = member_with(&app, &pool, &ws, "bob@example.com", &["invite.create"]).await;

    let (status, res) = invite(&app, &bob, &ws, "new@example.com").await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(res["message"], "Verify your email address first.");
}
