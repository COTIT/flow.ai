//! Voice design writes run only through their own reviewed, signed, single-use card.

use super::assistant_preconditions::{approved, world};
use crate::common::{self, Api, send, send_raw};
use axum::http::{Method, StatusCode};
use serde_json::{Value, json};
use sqlx::PgPool;

fn home(api: &Api) -> String {
    format!("/v1/workspace/{}/storefront/pages/home", api.ws)
}
async fn dispatch(
    api: &Api,
    path: &str,
    body: Value,
    session: &str,
    action: &str,
) -> (StatusCode, Value) {
    let key = format!("voice-{action}");
    let (status, bytes) = send_raw(
        &api.app,
        Method::POST,
        path,
        Some(body),
        Some(&api.owner),
        &[
            ("Assistant-Action-Id", action),
            ("Assistant-Session-Id", session),
            ("Idempotency-Key", &key),
        ],
    )
    .await;
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}
async fn propose(
    api: &Api,
    token: &str,
    operation: &str,
    method: Method,
    path: &str,
    body: Value,
) -> (StatusCode, Value) {
    let base = format!("/v1/workspace/{}/assistant", api.ws);
    let (status, session) = send(
        &api.app,
        Method::POST,
        &format!("{base}/sessions"),
        Some(json!({"language":"en"})),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{session}");
    send(
        &api.app,
        Method::POST,
        &format!("{base}/actions"),
        Some(json!({"session_id":session["data"]["id"],"operation_id":operation,"method":method.as_str(),"path":path,"body":body})),
        Some(token),
    )
    .await
}
async fn revisions(api: &Api) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM storefront_design_revisions WHERE workspace_id=$1::uuid",
    )
    .bind(&api.ws)
    .fetch_one(&api.pool)
    .await
    .unwrap()
}
fn theme(version: i64, theme: &str) -> Value {
    json!({"expected_version":version,"changes":[{"op":"set_theme","theme":theme}]})
}

#[sqlx::test]
async fn edit_and_publish_run_once_through_reviewed_cards(pool: PgPool) {
    let (api, _, _) = world(pool).await;
    let edits = format!("{}/edits", home(&api));
    let body = theme(0, "neutral");
    let (session, action, review) =
        approved(&api, "storefront_design_edit", &edits, body.clone()).await;
    assert_eq!(review["review_facts"]["facts"]["kind"], "storefront_design");
    assert_eq!(
        review["review_facts"]["facts"]["changes"][0]["after"],
        "neutral"
    );

    let (status, _) = dispatch(&api, &edits, theme(0, "forest"), &session, &action).await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "a changed body is not the reviewed action"
    );
    assert_eq!(revisions(&api).await, 0);
    let (status, state) = dispatch(&api, &edits, body, &session, &action).await;
    assert_eq!(status, StatusCode::CREATED, "{state}");
    assert_eq!(state["data"]["draft"]["document"]["theme"], "neutral");
    assert_eq!(revisions(&api).await, 1);

    let draft = state["data"]["draft_revision_id"].clone();
    let publish = format!("{}/publish", home(&api));
    let body = json!({"expected_version":1,"revision_id":draft});
    let (session, action, review) =
        approved(&api, "storefront_design_publish", &publish, body.clone()).await;
    assert_eq!(
        review["review_facts"]["facts"]["changes"][0]["field"],
        "theme"
    );
    let (status, state) = dispatch(&api, &publish, body, &session, &action).await;
    assert_eq!(status, StatusCode::OK, "{state}");
    assert_eq!(state["data"]["published_revision_id"], draft);
}

#[sqlx::test]
async fn a_write_after_review_makes_the_card_stale(pool: PgPool) {
    let (api, _, _) = world(pool).await;
    let edits = format!("{}/edits", home(&api));
    let (session, action, _) =
        approved(&api, "storefront_design_edit", &edits, theme(0, "neutral")).await;
    let (status, bytes) = send_raw(
        &api.app,
        Method::POST,
        &edits,
        Some(json!({"expected_version":0,"changes":[{"op":"set_hero","section_id":"hero-main","heading":"Elsewhere"}]})),
        Some(&api.owner),
        &[("Idempotency-Key", "office-tab")],
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "{}",
        String::from_utf8_lossy(&bytes)
    );
    let (status, body) = dispatch(&api, &edits, theme(0, "neutral"), &session, &action).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(revisions(&api).await, 1, "only the other write");
}

#[sqlx::test]
async fn closed_design_routes_and_members_never_get_a_card(pool: PgPool) {
    let (api, _, _) = world(pool).await;
    let home = home(&api);
    for (operation, method, path, body) in [
        (
            "storefront_design_save",
            Method::POST,
            format!("{home}/revisions"),
            json!({"expected_version":0,"document":{}}),
        ),
        (
            "storefront_design_edit_preview",
            Method::POST,
            format!("{home}/edits/preview"),
            theme(0, "neutral"),
        ),
        (
            "storefront_design_preview",
            Method::GET,
            format!("{home}/preview"),
            Value::Null,
        ),
    ] {
        let (status, res) = propose(&api, &api.owner, operation, method, &path, body).await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{operation}: {res}"
        );
    }
    let member = common::member_with(
        &api.app,
        &api.pool,
        &api.ws,
        "member@example.com",
        &["setting.update"],
    )
    .await;
    let (status, res) = propose(
        &api,
        &member,
        "storefront_design_edit",
        Method::POST,
        &format!("{home}/edits"),
        theme(0, "neutral"),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{res}");
}
